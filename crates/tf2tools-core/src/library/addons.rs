//! Mastercomfig addons: VPKs kept in `library/addons/<asset name>`, installed by copying
//! into `tf/custom` and listing them in mastercomfig's `addons.cfg`. Ported from v5's
//! `commands/addons.rs` and `models/addon.rs`.

use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use super::{
    IoFailure, Library, check_vpk_magic, create_dir_all, game_custom_dir, import_db, install_db,
    io_at, is_plain_name, library_db, now, remove_file_if_present,
};
use crate::error::{ImportError, InstallError, LibraryError};
use crate::paths::LibraryKind;
use crate::templates::{self, ADDON_PREFIX};
use crate::tf2::Tf2Install;

/// Where mastercomfig reads the list of addons to exec, relative to `tf/custom`.
const ADDONS_CFG: [&str; 4] = ["comfig-custom", "cfg", "app", "addons.cfg"];

/// v5's display names and descriptions for the known addons; the GitHub release only
/// carries file names.
const ADDON_METADATA: &[(&str, &str, &str)] = &[
    (
        "mastercomfig-addon-null-canceling-movement.vpk",
        "Null-Canceling Movement",
        "Pressing opposing movement keys cancels to standing still instead of the last key winning",
    ),
    (
        "mastercomfig-addon-no-tutorial.vpk",
        "No Tutorial",
        "Disables tutorial popups, coaching messages, and map intro videos",
    ),
    (
        "mastercomfig-addon-no-pyroland.vpk",
        "No Pyroland",
        "Removes Pyroland textures and Pyrovision visual effects",
    ),
    (
        "mastercomfig-addon-flat-mouse.vpk",
        "Flat Mouse",
        "Ensures consistent raw mouse input with no OS acceleration and 1:1 zoom sensitivity",
    ),
    (
        "mastercomfig-addon-lowmem.vpk",
        "Low Memory",
        "Reduces memory usage for systems with 2GB RAM or less. May increase disk loading times",
    ),
    (
        "mastercomfig-addon-no-footsteps.vpk",
        "No Footsteps",
        "Removes all footstep sounds to reduce audio clutter",
    ),
    (
        "mastercomfig-addon-no-soundscapes.vpk",
        "No Soundscapes",
        "Removes ambient map noise and soundscapes for a quieter experience",
    ),
    (
        "mastercomfig-addon-transparent-viewmodels.vpk",
        "Transparent Viewmodels",
        "Enables transparent weapon viewmodels for better visibility",
    ),
    (
        "mastercomfig-addon-override-1.vpk",
        "Config Override 1",
        "Utility addon for custom config overrides (load order slot 1)",
    ),
    (
        "mastercomfig-addon-override-2.vpk",
        "Config Override 2",
        "Utility addon for custom config overrides (load order slot 2)",
    ),
    (
        "mastercomfig-addon-override-3.vpk",
        "Config Override 3",
        "Utility addon for custom config overrides (load order slot 3)",
    ),
];

/// A mastercomfig addon's asset name, `mastercomfig-addon-<name>.vpk`, safe to join
/// into a path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub struct AddonName(String);

impl AddonName {
    /// Accepts `mastercomfig-addon-<name>.vpk` (extension in any case, non-empty
    /// `<name>`) without `/`, `\` or `..`; anything else is `None`.
    pub fn new(name: impl Into<String>) -> Option<Self> {
        let name = name.into();
        let valid = !name.contains("..") && is_plain_name(&name) && addon_stem(&name).is_some();
        valid.then_some(Self(name))
    }

    /// The asset file name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `<name>` between the prefix and the extension; never empty.
    pub(crate) fn stem(&self) -> &str {
        addon_stem(&self.0).unwrap_or_default()
    }
}

/// The non-empty `<name>` of `mastercomfig-addon-<name>.vpk`, extension in any case.
fn addon_stem(file: &str) -> Option<&str> {
    let rest = file.strip_prefix(ADDON_PREFIX)?;
    let split = rest.len().checked_sub(".vpk".len())?;
    let (stem, ext) = (rest.get(..split)?, rest.get(split..)?);
    (ext.eq_ignore_ascii_case(".vpk") && !stem.is_empty()).then_some(stem)
}

