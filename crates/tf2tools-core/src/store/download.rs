//! Downloading a store file into a scratch dir under `<data>/tmp` and importing it into
//! the library; ported from v5's `services/download.rs` and the store download commands.

use std::path::{Component, Path, PathBuf};

use md5::{Digest, Md5};
use rusqlite::OptionalExtension;
use tokio::io::AsyncWriteExt;

use super::http::NetFailure;
use super::unpack::{crosshair_files, entry_name, sound_files};
use super::{StoreClient, StoreFile, StoreItemId, StoreKind};
use crate::db::Database;
use crate::error::{DownloadError, GbAddonFetchError, LibraryError};
use crate::library::{AddonName, GbId, Library, LibraryEntry, Provenance, is_plain_name};

/// What a running download is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DownloadPhase {
    /// Bytes are arriving.
    Downloading,
    /// The file has arrived and is being imported into the library.
    Importing,
}

/// A running download's progress, reported after every chunk and once before the import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DownloadProgress {
    phase: DownloadPhase,
    downloaded: u64,
    total: Option<u64>,
}

impl DownloadProgress {
    /// What the download is doing.
    pub fn phase(self) -> DownloadPhase {
        self.phase
    }

    /// Bytes received so far.
    pub fn downloaded(self) -> u64 {
        self.downloaded
    }

    /// The size the server announced, if it did.
    pub fn total(self) -> Option<u64> {
        self.total
    }
}

/// The item a download imports as, settled before any request is made.
enum Target {
    GameBananaHud(GbId),
    CuratedHud(String),
    Addon(AddonName, String),
    Sound(GbId),
    Crosshair(GbId),
}

impl StoreClient {
    /// Downloads one file into `<data>/tmp`, one download at a time, enforcing the kind's size
    /// cap while streaming and the md5 when the store gives one, then imports it through the
    /// library. HUDs give one entry, sounds and crosshairs one per usable file inside; a
    /// failed entry removes the ones imported before it.
    /// The scratch dir is removed on every return and when the future is dropped.
    pub async fn download(
        &self,
        kind: StoreKind,
        file: &StoreFile,
        progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    ) -> Result<Vec<LibraryEntry>, DownloadError> {
        let target = target(kind, file).ok_or_else(|| DownloadError::WrongKind {
            file_name: file.file_name().to_owned(),
        })?;
        let _turn = self.download_lock.lock().await;
        if let StoreItemId::GameBanana(id) = file.item() {
            self.refuse_duplicate(kind, *id)?;
        }

        let tmp = self.dir.tmp_dir();
        tokio::fs::create_dir_all(&tmp).await.map_err(io_at(&tmp))?;
        let scratch = tempfile::Builder::new()
            .prefix("download-")
            .tempdir_in(&tmp)
            .map_err(io_at(&tmp))?;
        let saved = scratch.path().join(save_name(file.file_name()));
        let (downloaded, total) = self.fetch(kind, file, &saved, progress).await?;

        progress(DownloadProgress {
            phase: DownloadPhase::Importing,
            downloaded,
            total,
        });
        let library = Library::new(self.db.clone(), self.dir.clone());
        let db = self.db.clone();
        // The blocking import owns the scratch dir, so a dropped future cannot remove the
        // files it is still reading; the dir goes when the import ends.
        // Tokio turns a panic in the import into Interrupted, so it runs marked as handled.
        tokio::task::spawn_blocking(move || {
            crate::panic_mark::marked(|| {
                let imported = import(&library, &db, target, &saved, scratch.path());
                drop(scratch);
                imported
            })
        })
        .await
        .map_err(|e| DownloadError::Interrupted {
            message: e.to_string(),
        })?
    }

