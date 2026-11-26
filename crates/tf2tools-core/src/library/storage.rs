//! Everything TF2Tools keeps on disk, split into the kinds the Settings page lists.

use std::path::{Component, Path};

use super::space::walk_files;
use super::{IoFailure, Library};
use crate::cleanup::{CleanupRoots, is_staged_appimage, is_staged_installer};
use crate::error::StorageError;
use crate::logstf::logs_cache_bytes;

/// One kind of data TF2Tools keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StorageKind {
    /// HUD copies under `library/huds`.
    Huds,
    /// Sounds under `library/sounds`.
    Sounds,
    /// Crosshairs under `library/crosshairs`.
    Crosshairs,
    /// mastercomfig add-ons and GameBanana add-on copies.
    Addons,
    /// The logs.tf tables in the database.
    LogsTf,
    /// The rest of the database: store listings, settings and history.
    StoreData,
    /// Store images and game weapon icons.
    ImageCache,
    /// Demos downloaded to watch from demos.tf and logs.tf.
    DemoCache,
    /// Install scratch and downloaded updates waiting to install.
    Temporary,
}

impl StorageKind {
    /// Every kind, in the order the Settings page lists them.
    pub const ALL: [StorageKind; 9] = [
        StorageKind::Huds,
        StorageKind::Sounds,
        StorageKind::Crosshairs,
        StorageKind::Addons,
        StorageKind::LogsTf,
        StorageKind::StoreData,
        StorageKind::ImageCache,
        StorageKind::DemoCache,
        StorageKind::Temporary,
    ];

    fn index(self) -> usize {
        match self {
            StorageKind::Huds => 0,
            StorageKind::Sounds => 1,
            StorageKind::Crosshairs => 2,
            StorageKind::Addons => 3,
            StorageKind::LogsTf => 4,
            StorageKind::StoreData => 5,
            StorageKind::ImageCache => 6,
            StorageKind::DemoCache => 7,
            StorageKind::Temporary => 8,
        }
    }
}

/// What each kind takes: bytes on disk, and the same with packed items counted unpacked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct StorageUse {
    bytes: [u64; 9],
    unpacked: [u64; 9],
}

impl StorageUse {
    /// Bytes `kind` takes on disk.
    pub fn bytes(&self, kind: StorageKind) -> u64 {
        self.bytes[kind.index()]
    }

    /// `bytes` with each packed item counted at its files' sizes; equal for unpacked kinds.
    pub fn unpacked(&self, kind: StorageKind) -> u64 {
        self.unpacked[kind.index()]
    }

    /// Bytes every kind takes together.
    pub fn total(&self) -> u64 {
        self.bytes.iter().fold(0, |sum, b| sum.saturating_add(*b))
    }

    fn add(&mut self, kind: StorageKind, bytes: u64, unpacked: u64) {
        let i = kind.index();
        self.bytes[i] = self.bytes[i].saturating_add(bytes);
        self.unpacked[i] = self.unpacked[i].saturating_add(unpacked);
    }
}

impl Library {
    /// Measures every kind TF2Tools keeps. Blocking: it walks the library, image and demo caches
    /// and `<data>/tmp`, looks for staged updates, and reads the database sizes in one read
    /// transaction.
    ///
    /// # Errors
    /// `StorageError::Io` when a data folder or database file cannot be read, and
    /// `StorageError::Database` when SQLite cannot measure the logs.tf tables. An unreadable
    /// system temp or AppImage folder is left out, not an error.
    pub fn storage(&self) -> Result<StorageUse, StorageError> {
        let mut use_ = StorageUse::default();
        // The copies guard covers only the library walk, so no conversion swaps an item
        // mid-count, and is dropped before the database read.
        {
            let _copies = self.copies_read();
            walk_files(
                &self.dir.root().join("library"),
                true,
                |rel, len, unpacked| {
                    if let Some(kind) = library_kind(rel) {
                        use_.add(kind, len, unpacked);
                    }
                },
            )
            .map_err(io_error)?;
        }
        for root in [self.dir.image_cache_dir(), self.dir.weapon_icon_dir()] {
            walk_files(&root, false, |_, len, _| {
                use_.add(StorageKind::ImageCache, len, len);
            })
            .map_err(io_error)?;
        }
        // Partial downloads count too: they take disk space until they finish or are swept.
        walk_files(&self.dir.demo_cache_dir(), false, |_, len, _| {
            use_.add(StorageKind::DemoCache, len, len);
        })
        .map_err(io_error)?;
        walk_files(&self.dir.tmp_dir(), false, |_, len, _| {
            use_.add(StorageKind::Temporary, len, len);
        })
        .map_err(io_error)?;
        let staged = staged_update_bytes(&CleanupRoots::for_data_dir(&self.dir));
        use_.add(StorageKind::Temporary, staged, staged);
        // Read the file lengths inside the same transaction as the logs.tf pages, which narrows
        // how far a commit between the two can skew the store data share.
        let db_path = self.dir.db_path();
        let (logs, files) = self
            .db
            .read(|conn| {
                let tx = conn.unchecked_transaction()?;
                let logs = logs_cache_bytes(&tx)?;
                Ok((logs, database_files(&db_path)))
            })
            .map_err(|e| StorageError::Database {
                message: e.to_string(),
            })?;
        let files = files.map_err(io_error)?;
        use_.add(StorageKind::LogsTf, logs, logs);
        let store = files.saturating_sub(logs);
        use_.add(StorageKind::StoreData, store, store);
        Ok(use_)
    }
}

