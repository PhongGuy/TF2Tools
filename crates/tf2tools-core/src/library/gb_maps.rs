//! GameBanana maps installed into `tf/custom`: where the game already has a map, and
//! installing or updating an item's chosen maps as one `tf2tools-gb-<id>-<slug>` folder.

use std::path::Path;

use super::gb_addons::{
    GbAddonEntry, installed_base, recorded_path, remove_entry, remove_installed,
};
use super::hud_lowercase::{LOWERCASE_INSTALLS, lowercase_tree};
use super::tree_copy::copy_tree;
use super::{
    GameMissing, GbId, IoFailure, Library, create_dir_all, game_custom_dir, io_at, is_plain_name,
    library_db,
};
use crate::error::{GbAddonInstallError, GbMapInstallError, LibraryError};
use crate::map_payloads::MapPayload;
use crate::store::{GbMapDownload, MapIndex, MapSource, downloaded_names, lowercase_all};
use crate::tf2::Tf2Install;

/// Where a new map folder is copied under `tf/custom` before it replaces the old one. The
/// leading dot and the copy one level down keep any scan from taking it for a map folder.
const MAP_STAGING_DIR: &str = ".tf2tools-map-staging";

impl Library {
    /// Where `tf2` already has map `name`, ignoring case, or None. `except` skips that
    /// GameBanana item's own folder, so an update does not find the map it replaces.
    pub fn map_presence(
        &self,
        tf2: &Tf2Install,
        name: &str,
        except: Option<GbId>,
    ) -> Result<Option<MapSource>, LibraryError> {
        let serveme = lowercase_all(self.db.read(downloaded_names).map_err(library_db)?);
        Ok(MapIndex::scan(tf2).source(name, except, &serveme))
    }

    /// Installs the `chosen` maps of `download` that the game does not already have into
    /// `tf/custom/tf2tools-gb-<id>-<slug>`, replacing any earlier install of the item, and
    /// returns its row. A new folder only loads after a restart, so TF2 may be running.
    ///
    /// # Errors
    /// [`GbMapInstallError`]: a refusal before anything changed, or an I/O or database
    /// failure; a failure while writing the folder leaves the row not installed.
    pub fn install_gb_map(
        &self,
        download: GbMapDownload,
        chosen: &[String],
        tf2: &Tf2Install,
        _tf2_running: &dyn Fn() -> bool,
    ) -> Result<GbAddonEntry, GbMapInstallError> {
        let _writes = self.lock_writes();
        self.write_gb_map(download, chosen, tf2)
    }

    /// Installs a newer download of a map item over its install, as
    /// [`install_gb_map`](Self::install_gb_map) does, but refuses while TF2 runs, since the
    /// game may hold the old maps open.
    ///
    /// # Errors
    /// [`GbMapInstallError::Tf2Running`] before anything changed, or as `install_gb_map`.
    pub fn update_gb_map(
        &self,
        download: GbMapDownload,
        chosen: &[String],
        tf2: &Tf2Install,
        tf2_running: &dyn Fn() -> bool,
    ) -> Result<GbAddonEntry, GbMapInstallError> {
        let _writes = self.lock_writes();
        if tf2_running() {
            return Err(GbMapInstallError::Tf2Running);
        }
        self.write_gb_map(download, chosen, tf2)
    }