/// v5's `addon_display_info`: the known addon's name and description, or for an unknown
/// one its `-`-separated words title-cased and a generic description.
pub(crate) fn display_info(name: &AddonName) -> (String, String) {
    if let Some(&(_, display, description)) = ADDON_METADATA
        .iter()
        .find(|(asset, _, _)| *asset == name.as_str())
    {
        return (display.to_owned(), description.to_owned());
    }
    let display = name
        .stem()
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    (display, "Mastercomfig addon".to_owned())
}

/// A mastercomfig addon in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AddonEntry {
    name: AddonName,
    display_name: String,
    description: String,
    installed_tag: String,
    store_tag: Option<String>,
    installed: bool,
    created_at: String,
    updated_at: String,
}

impl AddonEntry {
    /// The asset name.
    pub fn name(&self) -> &AddonName {
        &self.name
    }

    /// The name shown to the user.
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// v5's description for a known addon, or a generic one.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The release tag the library copy came from.
    pub fn installed_tag(&self) -> &str {
        &self.installed_tag
    }

    /// The release tag the addon store last saw, if the store lists this addon.
    pub fn store_tag(&self) -> Option<&str> {
        self.store_tag.as_deref()
    }

    /// True when the VPK is in `tf/custom`; false when listed without a TF2 install.
    pub fn is_installed(&self) -> bool {
        self.installed
    }

    /// True when the store has a different release than the library copy.
    pub fn update_available(&self) -> bool {
        self.store_tag
            .as_deref()
            .is_some_and(|tag| tag != self.installed_tag)
    }

    /// When the addon first entered the library, RFC 3339.
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    /// When the addon was last imported or installed, RFC 3339.
    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }
}

/// Every library row joined with its own store release tag.
const SELECT_ADDONS: &str = "SELECT asset_name, l.display_name, installed_tag, created_at,
        updated_at, s.release_tag
     FROM library_addons l LEFT JOIN store_addons s USING (asset_name)";

/// One row of [`SELECT_ADDONS`]; `None` when the asset name is not a valid addon name.
fn addon_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<AddonEntry>> {
    let Some(name) = AddonName::new(row.get::<_, String>(0)?) else {
        return Ok(None);
    };
    let (_, description) = display_info(&name);
    Ok(Some(AddonEntry {
        name,
        display_name: row.get(1)?,
        description,
        installed_tag: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        store_tag: row.get(5)?,
        installed: false,
    }))
}

