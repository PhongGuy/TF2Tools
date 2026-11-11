//! File and folder pickers behind one trait, so tests and the pilot can script the answers.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Mutex;

use gpui_kit::{App, PathPromptOptions, Task};

/// What the user is being asked to pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickKind {
    HudArchive,
    SoundFile,
    CrosshairImage,
    Tf2Folder,
    MakerPng,
    ExportPng,
    FontFile,
    /// A recipe file to preview and use.
    Recipe,
    /// Where to save the HUD shared as a recipe.
    RecipeExport,
    /// A PNG, JPEG or WebP to draw behind the HUD Maker's canvas.
    Backdrop,
    /// A PNG or JPEG for a HUD part's picture or the main menu's background.
    HudPicture,
    /// Where to save a logs.tf log's raw zip.
    LogZip,
}

/// Asks the user for one path; `None` means cancelled or unavailable.
pub trait PathPicker: Send + Sync {
    fn pick(&self, kind: PickKind, cx: &App) -> Task<Option<PathBuf>>;
    /// Asks where to save a new file, suggesting `suggested_name`; `None` means cancelled.
    fn save_to(&self, kind: PickKind, suggested_name: &str, cx: &App) -> Task<Option<PathBuf>>;
}

/// The platform's file dialog: `Tf2Folder` picks a directory, every other kind a file.
pub struct NativePicker;

impl PathPicker for NativePicker {
    fn pick(&self, kind: PickKind, cx: &App) -> Task<Option<PathBuf>> {
        let folder = kind == PickKind::Tf2Folder;
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: false,
            prompt: None,
        });
        // A dialog that fails to open (no portal) reads as a cancel; pages offer manual entry.
        cx.background_executor()
            .spawn(async move { answer.await.ok()?.ok()??.into_iter().next() })
    }

    fn save_to(&self, _: PickKind, suggested_name: &str, cx: &App) -> Task<Option<PathBuf>> {
        let dir = std::env::home_dir().unwrap_or_else(std::env::temp_dir);
        let answer = cx.prompt_for_new_path(&dir, Some(suggested_name));
        cx.background_executor()
            .spawn(async move { answer.await.ok()?.ok()? })
    }
}

/// Hands out queued paths in order, then `None`, and records every kind it was asked for.
pub struct ScriptedPicker {
    picks: Mutex<VecDeque<PathBuf>>,
    asked: Mutex<Vec<PickKind>>,
    suggested: Mutex<Vec<String>>,
    /// When set, every answer waits for `Release::release`, like a dialog left open.
    gate: Option<async_channel::Receiver<()>>,
}

/// Lets a held `ScriptedPicker` answer every pick asked so far or later.
pub struct Release(async_channel::Sender<()>);

impl Release {
    pub fn release(&self) {
        self.0.close();
    }
}

impl ScriptedPicker {
    pub fn new(picks: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            picks: Mutex::new(picks.into_iter().collect()),
            asked: Mutex::new(Vec::new()),
            suggested: Mutex::new(Vec::new()),
            gate: None,
        }
    }

    /// Like `new`, with each answer held back until the returned `Release` lets them go.
    pub fn held(picks: impl IntoIterator<Item = PathBuf>) -> (Self, Release) {
        let (tx, rx) = async_channel::bounded(1);
        let picker = Self {
            gate: Some(rx),
            ..Self::new(picks)
        };
        (picker, Release(tx))
    }

    // The next queued answer, now or once the gate is released.
    fn answer(&self, cx: &App) -> Task<Option<PathBuf>> {
        let answer = lock(&self.picks).pop_front();
        match self.gate.clone() {
            None => Task::ready(answer),
            Some(gate) => cx.background_executor().spawn(async move {
                let _ = gate.recv().await;
                answer
            }),
        }
    }

    /// A picker on which every pick is cancelled.
    pub fn empty() -> Self {
        Self::new([])
    }

    /// Every kind requested so far, in order.
    pub fn asked(&self) -> Vec<PickKind> {
        lock(&self.asked).clone()
    }

    /// Every suggested name `save_to` was given, in order.
    pub fn suggested(&self) -> Vec<String> {
        lock(&self.suggested).clone()
    }
}

impl PathPicker for ScriptedPicker {
    fn pick(&self, kind: PickKind, cx: &App) -> Task<Option<PathBuf>> {
        lock(&self.asked).push(kind);
        self.answer(cx)
    }

    fn save_to(&self, kind: PickKind, suggested_name: &str, cx: &App) -> Task<Option<PathBuf>> {
        lock(&self.asked).push(kind);
        lock(&self.suggested).push(suggested_name.to_owned());
        self.answer(cx)
    }
}

/// `name` as a file name ending `.{ext}`: characters other than letters, digits, space, `-`,
/// `_`, `(` and `)` become `_`, outer spaces and trailing dots go, an empty result reads
/// "crosshair", and a name Windows reserves for a device (CON, NUL, COM1...) gains a `_`.
pub fn file_name_for(name: &str, ext: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '(' | ')') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = match safe.trim().trim_end_matches(['.', ' ']) {
        "" => "crosshair",
        stem => stem,
    };
    let reserved = if is_windows_device(stem) { "_" } else { "" };
    format!("{stem}{reserved}.{ext}")
}

// Windows refuses these stems in any case, whatever extension follows them.
fn is_windows_device(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    match upper.as_bytes() {
        [a, b, c] => matches!(&[*a, *b, *c], b"CON" | b"PRN" | b"AUX" | b"NUL"),
        [a, b, c, d] => matches!(&[*a, *b, *c], b"COM" | b"LPT") && (b'1'..=b'9').contains(d),
        _ => false,
    }
}

/// `path` unchanged when its extension is `ext` in any case, else with `.{ext}` appended.
pub fn with_extension(path: PathBuf, ext: &str) -> PathBuf {
    let has = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|e| e.eq_ignore_ascii_case(ext));
    if has {
        return path;
    }
    // Append rather than `set_extension`, which would replace a `.txt` the player typed.
    let mut raw = path.into_os_string();
    raw.push(format!(".{ext}"));
    PathBuf::from(raw)
}

// A poisoned lock only means a test thread panicked mid-push; the data is still usable.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