    /// Install's body, under the write lock: stage the folder in the download's scratch dir,
    /// save the row not installed, then swap the game's folder for a copy of the stage.
    fn write_gb_map(
        &self,
        download: GbMapDownload,
        chosen: &[String],
        tf2: &Tf2Install,
    ) -> Result<GbAddonEntry, GbMapInstallError> {
        if chosen.is_empty() {
            return Err(GbMapInstallError::NothingChosen);
        }
        if let Some(name) = chosen
            .iter()
            .find(|c| !download.maps.iter().any(|m| m.name() == c.as_str()))
        {
            return Err(GbMapInstallError::UnknownMap { name: name.clone() });
        }
        let custom = game_custom_dir(tf2)?;
        recover_aside(&custom)?;
        let gb_id = download.gb_id;

        // One scan of the game serves every chosen map; the item's own folder is skipped so
        // installing again or updating does not find the maps it replaces.
        let serveme = lowercase_all(self.db.read(downloaded_names).map_err(|e| {
            GbMapInstallError::Database {
                message: e.to_string(),
            }
        })?);
        let index = MapIndex::scan(tf2);
        let kept: Vec<&MapPayload> = download
            .maps
            .iter()
            .filter(|m| chosen.iter().any(|c| c == m.name()))
            .filter(|m| index.source(m.name(), Some(gb_id), &serveme).is_none())
            .collect();
        if kept.is_empty() {
            return Err(GbMapInstallError::AlreadyPresent {
                names: chosen.to_vec(),
            });
        }

        let folder = installed_base(gb_id, &download.name);
        let stage = download.scratch_dir().join("install").join(&folder);
        stage_maps(&stage, &kept)?;
        if LOWERCASE_INSTALLS {
            lowercase_tree(&stage, &folder)?;
        }
        let payloads: Vec<String> = kept
            .iter()
            .map(|m| {
                if LOWERCASE_INSTALLS {
                    m.name().to_ascii_lowercase()
                } else {
                    m.name().to_owned()
                }
            })
            .collect();

        // The row is saved not installed before tf/custom changes, naming the old files as
        // well as the new, so a failure part-way leaves every file a row Remove can clear.
        let own = format!("custom/{folder}");
        let previous = self.gb_addon_row(gb_id).map_err(map_error)?;
        let mut files = vec![own.clone()];
        for file in previous.iter().flat_map(GbAddonEntry::files) {
            if !files.contains(file) {
                files.push(file.clone());
            }
        }
        let row = |files| {
            GbAddonEntry::new_map_row(
                gb_id,
                download.name.clone(),
                download.author.clone(),
                download.file_date,
                payloads.clone(),
                files,
            )
        };
        self.save_gb_addon(&row(files.clone()), download.file_id)
            .map_err(map_error)?;

        // The scratch dir and the game are often on different disks, so the stage is copied
        // into a staging folder beside the target first. Only a complete copy goes in, by
        // renames on the game's own disk that set the old folder aside rather than delete it.
        let tf = tf2.tf_dir();
        let target = custom.join(&folder);
        let staging = custom.join(MAP_STAGING_DIR);
        let (fresh, aside) = (
            staging.join("new").join(&folder),
            staging.join("old").join(&folder),
        );
        let ready = remove_entry(&staging)
            .and_then(|()| copy_tree(&stage, &fresh, &|_: &Path| false).map(drop))
            .and_then(|()| create_dir_all(&staging.join("old")))
            .and_then(|()| {
                files
                    .iter()
                    .filter(|file| **file != own)
                    .filter_map(|file| recorded_path(&tf, gb_id, file))
                    .try_for_each(|path| remove_installed(&path))
            });
        if let Err(e) = ready {
            let _ = remove_entry(&staging);
            return Err(e.into());
        }
        let swapped = swap_in(&target, &fresh, &aside);
        // A set-aside copy that could not be put back is the only one left, so it stays.
        if (swapped.is_ok() || !aside.exists())
            && let Err(IoFailure { path, source }) = remove_entry(&staging)
        {
            tracing::warn!(path = %path.display(), error = %source, "map staging left behind");
        }
        swapped?;

        let mut entry = row(vec![own]);
        entry.mark_installed();
        self.save_gb_addon(&entry, download.file_id)
            .map_err(map_error)?;
        Ok(entry)
    }
}

/// Puts back each folder an interrupted swap left in the staging dir's `old/` whose place in
/// `custom` is empty, so the scan sees it and the staging clear never deletes the only copy.
fn recover_aside(custom: &Path) -> Result<(), IoFailure> {
    let old = custom.join(MAP_STAGING_DIR).join("old");
    for entry in std::fs::read_dir(&old).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let home = custom.join(&name);
        if is_plain_name(&name) && !name.contains(':') && home.symlink_metadata().is_err() {
            std::fs::rename(entry.path(), &home).map_err(io_at(&home))?;
        }
    }
    Ok(())
}

/// Renames the folder at `target`, if any, aside to `aside`, then `fresh` in at `target`.
/// When the second rename fails the first is undone, so `target` never goes missing while
/// its old copy can be put back.
fn swap_in(target: &Path, fresh: &Path, aside: &Path) -> Result<(), IoFailure> {
    let had_old = target.symlink_metadata().is_ok();
    if had_old {
        std::fs::rename(target, aside).map_err(io_at(target))?;
    }
    std::fs::rename(fresh, target)
        .map_err(io_at(target))
        .inspect_err(|_| {
            if had_old {
                let _ = std::fs::rename(aside, target);
            }
        })
}

/// Moves each map into `stage/maps/<name>.bsp` and each companion to its destination, the
/// first to claim a destination winning. Renames stay inside the download's scratch dir.
fn stage_maps(stage: &Path, maps: &[&MapPayload]) -> Result<(), IoFailure> {
    remove_entry(stage)?;
    let mut moves = Vec::new();
    for map in maps {
        moves.push((map.bsp(), format!("maps/{}.bsp", map.name())));
        for (source, dest) in map.companions() {
            moves.push((source.as_path(), dest.clone()));
        }
    }
    // A companion shared by several maps of one root was already moved by the first.
    for (source, dest) in moves {
        let target = stage.join(dest);
        if target.exists() || !source.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            create_dir_all(parent)?;
        }
        std::fs::rename(source, &target).map_err(io_at(&target))?;
    }
    Ok(())
}

/// The add-on row helpers' errors as a map install's.
fn map_error(e: GbAddonInstallError) -> GbMapInstallError {
    match e {
        GbAddonInstallError::Io { path, source } => GbMapInstallError::Io { path, source },
        GbAddonInstallError::Database { message } => GbMapInstallError::Database { message },
        GbAddonInstallError::GameMissing { path } => GbMapInstallError::GameMissing { path },
        GbAddonInstallError::Tf2Running => GbMapInstallError::Tf2Running,
        GbAddonInstallError::NothingChosen => GbMapInstallError::NothingChosen,
        GbAddonInstallError::UnknownPayload { name } => GbMapInstallError::UnknownMap { name },
        GbAddonInstallError::NotInLibrary { gb_id } => GbMapInstallError::Database {
            message: format!("GameBanana item {gb_id} is not in the library"),
        },
    }
}

impl From<GameMissing> for GbMapInstallError {
    fn from(GameMissing { path }: GameMissing) -> Self {
        Self::GameMissing { path }
    }
}

impl From<IoFailure> for GbMapInstallError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}
