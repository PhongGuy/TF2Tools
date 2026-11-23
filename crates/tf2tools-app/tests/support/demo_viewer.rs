//! A scripted `SceneRenderer` and a viewport-only window for the demo viewer's headless tests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyWindowHandle, Bounds, Context, Entity, Pixels, RenderImage, TestAppContext, Window, div, px,
};
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::viewer::viewport::{Clock, DemoViewport};
use tf2tools_app::viewer::{
    DrawRequest, FrameStats, PlayerBox, RenderEvent, RendererError, RendererFactory, SceneRenderer,
    SnapshotReader,
};
use tf2tools_core::{DemoSnapshot, DemoSnapshotError, MapMesh, load_map_mesh};

use super::demo_view_fixtures::{BspOptions, bsp};

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
    pub players: Arc<Mutex<Vec<Arc<[PlayerBox]>>>>,
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
    pub fn players(&self) -> Vec<Arc<[PlayerBox]>> {
        self.players.lock().unwrap().clone()
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
        self.handle.draws.lock().unwrap().push(request);
        if self.answer == StubAnswer::Frames {
            let frame = RenderEvent::Frame {
                image: one_pixel(),
                seq: request.seq,
                stats: FrameStats::default(),
            };
            let _ = self.handle.events.try_send(frame);
        }
    }
    fn events(&self) -> async_channel::Receiver<RenderEvent> {
        self.receiver.clone()
    }
    fn set_players(&self, boxes: Arc<[PlayerBox]>) {
        self.handle.players.lock().unwrap().push(boxes);
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
        players: Arc::default(),
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

impl Render for ViewportHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

/// A 1200x900 window holding only a viewport over the synthetic map, drawn by an `answer` stub.
pub fn viewport_window(
    cx: &mut TestAppContext,
    answer: StubAnswer,
) -> (AnyWindowHandle, Entity<DemoViewport>, StubHandle) {
    viewport_window_with(cx, answer, None)
}

/// `viewport_window`, with `snapshot`'s players handed to the viewport.
pub fn viewport_window_with(
    cx: &mut TestAppContext,
    answer: StubAnswer,
    snapshot: Option<DemoSnapshot>,
) -> (AnyWindowHandle, Entity<DemoViewport>, StubHandle) {
    super::init(cx);
    let mesh = Arc::new(load_map_mesh(&bsp(&BspOptions::default())).unwrap());
    let (renderer, handle) = stub(answer);
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(super::window_options(), cx, |window, cx| {
                let viewport = cx.new(|cx| {
                    DemoViewport::new(mesh, snapshot.as_ref(), renderer, frame_clock(), window, cx)
                });
                cx.new(|_| ViewportHost(viewport))
            })
        })
        .unwrap();
    super::settle(cx, win);
    let viewport = cx.update(|cx| host.read(cx).0.clone());
    (win, viewport, handle)
}

/// A shell on the Demo Viewer whose picker answers `picks` and whose renderers are `answer` stubs.
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

/// `demo_viewer_shell`, with `snapshots` as the reader when given (the core reader otherwise).
pub fn demo_viewer_shell_with(
    cx: &mut TestAppContext,
    env: &super::Env,
    state: Tf2State,
    picks: Vec<PathBuf>,
    answer: StubAnswer,
    snapshots: Option<SnapshotReader>,
) -> DemoViewerShell {
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
            if let Some(snapshots) = snapshots {
                services.demo_snapshots = snapshots;
            }
        },
    );
    super::click_nav(cx, s.win, Page::DemoViewer);
    DemoViewerShell { s, picker, stubs }
}

/// A reader answering `answer()` every time, and how many times it was asked.
pub fn counting_snapshots(
    answer: impl Fn() -> Result<DemoSnapshot, DemoSnapshotError> + Send + Sync + 'static,
) -> (SnapshotReader, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&asked);
    let reader: SnapshotReader = Arc::new(move |_| {
        count.fetch_add(1, Ordering::SeqCst);
        answer()
    });
    (reader, asked)
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