/// The kind a path under `library/` belongs to, by its first folder; `None` is not counted.
fn library_kind(rel: &Path) -> Option<StorageKind> {
    let Some(Component::Normal(first)) = rel.components().next() else {
        return None;
    };
    match first.to_str()? {
        "huds" => Some(StorageKind::Huds),
        "sounds" => Some(StorageKind::Sounds),
        "crosshairs" => Some(StorageKind::Crosshairs),
        "addons" | "gb-addons" => Some(StorageKind::Addons),
        _ => None,
    }
}

/// The lengths of the database file and its `-wal` and `-shm` beside it; a missing one is 0.
fn database_files(db: &Path) -> Result<u64, IoFailure> {
    let mut total = 0_u64;
    for suffix in ["", "-wal", "-shm"] {
        let mut name = db.as_os_str().to_owned();
        name.push(suffix);
        let path = Path::new(&name);
        match std::fs::metadata(path) {
            Ok(meta) => total = total.saturating_add(meta.len()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(IoFailure {
                    path: path.to_path_buf(),
                    source,
                });
            }
        }
    }
    Ok(total)
}

/// Bytes of staged update installers in the temp folder and staged AppImages beside the
/// running AppImage. Never fails: an unreadable folder or entry counts nothing.
fn staged_update_bytes(roots: &CleanupRoots) -> u64 {
    let beside = roots
        .appimage()
        .and_then(Path::parent)
        .map_or(0, |dir| staged_in(dir, is_staged_appimage));
    staged_in(roots.temp(), is_staged_installer).saturating_add(beside)
}

/// The summed lengths of the regular files directly in `dir` whose names `ours` accepts.
/// Links and folders are never followed or counted.
fn staged_in(dir: &Path, ours: fn(&str) -> bool) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(ours))
        .filter_map(|entry| std::fs::symlink_metadata(entry.path()).ok())
        .filter(|meta| meta.file_type().is_file())
        .fold(0, |sum, meta| sum.saturating_add(meta.len()))
}

fn io_error(IoFailure { path, source }: IoFailure) -> StorageError {
    StorageError::Io { path, source }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, len: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; len]).unwrap();
    }

    #[test]
    fn both_staged_appimage_names_count_beside_the_appimage() {
        let tmp = tempfile::tempdir().unwrap();
        let apps = tmp.path().join("apps");
        let temp = tmp.path().join("temp");
        std::fs::create_dir_all(&temp).unwrap();
        write(&apps.join("TF2Tools.AppImage"), 9000);
        write(&apps.join(".tf2tools-update-6.1.0"), 100);
        write(&apps.join(".tf2tools-update-6.2.0.part"), 20);
        write(&apps.join(".tf2tools-update-nope"), 5000);
        write(&apps.join("TF2Tools.AppImage.bak"), 5000);
        write(&temp.join("tf2tools-update-6.1.0-setup.exe"), 3);
        let roots = CleanupRoots::new(tmp.path(), tmp.path(), tmp.path(), &temp)
            .with_appimage(apps.join("TF2Tools.AppImage"));
        assert_eq!(staged_update_bytes(&roots), 123);
    }

    #[test]
    fn missing_folders_count_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let gone = tmp.path().join("gone");
        let roots = CleanupRoots::new(&gone, &gone, &gone, &gone)
            .with_appimage(gone.join("TF2Tools.AppImage"));
        assert_eq!(staged_update_bytes(&roots), 0);
    }

    // Folder permissions that refuse a listing are a Unix feature.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_temp_folder_is_left_out() {
        use std::os::unix::fs::PermissionsExt as _;
        struct Unlock(std::path::PathBuf);
        impl Drop for Unlock {
            fn drop(&mut self) {
                let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        let temp = tmp.path().join("temp");
        let apps = tmp.path().join("apps");
        write(&temp.join("tf2tools-update-6.1.0-setup.exe"), 50);
        write(&apps.join(".tf2tools-update-6.1.0"), 7);
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o000)).unwrap();
        let _unlock = Unlock(temp.clone());
        if std::fs::read_dir(&temp).is_ok() {
            eprintln!("skipped: this user can list a folder with no permissions");
            return;
        }
        let roots = CleanupRoots::new(tmp.path(), tmp.path(), tmp.path(), &temp)
            .with_appimage(apps.join("TF2Tools.AppImage"));
        assert_eq!(staged_update_bytes(&roots), 7);
    }
}
