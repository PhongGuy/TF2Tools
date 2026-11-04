//! v5's Settings page: the app version with update status and actions, the TF2 path with Browse and
//! Auto-detect, the preview volume, the store cache duration, one sync card per store with Resync, and Save.
//! Below them, a Licences card shows the third-party notices the app carries.

use std::path::Path;

use gpui_kit::component::IndexPath;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::searchable_list::SearchableListItem;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, div,
    px, relative,
};

use tf2tools_core::{LogLevel, NewLogEntry, SettingsError, StoreKind, StoreSyncStatus, Tf2Install};

use crate::activity;
use crate::audio::volume::{bind_slider, shared_volume};
use crate::ids;
use crate::local_messages::{
    describe_detect, describe_settings, describe_store_status, describe_tf2_folder,
};
use crate::notices::{self, Notice};
use crate::picker::PickKind;
use crate::services::{Services, Tf2State, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, error_label, toast, toast_error};
use crate::sync::{SyncEvent, SyncHub, sync_hub};
use crate::updater::{self, updater};
use crate::widgets::scroll::scroll_area;

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

/// The Settings form. Built fresh on each visit, it loads the saved settings on creation.
pub struct SettingsPage {
    path: Entity<InputState>,
    volume: Entity<SliderState>,
    cache_hours: Entity<SliderState>,
    /// The install select, present only after Auto-detect found several installs.
    candidates: Option<Candidates>,
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
    scroll: ScrollHandle,
    notices_scroll: ScrollHandle,
}

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
const SYNC_CARDS: [(StoreKind, &str, &str); 5] = [
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
        "Curated HUDs from mastercomfig/hud-db",
    ),
    (
        StoreKind::Addons,
        "Addon Store",
        "Mastercomfig performance and QoL addons",
    ),
];

impl SettingsPage {
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
        // The App Updates card draws the updater's state, so every change redraws the page.
        let updates = updater(cx).map(|u| cx.observe(&u, |_, _, cx| cx.notify()));
        let mut page = Self {
            path,
            volume,
            cache_hours,
            candidates: None,
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
            scroll: ScrollHandle::new(),
            notices_scroll: ScrollHandle::new(),
        };
        page.load(window, cx);
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

    pub fn path_input(&self) -> &Entity<InputState> {
        &self.path
    }

    pub fn volume(&self) -> &Entity<SliderState> {
        &self.volume
    }

    pub fn cache_hours(&self) -> &Entity<SliderState> {
        &self.cache_hours
    }

    // Reads the settings in the background, then fills the form; a failed read toasts and
    // leaves the defaults so the form stays usable.
    fn load(&self, window: &mut Window, cx: &mut Context<Self>) {
        let reading = run_blocking(cx, |s| {
            s.db.settings()
                .map_err(|e| describe_settings(&e).into_op("Load settings"))
        });
        cx.spawn_in(window, async move |page, cx| {
            let read = reading.await;
            let _ = page.update_in(cx, |page, window, cx| {
                match read {
                    Ok(settings) => {
                        let path = settings
                            .tf2_path()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        page.path
                            .update(cx, |input, cx| input.set_value(path, window, cx));
                        shared_volume(cx)
                            .update(cx, |shared, cx| shared.loaded(settings.volume(), cx));
                        let hours = settings.sync_cache_hours() as f32;
                        page.cache_hours
                            .update(cx, |s, cx| s.set_value(hours, window, cx));
                    }
                    Err(err) => toast_error(window, cx, &err),
                }
                page.loaded = true;
                cx.notify();
            });
        })
        .detach();
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
        let path = self.path.read(cx).value().trim().to_owned();
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
        // Leaving Settings drops the page mid-save, so the state and the toast go through the
        // app and the window; only the busy flag needs the page.
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

// The Licences card's content: a toggle and, while shown, a scrollable box with each notice's
// title over its full text.
fn notices_content(page: &SettingsPage, cx: &mut Context<SettingsPage>) -> AnyElement {
    let theme = cx.theme();
    let shown = page.show_notices;
    let view = shown.then(|| {
        let view = v_flex()
            .id(ids::SETTINGS_NOTICES_VIEW)
            .test_support()
            .max_h(px(320.))
            .track_scroll(&page.notices_scroll)
            .overflow_y_scroll()
            .gap_4()
            .p_3()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .children(page.notices.iter().map(|notice| {
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .id(ids::settings_notice(notice.file()))
                            .aria_label(SharedString::from(notice.title().to_owned()))
                            .test_support()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(notice.title().to_owned()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(notice.text().to_owned()),
                    )
            }));
        scroll_area(ids::SETTINGS_NOTICES_SCROLLBAR, &page.notices_scroll, view)
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

// A slider's single value as the whole number the label and Save use.
fn whole(slider: &Entity<SliderState>, cx: &App) -> u32 {
    slider.read(cx).value().start().round() as u32
}

// v5's shadcn Card: a bordered, rounded panel with a title, a muted description and content.
fn card(
    title: &'static str,
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
                        .child(title),
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Until the load lands the buttons stay off, so a late load cannot overwrite an edit.
        let busy = self.busy.is_some() || !self.loaded;
        let muted = cx.theme().muted_foreground;
        let volume = whole(&self.volume, cx);
        let hours = whole(&self.cache_hours, cx);

        let version = labelled(
            ids::SETTINGS_VERSION,
            format!("Current version: v{}", env!("CARGO_PKG_VERSION")),
        );
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
                        "TF2Tools uses work made by others. These are their licences.",
                        Some(notices_content(self, cx)),
                        cx,
                    )),
            );
        scroll_area(ids::page_scrollbar(Page::Settings), &self.scroll, content).size_full()
    }
}