    /// AlreadyInLibrary when the kind's library table already holds this GameBanana id.
    fn refuse_duplicate(&self, kind: StoreKind, id: GbId) -> Result<(), DownloadError> {
        let table = match kind {
            StoreKind::GameBananaHuds => "library_huds",
            StoreKind::Sounds => "library_sounds",
            StoreKind::Crosshairs => "library_crosshairs",
            StoreKind::CuratedHuds | StoreKind::Addons => return Ok(()),
        };
        // An id past i64 cannot be in the database, so it cannot be a duplicate.
        let Ok(column) = i64::try_from(id.get()) else {
            return Ok(());
        };
        // The table name is one of the constants above, never caller text.
        let exists: bool = self
            .db
            .read(|conn| {
                conn.query_row(
                    &format!("SELECT COUNT(*) > 0 FROM {table} WHERE store_gb_id = ?1"),
                    [column],
                    |r| r.get(0),
                )
            })
            .map_err(database)?;
        if exists {
            return Err(DownloadError::AlreadyInLibrary {
                what: format!("GameBanana item {id}"),
            });
        }
        Ok(())
    }

    /// Streams the file to `dest` under the kind's size cap and checks its md5.
    async fn fetch(
        &self,
        kind: StoreKind,
        file: &StoreFile,
        dest: &Path,
        progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    ) -> Result<(u64, Option<u64>), DownloadError> {
        self.stream_to(
            file.url(),
            file.md5(),
            file.file_name(),
            self.config.size_cap(kind),
            dest,
            progress,
        )
        .await
        .map_err(DownloadError::from)
    }

    /// Streams `url` to `dest`, failing as soon as it passes `limit`, then checks `md5`
    /// when given (case-insensitive). Returns the bytes received and the announced size.
    pub(super) async fn stream_to(
        &self,
        url: &str,
        md5: Option<&str>,
        file_name: &str,
        limit: u64,
        dest: &Path,
        progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    ) -> Result<(u64, Option<u64>), StreamFailure> {
        let io = |source| StreamFailure::Io {
            path: dest.to_path_buf(),
            source,
        };
        let mut body = self.http.open(url).await.map_err(StreamFailure::Net)?;
        let total = body.total();
        let mut out = tokio::fs::File::create(dest).await.map_err(io)?;
        let mut hasher = Md5::new();
        let mut downloaded = 0u64;
        while let Some(chunk) = body.chunk().await.map_err(StreamFailure::Net)? {
            downloaded += chunk.len() as u64;
            if downloaded > limit {
                return Err(StreamFailure::TooLarge {
                    url: url.to_owned(),
                    limit,
                });
            }
            out.write_all(&chunk).await.map_err(io)?;
            hasher.update(&chunk);
            progress(DownloadProgress {
                phase: DownloadPhase::Downloading,
                downloaded,
                total,
            });
        }
        // Tokio finishes file writes in the background; flush before the importer reads it.
        out.flush().await.map_err(io)?;
        drop(out);

        if let Some(expected) = md5 {
            let found: String = hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            if found != expected.to_lowercase() {
                return Err(StreamFailure::ChecksumMismatch {
                    file_name: file_name.to_owned(),
                    expected: expected.to_owned(),
                    found,
                });
            }
        }
        Ok((downloaded, total))
    }
}

/// Why streaming a file to disk stopped. Each public download error takes these over
/// through its own From impl.
pub(super) enum StreamFailure {
    Net(NetFailure),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    TooLarge {
        url: String,
        limit: u64,
    },
    ChecksumMismatch {
        file_name: String,
        expected: String,
        found: String,
    },
}

/// Both public download errors carry the same four stream failures under the same names.
macro_rules! from_stream_failure {
    ($($target:ident),*) => {$(
        impl From<StreamFailure> for $target {
            fn from(failure: StreamFailure) -> Self {
                match failure {
                    StreamFailure::Net(net) => net.into(),
                    StreamFailure::Io { path, source } => Self::Io { path, source },
                    StreamFailure::TooLarge { url, limit } => Self::TooLarge { url, limit },
                    StreamFailure::ChecksumMismatch {
                        file_name,
                        expected,
                        found,
                    } => Self::ChecksumMismatch {
                        file_name,
                        expected,
                        found,
                    },
                }
            }
        }
    )*};
}

