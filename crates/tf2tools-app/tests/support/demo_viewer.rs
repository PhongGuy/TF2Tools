//! A scripted `SceneRenderer` and a viewport-only window for the demo viewer's headless tests.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyWindowHandle, Bounds, Context, Entity, Pixels, Point, RenderImage, TestAppContext, Window,
    div, px, relative,
};
use tf2tools_app::ids;
use tf2tools_app::pages::demo_viewer::ReadState;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::viewer::pointer::{FallbackLock, LockMode, PointerLock, PointerLockFactory};
use tf2tools_app::viewer::viewport::{Clock, DemoViewport, PlaybackState};
use tf2tools_app::viewer::{
    DrawRequest, FrameStats, PlaybackReader, PlayerBox, RenderEvent, RendererError,
    RendererFactory, SceneRenderer,
};
use tf2tools_core::{
    DemoSnapshot, DemoSnapshotError, DemoTracks, DemoTracksError, Endpoints, LookSensitivity,
    MapMesh, PlaybackEvent, SnapshotPlayer, Team, Tf2Install, TfClass, TrackCut, load_map_mesh,
};

use super::demo_view_fixtures::{BspOptions, bsp, demo_header};
use super::graphics::found;

/// How a stub answers once made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StubAnswer {
    /// `Ready`, then a one-pixel `Frame` for every draw, carrying its seq.
    Frames,
    /// `Failed(NoAdapter)` and nothing else.
    NoAdapter,
    /// `Failed(NoAdapter)`, then the channel closes, as the real worker's does when it stops.
    NoAdapterThenStop,
}

/// A test's view of one stub: what it was sent, whether it still lives, and its event sender.
#[derive(Clone)]
pub struct StubHandle {
    pub draws: Arc<Mutex<Vec<DrawRequest>>>,
    pub scenes: Arc<Mutex<usize>>,
    pub alive: Arc<AtomicBool>,
    pub events: async_channel::Sender<RenderEvent>,
}

impl StubHandle {
    pub fn draws(&self) -> Vec<DrawRequest> {
        self.draws.lock().unwrap().clone()
    }
    pub fn scenes(&self) -> usize {
        *self.scenes.lock().unwrap()
    }
    /// The boxes of each recorded draw, with consecutive draws sharing one `Arc` as one entry.
    pub fn players(&self) -> Vec<Arc<[PlayerBox]>> {
        let mut sent: Vec<Arc<[PlayerBox]>> = Vec::new();
        for draw in self.draws.lock().unwrap().iter() {
            if !sent
                .last()
                .is_some_and(|last| Arc::ptr_eq(last, &draw.players))
            {
                sent.push(Arc::clone(&draw.players));
            }
        }
        sent
    }
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

struct StubRenderer {
    handle: StubHandle,
    answer: StubAnswer,
    receiver: async_channel::Receiver<RenderEvent>,
}

impl Drop for StubRenderer {
    fn drop(&mut self) {
        self.handle.alive.store(false, Ordering::SeqCst);
    }
}

impl SceneRenderer for StubRenderer {
    fn load_scene(&self, _mesh: Arc<MapMesh>) {
        *self.handle.scenes.lock().unwrap() += 1;
    }
    fn draw(&self, request: DrawRequest) {
        let seq = request.seq;
        self.handle.draws.lock().unwrap().push(request);
        if self.answer == StubAnswer::Frames {
            let frame = RenderEvent::Frame {
                image: one_pixel(),
                seq,
                stats: FrameStats::default(),
            };
            let _ = self.handle.events.try_send(frame);
        }
    }
    fn events(&self) -> async_channel::Receiver<RenderEvent> {
        self.receiver.clone()
    }
}

pub fn one_pixel() -> Arc<RenderImage> {
    Arc::new(RenderImage::new([image::Frame::new(
        image::RgbaImage::new(1, 1),
    )]))
}

pub fn stub(answer: StubAnswer) -> (Box<dyn SceneRenderer>, StubHandle) {
    let (events, receiver) = async_channel::unbounded();
    let first = match answer {
        StubAnswer::Frames => RenderEvent::Ready,
        StubAnswer::NoAdapter | StubAnswer::NoAdapterThenStop => {
            RenderEvent::Failed(RendererError::NoAdapter)
        }
    };
    events.try_send(first).unwrap();
    // A closed async-channel still hands out what it holds, so the viewport sees Failed first.
    if answer == StubAnswer::NoAdapterThenStop {
        events.close();
    }
    let handle = StubHandle {
        draws: Arc::default(),
        scenes: Arc::default(),
        alive: Arc::new(AtomicBool::new(true)),
        events,
    };
    let renderer = StubRenderer {
        handle: handle.clone(),
        answer,
        receiver,
    };
    (Box::new(renderer), handle)
}

/// A factory making `answer` stubs, and every handle it has made so far.
pub fn stub_factory(answer: StubAnswer) -> (RendererFactory, Arc<Mutex<Vec<StubHandle>>>) {
    let made = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&made);
    let factory: RendererFactory = Arc::new(move || {
        let (renderer, handle) = stub(answer);
        record.lock().unwrap().push(handle);
        renderer
    });
    (factory, made)
}

