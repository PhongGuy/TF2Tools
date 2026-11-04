//! HUD font check: reading a HUD's declared fonts and the platform they are read for.

mod check;
mod host;
mod install;
pub(crate) mod keyvalues;
pub(crate) mod names;
mod scan;
mod winapi;

pub use check::{FontCheck, FontHost, FontHostError, check_hud_fonts};
pub use host::system_font_host;
pub use install::{FontInstallReport, FontSkipReason, install_hud_fonts};
pub use names::PROTECTED_FONT_NAMES;
pub use scan::{FontIssue, HudFont, HudFonts, scan_hud_fonts};

/// Reads a whole regular file of at most `cap` bytes, or `None` when it is not a regular file
/// (opening a FIFO blocks) or is larger; the read itself is bounded in case the file grows.
pub(crate) fn read_regular_capped(
    path: &std::path::Path,
    cap: u64,
) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    if !std::fs::metadata(path)?.is_file() {
        return Ok(None);
    }
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > cap {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    file.take(cap + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= cap).then_some(bytes))
}

#[cfg(test)]
#[path = "../../tests/common/sfnt.rs"]
pub(crate) mod test_sfnt;

/// The game platform a HUD is read for; conditionals and Linux-only issues depend on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Platform {
    /// Windows, where `$WIN32` and `$WINDOWS` hold.
    Windows,
    /// Linux, where `$WIN32`, `$POSIX` and `$LINUX` hold.
    Linux,
}

impl Platform {
    /// The running platform, `None` where TF2 no longer runs (macOS and others).
    pub fn current() -> Option<Platform> {
        if cfg!(windows) {
            Some(Platform::Windows)
        } else if cfg!(target_os = "linux") {
            Some(Platform::Linux)
        } else {
            None
        }
    }
}