from_stream_failure!(DownloadError, GbAddonFetchError);

/// How `file` imports for `kind`, or None when the file belongs to another store.
fn target(kind: StoreKind, file: &StoreFile) -> Option<Target> {
    if file.kind() != kind {
        return None;
    }
    match (kind, file.item()) {
        (StoreKind::GameBananaHuds, StoreItemId::GameBanana(id)) => {
            Some(Target::GameBananaHud(*id))
        }
        (StoreKind::Sounds, StoreItemId::GameBanana(id)) => Some(Target::Sound(*id)),
        (StoreKind::Crosshairs, StoreItemId::GameBanana(id)) => Some(Target::Crosshair(*id)),
        (StoreKind::CuratedHuds, StoreItemId::Curated(hud_id)) => {
            Some(Target::CuratedHud(hud_id.clone()))
        }
        (StoreKind::Addons, StoreItemId::Addon(name)) => Some(Target::Addon(
            name.clone(),
            file.release_tag().unwrap_or_default().to_owned(),
        )),
        _ => None,
    }
}

/// The scratch file name: the store name's last path component when it is a single plain
/// component, else `download` plus its extension when that is alphanumeric, since the
/// importers key off the extension.
pub(super) fn save_name(store_name: &str) -> String {
    let last = store_name.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut components = Path::new(last).components();
    let single =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if single && is_plain_name(last) {
        return last.to_owned();
    }
    match Path::new(last).extension().and_then(|e| e.to_str()) {
        Some(ext) if !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric()) => {
            format!("download.{ext}")
        }
        _ => "download".to_owned(),
    }
}

/// The blocking half of a download: look up the store row and import through the library.
/// `scratch` holds `saved` and is where an archive is extracted.
fn import(
    library: &Library,
    db: &Database,
    target: Target,
    saved: &Path,
    scratch: &Path,
) -> Result<Vec<LibraryEntry>, DownloadError> {
    let entry = match target {
        Target::GameBananaHud(gb_id) => {
            let author = media_row(db, StoreKind::GameBananaHuds, gb_id)?.1;
            LibraryEntry::Hud(library.import_hud(saved, Provenance::GameBanana { gb_id, author })?)
        }
        Target::CuratedHud(hud_id) => {
            LibraryEntry::Hud(library.import_hud(saved, Provenance::Curated { hud_id })?)
        }
        Target::Addon(name, tag) => LibraryEntry::Addon(library.import_addon(saved, &name, &tag)?),
        Target::Sound(gb_id) => {
            let files = sound_files(saved, &extract_dir(scratch)?)?;
            let (name, author) = media_row(db, StoreKind::Sounds, gb_id)?;
            return import_each(
                &files,
                name.as_deref(),
                |file, name| {
                    let provenance = Provenance::GameBanana {
                        gb_id,
                        author: author.clone(),
                    };
                    Ok(LibraryEntry::Sound(
                        library.import_sound(file, name, provenance)?,
                    ))
                },
                |entry| match entry {
                    LibraryEntry::Sound(sound) => library.delete_sound(sound.id(), None),
                    LibraryEntry::Hud(_) | LibraryEntry::Crosshair(_) | LibraryEntry::Addon(_) => {
                        Ok(())
                    }
                },
            );
        }
        Target::Crosshair(gb_id) => {
            let files = crosshair_files(saved, &extract_dir(scratch)?)?;
            let (name, author) = media_row(db, StoreKind::Crosshairs, gb_id)?;
            return import_each(
                &files,
                name.as_deref(),
                |file, name| {
                    let provenance = Provenance::GameBanana {
                        gb_id,
                        author: author.clone(),
                    };
                    Ok(LibraryEntry::Crosshair(
                        library.import_crosshair(file, name, provenance)?,
                    ))
                },
                |entry| match entry {
                    LibraryEntry::Crosshair(crosshair) => library.delete_crosshair(crosshair.id()),
                    LibraryEntry::Hud(_) | LibraryEntry::Sound(_) | LibraryEntry::Addon(_) => {
                        Ok(())
                    }
                },
            );
        }
    };
    Ok(vec![entry])
}

