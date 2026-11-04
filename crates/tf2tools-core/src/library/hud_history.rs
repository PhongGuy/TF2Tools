//! A made HUD's history: named snapshots of its changes, one file each under
//! `<hud folder>/.tf2tools/history/`, named by a fresh uuid so a player's name never becomes a
//! path. Every write goes through here, inside the HUD's own folder.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use super::hud_presets::read_capped;
use super::made_huds::{HudAssets, asset_source, read_kept_copy, write_kept_copy};
use super::{HudId, IoFailure, Library, METADATA_DIR, io_at};
use crate::error::{HistoryError, MakeHudError};
use crate::hud_maker::{Change, ChangeTarget, IMAGES_DIR, SnapshotFile};
use crate::paths::LibraryKind;

/// The most snapshots one HUD keeps; saving more is refused until the player deletes one.
pub const MAX_SNAPSHOTS: usize = 50;

/// Directory entries a listing looks at; the rest are skipped.
const MAX_LISTED: usize = 1000;
/// The snapshots folder inside `.tf2tools/`.
const HISTORY_DIR: &str = "history";

/// One saved snapshot as a list row: its file, name, time and change count.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SnapshotEntry {
    file: String,
    name: String,
    at: String,
    changes: usize,
}

impl SnapshotEntry {
    /// The file name inside the HUD's `history/` folder.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The cleaned name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// When the snapshot was taken, as UTC in the library's fixed-width RFC 3339 format.
    pub fn at(&self) -> &str {
        &self.at
    }

    /// How many changes it holds.
    pub fn changes(&self) -> usize {
        self.changes
    }
}

/// A snapshot read back whole: its changes, the kept copy of each font they add, and how many
/// of the file's changes could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HudSnapshot {
    name: String,
    at: String,
    changes: Vec<Change>,
    assets: HudAssets,
    skipped: usize,
}

impl HudSnapshot {
    /// The cleaned name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// When the snapshot was taken, as UTC in the library's fixed-width RFC 3339 format.
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The changes in the order they were made.
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// The kept copy of each added font the changes name; a font whose copy is gone has none.
    pub fn assets(&self) -> &HudAssets {
        &self.assets
    }

    /// How many of the file's changes could not be read and were left out.
    pub fn skipped(&self) -> usize {
        self.skipped
    }
}

impl Library {
    /// Saves `changes` as a snapshot of the made HUD `id`, named `name` (cleaned).
    ///
    /// # Errors
    /// `NameBlank`, `TooLarge` (over 10 000 changes or 32 MiB), `NotMade` (no such row, or no
    /// `maker.json`), `Full` at 50 snapshots, `Io` (including a `.tf2tools` or `history` that
    /// is a link or not a folder) and `Database`. Nothing is written on an error.
    pub fn save_hud_snapshot(
        &self,
        id: &HudId,
        name: &str,
        changes: &[Change],
    ) -> Result<SnapshotEntry, HistoryError> {
        self.save_hud_snapshot_with_assets(id, name, changes, &HudAssets::new())
    }

