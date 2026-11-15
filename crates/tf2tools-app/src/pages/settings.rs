//! v5's Settings page: the app version with update status and actions, the TF2 path with Browse and
//! Auto-detect, the preview volume, the store cache duration, one sync card per store with Resync, and Save.
//! Below them, a Licences card shows the third-party notices the app carries, and a Help and
//! feedback card links to the help page and the place to report a problem.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use gpui_kit::component::IndexPath;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::searchable_list::SearchableListItem;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, Font, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, TestSupportExt as _,
    UniformListScrollHandle, Window, canvas, div, px, relative, rems, uniform_list,
};

use tf2tools_core::{
    LogLevel, NewLogEntry, Settings, SettingsError, StoreKind, StoreSyncStatus, Tf2Install,
};

use crate::activity;
use crate::audio::volume::{bind_slider, shared_volume};
use crate::i18n::{self, tr};
use crate::ids;
use crate::links;
use crate::local_messages::{
    describe_detect, describe_settings, describe_store_status, describe_tf2_folder,
};
use crate::log_nicknames::LogNicknames;
use crate::mature::MatureAddons;
use crate::notices::{self, Notice};
use crate::picker::PickKind;
use crate::services::{Services, Tf2State, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, error_label, toast, toast_error};
use crate::sync::{SyncEvent, SyncHub, sync_hub};
use crate::updater::{self, updater};
use crate::widgets::scroll::{BAR_WIDTH, scroll_area};

/// What the page is waiting on; while set, the form's buttons are disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Busy {
    Browse,
    Detect,
    Save,
}

/// One install Auto-detect found, as a row of the install select.
#[derive(Debug, Clone)]
struct Tf2Candidate {
    index: usize,
    path: SharedString,
}

impl SearchableListItem for Tf2Candidate {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.path.clone()
    }

    // Kit rows carry no test id, so each row draws its own for headless clicks and the pilot.
    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(ids::settings_candidate(self.index))
            .test_support()
            .child(self.path.clone())
    }

    fn value(&self) -> &SharedString {
        &self.path
    }
}

type Candidates = Entity<SelectState<Vec<Tf2Candidate>>>;

/// One offered language, as a row of the Language select.
#[derive(Debug, Clone)]
struct LanguageRow {
    code: SharedString,
    name: SharedString,
}

impl SearchableListItem for LanguageRow {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    // Like the install rows, each row draws its own test id.
    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(ids::settings_language(&self.code))
            .test_support()
            .child(self.name.clone())
    }

    fn value(&self) -> &SharedString {
        &self.code
    }
}

/// The Settings form. The shell keeps it across visits, so unsaved edits stay until Save; it
/// loads the saved settings on creation and rereads them each time it shows again.
pub struct SettingsPage {
    window: AnyWindowHandle,
    path: Entity<InputState>,
    volume: Entity<SliderState>,
    cache_hours: Entity<SliderState>,
    /// The path and hours last read or saved; a field still holding them is untouched.
    saved_path: SharedString,
    saved_hours: u32,
    /// The install select, present only after Auto-detect found several installs.
    candidates: Option<Candidates>,
    language: Entity<SelectState<Vec<LanguageRow>>>,
    loaded: bool,
    busy: Option<Busy>,
    /// Each store's cached count and last sync, absent until the first read lands or if it failed.
    sync_status: Option<Vec<StoreSyncStatus>>,
    sync_loaded: bool,
    sync_generation: u64,
    /// The last failed status read's toast label, so a repeat stays quiet until a read succeeds.
    last_failure: Option<SharedString>,
    /// What the page last drew from the hub, so progress ticks do not redraw it.
    hub_drawn: Vec<HubDrawn>,
    _hub: Vec<Subscription>,
    _updates: Option<Subscription>,
    _volume: [Subscription; 3],
    /// The embedded licence notices, and whether the Licences card shows them.
    notices: Vec<Notice>,
    show_notices: bool,
    /// Every notice as list rows, shared with the list's row builder, and the text width they
    /// were wrapped to; split once the list's width is known and again when it changes.
    notice_rows: Option<Rc<[NoticeRow]>>,
    notice_width: Pixels,
    /// The notices list's text width and font as its last layout measured them.
    notice_measure: Rc<RefCell<Option<(Pixels, Font)>>>,
    scroll: ScrollHandle,
    notices_scroll: UniformListScrollHandle,
}