/// One call a viewport made on its pointer lock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LockCall {
    Lock,
    Unlock,
}

/// A test's view of one fake lock: the calls it got and the motions it will hand out.
#[derive(Clone)]
pub struct FakeLockHandle {
    pub calls: Arc<Mutex<Vec<LockCall>>>,
    /// Motions `motion()` hands out, front first; empty means `None`.
    pub motions: Arc<Mutex<VecDeque<Point<Pixels>>>>,
}

impl FakeLockHandle {
    pub fn calls(&self) -> Vec<LockCall> {
        self.calls.lock().unwrap().clone()
    }
    pub fn push_motion(&self, by: Point<Pixels>) {
        self.motions.lock().unwrap().push_back(by)
    }
}

// Unlocks only a held lock, as the contract's idempotent `unlock` asks, so a drop after a
// finished drag records nothing.
struct FakeLock {
    handle: FakeLockHandle,
    mode: LockMode,
    locked: bool,
}

impl PointerLock for FakeLock {
    fn lock(&mut self, _: Point<Pixels>, _: Point<Pixels>, _: &Window) -> LockMode {
        self.locked = true;
        self.handle.calls.lock().unwrap().push(LockCall::Lock);
        self.mode
    }
    fn motion(&mut self, _: &Window) -> Option<Point<Pixels>> {
        self.handle.motions.lock().unwrap().pop_front()
    }
    fn unlock(&mut self) {
        if std::mem::take(&mut self.locked) {
            self.handle.calls.lock().unwrap().push(LockCall::Unlock);
        }
    }
}

/// A fake lock that takes hold as `mode`, and its handle.
pub fn fake_lock(mode: LockMode) -> (Box<dyn PointerLock>, FakeLockHandle) {
    let handle = FakeLockHandle {
        calls: Arc::default(),
        motions: Arc::default(),
    };
    let lock = FakeLock {
        handle: handle.clone(),
        mode,
        locked: false,
    };
    (Box::new(lock), handle)
}

/// A clock that moves 16 ms on every read, so each render is one 60 fps frame.
pub fn frame_clock() -> Clock {
    let now = Arc::new(Mutex::new(Instant::now()));
    Arc::new(move || {
        let mut t = now.lock().unwrap();
        *t += Duration::from_millis(16);
        *t
    })
}

pub struct ViewportHost(pub Entity<DemoViewport>);

// The viewport fills the window's top-left quarter, so a drag can leave it and still be in the
// window, and a window resize still resizes it.
impl Render for ViewportHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
                .w(relative(0.5))
                .h(relative(0.5))
                .child(self.0.clone()),
        )
    }
}

/// A 1200x900 window holding only a viewport, in its top-left quarter, over the synthetic map,
/// drawn by an `answer` stub.
pub fn viewport_window(
    cx: &mut TestAppContext,
    answer: StubAnswer,
) -> (AnyWindowHandle, Entity<DemoViewport>, StubHandle) {
    viewport_window_with(cx, answer, None, fake_lock(LockMode::Grabbed).0)
}