    /// As `save_hud_snapshot`, first writing into the HUD's folder each picked picture copy
    /// that `changes` name, `assets` holds and the folder lacks, so the snapshot can restore it.
    ///
    /// # Errors
    /// As `save_hud_snapshot`; `Io` when a picture copy cannot be written (a link on the way,
    /// over the picture cap, or a failed write), and then no snapshot is written.
    pub fn save_hud_snapshot_with_assets(
        &self,
        id: &HudId,
        name: &str,
        changes: &[Change],
        assets: &HudAssets,
    ) -> Result<SnapshotEntry, HistoryError> {
        let name = SnapshotFile::clean_name(name).ok_or(HistoryError::NameBlank)?;
        if changes.len() > SnapshotFile::MAX_CHANGES {
            return Err(HistoryError::TooLarge);
        }
        let folder = self.writable_made_folder(id)?;
        let dir = checked_history_dir(&folder)?;
        if listed(&dir).len() >= MAX_SNAPSHOTS {
            return Err(HistoryError::Full {
                limit: MAX_SNAPSHOTS,
            });
        }
        let file = SnapshotFile {
            name,
            at: super::now(),
            changes: changes.to_vec(),
        };
        let bytes = file.to_json();
        if bytes.len() > SnapshotFile::MAX_BYTES {
            return Err(HistoryError::TooLarge);
        }

        // Picture copies are named by content, so writing one that is missing never replaces
        // another; they land before the snapshot that names them.
        for change in changes {
            let ChangeTarget::Asset(asset) = change.target() else {
                continue;
            };
            let Some(source) = asset_source(asset) else {
                continue;
            };
            if !source.starts_with(&format!("{IMAGES_DIR}/")) {
                continue;
            }
            if let Some(bytes) = assets.get(&source) {
                write_kept_copy(&folder, &source, bytes)?;
            }
        }

        // Written beside its final name and renamed in, so a snapshot is never seen half written.
        std::fs::create_dir_all(&dir).map_err(io_at(&dir))?;
        let name = format!("{}.json", uuid::Uuid::new_v4());
        let path = dir.join(&name);
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, bytes).map_err(io_at(&tmp))?;
        if let Err(source) = std::fs::rename(&tmp, &path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(HistoryError::Io { path, source });
        }
        Ok(SnapshotEntry {
            file: name,
            name: file.name,
            at: file.at,
            changes: file.changes.len(),
        })
    }

    /// The snapshots of the made HUD `id`, newest first and then by file; empty without a
    /// history folder. Never writes.
    ///
    /// # Errors
    /// `NotMade` for an unknown id or a HUD with no `maker.json`, `Database`.
    pub fn list_hud_snapshots(&self, id: &HudId) -> Result<Vec<SnapshotEntry>, HistoryError> {
        let folder = self.readable_made_folder(id)?;
        let mut entries: Vec<SnapshotEntry> = listed(&checked_history_dir(&folder)?);
        entries.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.file.cmp(&b.file)));
        Ok(entries)
    }

    /// The snapshot in `file`, a plain file name inside the HUD's `history/` folder. Never
    /// writes.
    ///
    /// # Errors
    /// `NotMade`, `NotFound` (not a plain `.json` name, or not a regular file there; a link is
    /// never followed), `Damaged`, `Newer`, `Io`, `Database`.
    pub fn load_hud_snapshot(&self, id: &HudId, file: &str) -> Result<HudSnapshot, HistoryError> {
        let folder = self.readable_made_folder(id)?;
        let path = checked_history_dir(&folder)?.join(plain_snapshot_name(file)?);
        let bytes = read_capped(&path)
            .map_err(not_found_or_io)?
            .ok_or(HistoryError::NotFound)?;
        let (snapshot, skipped) = SnapshotFile::parse(&bytes)?;

        // A font's bytes are the HUD's kept copy; a copy that is gone leaves the change as it was.
        let mut assets = HudAssets::new();
        for change in &snapshot.changes {
            let ChangeTarget::Asset(asset) = change.target() else {
                continue;
            };
            let Some(source) = asset_source(asset) else {
                continue;
            };
            if assets.get(&source).is_none()
                && let Some(bytes) = read_kept_copy(&folder, &source)
            {
                assets.insert(&source, bytes);
            }
        }
        Ok(HudSnapshot {
            name: snapshot.name,
            at: snapshot.at,
            changes: snapshot.changes,
            assets,
            skipped,
        })
    }

    /// Deletes the snapshot in `file`, a plain file name inside the HUD's `history/` folder.
    ///
    /// # Errors
    /// `NotMade`, `NotFound` (not a plain `.json` name, missing, or neither a file nor a link;
    /// a link is removed, not followed), `Io`, `Database`.
    pub fn delete_hud_snapshot(&self, id: &HudId, file: &str) -> Result<(), HistoryError> {
        let path = self
            .writable_history_dir(id)?
            .join(plain_snapshot_name(file)?);
        let meta =
            std::fs::symlink_metadata(&path).map_err(|e| not_found_or_io(io_at(&path)(e)))?;
        if !meta.is_file() && !meta.file_type().is_symlink() {
            return Err(HistoryError::NotFound);
        }
        std::fs::remove_file(&path).map_err(|e| not_found_or_io(io_at(&path)(e)))
    }

    /// The HUD folder to read from, refusing an unknown id or a HUD with no `maker.json`.
    /// A re-save caught between its renames reads from the previous save; readers never write.
    fn readable_made_folder(&self, id: &HudId) -> Result<PathBuf, HistoryError> {
        let not_made = || HistoryError::NotMade {
            id: id.as_str().to_owned(),
        };
        let name = self
            .hud_folder(id)
            .map_err(|e| HistoryError::Database {
                message: e.to_string(),
            })?
            .ok_or_else(not_made)?;
        let live = self.dir.library_dir(LibraryKind::Huds).join(name);
        let folder = super::made_huds::readable_folder(live);
        if !folder.join(METADATA_DIR).join("maker.json").is_file() {
            return Err(not_made());
        }
        Ok(folder)
    }

    /// The history folder of the made HUD `id` for a write, after finishing any interrupted
    /// re-save. The `.tf2tools` and `history` steps must each be a folder or missing.
    fn writable_history_dir(&self, id: &HudId) -> Result<PathBuf, HistoryError> {
        checked_history_dir(&self.writable_made_folder(id)?)
    }

    /// The live folder of the made HUD `id` for a write, after finishing any interrupted
    /// re-save.
    fn writable_made_folder(&self, id: &HudId) -> Result<PathBuf, HistoryError> {
        let name = self.made_folder(id).map_err(|e| match e {
            MakeHudError::Io { path, source } => HistoryError::Io { path, source },
            MakeHudError::Database { message } => HistoryError::Database { message },
            _ => HistoryError::NotMade {
                id: id.as_str().to_owned(),
            },
        })?;
        Ok(self.dir.library_dir(LibraryKind::Huds).join(name))
    }
}

