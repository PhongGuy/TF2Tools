//! The live link: swapping a fresh copy of the installed HUD into `tf/custom` through a
//! staging folder, so TF2 always has a complete copy, and putting back a copy a crash
//! left set aside.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::OptionalExtension;

use super::{
    HudId, HudStatus, IoFailure, Library, copy_dir_excluding_metadata, create_dir_all,
    game_custom_dir, is_plain_name, made_huds, remove_dir_if_present,
};
use crate::error::{LibraryError, ReinstallError};
use crate::hud_maker::is_device_name;
use crate::paths::LibraryKind;
use crate::tf2::Tf2Install;

/// The staging folder under `tf/custom`. Its leading dot keeps it from ever being a HUD's
/// folder name, and its copies sit one level down so no scan mistakes it for a HUD.
pub(crate) const STAGING_DIR: &str = ".tf2tools-reinstall";
/// Inside `STAGING_DIR`: names the folder while its installed copy may sit in `old/`.
const MARKER: &str = "swapping";

/// Serialises every change to installed HUD folders across the process.
static HUD_SWAP: Mutex<()> = Mutex::new(());

/// Takes the swap lock; a panic in another holder leaves nothing half-locked, so a
/// poisoned lock is taken anyway.
pub(super) fn hud_swap_lock() -> MutexGuard<'static, ()> {
    HUD_SWAP.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Deletes any staging folder under `custom`.
pub(super) fn clear_staging(custom: &Path) -> Result<(), IoFailure> {
    remove_dir_if_present(&custom.join(STAGING_DIR))
}

/// What `Library::reinstall_hud` did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reinstall {
    /// The installed copy at `tf/custom/<folder>` now matches the library.
    Updated {
        /// The folder under `tf/custom/` that was updated.
        folder: String,
    },
    /// The HUD is not the one installed HUD, so nothing was touched.
    NotInstalled,
}

/// What `Library::recover_reinstall` found.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Recovery {
    /// No staging folder was there.
    Clean,
    /// A leftover staging folder was deleted and nothing was put back.
    Cleared,
    /// A HUD an interrupted swap set aside was put back.
    Restored {
        /// The folder under `tf/custom/` that was put back.
        folder: String,
    },
}

impl Library {
    /// Copies library HUD `id` over its installed copy when it is the one installed HUD (a
    /// `HudStatus::Managed` with this id), else returns `NotInstalled` and touches nothing.
    /// Fails with `Changed` and touches nothing when the folder is no longer what TF2Tools
    /// put there. The copy is staged beside the installed one and swapped in only once
    /// complete, so a failure leaves the previous installed copy. Records only the new
    /// copy's fingerprint.
    pub fn reinstall_hud(&self, id: &HudId, tf2: &Tf2Install) -> Result<Reinstall, ReinstallError> {
        let _swap = hud_swap_lock();
        let custom = game_custom_dir(tf2)?;
        // A crash's leftover goes first, so the status below sees the HUD where it belongs.
        recover_in(&custom, &mut std_rename)?;
        let folder = match self.hud_status(tf2).map_err(status_error)? {
            HudStatus::Managed {
                folder_name,
                id: installed,
            } if &installed == id && is_restorable_name(&folder_name) => folder_name,
            HudStatus::NotInstalled
            | HudStatus::Managed { .. }
            | HudStatus::Unmanaged { .. }
            | HudStatus::Multiple { .. } => return Ok(Reinstall::NotInstalled),
        };
        let src = self.reinstall_source(id)?;
        // A folder that is no longer what TF2Tools put there is the player's, so it stays.
        let external = self.external_in(&custom).map_err(status_error)?;
        if external.iter().any(|hud| hud.folder_name() == folder) {
            return Err(ReinstallError::Changed { folder });
        }

        // Staged one level down, so no scan of `tf/custom` takes the copy for a HUD.
        let staging = custom.join(STAGING_DIR);
        let staged = copy_dir_excluding_metadata(&src, &staging.join("new").join(&folder))
            .and_then(|()| create_dir_all(&staging.join("old")));
        if let Err(IoFailure { path, source }) = staged {
            discard(&staging);
            return Err(ReinstallError::Unchanged { path, source });
        }
        match swap_in(&custom, &folder, &mut std_rename) {
            // The set-aside copy and its marker are the only way back; recovery needs both.
            Err(e @ ReinstallError::SetAside { .. }) => Err(e),
            result => {
                discard(&staging);
                result?;
                // The HUD is updated by now, so a settings file that cannot be written is
                // only logged; the next install tries again.
                if let Err(IoFailure { path, source }) = self.write_hud_cfg(&src, tf2) {
                    tracing::warn!(path = %path.display(), error = %source, "HUD settings file not written");
                }
                self.record_install_print(&custom, &folder).map_err(|e| {
                    ReinstallError::Database {
                        message: e.to_string(),
                    }
                })?;
                Ok(Reinstall::Updated { folder })
            }
        }
    }

