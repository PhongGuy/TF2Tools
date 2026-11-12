//! mastercomfig's base VPK in `tf/custom`: whether it is there, who manages it, its
//! version, its preset, taking over a hand install, writing back files another tool
//! cleared, and removing a TF2Tools-managed install with the files it wrote.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;

use super::addons::{ADDONS_CFG, rewrite_addons_cfg};
use super::movement::restore_config;
use super::{
    Choices, IoFailure, Library, ModuleCatalog, ModuleChoice, game_custom_dir, io_at,
    remove_file_if_present,
};
use crate::db::Database;
use crate::error::{
    ApplyPresetError, MastercomfigStatusError, RemoveMastercomfigError, RestoreMastercomfigError,
    TakeOverError,
};
use crate::pak;
use crate::templates::{BASE_VPK, override_vpk};
use crate::tf2::Tf2Install;

/// The cfg mastercomfig's autoexec runs to pick the preset, relative to `tf/custom`.
const SETUP_HOOK: &str = "comfig-custom/cfg/app/setup_hook.cfg";
/// The first line of a `modules.cfg` TF2Tools wrote; any other file is the player's.
const MODULES_HEADER: &str = "// Written by TF2Tools; changes here are replaced";

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
    /// line is MODULES_HEADER, then the state row. Refuses an unmanaged install. Returns true
    /// when `config.cfg` had Null-Canceling Movement's binds put back on the stock commands.
    pub fn remove_mastercomfig(&self, tf2: &Tf2Install) -> Result<bool, RemoveMastercomfigError> {
        let db = |e: rusqlite::Error| RemoveMastercomfigError::Database {
            message: e.to_string(),
        };
        let custom = game_custom_dir(tf2)?;
        if self.mastercomfig_state().map_err(db)?.is_none() {
            return Err(RemoveMastercomfigError::NotManaged);
        }

        // Files go first and the row last, so a failed delete stays Managed and retryable.
        let overrides = (0..3).map(override_vpk);
        for vpk in std::iter::once(BASE_VPK.to_owned()).chain(overrides) {
            remove_file_if_present(&custom.join(&vpk))?;
            remove_file_if_present(&custom.join(format!("{vpk}.sound.cache")))?;
        }
        remove_file_if_present(&custom.join(SETUP_HOOK))?;
        let modules = modules_cfg_path(tf2);
        if written_by_us(&modules)? {
            remove_file_if_present(&modules)?;
        }
        // Without mastercomfig no addon runs, so keys left on +mf/+mb/+ml/+mr would not move.
        let restored = restore_config(&tf2.tf_dir())?;

        self.db
            .write(|conn| conn.execute("DELETE FROM mastercomfig_state", []))
            .map_err(db)?;
        Ok(restored)
    }

    /// The saved preset and module choices of a TF2Tools-managed install; None when not managed.
    /// A modules column that does not parse reads as no choices. Blocking.
    pub fn mastercomfig_choices(&self) -> Result<Option<Choices>, MastercomfigStatusError> {
        read_choices(&self.db).map_err(|e| MastercomfigStatusError::Database {
            message: e.to_string(),
        })
    }

    /// Writes `setup_hook.cfg` = "preset=<name>\n" and saves the preset. Needs a managed install.
    pub fn apply_preset(&self, tf2: &Tf2Install, preset: Preset) -> Result<(), ApplyPresetError> {
        let db = |e: rusqlite::Error| ApplyPresetError::Database {
            message: e.to_string(),
        };
        let custom = game_custom_dir(tf2)?;
        if self.mastercomfig_state().map_err(db)?.is_none() {
            return Err(ApplyPresetError::NotManaged);
        }
        // The file goes first, so a failed write leaves the saved preset matching the file.
        write_setup_hook(&custom, preset)?;
        self.db
            .write(|conn| {
                conn.execute(
                    "UPDATE mastercomfig_state SET preset = ?1 WHERE id = 1",
                    [preset.name()],
                )
            })
            .map_err(db)?;
        Ok(())
    }

    /// Records an installed but unmanaged base as managed: version from the VPK ("unknown" when
    /// unreadable), preset from an existing setup_hook.cfg's `preset=<name>` line (else Medium),
    /// no modules. Touches no file. Returns the new Managed status.
    pub fn take_over_mastercomfig(
        &self,
        tf2: &Tf2Install,
    ) -> Result<MastercomfigStatus, TakeOverError> {
        let db = |e: rusqlite::Error| TakeOverError::Database {
            message: e.to_string(),
        };
        let custom = tf2.custom_dir();
        let base = custom.join(BASE_VPK);
        if !base.is_file() {
            return Err(TakeOverError::NotInstalled);
        }
        if self.mastercomfig_state().map_err(db)?.is_some() {
            return Err(TakeOverError::AlreadyManaged);
        }
        let version = read_base_version(&base).unwrap_or_else(|| "unknown".to_owned());
        let preset = std::fs::read_to_string(custom.join(SETUP_HOOK))
            .ok()
            .and_then(|hook| preset_in(&hook))
            .unwrap_or(Preset::Medium);
        self.db
            .write(|conn| {
                conn.execute(
                    "INSERT INTO mastercomfig_state (id, version, preset, modules)
                     VALUES (1, ?1, ?2, '{}')",
                    [version.as_str(), preset.name()],
                )
            })
            .map_err(db)?;
        Ok(MastercomfigStatus::Managed { version, preset })
    }

    /// Writes back a missing setup_hook.cfg for a managed install with its base present, and a
    /// missing addons.cfg when addon VPKs sit in tf/custom. True when it wrote anything.
    pub fn restore_mastercomfig_files(
        &self,
        tf2: &Tf2Install,
    ) -> Result<bool, RestoreMastercomfigError> {
        // A game folder that moved away is not recreated; there is nothing to restore into.
        if !tf2.tf_dir().is_dir() {
            return Ok(false);
        }
        let custom = tf2.custom_dir();
        let mut wrote = false;

        let hook_missing = !custom.join(SETUP_HOOK).is_file();
        if hook_missing && custom.join(BASE_VPK).is_file() {
            let state =
                self.mastercomfig_state()
                    .map_err(|e| RestoreMastercomfigError::Database {
                        message: e.to_string(),
                    })?;
            if let Some((_, preset)) = state {
                write_setup_hook(
                    &custom,
                    Preset::from_name(&preset).unwrap_or(Preset::Medium),
                )?;
                wrote = true;
            }
        }

        // addons.cfg is TF2Tools' own addon list, kept even for a hand-installed mastercomfig;
        // regenerating it writes the file only when addon VPKs are present.
        let addons = ADDONS_CFG.iter().fold(custom.clone(), |p, c| p.join(c));
        if !addons.is_file() {
            rewrite_addons_cfg(&custom)?;
            wrote |= addons.is_file();
        }
        Ok(wrote)
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

/// The saved preset and modules column as Choices; None without a state row.
pub(crate) fn read_choices(db: &Database) -> rusqlite::Result<Option<Choices>> {
    let row: Option<(String, String)> = db.read(|conn| {
        conn.query_row(
            "SELECT preset, modules FROM mastercomfig_state WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
    })?;
    Ok(row.map(|(preset, modules)| {
        let saved: BTreeMap<String, Option<String>> =
            serde_json::from_str(&modules).unwrap_or_default();
        let modules = saved
            .into_iter()
            .map(|(name, v)| (name, v.map_or(ModuleChoice::NotSet, ModuleChoice::Value)))
            .collect();
        Choices::from_saved(
            Preset::from_name(&preset).unwrap_or(Preset::Medium),
            modules,
        )
    }))
}

/// Saves `choices`' modules as `{"name":"value"|null}`, leaving the saved preset alone.
pub(crate) fn save_modules(db: &Database, choices: &Choices) -> rusqlite::Result<()> {
    let saved: BTreeMap<&str, Option<&str>> = choices
        .modules()
        .iter()
        .map(|(name, choice)| {
            let value = match choice {
                ModuleChoice::Value(v) => Some(v.as_str()),
                ModuleChoice::NotSet => None,
            };
            (name.as_str(), value)
        })
        .collect();
    let json = serde_json::to_string(&saved)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
    db.write(|conn| {
        conn.execute(
            "UPDATE mastercomfig_state SET modules = ?1 WHERE id = 1",
            [json],
        )
    })?;
    Ok(())
}

/// `tf/cfg/overrides/modules.cfg`, which mastercomfig runs for module choices.
pub(crate) fn modules_cfg_path(tf2: &Tf2Install) -> PathBuf {
    tf2.tf_dir().join("cfg/overrides/modules.cfg")
}

/// Writes MODULES_HEADER and one line per choice in catalog order: `name=value`, or
/// `alias name` to leave a module unset.
pub(crate) fn write_modules_cfg(
    path: &Path,
    catalog: &ModuleCatalog,
    choices: &Choices,
) -> Result<(), IoFailure> {
    let mut text = format!("{MODULES_HEADER}\n");
    let modules = catalog.categories().iter().flat_map(|c| c.modules());
    for (name, choice) in modules.filter_map(|m| Some((m.name(), choices.get(m.name())?))) {
        match choice {
            ModuleChoice::Value(v) => text.push_str(&format!("{name}={v}\n")),
            ModuleChoice::NotSet => text.push_str(&format!("alias {name}\n")),
        }
    }
    // A temp file beside the target, renamed over it, so the game never reads a half-written cfg.
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(io_at(dir))?;
    let mut part = tempfile::NamedTempFile::new_in(dir).map_err(io_at(dir))?;
    part.write_all(text.as_bytes())
        .map_err(io_at(part.path()))?;
    part.persist(path).map_err(|e| io_at(path)(e.error))?;
    Ok(())
}

/// Renames the player's own `path` to the first free of `<name>.bak`, `.bak.1`, `.bak.2`, ...
pub(crate) fn back_up(path: &Path) -> Result<(), IoFailure> {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    let first = PathBuf::from(backup);
    let mut target = first.clone();
    let mut n = 0u32;
    while target.try_exists().map_err(io_at(&target))? {
        n += 1;
        let mut next = first.clone().into_os_string();
        next.push(format!(".{n}"));
        target = PathBuf::from(next);
    }
    std::fs::rename(path, &target).map_err(io_at(path))
}

/// Writes `comfig-custom/cfg/app/setup_hook.cfg` under `custom` so mastercomfig loads `preset`.
pub(crate) fn write_setup_hook(custom: &Path, preset: Preset) -> Result<(), IoFailure> {
    let hook = custom.join(SETUP_HOOK);
    if let Some(dir) = hook.parent() {
        std::fs::create_dir_all(dir).map_err(io_at(dir))?;
    }
    std::fs::write(&hook, format!("preset={}\n", preset.name())).map_err(io_at(&hook))
}

/// The first `preset=<name>` line naming a known preset, skipping `//` comments.
fn preset_in(hook: &str) -> Option<Preset> {
    hook.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//"))
        .filter_map(|line| line.strip_prefix("preset="))
        .find_map(Preset::from_name)
}

/// Whether `path` is a `modules.cfg` TF2Tools wrote; a missing file is not.
pub(crate) fn written_by_us(path: &Path) -> Result<bool, IoFailure> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io_at(path)(e)),
    };
    let first = bytes.split(|&b| b == b'\n').next().unwrap_or_default();
    Ok(first.strip_suffix(b"\r").unwrap_or(first) == MODULES_HEADER.as_bytes())
}

/// The release version the base VPK's `cfg/comfig/echo.cfg` names; None when unreadable.
fn read_base_version(vpk: &Path) -> Option<String> {
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
