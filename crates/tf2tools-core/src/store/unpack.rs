//! Picking the sound and crosshair files out of a store download, and naming the library
//! entries they become; ported from v5's `sound_store.rs` and `crosshair_store.rs`.

use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use walkdir::WalkDir;

use super::download::io_at;
use crate::archive::{VPK_MAGIC, extract_archive};
use crate::error::DownloadError;
use crate::wav::inspect_wav;

/// How many WAV refusals a NoUsableFiles reason lists before "and N more".
const SHOWN_REFUSALS: usize = 3;

/// The files a sound download imports, in path order: a bare VPK whole; else the VPKs
/// inside; else every WAV TF2 can play. `work` is an empty scratch directory to extract into.
pub(super) fn sound_files(saved: &Path, work: &Path) -> Result<Vec<PathBuf>, DownloadError> {
    if starts_with_vpk_magic(saved)? {
        return Ok(vec![saved.to_path_buf()]);
    }
    let root = extract_archive(saved, work)?;
    let files = files_under(&root)?;

    let vpks: Vec<PathBuf> = files
        .iter()
        .filter(|f| has_extension(f, "vpk"))
        .cloned()
        .collect();
    if !vpks.is_empty() {
        return Ok(vpks);
    }

    let mut wavs = Vec::new();
    let mut refusals = Vec::new();
    let mut compressed = false;
    for file in files {
        if has_extension(&file, "mp3") || has_extension(&file, "ogg") {
            compressed = true;
        } else if has_extension(&file, "wav") {
            let bytes = std::fs::read(&file).map_err(io_at(&file))?;
            match inspect_wav(&bytes) {
                Ok(_) => wavs.push(file),
                Err(e) => refusals.push(format!("{}: {e}", display_name(&file))),
            }
        }
    }
    if !wavs.is_empty() {
        return Ok(wavs);
    }

    // v5's order: compressed audio explains a refusal better than a list of bad WAVs.
    let reason = if compressed {
        "this sound contains MP3/OGG files; TF2 needs 16-bit 44100 Hz stereo PCM WAV".to_owned()
    } else if !refusals.is_empty() {
        let mut reason = format!(
            "no WAV here is playable in TF2: {}",
            refusals[..refusals.len().min(SHOWN_REFUSALS)].join("; ")
        );
        if refusals.len() > SHOWN_REFUSALS {
            reason.push_str(&format!(", and {} more", refusals.len() - SHOWN_REFUSALS));
        }
        reason
    } else {
        "no WAV or VPK files in this download".to_owned()
    };
    Err(DownloadError::NoUsableFiles { reason })
}

/// The files a crosshair download imports, in path order: the VTFs outside any `scripts`
/// directory (VPKs inside opened first), else the PNGs. `work` is an empty scratch directory.
pub(super) fn crosshair_files(saved: &Path, work: &Path) -> Result<Vec<PathBuf>, DownloadError> {
    let root = extract_archive(saved, work)?;
    if root.is_dir() {
        open_inner_vpks(&root)?;
    }
    let files = files_under(&root)?;

    // Weapon scripts ship beside crosshairs; only the root-relative path is tested, so a
    // data dir that happens to sit under a `scripts` folder is not mistaken for one.
    let usable: Vec<&PathBuf> = files
        .iter()
        .filter(|f| {
            !f.strip_prefix(&root)
                .unwrap_or(f)
                .components()
                .any(|c| c == Component::Normal("scripts".as_ref()))
        })
        .collect();
    for ext in ["vtf", "png"] {
        let picked: Vec<PathBuf> = usable
            .iter()
            .filter(|f| has_extension(f, ext))
            .map(|f| (*f).clone())
            .collect();
        if !picked.is_empty() {
            return Ok(picked);
        }
    }
    Err(DownloadError::NoUsableFiles {
        reason: "no VTF or PNG crosshair files in this download".to_owned(),
    })
}

/// v5's names: one file takes the store item's name, several are "<store name> - <stem>",
/// and without a store row each is named after its file stem.
pub(super) fn entry_name(file: &Path, store_name: Option<&str>, several: bool) -> String {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    match (store_name, several) {
        (Some(name), false) => name.to_owned(),
        (Some(name), true) => format!("{name} - {stem}"),
        (None, _) => stem,
    }
}

/// Extracts every VPK under `root` into a sibling `<stem>-vpk/` directory. A VPK that does
/// not open is skipped with a warning, as v5 skipped it, and its partial output removed.
fn open_inner_vpks(root: &Path) -> Result<(), DownloadError> {
    for vpk in files_under(root)?
        .into_iter()
        .filter(|f| has_extension(f, "vpk"))
    {
        let stem = vpk.file_stem().unwrap_or_default().to_string_lossy();
        let dest = vpk.with_file_name(format!("{stem}-vpk"));
        if let Err(e) = extract_archive(&vpk, &dest) {
            tracing::warn!(vpk = %vpk.display(), error = %e, "skipping a VPK that does not open");
            let _ = std::fs::remove_dir_all(&dest);
        }
    }
    Ok(())
}

/// Every file at or under `root`, sorted by path so imports run in path order.
fn files_under(root: &Path) -> Result<Vec<PathBuf>, DownloadError> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).sort_by_file_name() {
        let entry = entry.map_err(|e| DownloadError::Io {
            path: e.path().unwrap_or(root).to_path_buf(),
            source: std::io::Error::other(e.to_string()),
        })?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

fn starts_with_vpk_magic(path: &Path) -> Result<bool, DownloadError> {
    let mut header = Vec::with_capacity(VPK_MAGIC.len());
    File::open(path)
        .and_then(|f| f.take(VPK_MAGIC.len() as u64).read_to_end(&mut header))
        .map_err(io_at(path))?;
    Ok(header == VPK_MAGIC)
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