    /// Where library HUD `id`'s files are: its library folder, or a made HUD's previous
    /// save stranded by an interrupted re-save.
    fn reinstall_source(&self, id: &HudId) -> Result<PathBuf, ReinstallError> {
        let not_in_library = || ReinstallError::NotInLibrary {
            what: format!("HUD {id}"),
        };
        let folder: Option<String> = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT folder_name FROM library_huds WHERE id = ?1",
                    [id.as_str()],
                    |r| r.get(0),
                )
                .optional()
            })
            .map_err(|e| ReinstallError::Database {
                message: e.to_string(),
            })?;
        let folder = folder
            .filter(|f| is_plain_name(f))
            .ok_or_else(not_in_library)?;
        let live = self.dir.library_dir(LibraryKind::Huds).join(folder);
        if live.is_dir() {
            Ok(live)
        } else {
            made_huds::stranded_save(&live).ok_or_else(not_in_library)
        }
    }

    /// Puts back the HUD an interrupted swap set aside, when the staging folder's marker
    /// names it and its folder in `tf/custom` is empty, then removes the staging folder.
    /// Holds the swap lock.
    pub fn recover_reinstall(&self, tf2: &Tf2Install) -> Result<Recovery, ReinstallError> {
        let _swap = hud_swap_lock();
        recover_in(&game_custom_dir(tf2)?, &mut std_rename)
    }
}

/// `hud_status` and `external_in` fail only with `Io` or `Database`; they look up no single
/// item, so `NotFound` and the rest never reach here.
fn status_error(e: LibraryError) -> ReinstallError {
    match e {
        LibraryError::Io { path, source } => ReinstallError::Unchanged { path, source },
        other @ LibraryError::NotFound { .. }
        | other @ LibraryError::InvalidWeaponScript { .. }
        | other @ LibraryError::Vpk(_)
        | other @ LibraryError::Vtf(_)
        | other @ LibraryError::Database { .. } => ReinstallError::Database {
            message: other.to_string(),
        },
    }
}

/// Deletes the staging folder after a swap that needs nothing in it; a leftover is only
/// logged, since recovery deletes it later and never restores an unmarked copy.
fn discard(staging: &Path) {
    if let Err(IoFailure { path, source }) = remove_dir_if_present(staging) {
        tracing::warn!(path = %path.display(), error = %source, "reinstall staging left behind");
    }
}

/// Whether a marker's name can only be one folder under `tf/custom`: no drive prefix, no
/// device name, and on Windows no outer dot or space that the system would trim away.
fn is_restorable_name(name: &str) -> bool {
    let trimmed_on_windows = cfg!(windows) && name.ends_with(['.', ' ']);
    is_plain_name(name)
        && !name.contains(':')
        && !trimmed_on_windows
        && !is_device_name(&name.to_ascii_lowercase())
}

fn std_rename(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)
}

type Rename<'a> = &'a mut dyn FnMut(&Path, &Path) -> io::Result<()>;

/// Swaps `custom/STAGING_DIR/new/<folder>` in at `custom/<folder>`: marks the swap, renames
/// the installed copy aside to `old/<folder>`, renames the staged copy in, unmarks. When the
/// second rename fails the first is undone; when the undo fails too the marker stays, so
/// recovery puts the old copy back.
fn swap_in(custom: &Path, folder: &str, rename: Rename<'_>) -> Result<(), ReinstallError> {
    let staging = custom.join(STAGING_DIR);
    let (installed, staged) = (custom.join(folder), staging.join("new").join(folder));
    let (aside, marker) = (staging.join("old").join(folder), staging.join(MARKER));
    std::fs::write(&marker, folder).map_err(|source| ReinstallError::Unchanged {
        path: marker.clone(),
        source,
    })?;
    // A folder gone before it could move was never held; only a refused move means in use.
    rename(&installed, &aside).map_err(|source| {
        let path = installed.clone();
        if source.kind() == io::ErrorKind::NotFound {
            ReinstallError::Unchanged { path, source }
        } else {
            ReinstallError::InUse { path, source }
        }
    })?;
    if let Err(source) = rename(&staged, &installed) {
        return Err(match rename(&aside, &installed) {
            Ok(()) => ReinstallError::InUse {
                path: staged,
                source,
            },
            Err(_) => ReinstallError::SetAside {
                path: aside,
                source,
            },
        });
    }
    // Once the new copy is in, the old one must never come back; a marker that cannot be
    // removed still names a folder that is present, which recovery leaves alone.
    if let Err(e) = std::fs::remove_file(&marker) {
        tracing::warn!(path = %marker.display(), error = %e, "swap marker left behind");
    }
    Ok(())
}

