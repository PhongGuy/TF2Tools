//! Store downloads: the one running download as an app global, and the wording its button
//! and toasts show.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::{Disableable as _, v_flex};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, EventEmitter, Global,
    InteractiveElement as _, IntoElement as _, ParentElement as _, SharedString, Styled as _,
    Subscription, Task, TestSupportExt as _, Window, div,
};
use tf2tools_core::{
    DownloadError, DownloadPhase, DownloadProgress, GbId, LibraryEntry, StoreCard, StoreFile,
    StoreItemId, StoreKind,
};

use crate::ids;
use crate::library_events::LibraryEvents;
use crate::services::Services;
use crate::shell::toast::{ToastKind, toast};
use crate::store_grid::StoreGrid;
use crate::store_messages::{describe_download, download_stopped};
use crate::tokio_bridge::{self, JoinError};
use crate::widgets::labelled;

/// A copy of core's download progress the UI can match on and build in tests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    pub phase: DownloadPhase,
    pub downloaded: u64,
    pub total: Option<u64>,
}

impl From<DownloadProgress> for Progress {
    fn from(p: DownloadProgress) -> Self {
        Self {
            phase: p.phase(),
            downloaded: p.downloaded(),
            total: p.total(),
        }
    }
}

/// A byte count in binary units: whole bytes below 1 KB, else one decimal ("1.5 KB", "2.0 GB").
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // Step up while the value would print as 1024.0 or more (judged on the one-decimal
    // rounding the label shows), stopping at the largest unit.
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while (value * 10.0).round() / 10.0 >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// None or Downloading: "Downloading..." plus " {done} / {total}" or " {done}"; Importing: "Extracting...".
pub fn progress_label(p: Option<Progress>) -> String {
    let Some(p) = p else {
        return "Downloading...".to_string();
    };
    match p.phase {
        DownloadPhase::Importing => "Extracting...".to_string(),
        _ => match p.total {
            Some(total) => {
                format!(
                    "Downloading... {} / {}",
                    format_bytes(p.downloaded),
                    format_bytes(total)
                )
            }
            None => format!("Downloading... {}", format_bytes(p.downloaded)),
        },
    }
}

/// 0..=100; 0 without a total (or a zero total), 100 while importing.
pub fn progress_percent(p: Option<Progress>) -> f32 {
    let Some(p) = p else {
        return 0.0;
    };
    match (p.phase, p.total) {
        (DownloadPhase::Importing, _) => 100.0,
        (_, Some(total)) if total > 0 => {
            (p.downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0) as f32
        }
        _ => 0.0,
    }
}

/// The lowercase noun for a store's items, as toasts use it mid-sentence.
pub fn kind_noun(kind: StoreKind) -> &'static str {
    match kind {
        StoreKind::CuratedHuds | StoreKind::GameBananaHuds => "HUD",
        StoreKind::Sounds => "sound",
        StoreKind::Crosshairs => "crosshair",
        StoreKind::Addons => "addon",
        _ => "item",
    }
}

/// The capitalized noun for a store's items, as toast titles use it.
pub fn kind_title(kind: StoreKind) -> &'static str {
    match kind {
        StoreKind::CuratedHuds | StoreKind::GameBananaHuds => "HUD",
        StoreKind::Sounds => "Sound",
        StoreKind::Crosshairs => "Crosshair",
        StoreKind::Addons => "Addon",
        _ => "Item",
    }
}

/// The player message, followed by the entries a failed rollback left in the library, if any.
pub fn stayed_description(message: &str, stayed: &[String]) -> String {
    if stayed.is_empty() {
        message.to_string()
    } else {
        format!("{message} Still in your library: {}.", stayed.join(", "))
    }
}

/// The store item a download is for, as its buttons and toasts name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTarget {
    pub kind: StoreKind,
    pub item: StoreItemId,
    pub name: String,
}

/// How a download ended, emitted after its toast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadEvent {
    Finished {
        target: DownloadTarget,
    },
    Failed {
        target: DownloadTarget,
        message: String,
    },
}

/// GameBanana items of one kind downloaded this session. A grid re-queries only during a sync,
/// so its cards' `in_library` lags a download; clones share one set.
#[derive(Clone)]
pub struct SessionDownloads {
    kind: StoreKind,
    ids: Rc<RefCell<HashSet<GbId>>>,
}

impl SessionDownloads {
    pub fn new(kind: StoreKind) -> Self {
        Self {
            kind,
            ids: Rc::default(),
        }
    }

    /// Records a `Finished` of this kind's GameBanana item; true when it did.
    pub fn record(&self, event: &DownloadEvent) -> bool {
        if let DownloadEvent::Finished { target } = event
            && target.kind == self.kind
            && let StoreItemId::GameBanana(id) = &target.item
        {
            self.ids.borrow_mut().insert(*id);
            return true;
        }
        false
    }

