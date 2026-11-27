//! The Demo Viewer: shows a local TF2 demo's map in 3D inside the Demos.tf page.

use std::cell::{Cell, RefCell};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, AppContext as _, AsyncWindowContext, Bounds, Context, CursorStyle,
    DispatchPhase, Entity, Hsla, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Render, SharedString, Size,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _,
    WeakEntity, Window, canvas, div, px,
};
use tf2tools_core::{
    CameraMode, DemoSnapshot, DownloadPhase, GbMapDownload, GbMapInstallError, GbMapSearchError,
    GbMapSearchProgress, LookSensitivity, MapDownloadError, PlaybackEvent, Tf2Install,
};

use crate::i18n::{format, tr};
use crate::ids;
use crate::kill_icons::LoadedKillIcons;
use crate::local_messages::{describe_gb_map_install, describe_settings};
use crate::map_messages::{describe_gb_map_search, describe_map_download, map_stopped};
use crate::pages::profiles::probe::Tf2Probe;
use crate::picker::PickKind;
use crate::services::{OpError, Services, run_blocking, tf2_state};
use crate::shell::toast::toast_error;
use crate::store_download::{Progress, format_bytes, progress_percent};
use crate::tokio_bridge;
use crate::viewer::load::{DemoScene, LoadError, load_demo_scene, snapshot_time};
use crate::viewer::viewport::DemoViewport;
use crate::weapon_names::LoadedWeaponNames;
use crate::widgets::{Named as _, labelled};

mod bar;
mod camera;
mod killfeed;
mod timeline;

pub use bar::BarPlace;
pub use killfeed::{
    FEED_HOLD_S, FEED_ROWS, FeedLayout, FeedParty, FeedRow, NAME_CAP, NAME_MIN, feed_key,
    feed_layout, feed_on_top, feed_rows, feed_words,
};
pub use timeline::timeline_fraction;

// A demo's file name without its extension, for the title before its map is read.
fn stem(path: &std::path::Path) -> SharedString {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned().into())
        .unwrap_or_default()
}

/// The bar's frame is the page width less this at each side; a bar dragged to an edge can
/// still touch it.
const BAR_MARGIN: Pixels = px(8.);

/// How far the open demo's playback read has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadState {
    /// Still reading, with the whole percent read once the reader has said.
    Reading { percent: Option<u8> },
    /// The tracks are in the viewport.
    Ready,
    /// The read ended without tracks; the still picture stays.
    Failed,
}

/// What the load thread sends the page: the map first, then the reader's events.
enum Load {
    Map(Result<DemoScene, LoadError>),
    Playback(PlaybackEvent),
}

/// Sets its flag when dropped, so a replaced or dropped load tells its reader to stop.
struct CancelGuard(Arc<AtomicBool>);

impl Drop for CancelGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

// A reader's fraction as a whole percent; the nudge keeps 0.29 from reading as 28.
fn whole_percent(read: f32) -> u8 {
    (f64::from(read) * 100.0 + 1e-6).floor().clamp(0.0, 100.0) as u8
}