/// Every asset source named by a snapshot in the made HUD folder `folder`, or `None` when the
/// history cannot be read whole: a folder that cannot be listed, or a snapshot that cannot be
/// read or does not parse. An absent history names nothing.
pub(super) fn snapshot_sources(folder: &Path) -> Option<BTreeSet<String>> {
    let dir = folder.join(METADATA_DIR).join(HISTORY_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Some(BTreeSet::new()),
        Err(_) => return None,
    };
    let mut sources = BTreeSet::new();
    for entry in entries {
        let file = entry.ok()?.file_name().into_string().ok()?;
        if !is_snapshot_name(&file) {
            continue;
        }
        let bytes = read_capped(&dir.join(&file)).ok()??;
        let (snapshot, _skipped) = SnapshotFile::parse(&bytes).ok()?;
        sources.extend(snapshot.changes.iter().filter_map(|c| match c.target() {
            ChangeTarget::Asset(asset) => asset_source(asset),
            _ => None,
        }));
    }
    Some(sources)
}

/// `<folder>/.tf2tools/history`, refusing a `.tf2tools` or `history` that exists as anything
/// but a plain folder, so no write follows a link out of the HUD's folder.
fn checked_history_dir(folder: &Path) -> Result<PathBuf, HistoryError> {
    let mut path = folder.to_path_buf();
    for step in [METADATA_DIR, HISTORY_DIR] {
        path.push(step);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(HistoryError::Io {
                    path,
                    source: io::Error::other("not a plain folder"),
                });
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(HistoryError::Io { path, source: e }),
        }
    }
    Ok(path)
}

/// Every entry of `dir` that reads as a snapshot; anything else is left out rather than
/// failing the list.
fn listed(dir: &Path) -> Vec<SnapshotEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .take(MAX_LISTED)
        .filter_map(|entry| {
            let file = entry.file_name().into_string().ok()?;
            if !is_snapshot_name(&file) {
                return None;
            }
            let bytes = read_capped(&dir.join(&file)).ok()??;
            let (snapshot, _skipped) = SnapshotFile::parse(&bytes).ok()?;
            Some(SnapshotEntry {
                file,
                name: snapshot.name,
                at: snapshot.at,
                changes: snapshot.changes.len(),
            })
        })
        .collect()
}

/// `file` itself when it is a snapshot's plain name, else `NotFound`.
fn plain_snapshot_name(file: &str) -> Result<&str, HistoryError> {
    if is_snapshot_name(file) {
        Ok(file)
    } else {
        Err(HistoryError::NotFound)
    }
}

/// A missing file is `NotFound`; any other I/O failure keeps its path.
fn not_found_or_io(failure: IoFailure) -> HistoryError {
    match HistoryError::from(failure) {
        HistoryError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound => {
            HistoryError::NotFound
        }
        other => other,
    }
}

/// A single plain component ending `.json`, so no name can reach outside the folder.
fn is_snapshot_name(file: &str) -> bool {
    let mut parts = Path::new(file).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(std::path::Component::Normal(_)), None)
    ) && file.ends_with(".json")
}