/// One row of the notices list: the blank before a title, a notice's title, or one wrapped line.
enum NoticeRow {
    Gap,
    Title(SharedString, SharedString),
    Line(SharedString, usize, SharedString),
}

/// The notices list's height and its fixed row height, since `uniform_list` sizes rows alike.
const NOTICE_LIST_HEIGHT: f32 = 320.;
const NOTICE_ROW_HEIGHT: f32 = 16.;
/// The notices list's left padding; the right holds the scrollbar's width instead.
const NOTICE_PADDING: f32 = 12.;

/// One store's running flag and failure text, the only hub state Settings draws.
type HubDrawn = (StoreKind, bool, Option<String>);

// Reads each store's running flag and failure, in StoreKind::ALL order.
fn hub_drawn(hub: &SyncHub) -> Vec<HubDrawn> {
    StoreKind::ALL
        .iter()
        .map(|&kind| {
            (
                kind,
                hub.is_running(kind),
                hub.failure(kind).map(str::to_owned),
            )
        })
        .collect()
}

/// The sync cards in v5's order, with v5's titles and descriptions.
pub const SYNC_CARDS: [(StoreKind, &str, &str); 5] = [
    (
        StoreKind::Sounds,
        "Sound Store",
        "GameBanana hitsounds and killsounds",
    ),
    (
        StoreKind::Crosshairs,
        "Crosshair Store",
        "GameBanana crosshair mods",
    ),
    (
        StoreKind::GameBananaHuds,
        "GameBanana HUD Store",
        "GameBanana HUD mods",
    ),
    (
        StoreKind::CuratedHuds,
        "Curated HUD Store",
        "Curated HUDs from mastercomfig",
    ),
    (
        StoreKind::Addons,
        "Addon Store",
        "Mastercomfig performance and QoL addons",
    ),
];

impl SettingsPage {
    // Splits every notice into a title and lines wrapped to the list's measured text width, only
    // while shown and when that width changed, so neither startup nor a frame pays for it. A blank
    // row parts each notice from the one before.
    fn split_notices(&mut self, window: &Window) {
        let measured = self.notice_measure.borrow().clone();
        let Some((width, font)) = measured.filter(|_| self.show_notices) else {
            return;
        };
        if self.notice_rows.is_some() && self.notice_width == width {
            return;
        }
        let text = window.text_system();
        let (font, size) = (
            text.resolve_font(&font),
            rems(0.75).to_pixels(window.rem_size()),
        );
        let mut widths = HashMap::new();
        let mut advance = |c: char| {
            *widths
                .entry(c)
                .or_insert_with(|| (text.advance(font, size, c)).map_or(0., |a| a.width / px(1.)))
        };
        // A pixel of slack keeps shaping from drawing a row wider than its measured characters.
        let room = (width - px(1.)) / px(1.);
        let mut rows = Vec::new();
        for notice in &self.notices {
            if !rows.is_empty() {
                rows.push(NoticeRow::Gap);
            }
            let file = SharedString::from(notice.file().to_owned());
            rows.push(NoticeRow::Title(
                file.clone(),
                notice.title().to_owned().into(),
            ));
            let lines = notice.lines(room, &mut advance).into_iter().enumerate();
            rows.extend(lines.map(|(row, text)| NoticeRow::Line(file.clone(), row, text.into())));
        }
        self.notice_rows = Some(rows.into());
        self.notice_width = width;
    }

