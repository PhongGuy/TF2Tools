//! Whether a HUD folder in `tf/custom` is still what TF2Tools can put back. An install is
//! recognised by a fingerprint of file names, sizes and modified times, one stat per file,
//! so the check stays cheap on every live update; only a folder with no install record is
//! compared with its library copy byte by byte.

use std::path::Path;

use super::hud_copy::{mtime_ns, sha256_file, sha256_hex};
use super::{IoFailure, METADATA_DIR, io_at, tree_copy};

/// What the fingerprint and comparison ignore: the app's metadata and the sound cache the
/// game writes into a custom folder on its own.
fn is_generated(rel: &Path) -> bool {
    rel.file_name()
        .is_some_and(|name| name == METADATA_DIR || name.eq_ignore_ascii_case("sound.cache"))
}

/// The fingerprint of the folder at `dir`. Install writes every file anew, so any later edit
/// or replacement changes a modified time even when the size stays.
pub(super) fn fingerprint(dir: &Path) -> Result<String, IoFailure> {
    let mut lines = String::new();
    for rel in tree_copy::tree_files(dir, &is_generated)? {
        let path = dir.join(&rel);
        let meta = std::fs::symlink_metadata(&path).map_err(io_at(&path))?;
        lines.push_str(&format!(
            "{}\0{}\0{}\n",
            rel.display(),
            meta.len(),
            mtime_ns(&meta)
        ));
    }
    Ok(sha256_hex(lines.as_bytes()))
}

/// Whether two folders hold the same files with the same bytes. Sizes are compared first, so
/// folders that differ visibly cost no reading.
pub(super) fn same_files(a: &Path, b: &Path) -> Result<bool, IoFailure> {
    let files = tree_copy::tree_files(a, &is_generated)?;
    if files != tree_copy::tree_files(b, &is_generated)? {
        return Ok(false);
    }
    let len = |path: &Path| {
        std::fs::symlink_metadata(path)
            .map(|meta| meta.len())
            .map_err(io_at(path))
    };
    for rel in &files {
        if len(&a.join(rel))? != len(&b.join(rel))? {
            return Ok(false);
        }
    }
    for rel in &files {
        if sha256_file(&a.join(rel))? != sha256_file(&b.join(rel))? {
            return Ok(false);
        }
    }
    Ok(true)
}
