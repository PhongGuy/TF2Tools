//! Crash details a player can share: a panic's report, rendered with the home folder hidden,
//! written atomically to `<data>/crash/last-crash.txt`, read on the next start and moved aside
//! once its banner has shown.

use std::borrow::Cow;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::SecondsFormat;
use tf2tools_core::panic_mark;

pub const CRASH_FILE: &str = "last-crash.txt";
pub const PREVIOUS_FILE: &str = "previous-crash.txt";
pub const MAX_REPORT_BYTES: usize = 64 * 1024;

pub mod notice;
pub mod recent;

const CUT_SHORT: &str = "\n[cut short]\n";

/// `<root>/crash`.
pub fn crash_dir(root: &Path) -> PathBuf {
    root.join("crash")
}

/// What one panic left behind, before rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashReport {
    pub version: String,
    pub time: chrono::DateTime<chrono::Utc>,
    /// "{OS} {ARCH}", from `system()` in the hook.
    pub system: String,
    pub thread: Option<String>,
    pub message: String,
    /// "file:line:col".
    pub location: Option<String>,
    pub backtrace: String,
    pub recent: Vec<String>,
}

impl CrashReport {
    /// The report text, header lines then the backtrace and recent lines, with the home folder
    /// replaced by `~` through `redact_home(_, home)`. An empty section reads "None".
    pub fn render(&self, home: Option<&Path>) -> String {
        let backtrace = self.backtrace.trim_end();
        let backtrace = if backtrace.is_empty() {
            "None"
        } else {
            backtrace
        };
        let recent = if self.recent.is_empty() {
            "None".to_owned()
        } else {
            self.recent.join("\n")
        };
        let text = format!(
            "TF2Tools {} closed unexpectedly.\nTime: {}\nSystem: {}\nThread: {}\nMessage: {}\n\
             Location: {}\n\nBacktrace:\n{}\n\nRecent warnings and errors:\n{}\n",
            self.version,
            self.time.to_rfc3339_opts(SecondsFormat::Secs, true),
            self.system,
            self.thread.as_deref().unwrap_or("unnamed"),
            self.message,
            self.location.as_deref().unwrap_or("unknown"),
            backtrace,
            recent,
        );
        redact_home(&text, home)
    }
}

/// `std::env::consts::OS` and `ARCH`, space-separated.
pub fn system() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Every occurrence of `home`, less any trailing separator, becomes "~". A Windows-style home
/// (a `\` or a drive letter) matches in any ASCII case and with `\` and `/` mixed, as Windows paths do.
/// `None` or a home under 3 bytes returns the text unchanged.
pub fn redact_home(text: &str, home: Option<&Path>) -> String {
    let Some(home) = home else {
        return text.to_owned();
    };
    // A trailing separator would hide only "home/" and leave a bare "home" in view.
    let home = home.to_string_lossy();
    let home = home.trim_end_matches(['/', '\\']);
    if home.len() < 3 {
        return text.to_owned();
    }
    let windows = home.contains('\\') || home.as_bytes().get(1) == Some(&b':');
    if !windows {
        return text.replace(home, "~");
    }
    replace_windows_home(text, home)
}

/// Swaps a Windows `home` for "~" in any case and with `\` and `/` mixed freely. Matching runs on
/// a folded copy (ASCII-lowercased, `\` as `/`), which keeps every byte offset (and char
/// boundary) of the original, so slices never panic.
fn replace_windows_home(text: &str, home: &str) -> String {
    let fold = |s: &str| s.to_ascii_lowercase().replace('\\', "/");
    let haystack = fold(text);
    let needle = fold(home);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (start, _) in haystack.match_indices(&needle) {
        out.push_str(text.get(last..start).unwrap_or_default());
        out.push('~');
        last = start + needle.len();
    }
    out.push_str(text.get(last..).unwrap_or_default());
    out
}

/// The panic payload as text: a `&str` or `String` payload, else "panic".
pub fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "panic".to_owned()
    }
}