/// Puts back `old/<name>` when the marker names `<name>` and `custom/<name>` is absent, then
/// removes the staging folder. Without the marker a set-aside copy is stale and is deleted.
fn recover_in(custom: &Path, rename: Rename<'_>) -> Result<Recovery, ReinstallError> {
    let staging = custom.join(STAGING_DIR);
    if staging.symlink_metadata().is_err() {
        return Ok(Recovery::Clean);
    }
    let marker = staging.join(MARKER);
    let named = match std::fs::read_to_string(&marker) {
        Ok(name) => Some(name),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(ReinstallError::Unchanged {
                path: marker,
                source,
            });
        }
    };
    let mut restored = None;
    if let Some(name) = named.filter(|n| is_restorable_name(n)) {
        let (aside, home) = (staging.join("old").join(&name), custom.join(&name));
        if aside.is_dir() && home.symlink_metadata().is_err() {
            rename(&aside, &home).map_err(|source| ReinstallError::SetAside {
                path: aside.clone(),
                source,
            })?;
            restored = Some(name);
        }
    }
    remove_dir_if_present(&staging).map_err(|e| ReinstallError::Unchanged {
        path: e.path,
        source: e.source,
    })?;
    Ok(restored.map_or(Recovery::Cleared, |folder| Recovery::Restored { folder }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    // A rename that fails on the calls numbered in `fail` (1-based) and renames otherwise.
    fn failing_on(fail: &'static [usize]) -> impl FnMut(&Path, &Path) -> io::Result<()> {
        let mut call = 0;
        move |from, to| {
            call += 1;
            if fail.contains(&call) {
                Err(io::Error::other("held open"))
            } else {
                std::fs::rename(from, to)
            }
        }
    }

    // `custom/hud` holding "old", `custom/.tf2tools-reinstall/new/hud` holding "new", and the
    // empty `old/` folder a reinstall prepares.
    fn staged() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tmp.path().join("custom");
        let staging = custom.join(STAGING_DIR);
        std::fs::create_dir_all(custom.join("hud")).unwrap();
        std::fs::write(custom.join("hud/info.vdf"), "old").unwrap();
        std::fs::create_dir_all(staging.join("new/hud")).unwrap();
        std::fs::write(staging.join("new/hud/info.vdf"), "new").unwrap();
        std::fs::create_dir_all(staging.join("old")).unwrap();
        (tmp, custom)
    }

    fn info(custom: &Path) -> String {
        std::fs::read_to_string(custom.join("hud/info.vdf")).unwrap()
    }

    fn marked(custom: &Path) -> bool {
        custom.join(STAGING_DIR).join(MARKER).is_file()
    }

    #[test]
    fn a_swap_puts_the_staged_copy_in_and_unmarks() {
        let (_tmp, custom) = staged();
        swap_in(&custom, "hud", &mut std_rename).unwrap();
        assert_eq!(info(&custom), "new");
        let aside = custom.join(STAGING_DIR).join("old/hud/info.vdf");
        assert_eq!(std::fs::read_to_string(aside).unwrap(), "old");
        assert!(!marked(&custom));
    }

    #[test]
    fn a_refused_set_aside_changes_nothing() {
        let (_tmp, custom) = staged();
        let err = swap_in(&custom, "hud", &mut failing_on(&[1])).unwrap_err();
        let ReinstallError::InUse { path, .. } = &err else {
            panic!("{err:?}");
        };
        assert_eq!(path, &custom.join("hud"));
        assert_eq!(info(&custom), "old");
    }

    #[test]
    fn a_failed_swap_in_puts_the_old_copy_back() {
        let (_tmp, custom) = staged();
        let err = swap_in(&custom, "hud", &mut failing_on(&[2])).unwrap_err();
        assert!(matches!(err, ReinstallError::InUse { .. }), "{err:?}");
        assert_eq!(info(&custom), "old");
        assert!(!custom.join(STAGING_DIR).join("old/hud").exists());
    }

    #[test]
    fn a_failed_undo_leaves_the_old_copy_marked_for_recovery() {
        let (_tmp, custom) = staged();
        let err = swap_in(&custom, "hud", &mut failing_on(&[2, 3])).unwrap_err();
        let ReinstallError::SetAside { path, .. } = &err else {
            panic!("{err:?}");
        };
        assert_eq!(path, &custom.join(STAGING_DIR).join("old/hud"));
        assert!(!custom.join("hud").exists());
        assert!(marked(&custom));

        let recovered = recover_in(&custom, &mut std_rename).unwrap();
        assert_eq!(
            recovered,
            Recovery::Restored {
                folder: "hud".to_owned()
            }
        );
        assert_eq!(info(&custom), "old");
        assert!(!custom.join(STAGING_DIR).exists());
    }

    #[test]
    fn a_recovery_that_cannot_put_back_says_where_the_hud_is() {
        let (_tmp, custom) = staged();
        swap_in(&custom, "hud", &mut failing_on(&[2, 3])).unwrap_err();
        let err = recover_in(&custom, &mut failing_on(&[1])).unwrap_err();
        assert!(matches!(err, ReinstallError::SetAside { .. }), "{err:?}");
        assert!(custom.join(STAGING_DIR).join("old/hud/info.vdf").is_file());
        assert!(marked(&custom));
    }

    #[test]
    fn a_finished_swap_whose_staging_delete_failed_never_comes_back() {
        let (_tmp, custom) = staged();
        swap_in(&custom, "hud", &mut std_rename).unwrap();
        // The player deletes the HUD by hand before the leftover is cleared.
        std::fs::remove_dir_all(custom.join("hud")).unwrap();
        assert_eq!(
            recover_in(&custom, &mut std_rename).unwrap(),
            Recovery::Cleared
        );
        assert!(!custom.join("hud").exists());
        assert!(!custom.join(STAGING_DIR).exists());
    }

    // Windows trims trailing spaces from folder names, so such a folder cannot exist there.
    #[cfg(not(windows))]
    #[test]
    fn a_folder_named_with_outer_spaces_comes_back_as_named() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tmp.path().join("custom");
        let staging = custom.join(STAGING_DIR);
        std::fs::create_dir_all(custom.join(" spaced ")).unwrap();
        std::fs::create_dir_all(staging.join("new/ spaced ")).unwrap();
        std::fs::create_dir_all(staging.join("old")).unwrap();
        swap_in(&custom, " spaced ", &mut failing_on(&[2, 3])).unwrap_err();
        let recovered = recover_in(&custom, &mut std_rename).unwrap();
        assert_eq!(
            recovered,
            Recovery::Restored {
                folder: " spaced ".to_owned()
            }
        );
        assert!(custom.join(" spaced ").is_dir());
    }

    #[test]
    fn a_vanished_installed_folder_is_unchanged_not_in_use() {
        let (_tmp, custom) = staged();
        std::fs::remove_dir_all(custom.join("hud")).unwrap();
        let err = swap_in(&custom, "hud", &mut std_rename).unwrap_err();
        let ReinstallError::Unchanged { path, .. } = &err else {
            panic!("{err:?}");
        };
        assert_eq!(path, &custom.join("hud"));
    }

    #[test]
    fn a_marker_naming_a_drive_or_device_only_clears_staging() {
        for name in ["C:x", "con", "NUL.txt", "Lpt1", "com9.res"] {
            let tmp = tempfile::tempdir().unwrap();
            let custom = tmp.path().join("custom");
            let staging = custom.join(STAGING_DIR);
            // Windows cannot create these names, but the marker alone must still be refused.
            #[cfg(not(windows))]
            std::fs::create_dir_all(staging.join("old").join(name)).unwrap();
            #[cfg(windows)]
            std::fs::create_dir_all(staging.join("old")).unwrap();
            std::fs::write(staging.join(MARKER), name).unwrap();
            assert_eq!(
                recover_in(&custom, &mut std_rename).unwrap(),
                Recovery::Cleared,
                "{name}"
            );
            // On Windows these names resolve to a drive or a device, never a folder here.
            #[cfg(not(windows))]
            assert!(!custom.join(name).exists(), "{name}");
            assert!(!staging.exists(), "{name}");
        }
    }

    #[test]
    fn outer_dots_and_spaces_are_refused_only_where_windows_trims_them() {
        for name in ["hud.", "hud ", " hud"] {
            let trimmed_away = name.ends_with(['.', ' ']);
            assert_eq!(
                is_restorable_name(name),
                !(cfg!(windows) && trimmed_away),
                "{name:?}"
            );
        }
        assert!(is_restorable_name("rayshud"));
    }
}
