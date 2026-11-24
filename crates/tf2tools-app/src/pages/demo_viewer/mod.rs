//! The Demo Viewer page: opens a local TF2 demo and shows its map in 3D.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, AppContext as _, Context, CursorStyle, DispatchPhase, Entity, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Point, Render, SharedString, Size,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    canvas, div, px,
};
use tf2tools_core::{DownloadPhase, LookSensitivity, MapDownloadError, Tf2Install};

use crate::i18n::{format, tr};
use crate::ids;
use crate::local_messages::describe_settings;
use crate::map_messages::{describe_map_download, map_stopped};
use crate::picker::PickKind;
use crate::services::{OpError, Services, run_blocking, tf2_state};
use crate::shell::route::Page;
use crate::shell::toast::toast_error;
use crate::store_download::{Progress, format_bytes, progress_percent};
use crate::tokio_bridge;
use crate::viewer::load::{LoadError, LoadedDemo, load_demo, snapshot_time};
use crate::viewer::viewport::DemoViewport;
use crate::widgets::{Named as _, labelled};

mod bar;

pub use bar::BarPlace;

/// The bar's frame is the page width less this at each side; a bar dragged to an edge can
/// still touch it.
const BAR_MARGIN: Pixels = px(8.);

/// What the page shows under its control bar.
enum View {
    Empty,
    Loading,
    /// The demo's map is being fetched from serveme.tf: bytes so far and the announced size.
    Downloading {
        map: String,
        done: u64,
        total: Option<u64>,
    },
    Failed(SharedString),
    /// A map download that may work if tried again: what went wrong, and what to retry.
    DownloadFailed {
        path: PathBuf,
        map: String,
        detail: SharedString,
    },
    Loaded {
        /// The map's name, with the players' count and moment when they were read.
        caption: SharedString,
        /// Whether the players could not be read, so a note says so.
        players_note: bool,
        viewport: Entity<DemoViewport>,
        _failure_watch: Subscription,
    },
}

/// A grip drag under way: where the pointer and the bar's centre were when it began, and the
/// place it began from, so a press that moves nothing saves nothing.
#[derive(Clone, Copy)]
struct BarDrag {
    pointer: Point<Pixels>,
    centre: Point<Pixels>,
    start: BarPlace,
}

/// The newest value of one setting waiting to be written, and whether a write is running.
struct SerialSaves<T> {
    latest: Option<T>,
    writing: bool,
}

impl<T> Default for SerialSaves<T> {
    fn default() -> Self {
        Self {
            latest: None,
            writing: false,
        }
    }
}

/// Writes one setting's value off the UI thread.
type SaveFn<T> = fn(&Services, T) -> Result<(), OpError>;

// One writer per setting at a time, each taking the newest value, so quick changes never land
// out of order. A failed write toasts; the value shown stays as the player set it.
fn save_serially<T: Send + 'static>(
    saves: &Rc<RefCell<SerialSaves<T>>>,
    value: T,
    write: SaveFn<T>,
    window: &mut Window,
    cx: &mut Context<DemoViewerPage>,
) {
    let mut queued = saves.borrow_mut();
    queued.latest = Some(value);
    if std::mem::replace(&mut queued.writing, true) {
        return;
    }
    drop(queued);
    let saves = Rc::clone(saves);
    cx.spawn_in(window, async move |_, cx| {
        loop {
            let next = {
                let mut saves = saves.borrow_mut();
                let next = saves.latest.take();
                saves.writing = next.is_some();
                next
            };
            let Some(value) = next else { break };
            let Ok(save) = cx.update(|_, cx| run_blocking(cx, move |s| write(s, value))) else {
                break;
            };
            if let Err(err) = save.await {
                let _ = cx.update(|window, cx| toast_error(window, cx, &err));
            }
        }
    })
    .detach();
}

// A slider value as a sensitivity on its 5 % steps; a NaN reads as the default.
fn look_at(v: f32) -> LookSensitivity {
    if v.is_nan() {
        return LookSensitivity::default();
    }
    let step = f32::from(LookSensitivity::STEP);
    let range = f32::from(LookSensitivity::MIN)..=f32::from(LookSensitivity::MAX);
    let snapped = ((v / step).round() * step).clamp(*range.start(), *range.end());
    LookSensitivity::new(snapped as u16).unwrap_or_default()
}

