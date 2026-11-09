//! mastercomfig's base VPK in `tf/custom`: whether it is there, who manages it, its
//! version, and removing a TF2Tools-managed install with the files it wrote.

use std::path::Path;

use rusqlite::OptionalExtension;

use super::{IoFailure, Library, game_custom_dir, io_at, remove_file_if_present};
use crate::db::Database;
use crate::error::{MastercomfigStatusError, RemoveMastercomfigError};
use crate::pak;
use crate::templates::ADDON_PREFIX;
use crate::tf2::Tf2Install;

pub(crate) const BASE_VPK: &str = "mastercomfig-base.vpk";
/// The cfg mastercomfig's autoexec runs to pick the preset, relative to `tf/custom`.
const SETUP_HOOK: &str = "comfig-custom/cfg/app/setup_hook.cfg";
/// The first line of a `modules.cfg` TF2Tools wrote; any other file is the player's.
pub(crate) const MODULES_HEADER: &str = "// Written by TF2Tools; changes here are replaced";

/// A mastercomfig graphics preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Preset {
    /// Ultra.
    Ultra,
    /// High.
    High,
    /// Medium, mastercomfig's default.
    Medium,
    /// Low.
    Low,
    /// Custom: the player's own module choices over Medium.
    Custom,
}

impl Preset {
    /// The picker's order; Destitute is left out, as comfig-app does.
    pub const PICKER: [Preset; 5] = [
        Preset::Ultra,
        Preset::High,
        Preset::Medium,
        Preset::Low,
        Preset::Custom,
    ];

    /// mastercomfig's name: "ultra", "high", "medium", "low", "custom".
    pub fn name(self) -> &'static str {
        match self {
            Preset::Ultra => "ultra",
            Preset::High => "high",
            Preset::Medium => "medium",
            Preset::Low => "low",
            Preset::Custom => "custom",
        }
    }

    /// The preset `name` names, trimmed and ignoring case; None for anything else.
    pub fn from_name(name: &str) -> Option<Preset> {
        let name = name.trim();
        Self::PICKER
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
    }
}

/// Whether mastercomfig is in `tf/custom` and who looks after it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MastercomfigStatus {
    /// No `mastercomfig-base.vpk` in `tf/custom`.
    NotInstalled,
    /// Installed by TF2Tools, at the recorded version and preset.
    Managed {
        /// The installed release tag.
        version: String,
        /// The chosen preset.
        preset: Preset,
    },
    /// Installed by hand or another tool; the version is read from the VPK when it can be.
    Outside {
        /// The version the VPK names, if it is readable.
        version: Option<String>,
    },
}

impl Library {
    /// NotInstalled without `tf/custom/mastercomfig-base.vpk`; Managed when a state row exists;
    /// else Outside with the version read from the VPK. Blocking.
    pub fn mastercomfig_status(
        &self,
        tf2: &Tf2Install,
    ) -> Result<MastercomfigStatus, MastercomfigStatusError> {
        let base = tf2.custom_dir().join(BASE_VPK);
        if !base.is_file() {
            return Ok(MastercomfigStatus::NotInstalled);
        }
        let state = self
            .mastercomfig_state()
            .map_err(|e| MastercomfigStatusError::Database {
                message: e.to_string(),
            })?;
        Ok(match state {
            // A preset name this build does not know reads as mastercomfig's default.
            Some((version, preset)) => MastercomfigStatus::Managed {
                version,
                preset: Preset::from_name(&preset).unwrap_or(Preset::Medium),
            },
            None => MastercomfigStatus::Outside {
                version: read_base_version(&base),
            },
        })
    }