    /// Needs the `Services` global. Starts the settings load off the UI thread.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let path = cx.new(|cx| InputState::new(window, cx).placeholder("/path/to/Team Fortress 2"));
        let percent = f32::from(shared_volume(cx).read(cx).get().get());
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(percent)
        });
        let cache_hours = cx.new(|_| SliderState::new().min(1.).max(720.).step(1.));
        // The labels are drawn from the slider values, so any change redraws the page.
        let volume_bound = bind_slider(&volume, window, cx);
        cx.observe(&cache_hours, |_, _, cx| cx.notify()).detach();
        cx.observe(&path, |_, _, cx| cx.notify()).detach();
        let mature = MatureAddons::global(cx);
        cx.observe(&mature, |_, _, cx| cx.notify()).detach();
        let nicknames = LogNicknames::global(cx);
        cx.observe(&nicknames, |_, _, cx| cx.notify()).detach();
        // The Resync buttons follow the hub's running kinds and failures, so only a change
        // there redraws; every ended run may have written a new stamp, so the statuses are
        // read again. With no hub, Resync stays off.
        let hub_drawn_now = sync_hub(cx)
            .map(|hub| hub_drawn(hub.read(cx)))
            .unwrap_or_default();
        let hub = sync_hub(cx)
            .map(|hub| {
                vec![
                    cx.observe(&hub, |page: &mut Self, hub, cx| {
                        let now = hub_drawn(hub.read(cx));
                        if now != page.hub_drawn {
                            page.hub_drawn = now;
                            cx.notify();
                        }
                    }),
                    cx.subscribe_in(&hub, window, |page, _, _: &SyncEvent, window, cx| {
                        page.load_sync(window, cx)
                    }),
                ]
            })
            .unwrap_or_default();
        let language = Self::language_select(window, cx);
        // The App Updates card draws the updater's state, so every change redraws the page.
        let updates = updater(cx).map(|u| cx.observe(&u, |_, _, cx| cx.notify()));
        let mut page = Self {
            window: window.window_handle(),
            path,
            volume,
            cache_hours,
            saved_path: SharedString::default(),
            saved_hours: 0,
            candidates: None,
            language,
            loaded: false,
            busy: None,
            sync_status: None,
            sync_loaded: false,
            sync_generation: 0,
            last_failure: None,
            hub_drawn: hub_drawn_now,
            _hub: hub,
            _updates: updates,
            _volume: volume_bound,
            notices: notices::all(),
            show_notices: false,
            notice_rows: None,
            notice_width: px(0.),
            notice_measure: Rc::default(),
            scroll: ScrollHandle::new(),
            notices_scroll: UniformListScrollHandle::new(),
        };
        page.load(true, cx);
        page.load_sync(window, cx);
        page
    }

    /// True once the first store status read has landed, whether or not it succeeded.
    pub fn is_sync_loaded(&self) -> bool {
        self.sync_loaded
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// The wrapped notice lines the open notices list holds, as (file, text) in order.
    pub fn notice_lines(&self) -> Vec<(String, String)> {
        let rows = self.notice_rows.as_deref().unwrap_or_default();
        (rows.iter())
            .filter_map(|row| match row {
                NoticeRow::Line(file, _, text) => Some((file.to_string(), text.to_string())),
                NoticeRow::Gap | NoticeRow::Title(..) => None,
            })
            .collect()
    }

    pub fn path_input(&self) -> &Entity<InputState> {
        &self.path
    }

    pub fn volume(&self) -> &Entity<SliderState> {
        &self.volume
    }

    pub fn cache_hours(&self) -> &Entity<SliderState> {
        &self.cache_hours
    }

    /// Rereads the saved settings on return, so a field left untouched shows what changed
    /// while the page was hidden; an edited field keeps its edit.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if !hidden {
            self.load(false, cx);
        }
    }

    // Reads the settings in the background, then fills the form: every field on the first
    // load, afterwards only the untouched ones. A failed read toasts and leaves the form as is.
    fn load(&self, first: bool, cx: &mut Context<Self>) {
        let reading = run_blocking(cx, |s| {
            s.db.settings()
                .map_err(|e| describe_settings(&e).into_op("Load settings"))
        });
        let win = self.window;
        cx.spawn(async move |page, cx| {
            let read = reading.await;
            let _ = cx.update_window(win, |_, window, cx| {
                page.update(cx, |page, cx| {
                    match read {
                        Ok(settings) => page.fill(&settings, first, window, cx),
                        Err(err) => toast_error(window, cx, &err),
                    }
                    page.loaded = true;
                    cx.notify();
                })
            });
        })
        .detach();
    }

    // Puts the saved values in the fields still holding the last ones read, or in all of them.
    fn fill(
        &mut self,
        settings: &Settings,
        all: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path: SharedString = settings
            .tf2_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
            .into();
        if all || self.path.read(cx).value() == self.saved_path {
            self.path
                .update(cx, |input, cx| input.set_value(path.clone(), window, cx));
        }
        self.saved_path = path;
        shared_volume(cx).update(cx, |shared, cx| shared.loaded(settings.volume(), cx));
        let hours = settings.sync_cache_hours();
        if all || whole(&self.cache_hours, cx) == self.saved_hours {
            self.cache_hours
                .update(cx, |s, cx| s.set_value(hours as f32, window, cx));
        }
        self.saved_hours = hours;
    }

    // Reads every store's status off the UI thread; only the newest read may land, so a slow
    // read cannot overwrite a later one. A failed read toasts and keeps what was shown.
    fn load_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_generation += 1;
        let generation = self.sync_generation;
        let reading = run_blocking(cx, |s| {
            s.store
                .sync_status()
                .map_err(|e| describe_store_status(&e).into_op("Failed to read store status"))
        });
        cx.spawn_in(window, async move |page, cx| {
            let read = reading.await;
            let _ = page.update_in(cx, |page, window, cx| {
                if page.sync_generation != generation {
                    return;
                }
                match read {
                    Ok(status) => {
                        page.sync_status = Some(status);
                        page.last_failure = None;
                    }
                    Err(err) => {
                        let label = Some(error_label(&err));
                        if page.last_failure != label {
                            toast_error(window, cx, &err);
                            page.last_failure = label;
                        }
                    }
                }
                page.sync_loaded = true;
                cx.notify();
            });
        })
        .detach();
    }

    // One store's card: v5's two status lines left, an outline small Resync right. The status
    // block carries the whole status as its label, with the failure while the kind has one.
    fn sync_card(
        &self,
        (kind, title, description): (StoreKind, &'static str, &'static str),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let status = self
            .sync_status
            .as_ref()
            .and_then(|all| all.iter().find(|s| s.kind() == kind));
        let count = status.map_or(0, StoreSyncStatus::item_count);
        let when = format_synced(status.and_then(StoreSyncStatus::last_synced));
        let hub = sync_hub(cx);
        let (running, failure) = hub
            .as_ref()
            .map(|hub| {
                let hub = hub.read(cx);
                (hub.is_running(kind), hub.failure(kind).map(str::to_owned))
            })
            .unwrap_or((false, None));
        let mut label = format!("{count} items cached. Last synced: {when}");
        if let Some(error) = &failure {
            label.push_str(&format!(". Sync failed: {error}"));
        }
        // The lines take the rest of the row and wrap, so a long failure never pushes Resync out.
        let lines = v_flex()
            .id(ids::settings_sync(kind))
            .aria_label(label)
            .test_support()
            .flex_1()
            .min_w_0()
            .gap_1()
            .text_sm()
            .child(format!("{count} items cached"))
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .child(format!("Last synced: {when}")),
            )
            .children(failure.map(|error| {
                div()
                    .text_color(theme.danger)
                    .child(format!("Sync failed: {error}"))
            }));
        let resync = Button::new(ids::settings_resync(kind))
            .flex_none()
            .outline()
            .label(if running { "Syncing..." } else { "Resync" })
            .disabled(running || hub.is_none())
            .on_click(cx.listener(move |_, _, _, cx| {
                if let Some(hub) = sync_hub(cx) {
                    hub.update(cx, |hub, cx| hub.start(kind, true, cx));
                }
            }));
        let content = h_flex()
            .items_center()
            .justify_between()
            .child(lines)
            .child(resync);
        card(title, description, Some(content.into_any_element()), cx)
    }

    // Asks for a folder and puts it in the input; a cancel leaves the input as it was.
    fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picking = cx.global::<Services>().picker.pick(PickKind::Tf2Folder, cx);
        self.busy = Some(Busy::Browse);
        cx.notify();
        cx.spawn_in(window, async move |page, cx| {
            let picked = picking.await;
            let _ = page.update_in(cx, |page, window, cx| {
                if let Some(path) = picked {
                    let path = path.display().to_string();
                    page.path
                        .update(cx, |input, cx| input.set_value(path, window, cx));
                }
                page.busy = None;
                cx.notify();
            });
        })
        .detach();
    }

    // Runs the detector off the UI thread and fills the input with the first install; with
    // several, a select lists them all. Nothing is saved until Save.
    fn detect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let detecting = run_blocking(cx, |s| {
            (s.detector)().map_err(|e| describe_detect(&e).into_op("Auto-detect failed"))
        });
        self.busy = Some(Busy::Detect);
        cx.notify();
        cx.spawn_in(window, async move |page, cx| {
            let found = detecting.await;
            let _ = page.update_in(cx, |page, window, cx| {
                page.busy = None;
                page.candidates = None;
                cx.notify();
                let installs = match found {
                    Ok(installs) => installs,
                    Err(err) => return toast_error(window, cx, &err),
                };
                let paths: Vec<SharedString> = installs
                    .iter()
                    .map(|i| i.game_dir().display().to_string().into())
                    .collect();
                let Some(first) = paths.first().cloned() else {
                    let manual = "Select your TF2 folder manually";
                    return toast(window, cx, ToastKind::Warning, "TF2 not found", manual);
                };
                page.path
                    .update(cx, |input, cx| input.set_value(first.clone(), window, cx));
                let description = if paths.len() == 1 {
                    first.to_string()
                } else {
                    let count = paths.len();
                    page.candidates = Some(page.candidate_select(paths, window, cx));
                    format!("{count} installs found; choose one below")
                };
                toast(window, cx, ToastKind::Success, "TF2 found", description);
            });
        })
        .detach();
    }

    // The Language select with the current language chosen; confirming a row switches to it.
    fn language_select(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SelectState<Vec<LanguageRow>>> {
        let rows: Vec<LanguageRow> = i18n::LANGUAGES
            .iter()
            .map(|l| LanguageRow {
                code: l.code.into(),
                name: l.name.into(),
            })
            .collect();
        let chosen = rows
            .iter()
            .position(|r| r.code == i18n::current())
            .unwrap_or_default();
        let select =
            cx.new(|cx| SelectState::new(rows, Some(IndexPath::default().row(chosen)), window, cx));
        cx.subscribe_in(&select, window, |page, _, event, window, cx| {
            if let SelectEvent::Confirm(Some(code)) = event {
                page.pick_language(code, window, cx);
            }
        })
        .detach();
        select
    }

    // Switches the whole app at once and saves the choice in the background, as the
    // mature add-ons switch does; a failed save only logs, the switch already happened.
    fn pick_language(&mut self, code: &str, _: &mut Window, cx: &mut Context<Self>) {
        let applied = i18n::apply(code);
        let save = run_blocking(cx, move |s| {
            s.db.set_language(applied)
                .map_err(|e| describe_settings(&e).into_op("Save language"))
        });
        cx.spawn(async move |_, _| {
            if let Err(err) = save.await {
                tracing::warn!("the language setting could not be saved: {err}");
            }
        })
        .detach();
        cx.refresh_windows();
    }

    // A select over `paths` with the first chosen; confirming a row puts its path in the input.
    fn candidate_select(
        &self,
        paths: Vec<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Candidates {
        let rows: Vec<Tf2Candidate> = paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| Tf2Candidate { index, path })
            .collect();
        let select = cx.new(|cx| SelectState::new(rows, Some(IndexPath::default()), window, cx));
        cx.subscribe_in(&select, window, |page, _, event, window, cx| {
            if let SelectEvent::Confirm(Some(path)) = event {
                page.path
                    .update(cx, |input, cx| input.set_value(path.clone(), window, cx));
            }
        })
        .detach();
        select
    }

    /// Validates a non-empty path before any write, so a bad path leaves the database as it
    /// was; an empty one keeps the saved path, as v5 does. A found install then drives `Tf2State`.
    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.path.read(cx).value();
        let path = typed.trim().to_owned();
        let hours = whole(&self.cache_hours, cx).clamp(1, 720);
        let saving = run_blocking(cx, move |s| {
            let failed = |e: SettingsError| describe_settings(&e).into_op("Failed to save");
            let install = if path.is_empty() {
                None
            } else {
                let install = Tf2Install::from_path(Path::new(&path))
                    .map_err(|e| describe_tf2_folder(&e).into_op("Failed to save"))?;
                s.db.set_tf2_path(Some(install.game_dir()))
                    .map_err(failed)?;
                Some(install)
            };
            s.db.set_sync_cache_hours(hours).map_err(failed)?;
            let entry = NewLogEntry::new(LogLevel::Info, "settings", "update", "Settings updated");
            activity::write(s, &entry);
            Ok(install)
        });
        self.busy = Some(Busy::Save);
        cx.notify();
        // The state and the toast go through the app and the window, so they land even if the
        // page is gone; the page only takes its busy flag and its new untouched values.
        let win = window.window_handle();
        cx.spawn(async move |page, cx| {
            let saved = saving.await;
            // The volume goes through its one writer, so Save can't undo a newer drag.
            let saved = match saved {
                Ok(install) => cx
                    .update(|cx| shared_volume(cx).update(cx, |shared, cx| shared.save(cx)))
                    .await
                    .map(|()| install),
                failed => failed,
            };
            let ok = saved.is_ok();
            cx.update(|cx| {
                if let Ok(Some(install)) = &saved {
                    tf2_state(cx).update(cx, |tf2, cx| {
                        *tf2 = Tf2State::Found(install.clone());
                        cx.notify();
                    });
                }
                let _ = win.update(cx, |_, window, cx| match &saved {
                    Ok(_) => toast(window, cx, ToastKind::Success, "Settings saved", ""),
                    Err(err) => toast_error(window, cx, err),
                });
            });
            let _ = page.update(cx, |page, cx| {
                if ok {
                    page.saved_path = typed;
                    page.saved_hours = hours;
                }
                page.busy = None;
                cx.notify();
            });
        })
        .detach();
    }
}