/// The Demo Viewer page: the chosen demo's map over the whole page, under a floating control bar.
pub struct DemoViewerPage {
    view: View,
    /// Bumped by every resolved pick, so a slower earlier load cannot land over a later one.
    generation: u64,
    /// The page's size from the last frame, which places and caps the bar.
    area: Rc<Cell<Size<Pixels>>>,
    /// The bar's own size from the last frame, which the clamp keeps inside the page.
    bar_size: Rc<Cell<Size<Pixels>>>,
    /// The player's place for the bar, as stored; drawing clamps it but never writes back.
    bar: BarPlace,
    /// Set once the player moves or minimises the bar, so a slow stored place cannot undo it.
    bar_touched: bool,
    bar_drag: Option<BarDrag>,
    bar_saves: Rc<RefCell<SerialSaves<String>>>,
    /// The running map download; dropping it aborts the transfer and its partial file.
    download: Option<Task<()>>,
    /// How fast a drag turns the camera; every new viewport starts with it.
    look: LookSensitivity,
    /// Set once the player picks a sensitivity, so a slow stored one cannot undo it.
    look_touched: bool,
    look_saves: Rc<RefCell<SerialSaves<LookSensitivity>>>,
    look_slider: Entity<SliderState>,
    options_open: bool,
    _look_watch: [Subscription; 2],
}

