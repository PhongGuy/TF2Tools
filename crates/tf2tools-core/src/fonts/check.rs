//! Checking a scanned HUD against the fonts the system already has, through an injectable
//! host so tests never query or change the real machine.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use super::scan::{FontIssue, HudFont, HudFonts};
use crate::error::FontCheckError;

/// The system side, injectable so tests never query or change the real machine.
pub trait FontHost: Send + Sync {
    /// Every installed family and full name.
    fn installed_names(&self) -> Result<Vec<String>, FontHostError>;
    /// The per-user directory fonts are copied into.
    fn fonts_dir(&self) -> Result<PathBuf, FontHostError>;
    /// Makes one copied file known to the system.
    fn register(
        &self,
        installed_file: &Path,
        full_name: &str,
        open_type: bool,
    ) -> Result<(), FontHostError>;
    /// Tells the system the font set changed.
    fn refresh(&self) -> Result<(), FontHostError>;
}

/// A font host operation failed; `reason` is the host's own description.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
#[non_exhaustive]
pub struct FontHostError {
    reason: String,
}

impl FontHostError {
    /// A failure described by `reason`.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// The host's description of the failure.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// What a HUD needs from the system, as found by [`check_hud_fonts`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FontCheck {
    missing: Vec<HudFont>,
    protected: Vec<String>,
    issues: Vec<FontIssue>,
}

impl FontCheck {
    /// The shipped fonts the system lacks, in first-declared order.
    pub fn missing(&self) -> &[HudFont] {
        &self.missing
    }

    /// Missing names the HUD ships under a protected name; listed, never installed.
    pub fn protected(&self) -> &[String] {
        &self.protected
    }

    /// The Linux file-name problems the scan found.
    pub fn issues(&self) -> &[FontIssue] {
        &self.issues
    }

    /// True when a font is missing or a file-name problem exists.
    pub fn needs_attention(&self) -> bool {
        !self.missing.is_empty() || !self.issues.is_empty()
    }

    /// Sorted, deduplicated keys naming what needs attention, so a dismissal can tell when
    /// the HUD changed: `font:<name lowercased>` and `issue:<kind>:<path>` with `/` paths.
    pub fn fingerprint(&self) -> Vec<String> {
        let fonts = self
            .missing
            .iter()
            .map(|font| format!("font:{}", font.name().to_lowercase()));
        let issues = self.issues.iter().map(|issue| match issue {
            FontIssue::PathCaseMismatch { declared, .. } => format!("issue:case:{declared}"),
            FontIssue::UppercaseFileName { file } => {
                format!("issue:uppercase:{}", slash_path(file))
            }
            FontIssue::InsideVpk { declared, .. } => format!("issue:vpk:{declared}"),
        });
        fonts
            .chain(issues)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

/// A font is missing when no host name equals it case-insensitively; a missing font with any
/// protected file goes to `protected` by name instead. The host is asked only when the HUD ships
/// a referenced font; its failure is `FontCheckError::Host`.
pub fn check_hud_fonts(fonts: &HudFonts, host: &dyn FontHost) -> Result<FontCheck, FontCheckError> {
    // Nothing shipped means nothing can be missing, so the system is not asked; the issues
    // still surface when the host itself is unavailable.
    let mut check = FontCheck {
        issues: fonts.issues().to_vec(),
        ..FontCheck::default()
    };
    if fonts.fonts().is_empty() {
        return Ok(check);
    }
    let installed: HashSet<String> = host
        .installed_names()
        .map_err(|e| FontCheckError::Host {
            reason: e.reason().to_owned(),
        })?
        .iter()
        .map(|name| name.to_lowercase())
        .collect();

    for font in fonts.fonts() {
        if installed.contains(&font.name().to_lowercase()) {
            continue;
        }
        if font
            .files()
            .iter()
            .any(|file| fonts.protected_files.contains(file))
        {
            check.protected.push(font.name().to_owned());
        } else {
            check.missing.push(font.clone());
        }
    }
    Ok(check)
}

/// `path` with its components joined by `/` whatever the platform separator.
pub(crate) fn slash_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
