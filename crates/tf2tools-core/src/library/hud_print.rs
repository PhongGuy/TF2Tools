//! Whether a HUD folder in `tf/custom` is still what TF2Tools can put back. An install is
//! recognised by a fingerprint of file names, sizes and modified times, one stat per file,
//! so the check stays cheap on every live update; only a folder with no install record is
//! compared with its library copy byte by byte.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::hud_copy::{mtime_ns, sha256_file, sha256_hex};
use super::packed::ItemSource;
use super::{IoFailure, METADATA_DIR, hud_lowercase, io_at, tree_copy};

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

/// Whether the folder at `installed` holds exactly the library HUD's files with the same bytes.
/// The packed or loose source compares lists and sizes first, so a visible difference costs
/// no reading.
pub(super) fn same_files(installed: &Path, src: &ItemSource) -> Result<bool, IoFailure> {
    src.same_as(installed, &is_generated)
}

/// Whether the install at `installed` is the library HUD `src` as a lowercase install
/// under `folder` would have written it: the same surviving files, renamed and rewritten.
pub(super) fn same_lowercased(
    installed: &Path,
    src: &ItemSource,
    folder: &str,
) -> Result<bool, IoFailure> {
    let entries = src.entries(&is_generated)?;
    let rels: Vec<_> = entries.iter().map(|(rel, _)| rel.clone()).collect();
    let plan = hud_lowercase::plan(&rels);
    let own = hud_lowercase::Own::new(folder, &plan);
    let mut expected: Vec<_> = entries
        .iter()
        .filter(|(_, is_dir)| !is_dir)
        .filter_map(|(rel, _)| Some((plan.get(rel)?.clone(), rel.clone())))
        .collect();
    expected.sort();
    let files = tree_copy::tree_files(installed, &is_generated)?;
    if files.len() != expected.len() || files.iter().zip(&expected).any(|(a, (b, _))| a != b) {
        return Ok(false);
    }
    // Lowercasing keeps every length, so sizes still rule out visible differences cheaply.
    let sizes: HashMap<PathBuf, u64> = src.files(&is_generated)?.into_iter().collect();
    for (lower, rel) in &expected {
        let have = installed.join(lower);
        let len = std::fs::symlink_metadata(&have)
            .map_err(io_at(&have))?
            .len();
        if sizes.get(rel) != Some(&len) {
            return Ok(false);
        }
    }
    for (lower, rel) in &expected {
        let have = installed.join(lower);
        let same = if hud_lowercase::is_text_file(lower) {
            std::fs::read(&have).map_err(io_at(&have))?
                == hud_lowercase::lowercase_bytes(lower, &src.read(rel)?, &own)
        } else {
            sha256_file(&have)? == src.sha256(rel)?
        };
        if !same {
            return Ok(false);
        }
    }
    Ok(true)
}