/// What the page shows under its control bar.
enum View {
    Loading,
    /// The demo's map is being fetched: bytes so far and the announced size. `searching` is
    /// set once serveme.tf lacked it, holding the GameBanana candidate and the count, or 0s.
    Downloading {
        map: String,
        done: u64,
        total: Option<u64>,
        searching: Option<(u8, u8)>,
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
        /// The caption once the tracks play, without the still picture's moment.
        playing_caption: SharedString,
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

/// The Demo Viewer: the chosen demo's map over the whole page, under a floating control bar.
pub struct DemoViewerPage {
    view: View,
    /// The open demo's file stem until its map is known, then the map's name.
    title: SharedString,
    /// Bumped by every resolved pick, so a slower earlier load cannot land over a later one.
    generation: u64,
    /// The page's size from the last frame, which places and caps the bar.
    area: Rc<Cell<Size<Pixels>>>,
    /// The bar's own size from the last frame, which the clamp keeps inside the page.
    bar_size: Rc<Cell<Size<Pixels>>>,
    /// The bar's window top and the camera row's window bounds from the last frame, which move
    /// the players list clear of the bar.
    bar_top: Rc<Cell<Pixels>>,
    chip_row: Rc<Cell<Bounds<Pixels>>>,
    /// The player's place for the bar, as stored; drawing clamps it but never writes back.
    bar: BarPlace,
    /// Set once the player moves or minimises the bar, so a slow stored place cannot undo it.
    bar_touched: bool,
    bar_drag: Option<BarDrag>,
    bar_saves: Rc<RefCell<SerialSaves<String>>>,
    /// The running map download; dropping it aborts the transfer and its partial file.
    download: Option<Task<()>>,
    /// The task receiving the current load's events; dropping it closes their channel.
    reading: Option<Task<()>>,
    /// Stops the current load's reader once replaced or dropped.
    _cancel: Option<CancelGuard>,
    read_state: ReadState,
    /// How fast a drag turns the camera; every new viewport starts with it.
    look: LookSensitivity,
    /// Set once the player picks a sensitivity, so a slow stored one cannot undo it.
    look_touched: bool,
    look_saves: Rc<RefCell<SerialSaves<LookSensitivity>>>,
    look_slider: Entity<SliderState>,
    options_open: bool,
    players_open: bool,
    _look_watch: [Subscription; 2],
    /// The open demo's timeline, once its tracks arrive.
    timeline: Option<timeline::Timeline>,
    /// The disabled slider the row shows until then.
    timeline_placeholder: Entity<SliderState>,
    kill_icons: Entity<LoadedKillIcons>,
    weapon_names: Entity<LoadedWeaponNames>,
    _feed_watch: [Subscription; 2],
}

impl DemoViewerPage {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        let timeline_placeholder = cx.new(|_| SliderState::new().min(0.).max(1.).step(1.));
        // The game's kill icons and weapon names landing redraw the feed.
        let kill_icons = LoadedKillIcons::global(cx);
        let weapon_names = LoadedWeaponNames::global(cx);
        let feed_watch = [
            cx.observe(&kill_icons, |_, _, cx| cx.notify()),
            cx.observe(&weapon_names, |page: &mut Self, names, cx| {
                if let Some(timeline) = page.timeline.as_mut() {
                    timeline.reword_feed(names.read(cx).names());
                }
                cx.notify();
            }),
        ];
        Self {
            view: View::Loading,
            title: SharedString::default(),
            generation: 0,
            area: Rc::default(),
            bar_size: Rc::default(),
            bar_top: Rc::default(),
            chip_row: Rc::default(),
            bar: BarPlace::DEFAULT,
            bar_touched: false,
            bar_drag: None,
            bar_saves: Rc::default(),
            download: None,
            reading: None,
            _cancel: None,
            read_state: ReadState::Reading { percent: None },
            look,
            look_touched: false,
            look_saves: Rc::default(),
            look_slider,
            options_open: false,
            players_open: false,
            _look_watch: look_watch,
            timeline: None,
            timeline_placeholder,
            kill_icons,
            weapon_names,
            _feed_watch: feed_watch,
        }
    }

    /// A viewer loading the demo at `path` at once.
    pub fn open_path(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut page = Self::new(window, cx);
        page.generation = 1;
        page.title = stem(&path);
        page.load(path, 1, false, window, cx);
        page
    }

    /// The open demo's file stem until its map is known, then the map's name.
    pub fn title(&self) -> SharedString {
        self.title.clone()
    }

    /// How far the open demo's playback read has come.
    pub fn read_state(&self) -> ReadState {
        self.read_state
    }

    /// The options popover's look sensitivity slider, in percent.
    pub fn look_slider(&self) -> &Entity<SliderState> {
        &self.look_slider
    }

    /// The timeline's slider over the open demo's ticks, once its tracks arrive.
    pub fn timeline_slider(&self) -> Option<&Entity<SliderState>> {
        self.timeline.as_ref().map(|t| &t.slider)
    }

    /// The viewport over the loaded map, if one shows.
    pub fn viewport(&self) -> Option<&Entity<DemoViewport>> {
        match &self.view {
            View::Loaded { viewport, .. } => Some(viewport),
            View::Loading
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
                page.title = stem(&path);
                page.load(path, page.generation, false, window, cx);
            });
        })
        .detach();
    }

    // Meshes the demo's map and then reads its playback on a thread of its own, streaming both
    // to the page. A map missing the first time is downloaded and loaded once more; missing
    // again after that, it shows as missing.
    fn load(
        &mut self,
        path: PathBuf,
        generation: u64,
        downloaded: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.view = View::Loading;
        self.timeline = None;
        self.read_state = ReadState::Reading { percent: None };
        cx.notify();
        let install = tf2_state(cx).read(cx).install().cloned();
        let reader = Arc::clone(&cx.global::<Services>().demo_playback);
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = async_channel::unbounded();
        let reading = path.clone();
        let found = install.clone();
        let stop = Arc::clone(&cancel);
        // A parse can take seconds, so it holds a thread of its own rather than an executor's.
        // A thread the OS refuses closes the channel unsent, which reads as an unreadable map.
        if let Err(error) = std::thread::Builder::new()
            .name("demo-viewer-load".into())
            .spawn(move || read_demo(&reading, found.as_ref(), &reader, &stop, &tx))
        {
            tracing::warn!(%error, "the demo viewer could not start its load thread");
        }
        // Replacing the guard and the task cancels the previous load and closes its channel.
        self._cancel = Some(CancelGuard(cancel));
        self.reading = Some(cx.spawn_in(window, async move |page, cx| {
            let map = match rx.recv().await {
                Ok(Load::Map(map)) => map,
                Ok(Load::Playback(_)) | Err(_) => Err(LoadError::MapUnreadable),
            };
            let scene = match map {
                Ok(scene) => scene,
                Err(error) => {
                    let _ = page.update_in(cx, |page, window, cx| {
                        if page.generation != generation {
                            return;
                        }
                        match (error, install) {
                            (LoadError::MapMissing(map), Some(install)) if !downloaded => {
                                page.download_map(path, map, install, generation, window, cx);
                            }
                            (error, _) => {
                                page.view = View::Failed(error.message());
                                cx.notify();
                            }
                        }
                    });
                    return;
                }
            };
            // The map waits here for the still picture, so the viewport opens with its players.
            let mut scene = Some(scene);
            while let Ok(Load::Playback(event)) = rx.recv().await {
                let done = page.update_in(cx, |page, window, cx| {
                    page.on_playback(event, &mut scene, window, cx);
                });
                if done.is_err() {
                    return;
                }
            }
            let _ = page.update_in(cx, |page, window, cx| page.read_ended(scene, window, cx));
        }));
    }

    // One reader event: the still picture opens the viewport, progress and the tracks follow.
    fn on_playback(
        &mut self,
        event: PlaybackEvent,
        scene: &mut Option<DemoScene>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PlaybackEvent::Snapshot(snapshot) => {
                // Missing players still leave a map to fly around, so the error only reaches the log.
                let snapshot = snapshot
                    .inspect_err(|error| {
                        tracing::warn!(%error, "the demo viewer could not read the players");
                    })
                    .ok();
                if let Some(scene) = scene.take() {
                    self.show(scene, snapshot.as_ref(), window, cx);
                }
            }
            PlaybackEvent::Progress(read) => {
                let now = ReadState::Reading {
                    percent: Some(whole_percent(read)),
                };
                if matches!(self.read_state, ReadState::Reading { .. }) && self.read_state != now {
                    self.read_state = now;
                    cx.notify();
                }
            }
            PlaybackEvent::Tracks(tracks) => {
                if let Some(scene) = scene.take() {
                    self.show(scene, None, window, cx);
                }
                if let Some(viewport) = self.viewport().cloned() {
                    viewport.update(cx, |v, cx| v.set_tracks(Arc::clone(&tracks), cx));
                    self.start_timeline(&tracks, &viewport, window, cx);
                }
                self.read_state = ReadState::Ready;
                cx.notify();
            }
            PlaybackEvent::TracksFailed(error) => {
                tracing::warn!(%error, "the demo viewer could not read the playback");
                self.read_state = ReadState::Failed;
                cx.notify();
            }
            _ => {}
        }
    }

    // The stream closed: a map still waiting shows without players, and a read with no final
    // word counts as failed.
    fn read_ended(
        &mut self,
        scene: Option<DemoScene>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(scene) = scene {
            self.show(scene, None, window, cx);
        }
        if matches!(self.read_state, ReadState::Reading { .. }) {
            self.read_state = ReadState::Failed;
            cx.notify();
        }
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
            searching: None,
        };
        cx.notify();
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let store = cx.global::<Services>().store.clone();
        let (tx, mut rx) = tokio::sync::watch::channel((0u64, None::<u64>));
        let name = map.clone();
        let failed_map = map.clone();
        let searching = (path.clone(), install.clone());
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
            let absent = page.update_in(cx, |page, window, cx| {
                if page.generation != generation {
                    return false;
                }
                // A 404 means serveme.tf lacks the map, so GameBanana is searched next; any
                // other failure may pass, so it keeps its reason and a Retry.
                let (failure, absent) = match joined {
                    // A map that landed meanwhile, say from the Maps page, is simply shown.
                    Ok(Ok(()))
                    | Ok(Err(
                        MapDownloadError::AlreadyDownloaded { .. }
                        | MapDownloadError::InGame { .. },
                    )) => {
                        page.load(path, generation, true, window, cx);
                        return false;
                    }
                    Ok(Err(e)) => (
                        describe_map_download(&e),
                        matches!(e, MapDownloadError::Http { status: 404, .. }),
                    ),
                    // Only a panic in the download joins as an error; a dropped task has
                    // no one left to join it.
                    Err(e) => (map_stopped(e.to_string()), false),
                };
                failure.log("map download");
                if !absent {
                    page.view = View::DownloadFailed {
                        path,
                        map: failed_map,
                        detail: failure.message.into(),
                    };
                    cx.notify();
                }
                absent
            });
            if matches!(absent, Ok(true)) {
                let (path, install) = searching;
                search_gamebanana(page, cx, path, map, install, generation).await;
            }
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
        scene: DemoScene,
        snapshot: Option<&DemoSnapshot>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.title = scene.map.clone().into();
        // A snapshot with nobody in it counts as no players read.
        let players = snapshot.filter(|s| !s.players().is_empty());
        let (caption, playing_caption) = match players {
            Some(snapshot) => (
                tr!(
                    "demo_viewer.caption_players",
                    map = scene.map,
                    count = snapshot.players().len(),
                    time = snapshot_time(snapshot.tick(), scene.seconds_per_tick)
                ),
                tr!(
                    "demo_viewer.caption_players_playing",
                    map = scene.map,
                    count = snapshot.players().len()
                ),
            ),
            None => {
                let caption = tr!("demo_viewer.caption", map = scene.map);
                (caption.clone(), caption)
            }
        };
        let players_note = players.is_none();
        let renderer = (cx.global::<Services>().renderers)();
        let lock = (cx.global::<Services>().pointer_lock)();
        let viewport = cx.new(|cx| {
            DemoViewport::new(
                Arc::new(scene.mesh),
                snapshot,
                renderer,
                lock,
                Arc::new(Instant::now),
                self.look.scale(),
                window,
                cx,
            )
        });
        // A renderer that stops leaves nothing to look at, so its viewport and the read go too.
        let watch = cx.observe(&viewport, |page, viewport, cx| {
            if viewport.read(cx).failure().is_some() {
                page.view = View::Failed(tr!("demo_viewer.error.unsupported"));
                page._cancel = None;
                page.reading = None;
                cx.notify();
            }
        });
        self.timeline = None;
        self.view = View::Loaded {
            caption,
            playing_caption,
            players_note,
            viewport,
            _failure_watch: watch,
        };
        cx.notify();
    }
}

