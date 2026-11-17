//! Everything TF2Tools keeps on disk, split into the kinds the Settings page lists.

use std::path::{Component, Path};

use super::space::walk_files;
use super::{IoFailure, Library};
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
}

impl StorageKind {
    /// Every kind, in the order the Settings page lists them.
    pub const ALL: [StorageKind; 7] = [
        StorageKind::Huds,
        StorageKind::Sounds,
        StorageKind::Crosshairs,
        StorageKind::Addons,
        StorageKind::LogsTf,
        StorageKind::StoreData,
        StorageKind::ImageCache,
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
        }
    }
}

/// What each kind takes: bytes on disk, and the same with packed items counted unpacked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct StorageUse {
    bytes: [u64; 7],
    unpacked: [u64; 7],
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
    /// Measures every kind TF2Tools keeps. Blocking: it walks the library and image caches
    /// and reads the database sizes in one read transaction.
    ///
    /// # Errors
    /// `StorageError::Io` when a folder or database file cannot be read, and
    /// `StorageError::Database` when SQLite cannot measure the logs.tf tables.
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

fn io_error(IoFailure { path, source }: IoFailure) -> StorageError {
    StorageError::Io { path, source }
}
