//! The HUD font check's blocking jobs: find a HUD whose fonts need the user, install its
//! missing fonts, or remember that the user said no. Call each inside `run_blocking`.

use std::collections::HashSet;
use std::path::PathBuf;

use gpui_kit::{App, Global, SharedString};

use tf2tools_core::{
    FontCheck, FontInstallError, FontInstallReport, FontIssue, FontSkipReason, HudStatus, LogLevel,
    NewLogEntry, Platform, Tf2Install, check_hud_fonts, install_hud_fonts, scan_hud_fonts,
};

use crate::activity;
use crate::i18n::{counted_plural, tr};
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
                title: tr!("font_check.toast.copied_title").to_string(),
                description: tr!("font_check.toast.copied").to_string(),
            };
            let message = tr!("font_check.activity.copied", folder = folder).to_string();
            (Ok(toast), row(LogLevel::Warning, message, err.to_string()))
        }
        Err(err) => {
            let message = tr!("font_check.activity.failed", folder = folder).to_string();
            let entry = row(LogLevel::Error, message, err.to_string());
            (
                Err(describe_font_install(&err).into_op(tr!("messages.action.install_fonts"))),
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
        let fonts = crate::i18n::separated(report.installed());
        lines.push(tr!("font_check.activity.installed_list", fonts = fonts).to_string());
    }
    if k > 0 {
        let skipped: Vec<String> = report
            .skipped()
            .iter()
            .map(|(path, reason)| format!("{path} ({})", skip_text(reason)))
            .collect();
        let fonts = crate::i18n::separated(&skipped);
        lines.push(tr!("font_check.activity.skipped_list", fonts = fonts).to_string());
    }
    let message = if n == 0 {
        tr!("font_check.activity.none", folder = folder)
    } else {
        let wide = u64::try_from(n).unwrap_or(u64::MAX);
        let folder = [("folder", folder)];
        counted_plural(
            "font_check.activity.installed",
            wide,
            &n.to_string(),
            &folder,
        )
    };
    let (kind, title) = if k == 0 {
        (ToastKind::Success, tr!("font_check.toast.installed_title"))
    } else if n > 0 {
        (ToastKind::Warning, tr!("font_check.toast.some_title"))
    } else {
        (ToastKind::Warning, tr!("font_check.toast.none_title"))
    };
    let description = installed_text(folder, n, k);
    let level = if k == 0 {
        LogLevel::Info
    } else {
        LogLevel::Warning
    };
    let toast = FontToast {
        kind,
        title: title.to_string(),
        description,
    };
    (Ok(toast), row(level, message.to_string(), lines.join("\n")))
}

/// The install toast's description for `installed` fonts copied and `skipped` left out.
pub fn installed_text(folder: &str, installed: usize, skipped: usize) -> String {
    // Each sentence counts the fonts it names: the installed, all of them, or the skipped.
    let wide = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    let folder = [("folder", folder)];
    let text = if skipped == 0 {
        counted_plural(
            "font_check.installed",
            wide(installed),
            &installed.to_string(),
            &folder,
        )
    } else if installed > 0 {
        let total = installed.saturating_add(skipped);
        let (done, all) = (installed.to_string(), total.to_string());
        let vars = [folder[0], ("installed", done.as_str())];
        counted_plural("font_check.installed_some", wide(total), &all, &vars)
    } else {
        counted_plural(
            "font_check.skipped",
            wide(skipped),
            &skipped.to_string(),
            &folder,
        )
    };
    text.to_string()
}

fn row(level: LogLevel, message: String, details: String) -> NewLogEntry {
    NewLogEntry::new(level, "hud", "install_fonts", message).with_details(details)
}

/// Blocking. Stores `prompt.check.fingerprint()` for `prompt.folder`;
/// a failed save toasts "Failed to save the font choice" in player words.
pub fn dismiss_job(s: &Services, prompt: &FontPrompt) -> Result<(), OpError> {
    s.db.set_font_check_dismissed(&prompt.folder, &prompt.check.fingerprint())
        .map_err(|err| describe_settings(&err).into_op(tr!("messages.action.save_font_choice")))
}

/// One Linux font issue in plain words.
pub fn issue_text(issue: &FontIssue) -> String {
    match issue {
        FontIssue::PathCaseMismatch { declared, on_disk } => {
            let file = on_disk.display().to_string();
            tr!("font_check.issue.case", declared = declared, file = file)
        }
        FontIssue::UppercaseFileName { file } => {
            let file = file.display().to_string();
            tr!("font_check.issue.uppercase", file = file)
        }
        FontIssue::InsideVpk { declared, vpk } => {
            let vpk = vpk.display().to_string();
            tr!("font_check.issue.vpk", declared = declared, vpk = vpk)
        }
        _ => tr!("font_check.issue.unknown"),
    }
    .to_string()
}

/// Why a font file was skipped, in words that follow "path (".
pub fn skip_text(reason: &FontSkipReason) -> SharedString {
    match reason {
        FontSkipReason::ProtectedName => tr!("font_check.skip.protected"),
        FontSkipReason::FileExists => tr!("font_check.skip.exists"),
        FontSkipReason::OutsideHud => tr!("font_check.skip.outside"),
        FontSkipReason::Unnamed => tr!("font_check.skip.unnamed"),
        _ => tr!("font_check.skip.failed"),
    }
}