    /// Deletes the base VPK and its .sound.cache, the three override VPKs and their caches,
    /// `comfig-custom/cfg/app/setup_hook.cfg`, `tf/cfg/overrides/modules.cfg` when its first
    /// line is MODULES_HEADER, then the state row. Refuses an unmanaged install.
    pub fn remove_mastercomfig(&self, tf2: &Tf2Install) -> Result<(), RemoveMastercomfigError> {
        let db = |e: rusqlite::Error| RemoveMastercomfigError::Database {
            message: e.to_string(),
        };
        let custom = game_custom_dir(tf2)?;
        if self.mastercomfig_state().map_err(db)?.is_none() {
            return Err(RemoveMastercomfigError::NotManaged);
        }

        // Files go first and the row last, so a failed delete stays Managed and retryable.
        let overrides = (1..=3).map(|n| format!("{ADDON_PREFIX}override-{n}.vpk"));
        for vpk in std::iter::once(BASE_VPK.to_owned()).chain(overrides) {
            remove_file_if_present(&custom.join(&vpk))?;
            remove_file_if_present(&custom.join(format!("{vpk}.sound.cache")))?;
        }
        remove_file_if_present(&custom.join(SETUP_HOOK))?;
        let modules = tf2.tf_dir().join("cfg/overrides/modules.cfg");
        if written_by_us(&modules)? {
            remove_file_if_present(&modules)?;
        }

        self.db
            .write(|conn| conn.execute("DELETE FROM mastercomfig_state", []))
            .map_err(db)?;
        Ok(())
    }

    /// The recorded (version, preset) of a TF2Tools-managed install, if there is one.
    fn mastercomfig_state(&self) -> rusqlite::Result<Option<(String, String)>> {
        self.db.read(|conn| {
            conn.query_row(
                "SELECT version, preset FROM mastercomfig_state WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
        })
    }
}

/// Records TF2Tools as managing mastercomfig at `version`, keeping a saved preset and
/// modules or starting on Medium with none. Returns the preset now in effect.
pub(crate) fn record_install(db: &Database, version: &str) -> rusqlite::Result<Preset> {
    let preset: String = db.write(|conn| {
        conn.execute(
            "INSERT INTO mastercomfig_state (id, version, preset, modules)
             VALUES (1, ?1, ?2, '{}')
             ON CONFLICT(id) DO UPDATE SET version = excluded.version",
            [version, Preset::Medium.name()],
        )?;
        conn.query_row(
            "SELECT preset FROM mastercomfig_state WHERE id = 1",
            [],
            |r| r.get(0),
        )
    })?;
    Ok(Preset::from_name(&preset).unwrap_or(Preset::Medium))
}

/// Writes `comfig-custom/cfg/app/setup_hook.cfg` under `custom` so mastercomfig loads `preset`.
pub(crate) fn write_setup_hook(custom: &Path, preset: Preset) -> Result<(), IoFailure> {
    let hook = custom.join(SETUP_HOOK);
    if let Some(dir) = hook.parent() {
        std::fs::create_dir_all(dir).map_err(io_at(dir))?;
    }
    std::fs::write(&hook, format!("preset={}\n", preset.name())).map_err(io_at(&hook))
}

/// Whether `path` is a `modules.cfg` TF2Tools wrote; a missing file is not.
fn written_by_us(path: &Path) -> Result<bool, RemoveMastercomfigError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io_at(path)(e).into()),
    };
    let first = bytes.split(|&b| b == b'\n').next().unwrap_or_default();
    Ok(first.strip_suffix(b"\r").unwrap_or(first) == MODULES_HEADER.as_bytes())
}

/// The release version the base VPK's `cfg/comfig/echo.cfg` names; None when unreadable.
pub(crate) fn read_base_version(vpk: &Path) -> Option<String> {
    const ECHO: &str = "cfg/comfig/echo.cfg";
    let pak = pak::open_vpk(vpk).ok()?;
    let data = pak::read_entry(vpk, ECHO, pak.tree.get(ECHO)?).ok()?;
    version_in(&String::from_utf8_lossy(&data))
}

/// The token after "mastercomfig version: " up to whitespace, a quote or a bar.
fn version_in(echo: &str) -> Option<String> {
    let rest = echo.split_once("mastercomfig version: ")?.1;
    let version: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '"' && *c != '|')
        .collect();
    (!version.is_empty()).then_some(version)
}
