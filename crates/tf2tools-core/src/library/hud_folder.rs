//! Which `tf/custom` folder a library HUD expects: HUDs that hard-code
//! `../../custom/<name>/` paths only work when installed under that name.

use std::collections::BTreeMap;
use std::path::Path;

use walkdir::WalkDir;

use super::{IoFailure, METADATA_DIR, io_at, walk_failure};
use crate::hud_maker::check_hud_path;

/// Text files the game reads, the only ones whose references count.
const TEXT_EXTENSIONS: [&str; 4] = [".res", ".vdf", ".cfg", ".txt"];

/// Larger text files are skipped: they are item or paint kit tables, never `#base` lines,
/// and the default HUD ships two of about 24 MB.
const MAX_SCANNED_BYTES: u64 = 4 * 1024 * 1024;

/// The longest folder name taken from a HUD's files or repo, kept well inside path limits
/// so the install can always create it.
const MAX_FOLDER_BYTES: usize = 64;

/// Folder names that are game directories inside a HUD, never a HUD's own folder.
const RESERVED: [&str; 11] = [
    "resource",
    "scripts",
    "materials",
    "sound",
    "cfg",
    "models",
    "particles",
    "maps",
    "media",
    "custom",
    "tf",
];

/// The `custom/<name>/` folder most often referenced by the HUD's text files under
/// `hud_dir` (ties go to the byte-wise smallest name), or `None` when nothing qualifies.
/// The name comes from untrusted files, so it is always a single safe path segment.
pub(super) fn referenced_folder(hud_dir: &Path) -> Result<Option<String>, IoFailure> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let walk = WalkDir::new(hud_dir)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| e.file_name() != METADATA_DIR);
    for entry in walk {
        let entry = entry.map_err(|e| walk_failure(hud_dir, e))?;
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !entry.file_type().is_file() || !TEXT_EXTENSIONS.iter().any(|ext| name.ends_with(ext)) {
            continue;
        }
        let len = entry
            .metadata()
            .map_err(|e| walk_failure(hud_dir, e))?
            .len();
        if len > MAX_SCANNED_BYTES {
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(io_at(entry.path()))?;
        for found in references(&String::from_utf8_lossy(&bytes)) {
            *counts.entry(found.to_owned()).or_default() += 1;
        }
    }
    // `max_by_key` keeps the last maximum, so walk in reverse to keep the smallest name.
    Ok(counts
        .into_iter()
        .rev()
        .max_by_key(|(_, count)| *count)
        .map(|(name, _)| name))
}

/// Every safe folder name in a `custom/<name>/` that begins a path in `text`.
fn references(text: &str) -> impl Iterator<Item = &str> {
    // ASCII lowercasing keeps byte offsets, so matches in `lower` slice `text` directly.
    let lower = text.to_ascii_lowercase();
    let starts: Vec<usize> = lower
        .match_indices("custom/")
        .filter(|(at, _)| starts_a_reference(&lower[..*at]))
        .map(|(at, m)| at + m.len())
        .collect();
    starts.into_iter().filter_map(move |start| {
        let rest = &text[start..];
        let len = rest.bytes().take_while(|b| is_name_byte(*b)).count();
        (rest.as_bytes().get(len) == Some(&b'/'))
            .then(|| &rest[..len])
            .filter(|name| is_safe_folder(name))
    })
}

/// Whether `custom/` after `before` begins a path: at the start, after a non-path byte,
/// or right after `../` or `tf/`.
fn starts_a_reference(before: &str) -> bool {
    match before.as_bytes().last() {
        None => true,
        Some(b) if !is_name_byte(*b) && *b != b'/' => true,
        Some(_) => before.ends_with("../") || before.ends_with("tf/"),
    }
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-')
}

/// A name that can only ever be one plain folder under `tf/custom`: at most 64 bytes, no
/// hidden or reserved name, and nothing the HUD Maker's path rule refuses (trailing dots,
/// device names).
pub(crate) fn is_safe_folder(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_FOLDER_BYTES
        && !name.starts_with('.')
        && !RESERVED.contains(&name.to_ascii_lowercase().as_str())
        && check_hud_path(name).is_some()
}

/// The folder a curated HUD's GitHub repo URL names: its last segment after trimming a
/// trailing `/` then `.git`, or `None` for another host or a name that is not one safe
/// plain folder name.
pub(super) fn repo_folder(repo: &str) -> Option<String> {
    let path = repo
        .strip_prefix("https://")
        .or_else(|| repo.strip_prefix("http://"))?;
    let path = path.strip_prefix("www.").unwrap_or(path);
    path.strip_prefix("github.com/")?;
    let trimmed = repo.strip_suffix('/').unwrap_or(repo);
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let name = trimmed.rsplit('/').next()?;
    (name.bytes().all(is_name_byte) && is_safe_folder(name)).then(|| name.to_owned())
}

/// `folder` without the trailing `-<7 to 40 hex digits>` a GitHub archive adds, or `None`
/// when there is no such suffix or the rest is not a safe folder name.
pub(super) fn strip_commit_hash(folder: &str) -> Option<String> {
    let (rest, hash) = folder.rsplit_once('-')?;
    let is_hash = (7..=40).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit());
    (is_hash && is_safe_folder(rest)).then(|| rest.to_owned())
}