impl Render for DemoViewerPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_feed_locale(cx);
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
        let downloading = |map: &str,
                           done: u64,
                           total: Option<u64>,
                           searching: Option<(u8, u8)>| {
            let progress = Some(Progress {
                phase: DownloadPhase::Downloading,
                downloaded: done,
                total,
            });
            let words = match searching {
                None => tr!("demo_viewer.downloading", map = map),
                Some((0, _)) => tr!("demo_viewer.searching", map = map),
                Some((n, of)) => tr!("demo_viewer.searching_candidate", map = map, n = n, of = of),
            };
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
                playing_caption,
                players_note,
                viewport,
                ..
            } => {
                // Once the tracks play, the timeline's clock holds the time.
                let caption = if self.read_state == ReadState::Ready {
                    playing_caption
                } else {
                    caption
                };
                (
                    viewport.clone().into_any_element(),
                    Some((caption.clone(), *players_note)),
                )
            }
            View::Loading => status(tr!("demo_viewer.loading")),
            View::Downloading {
                map,
                done,
                total,
                searching,
            } => downloading(map, *done, *total, *searching),
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
        // The playback row, the camera row under it and their notes show under a viewport
        // until its tracks fail.
        let failed = self.read_state == ReadState::Failed;
        let (row, camera_row) = match self.viewport().cloned() {
            Some(viewport) if !collapsed && !failed => (
                Some(self.playback_row(&viewport, cx)),
                Some(self.camera_chip(&viewport, cx)),
            ),
            _ => (None, None),
        };
        let reading = match self.read_state {
            ReadState::Reading { percent: Some(p) } => {
                Some(tr!("demo_viewer.playback.reading", percent = p))
            }
            ReadState::Reading { percent: None } => {
                Some(tr!("demo_viewer.playback.reading_no_percent"))
            }
            ReadState::Ready | ReadState::Failed => None,
        };
        // A failed read only needs its own note when the still picture showed players.
        let had_players = caption.as_ref().is_some_and(|(_, note)| !note);
        let playback_note = self
            .timeline
            .as_ref()
            .and_then(|t| t.cut_note())
            .or_else(|| (failed && had_players).then(|| tr!("demo_viewer.playback.tracks_failed")));

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
        let bar_top = Rc::clone(&self.bar_top);
        let bar_watch = canvas(
            move |inner, window, _| {
                // The watcher fills the inside of the 1 px border; the bar is that much bigger.
                let outer = inner.dilate(px(1.));
                if bar_size.get() != outer.size || bar_top.get() != outer.top() {
                    bar_size.set(outer.size);
                    bar_top.set(outer.top());
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        // Following a player, the hint names the follow controls instead of the flight keys.
        let following = self
            .viewport()
            .is_some_and(|v| v.read(cx).camera_state().mode != CameraMode::FreeFly);
        let hint = match following {
            true => tr!("demo_viewer.hint_follow"),
            false => tr!("demo_viewer.hint"),
        };
        let at = self.bar.clamp(self.area.get(), self.bar_size.get());
        // The bar's room is a fixed-width frame, so where it sits never changes how it wraps.
        let room = (self.area.get().width - BAR_MARGIN * 2.).max(px(0.));
        let theme = cx.theme();
        let first_row = h_flex()
            .min_w_0()
            .items_center()
            .gap_3()
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
                        .when_some(reading.clone(), |words, reading| {
                            words.child(line(ids::DEMO_VIEWER_READING, reading, Some(muted), true))
                        })
                        .when_some(playback_note.clone(), |words, note| {
                            words.child(line(
                                ids::DEMO_VIEWER_PLAYBACK_NOTE,
                                note,
                                Some(muted),
                                true,
                            ))
                        })
                        .child(line(ids::DEMO_VIEWER_HINT, hint.clone(), Some(muted), true));
                    bar.child(words)
                })
                .child(div().flex_none().child(open))
                .when_some(options, |bar, options| {
                    bar.child(div().flex_none().child(options))
                })
            })
            .child(div().flex_none().child(collapse));
        let bar = v_flex()
            .id(ids::DEMO_VIEWER_BAR)
            .test_support()
            .occlude()
            .max_w_full()
            .min_w_0()
            .gap_1()
            .pl_2()
            .pr_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .shadow_md()
            .child(bar_watch)
            .child(first_row)
            .children(row)
            .children(camera_row);
        let frame = div()
            .absolute()
            .left(at.x)
            .top(at.y)
            .w(room)
            .flex()
            .items_start()
            .child(bar);
        div()
            .id(ids::DEMO_VIEWER)
            .test_support()
            .relative()
            .size_full()
            .child(area_watch)
            .child(body)
            .children(self.killfeed(cx))
            .child(frame)
    }
}