/// v5's `formatDate`: "Never" without a sync, "Unknown" for a time that does not parse, else
/// the local time as "%Y-%m-%d %H:%M".
pub fn format_synced(rfc3339: Option<&str>) -> String {
    let Some(text) = rfc3339 else {
        return "Never".to_owned();
    };
    chrono::DateTime::parse_from_rfc3339(text).map_or_else(
        |_| "Unknown".to_owned(),
        |t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        },
    )
}

/// v5's `formatHours`: "1 hour", "5 hours", "1 day", "7 days", "1d 1h".
pub fn format_hours(hours: u32) -> String {
    let plural = |n: u32| if n == 1 { "" } else { "s" };
    if hours < 24 {
        return format!("{hours} hour{}", plural(hours));
    }
    let (days, rest) = (hours / 24, hours % 24);
    if rest == 0 {
        format!("{days} day{}", plural(days))
    } else {
        format!("{days}d {rest}h")
    }
}

// Saves the moment it flips, outside "Save Settings", as an open log redraws on it.
fn nicknames_switch(cx: &mut App) -> AnyElement {
    let nicknames = LogNicknames::global(cx);
    Switch::new(ids::SETTINGS_LOG_NICKNAMES)
        .checked(nicknames.read(cx).show())
        .label("Show nicknames in match logs")
        .on_click(move |&on, _, cx| nicknames.update(cx, |n, cx| n.set(on, cx)))
        .into_any_element()
}