/// `viewport_window` on a fake lock that takes hold as `mode`, with the lock's handle.
pub fn viewport_window_locked(
    cx: &mut TestAppContext,
    mode: LockMode,
) -> (
    AnyWindowHandle,
    Entity<DemoViewport>,
    StubHandle,
    FakeLockHandle,
) {
    let (lock, handle) = fake_lock(mode);
    let (win, viewport, stub) = viewport_window_with(cx, StubAnswer::Frames, None, lock);
    (win, viewport, stub, handle)
}

/// `viewport_window`, with `snapshot`'s players handed to the viewport and `lock` as its pointer lock.
pub fn viewport_window_with(
    cx: &mut TestAppContext,
    answer: StubAnswer,
    snapshot: Option<DemoSnapshot>,
    lock: Box<dyn PointerLock>,
) -> (AnyWindowHandle, Entity<DemoViewport>, StubHandle) {
    let opened = viewport_window_unsettled(cx, answer, snapshot, lock);
    super::settle(cx, opened.0);
    opened
}

/// `viewport_window_with` straight after the window opens: nothing has run or rendered since,
/// so a test can drive the frame loop itself without the forced re-render `settle` does.
pub fn viewport_window_unsettled(
    cx: &mut TestAppContext,
    answer: StubAnswer,
    snapshot: Option<DemoSnapshot>,
    lock: Box<dyn PointerLock>,
) -> (AnyWindowHandle, Entity<DemoViewport>, StubHandle) {
    super::init(cx);
    let mesh = Arc::new(load_map_mesh(&bsp(&BspOptions::default())).unwrap());
    let (renderer, handle) = stub(answer);
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(super::window_options(), cx, |window, cx| {
                let viewport = cx.new(|cx| {
                    DemoViewport::new(
                        mesh,
                        snapshot.as_ref(),
                        renderer,
                        lock,
                        frame_clock(),
                        1.0,
                        window,
                        cx,
                    )
                });
                cx.new(|_| ViewportHost(viewport))
            })
        })
        .unwrap();
    let viewport = cx.update(|cx| host.read(cx).0.clone());
    (win, viewport, handle)
}

/// A shell on the Demos.tf list whose picker answers `picks` and whose renderers are `answer` stubs.
pub struct DemoViewerShell {
    pub s: super::ShellWindow,
    pub picker: Arc<ScriptedPicker>,
    pub stubs: Arc<Mutex<Vec<StubHandle>>>,
}

pub fn demo_viewer_shell(
    cx: &mut TestAppContext,
    env: &super::Env,
    state: Tf2State,
    picks: Vec<PathBuf>,
    answer: StubAnswer,
) -> DemoViewerShell {
    demo_viewer_shell_with(cx, env, state, picks, answer, None)
}

/// `demo_viewer_shell`, with `playback` as the reader when given (the core reader otherwise).
pub fn demo_viewer_shell_with(
    cx: &mut TestAppContext,
    env: &super::Env,
    state: Tf2State,
    picks: Vec<PathBuf>,
    answer: StubAnswer,
    playback: Option<PlaybackReader>,
) -> DemoViewerShell {
    demo_viewer_shell_in(cx, env, state, picks, answer, playback, None, None, None)
}

/// A lock factory whose fake locks grab and share the one handle returned beside it.
fn fake_locks() -> (PointerLockFactory, FakeLockHandle) {
    let handle = FakeLockHandle {
        calls: Arc::default(),
        motions: Arc::default(),
    };
    let shared = handle.clone();
    let locks: PointerLockFactory = Arc::new(move || {
        Box::new(FakeLock {
            handle: shared.clone(),
            mode: LockMode::Grabbed,
            locked: false,
        })
    });
    (locks, handle)
}

/// `demo_viewer_shell` whose viewports all take fake locks that grab and share one handle,
/// with `stored` saved as the look sensitivity before the page opens.
pub fn demo_viewer_shell_locked(
    cx: &mut TestAppContext,
    env: &super::Env,
    state: Tf2State,
    picks: Vec<PathBuf>,
    stored: Option<LookSensitivity>,
) -> (DemoViewerShell, FakeLockHandle) {
    let (locks, handle) = fake_locks();
    let w = demo_viewer_shell_in(
        cx,
        env,
        state,
        picks,
        StubAnswer::Frames,
        None,
        None,
        Some(locks),
        stored,
    );
    (w, handle)
}