impl Library {
    /// Copies an addon VPK (which must carry the VPK magic) to `library/addons/<name>`
    /// and records it with `release_tag`; a re-import keeps the row's creation time.
    pub fn import_addon(
        &self,
        vpk: &Path,
        name: &AddonName,
        release_tag: &str,
    ) -> Result<AddonEntry, ImportError> {
        check_vpk_magic(vpk)?;
        let dest = self.addon_path(name);
        let existed = dest.exists();
        if let Some(dir) = dest.parent() {
            create_dir_all(dir)?;
        }
        std::fs::copy(vpk, &dest).map_err(io_at(&dest))?;

        let (display_name, _) = display_info(name);
        let stamp = now();
        let recorded = self.db.write(|conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO library_addons (asset_name, display_name, installed_tag, created_at,
                     updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(asset_name) DO UPDATE SET
                     installed_tag = excluded.installed_tag,
                     updated_at = excluded.updated_at",
                params![name.as_str(), display_name, release_tag, stamp],
            )?;
            let entry = tx
                .query_row(
                    &format!("{SELECT_ADDONS} WHERE asset_name = ?1"),
                    [name.as_str()],
                    addon_from_row,
                )?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            tx.commit()?;
            Ok(entry)
        });
        recorded.map_err(|e| {
            if !existed {
                let _ = remove_file_if_present(&dest);
            }
            import_db(e)
        })
    }

    /// The library's addons ordered by display name; with a TF2 install each one says
    /// whether its VPK is in `tf/custom`. Rows with an invalid asset name are skipped.
    pub fn list_addons(&self, tf2: Option<&Tf2Install>) -> Result<Vec<AddonEntry>, LibraryError> {
        let mut addons = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare(&format!("{SELECT_ADDONS} ORDER BY l.display_name"))?;
                let rows = stmt.query_map([], addon_from_row)?;
                rows.filter_map(Result::transpose)
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(library_db)?;
        if let Some(tf2) = tf2 {
            let custom = tf2.custom_dir();
            for addon in &mut addons {
                addon.installed = custom.join(addon.name.as_str()).is_file();
            }
        }
        Ok(addons)
    }

    /// Copies a library addon into `tf/custom`, drops its stale `.sound.cache` and
    /// rewrites `addons.cfg`.
    pub fn install_addon(&self, name: &AddonName, tf2: &Tf2Install) -> Result<(), InstallError> {
        let source = self.addon_path(name);
        let recorded = self.addon_recorded(name).map_err(install_db)?;
        if !recorded || !source.is_file() {
            return Err(InstallError::NotInLibrary {
                what: format!("addon {}", name.as_str()),
            });
        }

        let custom = game_custom_dir(tf2)?;
        create_dir_all(&custom)?;
        let dest = custom.join(name.as_str());
        std::fs::copy(&source, &dest).map_err(io_at(&dest))?;
        remove_file_if_present(&sound_cache(&custom, name))?;
        rewrite_addons_cfg(&custom)?;

        self.db
            .write(|conn| {
                conn.execute(
                    "UPDATE library_addons SET updated_at = ?1 WHERE asset_name = ?2",
                    [now().as_str(), name.as_str()],
                )
            })
            .map_err(install_db)?;
        Ok(())
    }

    /// Removes an addon's VPK and `.sound.cache` from `tf/custom` and rewrites
    /// `addons.cfg`; the library entry stays.
    pub fn uninstall_addon(&self, name: &AddonName, tf2: &Tf2Install) -> Result<(), InstallError> {
        Ok(remove_installed(name, &game_custom_dir(tf2)?)?)
    }

    /// Forgets an addon: with a TF2 install it is uninstalled first, then its library
    /// VPK and row are removed.
    pub fn delete_addon(
        &self,
        name: &AddonName,
        tf2: Option<&Tf2Install>,
    ) -> Result<(), LibraryError> {
        if !self.addon_recorded(name).map_err(library_db)? {
            return Err(LibraryError::NotFound {
                what: format!("addon {}", name.as_str()),
            });
        }
        if let Some(tf2) = tf2 {
            remove_installed(name, &tf2.custom_dir())?;
        }
        // The row goes before the library VPK, so a failed removal never leaves a row
        // pointing at a missing file.
        self.db
            .write(|conn| {
                conn.execute(
                    "DELETE FROM library_addons WHERE asset_name = ?1",
                    [name.as_str()],
                )
            })
            .map_err(library_db)?;
        remove_file_if_present(&self.addon_path(name))?;
        Ok(())
    }

    /// v5's backfill: when `library_addons` is empty, copies each valid
    /// `mastercomfig-addon-*.vpk` in `tf/custom` into the library (name order), records it with
    /// the store's `release_tag` or "unknown", and rewrites `addons.cfg`. A failed copy logs
    /// `warn!` and still records the row. Returns how many rows it added; 0 when the library
    /// already had addons.
    pub fn adopt_installed_addons(&self, tf2: &Tf2Install) -> Result<usize, ImportError> {
        // Adoption only seeds an empty library, so the check comes before any file is touched.
        let has_addons = self
            .db
            .read(|conn| {
                conn.query_row("SELECT EXISTS (SELECT 1 FROM library_addons)", [], |r| {
                    r.get::<_, bool>(0)
                })
            })
            .map_err(import_db)?;
        if has_addons {
            return Ok(0);
        }
        let custom = tf2.custom_dir();
        let mut names: Vec<AddonName> = custom_files(&custom)?
            .into_iter()
            .filter_map(AddonName::new)
            .collect();
        if names.is_empty() {
            return Ok(0);
        }
        names.sort();

        // The installed copy is the truth, so it overwrites any stale library file; a copy
        // that fails still gets its row, as in v5, and a later reinstall reports it missing.
        for name in &names {
            let dest = self.addon_path(name);
            let copied = dest.parent().map_or(Ok(()), create_dir_all).and_then(|()| {
                std::fs::copy(custom.join(name.as_str()), &dest)
                    .map(drop)
                    .map_err(io_at(&dest))
            });
            if let Err(e) = copied {
                tracing::warn!(
                    addon = name.as_str(),
                    path = %e.path.display(),
                    error = %e.source,
                    "could not copy an installed addon into the library"
                );
            }
        }

        let stamp = now();
        let added = self
            .db
            .write(|conn| {
                let tx = conn.transaction()?;
                let mut added = 0;
                for name in &names {
                    let tag: Option<String> = tx
                        .query_row(
                            "SELECT release_tag FROM store_addons WHERE asset_name = ?1",
                            [name.as_str()],
                            |r| r.get(0),
                        )
                        .optional()?;
                    let (display_name, _) = display_info(name);
                    added += tx.execute(
                        "INSERT OR IGNORE INTO library_addons (asset_name, display_name,
                             installed_tag, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?4)",
                        params![
                            name.as_str(),
                            display_name,
                            tag.as_deref().unwrap_or("unknown"),
                            stamp
                        ],
                    )?;
                }
                tx.commit()?;
                Ok(added)
            })
            .map_err(import_db)?;
        rewrite_addons_cfg(&custom)?;
        Ok(added)
    }

    /// Whether `library_addons` has a row for `name`.
    fn addon_recorded(&self, name: &AddonName) -> rusqlite::Result<bool> {
        self.db.read(|conn| {
            conn.query_row(
                "SELECT 1 FROM library_addons WHERE asset_name = ?1",
                [name.as_str()],
                |_| Ok(()),
            )
            .optional()
            .map(|row| row.is_some())
        })
    }

    fn addon_path(&self, name: &AddonName) -> PathBuf {
        self.dir
            .library_dir(LibraryKind::Addons)
            .join(name.as_str())
    }
}