impl DemoViewerPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A missing, damaged or unreadable place leaves the bar at its default; the stored
        // sensitivity comes in the same read.
        let stored = run_blocking(cx, |s| {
            s.db.settings()
                .map(|st| {
                    let place = st.demo_viewer_bar().and_then(BarPlace::from_setting);
                    (place, st.look_sensitivity())
                })
                .map_err(|e| describe_settings(&e).into_op(tr!("messages.action.load_bar_place")))
        });
        cx.spawn_in(window, async move |page, cx| {
            if let Ok((place, look)) = stored.await {
                let _ = page.update_in(cx, |page, window, cx| {
                    if let Some(place) = place.filter(|_| !page.bar_touched) {
                        page.bar = place;
                    }
                    if !page.look_touched {
                        page.show_look(look, window, cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        let look = LookSensitivity::default();
        let look_slider = cx.new(|_| {
            SliderState::new()
                .min(f32::from(LookSensitivity::MIN))
                .max(f32::from(LookSensitivity::MAX))
                .step(f32::from(LookSensitivity::STEP))
                .default_value(f32::from(look.get()))
        });
        // The percent beside the slider follows a drag; letting go applies and saves it.
        let look_watch = [
            cx.observe(&look_slider, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &look_slider,
                window,
                |page, _, ev: &SliderEvent, window, cx| {
                    if let SliderEvent::Release(v) = ev {
                        page.set_look(look_at(v.start()), window, cx);
                    }
                },
            ),
        ];
        Self {
            view: View::Empty,
            generation: 0,
            area: Rc::default(),
            bar_size: Rc::default(),
            bar: BarPlace::DEFAULT,
            bar_touched: false,
            bar_drag: None,
            bar_saves: Rc::default(),
            download: None,
            look,
            look_touched: false,
            look_saves: Rc::default(),
            look_slider,
            options_open: false,
            _look_watch: look_watch,
        }
    }

    /// The options popover's look sensitivity slider, in percent.
    pub fn look_slider(&self) -> &Entity<SliderState> {
        &self.look_slider
    }

    /// The viewport over the loaded map, if one shows.
    pub fn viewport(&self) -> Option<&Entity<DemoViewport>> {
        match &self.view {
            View::Loaded { viewport, .. } => Some(viewport),
            View::Empty
            | View::Loading
            | View::Downloading { .. }
            | View::Failed(_)
            | View::DownloadFailed { .. } => None,
        }
    }

    /// Asks the player for a demo and loads its map off the UI thread, fetching a missing map
    /// from serveme.tf first. A cancelled pick changes nothing.
    pub fn open_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picking = cx.global::<Services>().picker.pick(PickKind::OpenDemo, cx);
        cx.spawn_in(window, async move |page, cx| {
            let Some(path) = picking.await else { return };
            let _ = page.update_in(cx, |page, window, cx| {
                page.generation += 1;
                page.download = None;
                page.load(path, page.generation, false, window, cx);
            });
        })
        .detach();
    }

    // Reads the demo and meshes its map off the UI thread. A map that is missing the first time
    // is downloaded and loaded once more; missing again after that, it shows as missing.
    fn load(
        &mut self,
        path: PathBuf,
        generation: u64,
        downloaded: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.view = View::Loading;
        cx.notify();
        let install = tf2_state(cx).read(cx).install().cloned();
        let snapshots = Arc::clone(&cx.global::<Services>().demo_snapshots);
        let reading = path.clone();
        let found = install.clone();
        // A panic in the BSP reader is a map that cannot be shown, not a dead app.
        let loading = cx.background_spawn(async move {
            crate::crash::catching(|| load_demo(&reading, found.as_ref(), &snapshots))
                .unwrap_or_else(|payload| {
                    let message = crate::crash::panic_text(&*payload);
                    tracing::warn!("the demo viewer's map loader panicked: {message}");
                    Err(LoadError::MapUnreadable)
                })
        });
        cx.spawn_in(window, async move |page, cx| {
            let loaded = loading.await;
            let _ = page.update_in(cx, |page, window, cx| {
                if page.generation != generation {
                    return;
                }
                match (loaded, install) {
                    (Err(LoadError::MapMissing(map)), Some(install)) if !downloaded => {
                        page.download_map(path, map, install, generation, window, cx);
                    }
                    (loaded, _) => page.show(loaded, window, cx),
                }
            });
        })
        .detach();
    }

    // Runs a failed download again under a new generation; a TF2 install gone since then
    // loads the demo instead, which says so.
    fn retry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let View::DownloadFailed { path, map, .. } = &self.view else {
            return;
        };
        let (path, map) = (path.clone(), map.clone());
        self.generation += 1;
        let generation = self.generation;
        match tf2_state(cx).read(cx).install().cloned() {
            Some(install) => self.download_map(path, map, install, generation, window, cx),
            None => self.load(path, generation, false, window, cx),
        }
    }

    // Fetches `map` into the game with live progress, then loads the demo again. Only the newest
    // byte count is kept, so a slow frame skips counts rather than queueing them.
    fn download_map(
        &mut self,
        path: PathBuf,
        map: String,
        install: Tf2Install,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.view = View::Downloading {
            map: map.clone(),
            done: 0,
            total: None,
        };
        cx.notify();
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let store = cx.global::<Services>().store.clone();
        let (tx, mut rx) = tokio::sync::watch::channel((0u64, None::<u64>));
        let name = map.clone();
        let failed_map = map.clone();
        let job = tokio_bridge::spawn(cx, async move {
            let report = move |done, total| {
                tx.send_replace((done, total));
            };
            store.download_map(&install, &name, &report).await
        });
        self.download = Some(cx.spawn_in(window, async move |page, cx| {
            while rx.changed().await.is_ok() {
                let (now, size) = *rx.borrow_and_update();
                let _ = page.update(cx, |page, cx| {
                    if page.generation != generation {
                        return;
                    }
                    if let View::Downloading { done, total, .. } = &mut page.view {
                        (*done, *total) = (now, size);
                        cx.notify();
                    }
                });
            }
            let joined = job.await;
            let _ = page.update_in(cx, |page, window, cx| {
                if page.generation != generation {
                    return;
                }
                // A 404 means serveme.tf lacks the map, so retrying is pointless; any other
                // failure may pass, so it keeps its reason and a Retry.
                let (failure, absent) = match joined {
                    // A map that landed meanwhile, say from the Maps page, is simply shown.
                    Ok(Ok(()))
                    | Ok(Err(
                        MapDownloadError::AlreadyDownloaded { .. }
                        | MapDownloadError::InGame { .. },
                    )) => return page.load(path, generation, true, window, cx),
                    Ok(Err(e)) => (
                        describe_map_download(&e),
                        matches!(e, MapDownloadError::Http { status: 404, .. }),
                    ),
                    // Only a panic in the download joins as an error; a dropped task has
                    // no one left to join it.
                    Err(e) => (map_stopped(e.to_string()), false),
                };
                failure.log("map download");
                page.view = match absent {
                    true => View::Failed(tr!("demo_viewer.error.not_on_serveme", map = failed_map)),
                    false => View::DownloadFailed {
                        path,
                        map: failed_map,
                        detail: failure.message.into(),
                    },
                };
                cx.notify();
            });
        }));
    }

    // The drag starts from the bar's centre as drawn, so a bar pulled inside the page by the
    // clamp does not jump when grabbed.
    fn start_bar_drag(&mut self, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let size = self.bar_size.get();
        let at = self.bar.clamp(self.area.get(), size);
        let half = Point::new(size.width / 2., size.height / 2.);
        self.bar_touched = true;
        self.bar_drag = Some(BarDrag {
            pointer: ev.position,
            centre: at + half,
            start: self.bar,
        });
        cx.notify();
    }

    // A move with no button held ends the drag, in case its release never reached the window.
    fn drag_bar(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.bar_drag else { return };
        if ev.pressed_button.is_none() {
            self.end_bar_drag(window, cx);
            return;
        }
        let by = ev.position - drag.pointer;
        self.bar = drag.start.dragged(drag.centre, by, self.area.get());
        cx.notify();
    }

    fn end_bar_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.bar_drag.take() else {
            return;
        };
        if self.bar != drag.start {
            self.save_bar(window, cx);
        }
        cx.notify();
    }

    fn toggle_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.bar_touched = true;
        self.bar.collapsed = !self.bar.collapsed;
        self.save_bar(window, cx);
        cx.notify();
    }

    fn save_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        save_serially(
            &self.bar_saves,
            self.bar.to_setting(),
            |s, text| {
                s.db.set_demo_viewer_bar(&text).map_err(|e| {
                    describe_settings(&e).into_op(tr!("messages.action.save_bar_place"))
                })
            },
            window,
            cx,
        );
    }

    // The player's pick: the slider, the shown viewport and later ones follow it, and it saves.
    fn set_look(&mut self, look: LookSensitivity, window: &mut Window, cx: &mut Context<Self>) {
        self.look_touched = true;
        self.show_look(look, window, cx);
        save_serially(
            &self.look_saves,
            look,
            |s, look| {
                s.db.set_look_sensitivity(look).map_err(|e| {
                    describe_settings(&e).into_op(tr!("messages.action.save_look_sensitivity"))
                })
            },
            window,
            cx,
        );
    }

    fn show_look(&mut self, look: LookSensitivity, window: &mut Window, cx: &mut Context<Self>) {
        self.look = look;
        let at = f32::from(look.get());
        self.look_slider
            .update(cx, |st, cx| st.set_value(at, window, cx));
        if let Some(viewport) = self.viewport() {
            viewport.update(cx, |v, _| v.set_look_scale(look.scale()));
        }
        cx.notify();
    }

    // The options button and its popover, opening upward from the bar.
    fn options(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let shown = look_at(self.look_slider.read(cx).value().start());
        let reset = Button::new(ids::DEMO_VIEWER_LOOK_RESET)
            .ghost()
            .xsmall()
            .label(tr!(
                "demo_viewer.look_reset",
                percent = format::percent(u64::from(LookSensitivity::default().get()))
            ))
            .on_click(cx.listener(|page, _, window, cx| {
                page.set_look(LookSensitivity::default(), window, cx);
            }));
        let body = v_flex()
            .w(px(240.))
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr!("demo_viewer.look_sensitivity")),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .id(ids::DEMO_VIEWER_LOOK_SLIDER)
                            .test_support()
                            .flex_1()
                            .child(Slider::new(&self.look_slider)),
                    )
                    .child(labelled(
                        div().w_12().text_xs().text_color(muted),
                        ids::DEMO_VIEWER_LOOK_VALUE,
                        format::percent(u64::from(shown.get())),
                    )),
            )
            .child(div().child(reset));
        let trigger = Button::new(ids::DEMO_VIEWER_OPTIONS)
            .ghost()
            .small()
            .icon(IconName::Settings)
            .named(tr!("demo_viewer.options"));
        let page = cx.entity().downgrade();
        Popover::new(ids::DEMO_VIEWER_OPTIONS_POPOVER)
            .anchor(Anchor::BottomRight)
            .open(self.options_open)
            .on_open_change(move |&open, _, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.options_open = open;
                    cx.notify();
                });
            })
            .trigger(trigger)
            .child(body)
            .into_any_element()
    }

    // Puts a loaded map in a fresh viewport; the old one drops with its renderer.
    fn show(
        &mut self,
        loaded: Result<LoadedDemo, LoadError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.view = match loaded {
            Err(error) => View::Failed(error.message()),
            Ok(loaded) => {
                // A snapshot with nobody in it counts as no players read.
                let players = loaded.snapshot.as_ref().filter(|s| !s.players().is_empty());
                let caption = match players {
                    Some(snapshot) => tr!(
                        "demo_viewer.caption_players",
                        map = loaded.map,
                        count = snapshot.players().len(),
                        time = snapshot_time(snapshot.tick(), loaded.seconds_per_tick)
                    ),
                    None => tr!("demo_viewer.caption", map = loaded.map),
                };
                let players_note = players.is_none();
                let renderer = (cx.global::<Services>().renderers)();
                let lock = (cx.global::<Services>().pointer_lock)();
                let viewport = cx.new(|cx| {
                    DemoViewport::new(
                        Arc::new(loaded.mesh),
                        loaded.snapshot.as_ref(),
                        renderer,
                        lock,
                        Arc::new(Instant::now),
                        self.look.scale(),
                        window,
                        cx,
                    )
                });
                // A renderer that stops leaves nothing to look at, so its viewport goes too.
                let watch = cx.observe(&viewport, |page, viewport, cx| {
                    if viewport.read(cx).failure().is_some() {
                        page.view = View::Failed(tr!("demo_viewer.error.unsupported"));
                        cx.notify();
                    }
                });
                View::Loaded {
                    caption,
                    players_note,
                    viewport,
                    _failure_watch: watch,
                }
            }
        };
        cx.notify();
    }
}