/// `demo_viewer_shell` with TF2 found at the env's game and the store's serveme.tf map
/// downloads served by `server` at `/fastdl/maps/<name>.bsp`.
pub fn demo_viewer_shell_served(
    cx: &mut TestAppContext,
    env: &super::Env,
    server: &super::StoreServer,
    picks: Vec<PathBuf>,
    answer: StubAnswer,
) -> DemoViewerShell {
    // The download crosses the tokio bridge, so the test executor may park on it.
    cx.executor().allow_parking();
    let uri = server.uri();
    let endpoints = server
        .endpoints()
        .with_serveme(format!("{uri}/serveme"))
        .with_map_downloads(format!("{uri}/fastdl/maps"));
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    demo_viewer_shell_in(
        cx,
        env,
        state,
        picks,
        answer,
        None,
        Some(endpoints),
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn demo_viewer_shell_in(
    cx: &mut TestAppContext,
    env: &super::Env,
    state: Tf2State,
    picks: Vec<PathBuf>,
    answer: StubAnswer,
    playback: Option<PlaybackReader>,
    endpoints: Option<Endpoints>,
    locks: Option<PointerLockFactory>,
    stored: Option<LookSensitivity>,
) -> DemoViewerShell {
    // The Demos.tf list asks demos.tf over the tokio bridge, so the test executor may park.
    cx.executor().allow_parking();
    let picker = Arc::new(ScriptedPicker::new(picks));
    let (factory, stubs) = stub_factory(answer);
    let s = super::open_shell(
        cx,
        env,
        state,
        Arc::clone(&picker),
        super::no_steam(),
        None,
        |services| {
            services.renderers = factory;
            // Shell tests never reach a real OS lock.
            let fallback: PointerLockFactory = Arc::new(|| Box::new(FallbackLock));
            services.pointer_lock = locks.unwrap_or(fallback);
            if let Some(playback) = playback {
                services.demo_playback = playback;
            }
            if let Some(endpoints) = endpoints {
                *services = services.clone().with_endpoints(endpoints);
            }
        },
    );
    if let Some(look) = stored {
        s.services.db.set_look_sensitivity(look).unwrap();
    }
    super::click_nav(cx, s.win, Page::DemosTf);
    DemoViewerShell { s, picker, stubs }
}

/// The viewer's status after a file that is not a demo opens, which leaves it without a map.
pub const NOT_A_DEMO: &str = "This file isn't a TF2 demo.";

/// A file that is not a demo, for tests that want the viewer and its bar without a map.
pub fn not_a_demo(env: &super::Env) -> PathBuf {
    demo_file(env, "x.dem", b"not a demo")
}

/// Opens the next pick from the Demos.tf list and waits for the viewer to say it is no demo.
pub fn open_not_a_demo(cx: &mut TestAppContext, win: AnyWindowHandle) {
    super::click_id(cx, win, tf2tools_app::ids::DEMOS_TF_OPEN);
    super::wait_real(cx, NOT_A_DEMO, |cx| {
        super::label_of(cx, win, tf2tools_app::ids::DEMO_VIEWER_STATUS).as_deref()
            == Some(NOT_A_DEMO)
    });
}

/// A reader that sends `Snapshot(answer())` then `TracksFailed(NoPlayers)` every time, and how
/// many times it was asked.
pub fn counting_snapshots(
    answer: impl Fn() -> Result<DemoSnapshot, DemoSnapshotError> + Send + Sync + 'static,
) -> (PlaybackReader, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&asked);
    let reader: PlaybackReader = Arc::new(move |_, _, emit| {
        count.fetch_add(1, Ordering::SeqCst);
        if emit(PlaybackEvent::Snapshot(answer())).is_continue() {
            let _ = emit(PlaybackEvent::TracksFailed(DemoTracksError::NoPlayers));
        }
    });
    (reader, asked)
}

/// What a gated reader saw: whether it found its cancel flag set, and whether its send after
/// that was refused.
#[derive(Clone, Default)]
pub struct GateHandle {
    open: Arc<AtomicBool>,
    pub cancelled: Arc<AtomicBool>,
    pub broke: Arc<AtomicBool>,
}

impl GateHandle {
    /// Lets the reader send its `after` events.
    pub fn open(&self) {
        self.open.store(true, Ordering::SeqCst);
    }
}

/// A reader that sends `before()`, waits up to 10 s for `open()` or its cancel flag, then
/// sends `after()`. On a cancel it records it, sends one more `Progress` and records whether
/// that send was refused.
pub fn gated_playback(
    before: impl Fn() -> Vec<PlaybackEvent> + Send + Sync + 'static,
    after: impl Fn() -> Vec<PlaybackEvent> + Send + Sync + 'static,
) -> (PlaybackReader, GateHandle) {
    let handle = GateHandle::default();
    let seen = handle.clone();
    let reader: PlaybackReader = Arc::new(move |_, cancel, emit| {
        for event in before() {
            if emit(event).is_break() {
                return;
            }
        }
        for _ in 0..2000 {
            if seen.open.load(Ordering::SeqCst) || cancel.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if cancel.load(Ordering::SeqCst) {
            seen.cancelled.store(true, Ordering::SeqCst);
            // The page dropped its receiver with its task, so this send must be refused.
            for _ in 0..200 {
                if emit(PlaybackEvent::Progress(0.5)).is_break() {
                    seen.broke.store(true, Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            return;
        }
        for event in after() {
            if emit(event).is_break() {
                return;
            }
        }
    });
    (reader, handle)
}

/// Three soldiers; the first walks +x with `x`.
pub fn walking(tick: u32, x: f32) -> DemoSnapshot {
    let at =
        |team, x| SnapshotPlayer::new(team, TfClass::Soldier, [x, 32., 0.], 0., 0., 68., false);
    DemoSnapshot::new(
        tick,
        vec![at(Team::Blue, x), at(Team::Red, 20.), at(Team::Red, 40.)],
    )
}

/// Ticks 4000..=4200 at the fixture header's 60 s over 3960 ticks, rounds at 4050 and 4150.
pub fn walking_tracks(cut: Option<TrackCut>) -> Arc<DemoTracks> {
    Arc::new(
        DemoTracks::from_keyframes(
            60. / 3960.,
            &[walking(4000, 0.), walking(4200, 200.)],
            vec![4050, 4150],
            cut,
        )
        .unwrap(),
    )
}

/// Alice (entity 1, BLU) walks +x over ticks 4000..=4200 at the fixture's tick; Bob (2, RED)
/// stands and is dead from 4120 to 4169 (0.76 s); entity 3 (RED) has no name.
pub fn camera_tracks() -> Arc<DemoTracks> {
    let soldier =
        |team, x| SnapshotPlayer::new(team, TfClass::Soldier, [x, 32., 0.], 0., 0., 68., false);
    let frames: Vec<(u32, Vec<(u32, SnapshotPlayer)>)> = (4000..=4200u32)
        .map(|t| {
            let mut rows = vec![(1, soldier(Team::Blue, (t - 4000) as f32))];
            if !(4120..4170).contains(&t) {
                rows.push((2, soldier(Team::Red, 20.)));
            }
            rows.push((3, soldier(Team::Red, 40.)));
            (t, rows)
        })
        .collect();
    let names = [(1, "Alice"), (2, "Bob")];
    Arc::new(DemoTracks::from_entity_frames(60. / 3960., &frames, &names).expect("tracks"))
}

/// A full server over ticks 4000..=4200 at the fixture's tick: entities 1..=9 RED and 10..=18
/// BLU, each standing alone along +y and named "Player N".
pub fn full_server_tracks() -> Arc<DemoTracks> {
    let soldier = |e: u32| {
        let team = if e <= 9 { Team::Red } else { Team::Blue };
        let at = [60., 32. * e as f32, 64.];
        SnapshotPlayer::new(team, TfClass::Soldier, at, 0., 0., 68., false)
    };
    let frames: Vec<(u32, Vec<(u32, SnapshotPlayer)>)> = (4000..=4200u32)
        .map(|t| (t, (1..=18u32).map(|e| (e, soldier(e))).collect()))
        .collect();
    let names: Vec<(u32, String)> = (1..=18u32).map(|e| (e, format!("Player {e}"))).collect();
    let names: Vec<(u32, &str)> = names.iter().map(|(e, n)| (*e, n.as_str())).collect();
    Arc::new(DemoTracks::from_entity_frames(60. / 3960., &frames, &names).expect("tracks"))
}

/// Opens the fixture demo with `reader` and waits for the viewport.
pub fn open_scripted(
    cx: &mut TestAppContext,
    env: &super::Env,
    reader: PlaybackReader,
) -> DemoViewerShell {
    install_map(env, "cp_synthetic");
    let demo = demo_file(env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell_with(
        cx,
        env,
        found(env),
        vec![demo],
        StubAnswer::Frames,
        Some(reader),
    );
    super::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    super::wait_real(cx, "the viewport", |cx| {
        super::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    w
}

/// The open viewer's read state, if the viewer shows.
pub fn read_state(cx: &mut TestAppContext, w: &DemoViewerShell) -> Option<ReadState> {
    cx.update(|cx| {
        let page = w.s.shell.read(cx).demo_viewer_page(cx)?;
        Some(page.read(cx).read_state())
    })
}

/// The open viewport's playback, once it has tracks.
pub fn playback(cx: &mut TestAppContext, w: &DemoViewerShell) -> Option<PlaybackState> {
    cx.update(|cx| {
        let page = w.s.shell.read(cx).demo_viewer_page(cx)?;
        let viewport = page.read(cx).viewport()?.clone();
        viewport.read(cx).playback()
    })
}

/// A gated reader whose `before` is the still picture of `walking(4100, 100.)`.
pub fn still_then(
    after: impl Fn() -> Vec<PlaybackEvent> + Send + Sync + 'static,
) -> (PlaybackReader, GateHandle) {
    gated_playback(
        || vec![PlaybackEvent::Snapshot(Ok(walking(4100, 100.)))],
        after,
    )
}

/// Opens the fixture demo with `walking_tracks(cut)` already read, and waits for the tracks.
pub fn open_with_tracks(
    cx: &mut TestAppContext,
    env: &super::Env,
    cut: Option<TrackCut>,
) -> DemoViewerShell {
    open_with(cx, env, walking_tracks(cut))
}

/// Opens the fixture demo with `tracks` already read, and waits for them.
pub fn open_with(
    cx: &mut TestAppContext,
    env: &super::Env,
    tracks: Arc<DemoTracks>,
) -> DemoViewerShell {
    let (reader, gate) = still_then(move || vec![PlaybackEvent::Tracks(Arc::clone(&tracks))]);
    gate.open();
    let w = open_scripted(cx, env, reader);
    super::wait_real(cx, "the tracks", |cx| playback(cx, &w).is_some());
    super::settle(cx, w.s.win);
    w
}

/// `open_with` whose viewport takes fake locks that share the returned handle.
pub fn open_with_locked(
    cx: &mut TestAppContext,
    env: &super::Env,
    tracks: Arc<DemoTracks>,
) -> (DemoViewerShell, FakeLockHandle) {
    let (reader, gate) = still_then(move || vec![PlaybackEvent::Tracks(Arc::clone(&tracks))]);
    gate.open();
    install_map(env, "cp_synthetic");
    let demo = demo_file(env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let (locks, handle) = fake_locks();
    let w = demo_viewer_shell_in(
        cx,
        env,
        found(env),
        vec![demo],
        StubAnswer::Frames,
        Some(reader),
        None,
        Some(locks),
        None,
    );
    super::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    super::wait_real(cx, "the tracks", |cx| playback(cx, &w).is_some());
    super::settle(cx, w.s.win);
    (w, handle)
}

/// Puts the synthetic map at `tf/maps/<name>.bsp` in `env`'s game.
pub fn install_map(env: &super::Env, name: &str) {
    let dir = env.game.join("tf/maps");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.bsp")), bsp(&BspOptions::default())).unwrap();
}

/// Writes `bytes` as `<name>` in `env`'s temp dir.
pub fn demo_file(env: &super::Env, name: &str, bytes: &[u8]) -> PathBuf {
    let path = env.tmp.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// Whether `inner` lies wholly inside `outer`, give or take half a pixel of rounding.
pub fn inside(inner: Bounds<Pixels>, outer: Bounds<Pixels>) -> bool {
    let slack = px(0.5);
    inner.left() >= outer.left() - slack
        && inner.top() >= outer.top() - slack
        && inner.right() <= outer.right() + slack
        && inner.bottom() <= outer.bottom() + slack
}