/// Imports `files` in order under v5's names. When one fails, every entry imported before
/// it is undone newest first and the failure returned; RollbackFailed when any undo fails.
fn import_each(
    files: &[PathBuf],
    store_name: Option<&str>,
    import_one: impl Fn(&Path, &str) -> Result<LibraryEntry, DownloadError>,
    undo: impl Fn(&LibraryEntry) -> Result<(), LibraryError>,
) -> Result<Vec<LibraryEntry>, DownloadError> {
    let several = files.len() > 1;
    let mut imported = Vec::with_capacity(files.len());
    for file in files {
        match import_one(file, &entry_name(file, store_name, several)) {
            Ok(entry) => imported.push(entry),
            Err(cause) => return Err(roll_back(imported, cause, undo)),
        }
    }
    Ok(imported)
}

/// Undoes `imported` newest first so a retry starts from a clean library. Entries whose
/// undo fails stay listed, in import order, and the first such failure is the source.
fn roll_back(
    imported: Vec<LibraryEntry>,
    cause: DownloadError,
    undo: impl Fn(&LibraryEntry) -> Result<(), LibraryError>,
) -> DownloadError {
    let mut stayed = Vec::new();
    let mut first_failure = None;
    for entry in imported.into_iter().rev() {
        if let Err(e) = undo(&entry) {
            tracing::warn!(error = %e, ?entry, "could not undo a partial store import");
            first_failure.get_or_insert(e);
            stayed.push(entry);
        }
    }
    match first_failure {
        None => cause,
        Some(source) => {
            stayed.reverse();
            DownloadError::RollbackFailed {
                imported: stayed,
                cause: Box::new(cause),
                source,
            }
        }
    }
}

/// A fresh directory inside the scratch dir to extract into, named so it cannot collide
/// with the saved download.
fn extract_dir(scratch: &Path) -> Result<PathBuf, DownloadError> {
    let dir = tempfile::Builder::new()
        .prefix("extract-")
        .tempdir_in(scratch)
        .map_err(io_at(scratch))?;
    // The scratch dir's guard removes it with everything else.
    Ok(dir.keep())
}

/// The name and submitter of a GameBanana item as its `kind` store row has them; no name
/// and an empty author without a row.
fn media_row(
    db: &Database,
    kind: StoreKind,
    gb_id: GbId,
) -> Result<(Option<String>, String), DownloadError> {
    let Ok(column) = i64::try_from(gb_id.get()) else {
        return Ok((None, String::new()));
    };
    // The table name is one of the crate's own constants, never store text.
    let row: Option<(String, Option<String>)> = db
        .read(|conn| {
            conn.query_row(
                &format!("SELECT name, author FROM {} WHERE gb_id = ?1", kind.table()),
                [column],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
        })
        .map_err(database)?;
    Ok(match row {
        Some((name, author)) => (Some(name), author.unwrap_or_default()),
        None => (None, String::new()),
    })
}

fn database(e: rusqlite::Error) -> DownloadError {
    DownloadError::Database {
        message: e.to_string(),
    }
}

pub(super) fn io_at(path: &Path) -> impl FnOnce(std::io::Error) -> DownloadError + '_ {
    move |source| DownloadError::Io {
        path: PathBuf::from(path),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::save_name;

    #[test]
    fn a_store_name_keeps_only_a_plain_last_component() {
        assert_eq!(save_name("hud.zip"), "hud.zip");
        assert_eq!(save_name("../../evil.zip"), "evil.zip");
        assert_eq!(save_name("a\\b\\c.7z"), "c.7z");
        assert_eq!(save_name(".."), "download");
        assert_eq!(save_name("dir/"), "download");
        assert_eq!(save_name(""), "download");
    }
}