// The load thread's body: the map first, then, only for a map that loaded, the reader's events
// until it ends or the page stops listening. A panic in either is caught; one in the map is an
// unreadable map, one in the reader simply ends the stream.
fn read_demo(
    dem: &std::path::Path,
    install: Option<&Tf2Install>,
    reader: &crate::viewer::PlaybackReader,
    cancel: &AtomicBool,
    tx: &async_channel::Sender<Load>,
) {
    let map = crate::crash::catching(|| load_demo_scene(dem, install)).unwrap_or_else(|payload| {
        let message = crate::crash::panic_text(&*payload);
        tracing::warn!("the demo viewer's map loader panicked: {message}");
        Err(LoadError::MapUnreadable)
    });
    let loaded = map.is_ok();
    if tx.try_send(Load::Map(map)).is_err() || !loaded {
        return;
    }
    let mut emit = |event| match tx.try_send(Load::Playback(event)) {
        Ok(()) => ControlFlow::Continue(()),
        Err(_) => ControlFlow::Break(()),
    };
    if let Err(payload) = crate::crash::catching(|| reader(dem, cancel, &mut emit)) {
        let message = crate::crash::panic_text(&*payload);
        tracing::warn!("the demo viewer's playback reader panicked: {message}");
    }
}

// A found GameBanana download that removes its scratch dir on a plain thread when dropped
// unused, so no cancel or closed window leaves that disk work on the UI thread.
struct OffUi(Option<GbMapDownload>);