    pub fn contains(&self, id: GbId) -> bool {
        self.ids.borrow().contains(&id)
    }

    /// The downloads recorded so far.
    pub fn snapshot(&self) -> HashSet<GbId> {
        self.ids.borrow().clone()
    }

    /// Drops `ids`, once a store read taken after they were recorded speaks for them.
    pub fn forget(&self, ids: &HashSet<GbId>) {
        self.ids.borrow_mut().retain(|id| !ids.contains(id));
    }

    /// `card.in_library`, or downloaded this session.
    pub fn in_library(&self, card: &StoreCard) -> bool {
        card.in_library || matches!(&card.id, StoreItemId::GameBanana(id) if self.contains(*id))
    }

    /// Redraws `grid` on every `StoreDownloads` notify, and records then redraws on a finished
    /// download of this kind; `grid`'s re-reads then retire the downloads they cover. Empty
    /// without the `StoreDownloads` global.
    #[must_use = "dropping the subscriptions stops the grid redraws and the In Library flip"]
    pub fn track<T: 'static>(
        &self,
        grid: &Entity<StoreGrid>,
        cx: &mut Context<T>,
    ) -> Vec<Subscription> {
        // The grid's re-reads carry the library's word, so each one retires what it covers.
        let session = self.clone();
        grid.update(cx, |grid, _| grid.track_session(session));
        let Some(downloads) = StoreDownloads::global(cx) else {
            return Vec::new();
        };
        // The cards draw inside the grid, so progress redraws the grid itself; a finished
        // item flips its card to "In Library" without a re-query.
        let observed = grid.clone();
        let progress = cx.observe(&downloads, move |_, _, cx| {
            observed.update(cx, |_, cx| cx.notify());
        });
        let (session, finished_grid) = (self.clone(), grid.clone());
        let finished = cx.subscribe(&downloads, move |_, _, event, cx| {
            if session.record(event) {
                finished_grid.update(cx, |_, cx| cx.notify());
            }
        });
        vec![progress, finished]
    }
}

/// The app's one running store download. It lives in a global so leaving a page never drops
/// (and so aborts) the download task.
pub struct StoreDownloads {
    active: Option<DownloadTarget>,
    progress: Option<Progress>,
    task: Option<Task<()>>,
}

struct DownloadsGlobal(Entity<StoreDownloads>);

impl Global for DownloadsGlobal {}

impl EventEmitter<DownloadEvent> for StoreDownloads {}

impl StoreDownloads {
    /// Makes the app's download entity, once; later calls return the existing one.
    pub fn install(cx: &mut App) -> Entity<Self> {
        if let Some(existing) = Self::global(cx) {
            return existing;
        }
        let downloads = cx.new(|_| Self {
            active: None,
            progress: None,
            task: None,
        });
        cx.set_global(DownloadsGlobal(downloads.clone()));
        downloads
    }