/// TF2 writes `<vpk>.sound.cache` beside a VPK; removing it makes TF2 rebuild it.
fn sound_cache(custom: &Path, name: &AddonName) -> PathBuf {
    custom.join(format!("{}.sound.cache", name.as_str()))
}

fn remove_installed(name: &AddonName, custom: &Path) -> Result<(), IoFailure> {
    remove_file_if_present(&custom.join(name.as_str()))?;
    remove_file_if_present(&sound_cache(custom, name))?;
    rewrite_addons_cfg(custom)
}

/// Regenerates `addons.cfg` from the addon VPKs now in `tf/custom`. With none left the
/// file is removed and its directory kept, as v5 did.
fn rewrite_addons_cfg(custom: &Path) -> Result<(), IoFailure> {
    let files = custom_files(custom)?;
    let names: Vec<&str> = files.iter().map(String::as_str).collect();
    let cfg = ADDONS_CFG
        .iter()
        .fold(custom.to_path_buf(), |p, c| p.join(c));
    match templates::addons_cfg(&names) {
        Some(content) => {
            if let Some(dir) = cfg.parent() {
                create_dir_all(dir)?;
            }
            std::fs::write(&cfg, content).map_err(io_at(&cfg))
        }
        None => remove_file_if_present(&cfg),
    }
}

/// The UTF-8 names of the regular files directly in `tf/custom`; none when it is absent.
fn custom_files(custom: &Path) -> Result<Vec<String>, IoFailure> {
    let mut files = Vec::new();
    match std::fs::read_dir(custom) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(io_at(custom))?;
                let is_file = entry.file_type().map_err(io_at(&entry.path()))?.is_file();
                if let (true, Ok(file)) = (is_file, entry.file_name().into_string()) {
                    files.push(file);
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(io_at(custom)(e)),
    }
    Ok(files)
}
