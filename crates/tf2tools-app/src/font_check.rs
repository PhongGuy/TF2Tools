//! The HUD font check's blocking jobs: find a HUD whose fonts need the user, install its
//! missing fonts, or remember that the user said no. Call each inside `run_blocking`.

use std::collections::HashSet;
use std::path::PathBuf;

use gpui_kit::{App, Global};

use tf2tools_core::{
    FontCheck, FontInstallError, FontInstallReport, FontIssue, FontSkipReason, HudStatus, LogLevel,
    NewLogEntry, Platform, Tf2Install, check_hud_fonts, install_hud_fonts, scan_hud_fonts,
};

use crate::activity;
use crate::local_messages::{describe_font_install, describe_settings};
use crate::services::{OpError, Services};
use crate::shell::toast::ToastKind;

/// Per-session memory: folders already checked, which HUD's font dialog shows, and counters.
#[derive(Default)]
pub struct FontCheckSession {
    checked: HashSet<String>,
    showing: Option<String>,
    started: usize,
    finished: usize,
}

impl Global for FontCheckSession {}

impl FontCheckSession {
    /// Marks `folder` checked; true the first time this session.
    pub fn claim(cx: &mut App, folder: &str) -> bool {
        cx.default_global::<Self>()
            .checked
            .insert(folder.to_owned())
    }

    /// Checks `ask_about_fonts` has started; 0 without the global.
    pub fn started(cx: &App) -> usize {
        cx.try_global::<Self>().map_or(0, |s| s.started)
    }

    /// Checks whose `prompt_job` has returned; 0 without the global.
    pub fn finished(cx: &App) -> usize {
        cx.try_global::<Self>().map_or(0, |s| s.finished)
    }

    pub(crate) fn begin(cx: &mut App) {
        cx.default_global::<Self>().started += 1;
    }

    pub(crate) fn end(cx: &mut App) {
        cx.default_global::<Self>().finished += 1;
    }

    /// Claims the one dialog slot for `folder`. False while another font dialog shows; then
    /// `folder` is released unless it is the showing one, so a later page load asks again.
    pub(crate) fn try_show(cx: &mut App, folder: &str) -> bool {
        let showing = &mut cx.default_global::<Self>().showing;
        let Some(shown) = showing else {
            *showing = Some(folder.to_owned());
            return true;
        };
        // Keep the showing HUD claimed: its answer, even Skip, settles it this session.
        if shown != folder {
            Self::release(cx, folder);
        }
        false
    }

    /// Frees the dialog slot.
    pub(crate) fn hide(cx: &mut App) {
        cx.default_global::<Self>().showing = None;
    }

    /// Forgets `folder`, so a later page load checks it again.
    pub(crate) fn release(cx: &mut App, folder: &str) {
        cx.default_global::<Self>().checked.remove(folder);
    }
}

/// A HUD whose fonts need the user, as the dialog shows it.
#[derive(Debug, Clone)]
pub struct FontPrompt {
    pub folder: String,
    pub hud_dir: PathBuf,
    pub check: FontCheck,
}

/// A finished font install, shaped as a toast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontToast {
    pub kind: ToastKind,
    pub title: String,
    pub description: String,
}

/// Blocking. `folder: None` means the one HUD `hud_status` reports (Managed or Unmanaged).
/// `None` when there is no platform, no font host, no single HUD, the folder is not a dir,
/// the scan or check fails (warned), nothing needs attention, or the user already dismissed
/// everything the current check would ask about.
pub fn prompt_job(s: &Services, tf2: &Tf2Install, folder: Option<&str>) -> Option<FontPrompt> {
    let platform = Platform::current()?;
    let host = s.font_host.as_deref()?;
    let folder = match folder {
        Some(folder) => folder.to_owned(),
        None => active_hud(s, tf2)?,
    };
    let hud_dir = tf2.custom_dir().join(&folder);
    if !hud_dir.is_dir() {
        return None;
    }
    let check = scan_hud_fonts(&hud_dir, platform)
        .and_then(|fonts| check_hud_fonts(&fonts, host))
        .inspect_err(|err| tracing::warn!(%err, folder, "could not check the HUD's fonts"))
        .ok()?;
    if !check.needs_attention() {
        return None;
    }
    // A dismissal covers this check only while it names nothing new; a read failure asks.
    let dismissed =
        s.db.font_check_dismissed(&folder)
            .inspect_err(|err| tracing::warn!(%err, folder, "could not read the font dismissals"))
            .unwrap_or_default();
    if check
        .fingerprint()
        .iter()
        .all(|key| dismissed.contains(key))
    {
        return None;
    }
    Some(FontPrompt {
        folder,
        hud_dir,
        check,
    })
}

/// Blocking. The folder of the single installed HUD, or `None` (warning on a failed status
/// read).
pub fn active_hud(s: &Services, tf2: &Tf2Install) -> Option<String> {
    let status = s
        .library
        .hud_status(tf2)
        .inspect_err(|err| tracing::warn!(%err, "could not read the HUD status"))
        .ok()?;
    match status {
        HudStatus::Managed { folder_name, .. } | HudStatus::Unmanaged { folder_name, .. } => {
            Some(folder_name)
        }
        HudStatus::NotInstalled | HudStatus::Multiple { .. } | _ => None,
    }
}