    /// The app's download entity, if installed.
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<DownloadsGlobal>().map(|g| g.0.clone())
    }

    /// The item downloading now, if any.
    pub fn active(&self) -> Option<&DownloadTarget> {
        self.active.as_ref()
    }

    /// Whether a download runs, so every other download button rests disabled.
    pub fn is_busy(&self) -> bool {
        self.active.is_some()
    }

    /// Whether `item` of `kind` is the one downloading.
    pub fn is_downloading(&self, kind: StoreKind, item: &StoreItemId) -> bool {
        self.active
            .as_ref()
            .is_some_and(|t| t.kind == kind && &t.item == item)
    }

    /// The running download's latest progress; None before its first report or when idle.
    pub fn progress(&self) -> Option<Progress> {
        self.progress
    }

    /// Starts `file` unless a download runs (false); toasts in `win` when it ends.
    pub fn start(
        &mut self,
        target: DownloadTarget,
        file: StoreFile,
        win: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.is_busy() {
            return false;
        }
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        // The progress callback runs on a tokio worker, so it only sends; the sender lives in
        // the download future, so the channel closes when the download ends.
        let (tx, rx) = async_channel::unbounded::<Progress>();
        let store = cx.global::<Services>().store.clone();
        let kind = target.kind;
        let job = tokio_bridge::spawn(cx, async move {
            let report = move |p: DownloadProgress| {
                let _ = tx.try_send(p.into());
            };
            store.download(kind, &file, &report).await
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            while let Ok(p) = rx.recv().await {
                let _ = this.update(cx, |d, cx| {
                    d.progress = Some(p);
                    cx.notify();
                });
            }
            let result = job.await;
            let _ = this.update(cx, |d, cx| d.finish(result, win, cx));
        }));
        self.active = Some(target);
        self.progress = None;
        cx.notify();
        true
    }

    /// Returns to idle, toasts the outcome in `win` (if still open), then emits it.
    fn finish(
        &mut self,
        result: Result<Result<Vec<LibraryEntry>, DownloadError>, JoinError>,
        win: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        self.task = None;
        self.progress = None;
        let Some(target) = self.active.take() else {
            return;
        };
        // A HUD or crosshair that had to take a numbered name says so; anything else names
        // the store item.
        let added = match &result {
            Ok(Ok(entries)) => entries.iter().find_map(|entry| match entry {
                LibraryEntry::Hud(hud) if hud.wanted_name().is_some() => {
                    Some(crate::pages::huds::added_message(hud))
                }
                LibraryEntry::Crosshair(c) if c.wanted_name().is_some() => {
                    Some(crate::pages::crosshairs::added_message(c))
                }
                _ => None,
            }),
            _ => None,
        };
        let failure = match result {
            Ok(Ok(_)) => None,
            Ok(Err(err)) => Some(failure_description(target.kind, &err)),
            Err(_) => {
                let failure = download_stopped("the download task stopped unexpectedly");
                failure.log("store download");
                Some(failure.message)
            }
        };
        let (kind, title, description) = match &failure {
            None => (
                ToastKind::Success,
                format!("{} downloaded", kind_title(target.kind)),
                added.unwrap_or_else(|| format!("{} added to your library", target.name)),
            ),
            Some(message) => (
                ToastKind::Error,
                format!("Failed to download {}", kind_noun(target.kind)),
                message.clone(),
            ),
        };
        let _ = win.update(cx, |_, window, cx| {
            toast(window, cx, kind, title, description)
        });
        let finished = failure.is_none();
        cx.emit(match failure {
            None => DownloadEvent::Finished { target },
            Some(message) => DownloadEvent::Failed { target, message },
        });
        if finished {
            LibraryEvents::changed(cx);
        }
        cx.notify();
    }
}

/// Logs the raw error and returns its player message, naming what a failed rollback left in
/// the library.
fn failure_description(kind: StoreKind, err: &DownloadError) -> String {
    let failure = describe_download(kind, err);
    failure.log("store download");
    let stayed: Vec<String> = match err {
        DownloadError::RollbackFailed { imported, .. } => imported.iter().map(entry_name).collect(),
        _ => Vec::new(),
    };
    stayed_description(&failure.message, &stayed)
}

// A kind this match does not know yet reads as "an item", so no Debug text reaches the player.
fn entry_name(entry: &LibraryEntry) -> String {
    match entry {
        LibraryEntry::Hud(e) => e.name().to_string(),
        LibraryEntry::Sound(e) => e.name().to_string(),
        LibraryEntry::Crosshair(e) => e.name().to_string(),
        LibraryEntry::Addon(e) => e.name().as_str().to_string(),
        _ => "an item".to_string(),
    }
}

/// The style a resting download button takes: secondary for an item the player already has,
/// primary otherwise.
pub fn download_variant(secondary: bool) -> ButtonVariant {
    if secondary {
        ButtonVariant::Secondary
    } else {
        ButtonVariant::Primary
    }
}

/// A card's button: the progress label and bar while this item downloads, else a Button labelled
/// `rest`, disabled while another download runs or the caller's `blocked` holds. Callers
/// re-render by observing `StoreDownloads`.
pub fn download_button(
    kind: StoreKind,
    id: &StoreItemId,
    rest: SharedString,
    secondary: bool,
    blocked: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    // Without the global (tests, a page built before install) nothing can be running.
    let (mine, busy, progress) = match StoreDownloads::global(cx) {
        Some(d) => {
            let d = d.read(cx);
            (d.is_downloading(kind, id), d.is_busy(), d.progress())
        }
        None => (false, false, None),
    };
    if mine {
        return v_flex()
            .w_full()
            .gap_1()
            .child(labelled(
                div().text_sm(),
                ids::store_download(kind, id),
                progress_label(progress),
            ))
            // Kit Progress does not register for lookup, so a test-supported wrapper holds the id.
            .child(
                div().id(ids::STORE_PROGRESS).test_support().w_full().child(
                    ProgressBar::new(ids::STORE_PROGRESS_BAR).value(progress_percent(progress)),
                ),
            )
            .into_any_element();
    }
    Button::new(ids::store_download(kind, id))
        .w_full()
        .icon(IconName::Download)
        .label(rest)
        .disabled(busy || blocked)
        .with_variant(download_variant(secondary))
        .on_click(move |_, window, cx| on_click(window, cx))
        .into_any_element()
}