impl Render for DemoViewerPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        // A line of the bar's text column wraps; the status stays on one line.
        let line = |id: &'static str, words: SharedString, muted: Option<Hsla>, wraps: bool| {
            div()
                .id(id)
                .aria_label(words.clone())
                .test_support()
                .flex_none()
                .when(wraps, |el| el.min_w_0())
                .when(!wraps, |el| el.whitespace_nowrap())
                .text_sm()
                .when_some(muted, |el, colour| el.text_color(colour))
                .child(words)
        };
        // The viewport fills the whole page; without one, the status sits in its centre.
        let status = |words: SharedString| {
            let centre = div().size_full().flex().items_center().justify_center();
            let status = line(ids::DEMO_VIEWER_STATUS, words, Some(muted), false);
            (centre.child(status).into_any_element(), None)
        };
        // A download shows its status over a bar and the bytes so far, centred like the status.
        // The label holds only the bytes, since the status already says what is happening.
        let downloading = |map: &str, done: u64, total: Option<u64>| {
            let progress = Some(Progress {
                phase: DownloadPhase::Downloading,
                downloaded: done,
                total,
            });
            let words = tr!("demo_viewer.downloading", map = map);
            let received = match total {
                Some(total) => tr!(
                    "demo_viewer.received",
                    done = format_bytes(done),
                    total = format_bytes(total)
                ),
                None => format_bytes(done).into(),
            };
            let column = v_flex()
                .items_center()
                .gap_2()
                .child(line(ids::DEMO_VIEWER_STATUS, words, Some(muted), false))
                .child(div().w(px(240.)).child(
                    ProgressBar::new(ids::DEMO_VIEWER_PROGRESS).value(progress_percent(progress)),
                ))
                .child(line(
                    ids::DEMO_VIEWER_PROGRESS_LABEL,
                    received,
                    Some(muted),
                    false,
                ));
            let centre = div().size_full().flex().items_center().justify_center();
            (centre.child(column).into_any_element(), None)
        };
        // A download that may still work names the map, its reason, and a Retry beneath.
        let download_failed = |map: &str, detail: SharedString, cx: &mut Context<Self>| {
            let retry = Button::new(ids::DEMO_VIEWER_RETRY)
                .outline()
                .small()
                .label(tr!("widgets.store_error.retry"))
                .on_click(cx.listener(|page, _, window, cx| page.retry(window, cx)));
            let words = tr!("demo_viewer.error.download_failed", map = map);
            let column = v_flex()
                .items_center()
                .gap_2()
                .max_w(px(420.))
                .child(line(ids::DEMO_VIEWER_STATUS, words, Some(muted), false))
                .child(line(
                    ids::DEMO_VIEWER_STATUS_DETAIL,
                    detail,
                    Some(muted),
                    true,
                ))
                .child(retry);
            let centre = div().size_full().flex().items_center().justify_center();
            (centre.child(column).into_any_element(), None)
        };
        let (body, caption) = match &self.view {
            View::Loaded {
                caption,
                players_note,
                viewport,
                ..
            } => (
                viewport.clone().into_any_element(),
                Some((caption.clone(), *players_note)),
            ),
            View::Empty => status(tr!("demo_viewer.empty")),
            View::Loading => status(tr!("demo_viewer.loading")),
            View::Downloading { map, done, total } => downloading(map, *done, *total),
            View::Failed(message) => status(message.clone()),
            View::DownloadFailed { map, detail, .. } => download_failed(map, detail.clone(), cx),
        };
        let collapsed = self.bar.collapsed;
        let open = Button::new(ids::DEMO_VIEWER_OPEN)
            .outline()
            .small()
            .label(tr!("demo_viewer.open"))
            .on_click(cx.listener(|page, _, window, cx| page.open_demo(window, cx)));
        let grip = div()
            .id(ids::DEMO_VIEWER_BAR_GRIP)
            .aria_label(tr!("demo_viewer.bar_grip"))
            .test_support()
            .flex_none()
            .cursor(CursorStyle::OpenHand)
            .text_color(muted)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|page, ev: &MouseDownEvent, _, cx| page.start_bar_drag(ev, cx)),
            )
            .child(Icon::new(IconName::GripVertical).small());
        let (icon, tip) = match collapsed {
            true => (IconName::ChevronUp, tr!("demo_viewer.bar_expand")),
            false => (IconName::ChevronDown, tr!("demo_viewer.bar_collapse")),
        };
        let collapse = Button::new(ids::DEMO_VIEWER_BAR_COLLAPSE)
            .ghost()
            .small()
            .icon(icon)
            .named(tip)
            .on_click(cx.listener(|page, _, window, cx| page.toggle_bar(window, cx)));
        let options = (!collapsed).then(|| self.options(cx));

        // Both watchers keep last frame's sizes and ask for one more frame when either moves,
        // so the bar settles at its place a frame after it first draws.
        let area = Rc::clone(&self.area);
        // While the grip is held, moves and the release anywhere in the window reach the page
        // first, so the bar follows a fast pointer that outruns it.
        let page = cx.entity().downgrade();
        let dragging = self.bar_drag.is_some();
        let area_watch = canvas(
            move |bounds, window, _| {
                if area.get() != bounds.size {
                    area.set(bounds.size);
                    window.request_animation_frame();
                }
            },
            move |_, _, window, _| {
                if !dragging {
                    return;
                }
                window.set_window_cursor_style(CursorStyle::ClosedHand);
                let moved = page.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = moved.update(cx, |page, cx| page.drag_bar(ev, window, cx));
                    }
                });
                window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = page.update(cx, |page, cx| page.end_bar_drag(window, cx));
                    }
                });
            },
        )
        .absolute()
        .size_full();
        let bar_size = Rc::clone(&self.bar_size);
        let bar_watch = canvas(
            move |inner, window, _| {
                // The watcher fills the inside of the 1 px border; the bar is that much bigger.
                let size = inner.dilate(px(1.)).size;
                if bar_size.get() != size {
                    bar_size.set(size);
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let at = self.bar.clamp(self.area.get(), self.bar_size.get());
        // The bar's room is a fixed-width frame, so where it sits never changes how it wraps.
        let room = (self.area.get().width - BAR_MARGIN * 2.).max(px(0.));
        let theme = cx.theme();
        let bar = h_flex()
            .id(ids::DEMO_VIEWER_BAR)
            .test_support()
            .occlude()
            .max_w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .pl_2()
            .pr_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .shadow_md()
            .child(bar_watch)
            .child(grip)
            // Minimised, only the grip and the expand button stay. The caption, note and hint
            // share one column, the only part that shrinks and wraps on a narrow page.
            .when(!collapsed, |bar| {
                bar.when_some(caption.clone(), |bar, (caption, note)| {
                    let words = v_flex()
                        .min_w_0()
                        .flex_shrink(1.)
                        .child(line(ids::DEMO_VIEWER_CAPTION, caption, None, true))
                        .when(note, |words| {
                            let note = tr!("demo_viewer.players_note");
                            words.child(line(
                                ids::DEMO_VIEWER_PLAYERS_NOTE,
                                note,
                                Some(muted),
                                true,
                            ))
                        })
                        .child(line(
                            ids::DEMO_VIEWER_HINT,
                            tr!("demo_viewer.hint"),
                            Some(muted),
                            true,
                        ));
                    bar.child(words)
                })
                .child(div().flex_none().child(open))
                .when_some(options, |bar, options| {
                    bar.child(div().flex_none().child(options))
                })
            })
            .child(div().flex_none().child(collapse));
        let frame = div()
            .absolute()
            .left(at.x)
            .top(at.y)
            .w(room)
            .flex()
            .items_start()
            .child(bar);
        div()
            .id(ids::page(Page::DemosTf))
            .test_support()
            .relative()
            .size_full()
            .child(area_watch)
            .child(body)
            .child(frame)
    }
}