/// Blocking. Installs `prompt.check.missing()` from `prompt.hud_dir`, writes the `hud` /
/// `install_fonts` Activity Log row and returns the toast; any error other than
/// `FontInstallError::Refresh` toasts "Failed to install fonts" in player words.
pub fn install_job(s: &Services, prompt: &FontPrompt) -> Result<FontToast, OpError> {
    let folder = &prompt.folder;
    let result = match s.font_host.as_deref() {
        Some(host) => install_hud_fonts(&prompt.hud_dir, prompt.check.missing(), host),
        None => Err(FontInstallError::Host {
            reason: "no font host on this system".to_owned(),
        }),
    };
    let (toast, entry) = match result {
        Ok(report) => installed(folder, &report),
        Err(err @ FontInstallError::Refresh { .. }) => {
            let toast = FontToast {
                kind: ToastKind::Warning,
                title: "Fonts copied".to_owned(),
                description: "Log out and back in for TF2 to see the new fonts.".to_owned(),
            };
            let message = format!("Copied fonts for HUD: {folder}");
            (Ok(toast), row(LogLevel::Warning, message, err.to_string()))
        }
        Err(err) => {
            let message = format!("Failed to install fonts for HUD: {folder}");
            let entry = row(LogLevel::Error, message, err.to_string());
            (
                Err(describe_font_install(&err).into_op("Failed to install fonts")),
                entry,
            )
        }
    };
    activity::write(s, &entry);
    toast
}

/// The toast and row for an install that finished, by how much it skipped.
fn installed(
    folder: &str,
    report: &FontInstallReport,
) -> (Result<FontToast, OpError>, NewLogEntry) {
    let (n, k) = (report.installed().len(), report.skipped().len());
    let mut lines = Vec::new();
    if n > 0 {
        lines.push(format!("Installed: {}", report.installed().join(", ")));
    }
    if k > 0 {
        let skipped: Vec<String> = report
            .skipped()
            .iter()
            .map(|(path, reason)| format!("{path} ({})", skip_text(reason)))
            .collect();
        lines.push(format!("Skipped: {}", skipped.join(", ")));
    }
    let message = if n == 0 {
        format!("No fonts installed for HUD: {folder}")
    } else {
        format!("Installed {n} {} for HUD: {folder}", fonts(n))
    };
    let (kind, title, description) = if k == 0 {
        (
            ToastKind::Success,
            "Fonts installed",
            format!(
                "Installed {n} {} for {folder}. Restart TF2 to use them.",
                fonts(n)
            ),
        )
    } else if n > 0 {
        (
            ToastKind::Warning,
            "Some fonts were not installed",
            format!(
                "Installed {n} of {} {} for {folder}; see the Activity Log. Restart TF2 to use them.",
                n + k,
                fonts(n + k)
            ),
        )
    } else {
        (
            ToastKind::Warning,
            "No fonts installed",
            format!(
                "Skipped {k} {} for {folder}; see the Activity Log.",
                fonts(k)
            ),
        )
    };
    let level = if k == 0 {
        LogLevel::Info
    } else {
        LogLevel::Warning
    };
    let toast = FontToast {
        kind,
        title: title.to_owned(),
        description,
    };
    (Ok(toast), row(level, message, lines.join("\n")))
}

fn row(level: LogLevel, message: String, details: String) -> NewLogEntry {
    NewLogEntry::new(level, "hud", "install_fonts", message).with_details(details)
}

fn fonts(count: usize) -> &'static str {
    if count == 1 { "font" } else { "fonts" }
}

/// Blocking. Stores `prompt.check.fingerprint()` for `prompt.folder`;
/// a failed save toasts "Failed to save the font choice" in player words.
pub fn dismiss_job(s: &Services, prompt: &FontPrompt) -> Result<(), OpError> {
    s.db.set_font_check_dismissed(&prompt.folder, &prompt.check.fingerprint())
        .map_err(|err| describe_settings(&err).into_op("Failed to save the font choice"))
}

/// One Linux font issue in plain words.
pub fn issue_text(issue: &FontIssue) -> String {
    match issue {
        FontIssue::PathCaseMismatch { declared, on_disk } => format!(
            "The HUD asks for {declared} but ships {}; Linux file names are case-sensitive, so \
             TF2 may not find it.",
            on_disk.display()
        ),
        FontIssue::UppercaseFileName { file } => format!(
            "{} has capital letters in its file name, which TF2 on Linux may fail to load.",
            file.display()
        ),
        FontIssue::InsideVpk { declared, vpk } => format!(
            "{declared} is packed in {}, and TF2 does not load fonts from VPK files.",
            vpk.display()
        ),
        _ => "A font file in this HUD may not load in TF2.".to_owned(),
    }
}

/// Why a font file was skipped, in words that follow "path (".
pub fn skip_text(reason: &FontSkipReason) -> &'static str {
    match reason {
        FontSkipReason::ProtectedName => "it carries a system font name",
        FontSkipReason::FileExists => "a different file with that name is already installed",
        FontSkipReason::OutsideHud => "it is outside the HUD folder",
        FontSkipReason::Unnamed => "its font name cannot be read",
        _ => "it could not be installed",
    }
}