/// Creates `dir`, writes `text` capped at MAX_REPORT_BYTES to `CRASH_FILE.tmp`, renames it to
/// CRASH_FILE, so a reader never sees half a report.
pub fn write_crash(dir: &Path, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{CRASH_FILE}.tmp"));
    std::fs::write(&tmp, cap(text).as_bytes())?;
    std::fs::rename(&tmp, dir.join(CRASH_FILE))
}

/// The largest char-boundary prefix that fits with the cut-short marker inside the cap.
fn cap(text: &str) -> Cow<'_, str> {
    if text.len() <= MAX_REPORT_BYTES {
        return Cow::Borrowed(text);
    }
    let end = text.floor_char_boundary(MAX_REPORT_BYTES - CUT_SHORT.len());
    Cow::Owned(format!(
        "{}{CUT_SHORT}",
        text.get(..end).unwrap_or_default()
    ))
}

/// Reads CRASH_FILE (at most MAX_REPORT_BYTES, lossy UTF-8) and leaves it in place.
/// None when it is missing, unreadable or blank.
pub fn read_crash(dir: &Path) -> Option<String> {
    let file = std::fs::File::open(dir.join(CRASH_FILE)).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_REPORT_BYTES as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    (!text.trim().is_empty()).then_some(text)
}

/// Moves CRASH_FILE to PREVIOUS_FILE, removing it if the move fails; nothing when it is missing.
pub fn archive_crash(dir: &Path) {
    let path = dir.join(CRASH_FILE);
    if std::fs::rename(&path, dir.join(PREVIOUS_FILE)).is_err() {
        let _ = std::fs::remove_file(&path);
    }
}

/// `read_crash`, then `archive_crash`.
pub fn take_crash(dir: &Path) -> Option<String> {
    let text = read_crash(dir);
    archive_crash(dir);
    text
}

// Set by the first uncaught panic of the process, before its report is rendered or written.
static RECORDED: AtomicBool = AtomicBool::new(false);

/// Chains a hook in front of the current one: unless the panicking thread is marked (see
/// `tf2tools_core::panic_mark`), it renders a `CrashReport` and writes it to `dir` (errors
/// ignored), then calls the previous hook. Only the first uncaught panic of the process is written.
pub fn install_hook(dir: PathBuf, recent: recent::Recent) {
    let previous = std::panic::take_hook();
    // Nothing on this path may panic (a panic in the hook aborts): no unwrap, index or borrow.
    std::panic::set_hook(Box::new(move |info| {
        // A caught panic is shown in the app, which keeps running; only the rest is a crash.
        // The first crash is the cause and later ones its echoes, so only the first claims the file.
        let caught = panic_mark::is_marked();
        if !caught
            && RECORDED
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            let report = CrashReport {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                time: chrono::Utc::now(),
                system: system(),
                thread: std::thread::current().name().map(str::to_owned),
                message: info.payload_as_str().unwrap_or("panic").to_owned(),
                location: info.location().map(ToString::to_string),
                backtrace: std::backtrace::Backtrace::force_capture().to_string(),
                recent: recent.snapshot(),
            };
            let home = std::env::home_dir();
            let _ = write_crash(&dir, &report.render(home.as_deref()));
        }
        previous(info);
    }));
}

/// Runs `f` with the thread marked, so a panic inside it writes no crash file (the caller's
/// catcher handles it); the mark is lifted on return and on unwind.
pub fn marked<R>(f: impl FnOnce() -> R) -> R {
    panic_mark::marked(f)
}

/// `f`, with every poll run under `marked`, for futures whose panic tokio catches.
pub fn marked_future<F: Future>(f: F) -> impl Future<Output = F::Output> {
    panic_mark::marked_future(f)
}

/// `catch_unwind(AssertUnwindSafe(|| marked(f)))`.
pub fn catching<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    panic_mark::catching(f)
}
