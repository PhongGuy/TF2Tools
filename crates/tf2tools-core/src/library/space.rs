//! How much room the library takes, on disk and with its packed items counted unpacked.

use std::path::Path;

use walkdir::WalkDir;

use super::packed::{GB_ZIP, HUD_ZIP, ItemSource};
use super::{Library, walk_failure};
use crate::error::LibraryError;

/// What the library takes: file lengths on disk, and the same with every packed item counted
/// at its files' sizes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LibrarySpace {
    on_disk: u64,
    unpacked: u64,
}

impl LibrarySpace {
    /// Bytes the library's files take, by length.
    pub fn on_disk(&self) -> u64 {
        self.on_disk
    }

    /// `on_disk` with each item zip replaced by its entries' sizes.
    pub fn unpacked(&self) -> u64 {
        self.unpacked
    }
}

/// The kinds whose items may be packed, and where each keeps its zip under the item folder.
const ITEM_ZIPS: [(&str, &str); 2] = [("huds", HUD_ZIP), ("gb-addons", GB_ZIP)];

impl Library {
    /// Sums `<root>/library` under the copies read guard, so no conversion swaps an item
    /// mid-count. Links are not followed; a missing `library/` is empty.
    ///
    /// # Errors
    /// `LibraryError::Io` when a folder cannot be walked.
    pub fn space(&self) -> Result<LibrarySpace, LibraryError> {
        let _copies = self.copies_read();
        let root = self.dir.root().join("library");
        let mut space = LibrarySpace::default();
        if !root.is_dir() {
            return Ok(space);
        }
        for entry in WalkDir::new(&root).min_depth(1) {
            let entry = entry.map_err(|e| walk_failure(&root, e))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let len = entry.metadata().map_err(|e| walk_failure(&root, e))?.len();
            let rel = entry.path().strip_prefix(&root).unwrap_or(entry.path());
            space.on_disk = space.on_disk.saturating_add(len);
            space.unpacked = space
                .unpacked
                .saturating_add(item_zip_size(&root, rel).unwrap_or(len));
        }
        Ok(space)
    }
}

/// The unpacked size of the item zip at `rel`, or `None` when `rel` is not an item zip or
/// does not open as one, so the caller counts its length instead.
fn item_zip_size(root: &Path, rel: &Path) -> Option<u64> {
    let (kind, zip_rel) = ITEM_ZIPS
        .into_iter()
        .find(|(kind, zip_rel)| is_item_zip(rel, kind, zip_rel))?;
    let item = rel.strip_prefix(kind).ok()?.components().next()?;
    let source = ItemSource::open(&root.join(kind).join(item), zip_rel).ok()?;
    if source.is_packed() {
        source.unpacked_bytes().ok()
    } else {
        None
    }
}

/// Whether `rel` is `<kind>/<item>/<zip_rel>`.
fn is_item_zip(rel: &Path, kind: &str, zip_rel: &str) -> bool {
    let Ok(rest) = rel.strip_prefix(kind) else {
        return false;
    };
    let mut parts = rest.components();
    parts.next().is_some() && parts.as_path() == Path::new(zip_rel)
}
