//! Installing a HUD's missing fonts into the per-user font directory through a `FontHost`.

use std::collections::{BTreeSet, HashSet};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use super::FontHost;
use super::check::slash_path;
use super::names::{is_protected, names_in, read_capped};
use super::scan::HudFont;
use crate::error::FontInstallError;

/// Why a HUD font file was not installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontSkipReason {
    /// The file carries a name in `PROTECTED_FONT_NAMES`.
    ProtectedName,
    /// A different file with that name is already in the fonts directory.
    FileExists,
    /// The file resolves outside the HUD directory.
    OutsideHud,
    /// The file declares no family or full name, or one in an encoding not decoded here, so
    /// its protection cannot be judged.
    Unnamed,
}

/// What [`install_hud_fonts`] did, by HUD-relative file path with `/` separators.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FontInstallReport {
    installed: Vec<String>,
    skipped: Vec<(String, FontSkipReason)>,
}

impl FontInstallReport {
    /// Files now in the fonts directory and registered, including identical ones already there,
    /// one per destination: of several identical sources, the first in sorted order stands.
    pub fn installed(&self) -> &[String] {
        &self.installed
    }

    /// Files left out, each with its reason.
    pub fn skipped(&self) -> &[(String, FontSkipReason)] {
        &self.skipped
    }
}

/// Copies each unique file of `fonts` (HUD-relative) into `host.fonts_dir()`, registers it,
/// then refreshes once if anything was installed. Files are taken in sorted order and the first
/// `Io` or `Register` error stops the install; files placed before it stay, are still
/// refreshed, and the error is returned.
pub fn install_hud_fonts(
    hud_dir: &Path,
    fonts: &[HudFont],
    host: &dyn FontHost,
) -> Result<FontInstallReport, FontInstallError> {
    let mut run = Run::default();
    let files: BTreeSet<&PathBuf> = fonts.iter().flat_map(|font| font.files()).collect();
    if files.is_empty() {
        return Ok(run.report);
    }
    let result = install_files(hud_dir, &files, host, &mut run);
    if !run.placed {
        return result.map(|()| run.report);
    }
    let refreshed = host.refresh().map_err(|e| FontInstallError::Refresh {
        reason: e.reason().to_owned(),
    });
    result.and(refreshed).map(|()| run.report)
}

/// Progress of one install, kept outside the fallible loop so a failure part-way still knows
/// what reached the fonts directory.
#[derive(Default)]
struct Run {
    report: FontInstallReport,
    /// A file now sits in the fonts directory, even if registering it failed.
    placed: bool,
    /// Destinations already registered, so identical sources sharing one register and count once.
    registered: HashSet<PathBuf>,
}

fn install_files(
    hud_dir: &Path,
    files: &BTreeSet<&PathBuf>,
    host: &dyn FontHost,
    run: &mut Run,
) -> Result<(), FontInstallError> {
    let hud_root = hud_dir.canonicalize().map_err(io(hud_dir))?;
    let fonts_dir = host.fonts_dir().map_err(|e| FontInstallError::Host {
        reason: e.reason().to_owned(),
    })?;
    let report = &mut run.report;

    for rel in files {
        let shown = slash_path(rel);
        // The HUD may have changed since the scan, so containment, the regular-file check and
        // the names are judged again on the exact bytes that get copied.
        let source = hud_dir.join(rel);
        let real = source.canonicalize().map_err(io(&source))?;
        if !real.starts_with(&hud_root) {
            report.skipped.push((shown, FontSkipReason::OutsideHud));
            continue;
        }
        let data = read_capped(&real).map_err(io(&real))?;
        let names = names_in(&data).map_err(io(&real))?;
        // A file with no readable name, or with a name we cannot decode, could still carry one
        // the system reads, so it is treated like a protected one.
        let Some(first_name) = names.names.first().filter(|_| !names.undecoded) else {
            report.skipped.push((shown, FontSkipReason::Unnamed));
            continue;
        };
        if is_protected(&names.names) {
            report.skipped.push((shown, FontSkipReason::ProtectedName));
            continue;
        }
        let Some(file_name) = rel.file_name() else {
            return Err(io(&source)(ErrorKind::InvalidInput.into()));
        };
        let dest = fonts_dir.join(file_name);
        if !place(&fonts_dir, &dest, &data)? {
            report.skipped.push((shown, FontSkipReason::FileExists));
            continue;
        }
        run.placed = true;
        // An identical file already placed at this destination is the same font, already
        // counted under the first source, so it is left out of the report.
        if run.registered.contains(&dest) {
            continue;
        }
        let full_name = names.full_name.as_ref().unwrap_or(first_name);
        host.register(&dest, full_name, names.open_type)
            .map_err(|e| FontInstallError::Register {
                name: full_name.clone(),
                reason: e.reason().to_owned(),
            })?;
        run.registered.insert(dest);
        report.installed.push(shown);
    }
    Ok(())
}

/// Puts `data` at `dest` unless something else already holds that name: true when `dest` now
/// holds exactly `data`. The bytes go to a temporary file in the same directory that is linked
/// in without replacing, so a crash never leaves a half-written font and nothing is overwritten.
fn place(dir: &Path, dest: &Path, data: &[u8]) -> Result<bool, FontInstallError> {
    if let Some(same) = holds(dest, data)? {
        return Ok(same);
    }
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".tf2tools-")
        .suffix(".part")
        .tempfile_in(dir)
        .map_err(io(dir))?;
    temp.write_all(data)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(io(temp.path()))?;
    match temp.persist_noclobber(dest) {
        Ok(_) => Ok(true),
        // Another writer claimed the name meanwhile; its file is judged like any existing one.
        Err(e) if e.error.kind() == ErrorKind::AlreadyExists => {
            Ok(holds(dest, data)?.unwrap_or(false))
        }
        Err(e) => Err(io(dest)(e.error)),
    }
}

/// `None` when nothing is at `dest`; otherwise whether it is a regular file (not a link, FIFO
/// or directory, which are never opened) whose bytes equal `data`.
fn holds(dest: &Path, data: &[u8]) -> Result<Option<bool>, FontInstallError> {
    match std::fs::symlink_metadata(dest) {
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(dest)(e)),
        Ok(meta) if !meta.file_type().is_file() => Ok(Some(false)),
        Ok(_) => Ok(Some(read_capped(dest).is_ok_and(|bytes| bytes == data))),
    }
}

/// Maps an I/O error to `FontInstallError::Io` naming `path`.
fn io(path: &Path) -> impl FnOnce(std::io::Error) -> FontInstallError + '_ {
    move |source| FontInstallError::Io {
        path: path.to_path_buf(),
        source,
    }
}