impl OffUi {
    fn new(download: GbMapDownload) -> Self {
        Self(Some(download))
    }

    fn get(&self) -> Option<&GbMapDownload> {
        self.0.as_ref()
    }

    fn into_inner(mut self) -> Option<GbMapDownload> {
        self.0.take()
    }
}

impl Drop for OffUi {
    fn drop(&mut self) {
        if let Some(download) = self.0.take() {
            // If the OS refuses a thread, the download drops here instead of a panic
            // inside a Drop.
            let _ = std::thread::Builder::new().spawn(move || drop(download));
        }
    }
}

// Searches GameBanana for `map` with live progress, installs the match and loads the demo
// again. The match travels as an `OffUi`, so an unwanted download is never dropped here.
async fn search_gamebanana(
    page: WeakEntity<DemoViewerPage>,
    cx: &mut AsyncWindowContext,
    path: PathBuf,
    map: String,
    install: Tf2Install,
    generation: u64,
) {
    let (tx, mut rx) = tokio::sync::watch::channel(None::<GbMapSearchProgress>);
    let Ok(job) = page.update(cx, |page, cx| {
        if let View::Downloading { searching, .. } = &mut page.view {
            *searching = Some((0, 0));
            cx.notify();
        }
        let store = cx.global::<Services>().store.clone();
        let name = map.clone();
        tokio_bridge::spawn(cx, async move {
            let report = move |p: GbMapSearchProgress| {
                tx.send_replace(Some(p));
            };
            store.find_gb_map(&name, &report).await.map(OffUi::new)
        })
    }) else {
        return;
    };
    while rx.changed().await.is_ok() {
        let Some(now) = *rx.borrow_and_update() else {
            continue;
        };
        let _ = page.update(cx, |page, cx| {
            if page.generation != generation {
                return;
            }
            if let View::Downloading {
                done,
                total,
                searching,
                ..
            } = &mut page.view
            {
                (*done, *total) = (now.download.downloaded(), now.download.total());
                *searching = Some((now.candidate, now.of));
                cx.notify();
            }
        });
    }
    let failed = match job.await {
        Ok(Ok(found)) => {
            let current = page
                .update(cx, |page, _| page.generation == generation)
                .unwrap_or(false);
            if !current {
                return;
            }
            Ok(found)
        }
        Ok(Err(e)) => Err(Ok(e)),
        Err(e) => Err(Err(e)),
    };
    let found = match failed {
        Ok(found) => found,
        Err(failed) => {
            let _ = page.update(cx, |page, cx| {
                if page.generation != generation {
                    return;
                }
                // No match or an unsynced catalogue is a dead end with no Retry; a failure a
                // retry may fix keeps its reason and a Retry, which starts again from serveme.tf.
                page.view = match failed {
                    Ok(GbMapSearchError::NoMatch { .. } | GbMapSearchError::BadName { .. }) => {
                        View::Failed(tr!("demo_viewer.error.not_on_gamebanana", map = map))
                    }
                    Ok(GbMapSearchError::NotSynced) => {
                        View::Failed(tr!("demo_viewer.error.gb_not_synced", map = map))
                    }
                    failed => {
                        let failure = match failed {
                            Ok(e) => describe_gb_map_search(&e),
                            Err(e) => map_stopped(e.to_string()),
                        };
                        failure.log("GameBanana map search");
                        View::DownloadFailed {
                            path,
                            map,
                            detail: failure.message.into(),
                        }
                    }
                };
                cx.notify();
            });
            return;
        }
    };

    let Some((gb_id, file_id)) = found.get().map(|d| (d.gb_id(), d.file_id())) else {
        return;
    };
    let Ok(installing) = cx.update(|_, cx| {
        let probe = Tf2Probe::get(cx);
        let chosen: Vec<String> = found
            .get()
            .into_iter()
            .flat_map(|d| d.maps())
            .take(1)
            .map(|m| m.name().to_owned())
            .collect();
        run_blocking(cx, move |s| {
            // into_inner consumes the only handle, so an empty one never reaches here.
            let found = found
                .into_inner()
                .ok_or_else(|| OpError::Internal(std::any::type_name::<OffUi>().to_owned()))?;
            Ok(s.library.install_gb_map(found, &chosen, &install, &*probe))
        })
    }) else {
        return;
    };
    let installed = installing.await;
    let _ = page.update_in(cx, |page, window, cx| {
        if page.generation != generation {
            return;
        }
        // A map that landed meanwhile counts as installed; the demo loads either way.
        let failure = match installed {
            Ok(Ok(_)) => {
                tracing::info!(%gb_id, file_id, "installed {map} from GameBanana for a demo");
                return page.load(path, generation, true, window, cx);
            }
            Ok(Err(GbMapInstallError::AlreadyPresent { .. })) => {
                return page.load(path, generation, true, window, cx);
            }
            Ok(Err(e)) => {
                let failure = describe_gb_map_install(&e);
                failure.log("GameBanana map install");
                failure.message
            }
            Err(e) => {
                let failure = map_stopped(e.to_string());
                failure.log("GameBanana map install");
                failure.message.into()
            }
        };
        page.view = View::DownloadFailed {
            path,
            map,
            detail: failure,
        };
        cx.notify();
    });
}