// Saves the moment it flips, outside "Save Settings", as the add-on tabs relist on it.
fn mature_switch(cx: &mut App) -> AnyElement {
    let mature = MatureAddons::global(cx);
    Switch::new(ids::SETTINGS_MATURE_ADDONS)
        .checked(mature.read(cx).show())
        .label("Show mature add-ons")
        .on_click(move |&on, _, cx| mature.update(cx, |m, cx| m.set(on, cx)))
        .into_any_element()
}

// The Licences card's content: a toggle and, while shown, a bordered list of every notice's
// title over its text, laying out only the rows in view. An invisible layer measures the list's
// text width and font, and a change asks the page to rewrap.
fn notices_content(page: &SettingsPage, cx: &mut Context<SettingsPage>) -> AnyElement {
    let theme = cx.theme();
    let shown = page.show_notices;
    let view = shown.then(|| {
        let rows = page.notice_rows.clone().unwrap_or_else(|| Rc::from([]));
        let muted = theme.muted_foreground;
        let list = uniform_list(
            ids::SETTINGS_NOTICES_VIEW,
            rows.len(),
            move |range, _, _| range.map(|row| notice_row(&rows[row], muted)).collect(),
        )
        .track_scroll(&page.notices_scroll)
        .test_support()
        .w_full()
        .h(px(NOTICE_LIST_HEIGHT))
        .py_3()
        .pl(px(NOTICE_PADDING))
        .pr(BAR_WIDTH);
        let (measure, owner) = (Rc::clone(&page.notice_measure), cx.entity().downgrade());
        let watch = canvas(
            move |bounds, window, _| {
                let width = bounds.size.width - px(NOTICE_PADDING) - BAR_WIDTH;
                let font = window.text_style().font();
                let now = Some((width, font));
                if *measure.borrow() != now {
                    *measure.borrow_mut() = now;
                    window.on_next_frame(move |_, cx| {
                        let _ = owner.update(cx, |_, cx| cx.notify());
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let content = div().relative().text_xs().child(list).child(watch);
        div()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(scroll_area(
                ids::SETTINGS_NOTICES_SCROLLBAR,
                &page.notices_scroll,
                content,
            ))
    });
    v_flex()
        .gap_3()
        .child(
            h_flex().child(
                Button::new(ids::SETTINGS_NOTICES)
                    .outline()
                    .px_3()
                    .label(if shown {
                        "Hide notices"
                    } else {
                        "Show notices"
                    })
                    .on_click(cx.listener(|page, _, _, cx| {
                        page.show_notices = !page.show_notices;
                        cx.notify();
                    })),
            ),
        )
        .children(view)
        .into_any_element()
}

// One notices list row at the list's fixed height: a blank, a semibold title or a muted line.
// A line's text sits in a box as wide as the text itself, so its bounds show what it covers.
fn notice_row(row: &NoticeRow, muted: Hsla) -> AnyElement {
    fn cell<E: Styled>(el: E) -> E {
        (el.h(px(NOTICE_ROW_HEIGHT))
            .line_height(px(NOTICE_ROW_HEIGHT)))
        .whitespace_nowrap()
        .overflow_hidden()
    }
    match row {
        NoticeRow::Gap => cell(div()).into_any_element(),
        NoticeRow::Title(file, title) => cell(
            div()
                .id(ids::settings_notice(file))
                .aria_label(title.clone())
                .test_support(),
        )
        .text_ellipsis()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(title.clone())
        .into_any_element(),
        NoticeRow::Line(file, row, text) => cell(h_flex())
            .text_xs()
            .text_color(muted)
            .child(
                div()
                    .id(ids::settings_notice_line(file, *row))
                    .test_support()
                    .flex_none()
                    .child(text.clone()),
            )
            .into_any_element(),
    }
}

// The Help and feedback card's two links, opened in the player's browser.
fn help_content() -> AnyElement {
    let link = |id: &'static str, text: &'static str, url: &'static str| {
        Button::new(id)
            .outline()
            .px_3()
            .label(text)
            .accessibility_label(text)
            .on_click(move |_, _, cx| cx.open_url(url))
    };
    h_flex()
        .gap_2()
        .flex_wrap()
        .child(link(
            ids::SETTINGS_REPORT_PROBLEM,
            "Report a problem",
            links::REPORT_PROBLEM_URL,
        ))
        .child(link(ids::SETTINGS_HELP, "Help", links::HELP_URL))
        .into_any_element()
}

// A slider's single value as the whole number the label and Save use.
fn whole(slider: &Entity<SliderState>, cx: &App) -> u32 {
    slider.read(cx).value().start().round() as u32
}

// v5's shadcn Card: a bordered, rounded panel with a title, a muted description and content.
fn card(
    title: impl Into<SharedString>,
    description: impl IntoElement,
    content: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .gap_6()
        .p_6()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.group_box)
        .shadow_sm()
        .child(
            v_flex()
                .gap_1p5()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .line_height(relative(1.))
                        .child(title.into()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(description),
                ),
        )
        .children(content)
        .into_any_element()
}

// A text line the tests and the pilot read back by id.
fn labelled(id: &'static str, text: impl Into<SharedString>) -> impl IntoElement {
    let text = text.into();
    div()
        .id(id)
        .aria_label(text.clone())
        .test_support()
        .child(text)
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.split_notices(window);
        // Until the load lands the buttons stay off, so a late load cannot overwrite an edit.
        let busy = self.busy.is_some() || !self.loaded;
        let muted = cx.theme().muted_foreground;
        let volume = whole(&self.volume, cx);
        let hours = whole(&self.cache_hours, cx);

        let version = labelled(
            ids::SETTINGS_VERSION,
            format!("Current version: v{}", env!("CARGO_PKG_VERSION")),
        );
        let language = v_flex()
            .gap_3()
            .child(
                div()
                    .id(ids::SETTINGS_LANGUAGE)
                    .test_support()
                    .child(Select::new(&self.language)),
            )
            .child(div().text_xs().text_color(muted).child(labelled(
                ids::SETTINGS_LANGUAGE_NOTE,
                tr!("settings.language.note"),
            )));
        let installation = v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .id(ids::SETTINGS_PATH)
                            .test_support()
                            .flex_1()
                            .child(Input::new(&self.path)),
                    )
                    .child(
                        Button::new(ids::SETTINGS_BROWSE)
                            .outline()
                            .px_3()
                            .label("Browse")
                            .loading(self.busy == Some(Busy::Browse))
                            .disabled(busy)
                            .on_click(cx.listener(|page, _, window, cx| page.browse(window, cx))),
                    ),
            )
            // A row, so the button keeps its own width as v5's `w-fit` does.
            .child(
                h_flex().child(
                    Button::new(ids::SETTINGS_DETECT)
                        .secondary()
                        .px_3()
                        .label("Auto-detect")
                        .loading(self.busy == Some(Busy::Detect))
                        .disabled(busy)
                        .on_click(cx.listener(|page, _, window, cx| page.detect(window, cx))),
                ),
            )
            .children(self.candidates.as_ref().map(|select| {
                div()
                    .id(ids::SETTINGS_CANDIDATES)
                    .test_support()
                    .child(Select::new(select))
            }));
        let audio = v_flex()
            .gap_2()
            .text_sm()
            .child(labelled(
                ids::SETTINGS_VOLUME_LABEL,
                format!("Volume: {volume}%"),
            ))
            .child(
                div()
                    .id(ids::SETTINGS_VOLUME)
                    .test_support()
                    .child(Slider::new(&self.volume)),
            );
        let cache = v_flex()
            .gap_2()
            .text_sm()
            .child(labelled(
                ids::SETTINGS_CACHE_LABEL,
                format!("Cache duration: {}", format_hours(hours)),
            ))
            .child(
                div()
                    .id(ids::SETTINGS_CACHE)
                    .test_support()
                    .child(Slider::new(&self.cache_hours)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Stores will re-sync automatically when cache expires. Default: 7 days (168 hours)."),
            );

        let content = v_flex()
            .id(ids::page(Page::Settings))
            .test_support()
            .size_full()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .p(CONTENT_PADDING)
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(512.))
                    .mx_auto()
                    .px_6()
                    .gap_6()
                    .child(card(
                        "App Updates",
                        version,
                        updater::view::settings_content(cx),
                        cx,
                    ))
                    .child(card(
                        tr!("settings.language.title"),
                        tr!("settings.language.description"),
                        Some(language.into_any_element()),
                        cx,
                    ))
                    .child(card(
                        "TF2 Installation",
                        "Path to your Team Fortress 2 installation folder",
                        Some(installation.into_any_element()),
                        cx,
                    ))
                    .child(card(
                        "Audio",
                        "Preview volume for hitsounds and killsounds",
                        Some(audio.into_any_element()),
                        cx,
                    ))
                    .child(card(
                        "Store Cache",
                        "How often to re-sync store data",
                        Some(cache.into_any_element()),
                        cx,
                    ))
                    .child(card(
                        "Add-ons",
                        "GameBanana skins, effects, GUIs, textures and game files",
                        Some(mature_switch(cx)),
                        cx,
                    ))
                    .child(card(
                        "logs.tf",
                        "How opened match logs name the players you track",
                        Some(nicknames_switch(cx)),
                        cx,
                    ))
                    // v5 shows the store cards only once their status has been read.
                    .children(
                        self.sync_status
                            .is_some()
                            .then(|| SYNC_CARDS.map(|c| self.sync_card(c, cx)))
                            .into_iter()
                            .flatten(),
                    )
                    // v5's default-size button (h-9 px-4), at its own width.
                    .child(
                        h_flex().child(
                            Button::new(ids::SETTINGS_SAVE)
                                .primary()
                                .h_9()
                                .px_4()
                                .label("Save Settings")
                                .loading(self.busy == Some(Busy::Save))
                                .disabled(busy)
                                .on_click(cx.listener(|page, _, window, cx| page.save(window, cx))),
                        ),
                    )
                    .child(card(
                        "Licences",
                        "TF2Tools uses work and content made by others. These are their licences and credits.",
                        Some(notices_content(self, cx)),
                        cx,
                    ))
                    .child(card(
                        "Help and feedback",
                        "Stuck, or found something broken? Read the help or tell us about it.",
                        Some(help_content()),
                        cx,
                    )),
            );
        scroll_area(ids::page_scrollbar(Page::Settings), &self.scroll, content).size_full()
    }
}
