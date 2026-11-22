//! A scripted `SceneRenderer` and a viewport-only window for the demo viewer's headless tests.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{AnyWindowHandle, Context, Entity, RenderImage, TestAppContext, Window, div};
use tf2tools_app::viewer::viewport::{Clock, DemoViewport};
use tf2tools_app::viewer::{
    DrawRequest, FrameStats, RenderEvent, RendererError, RendererFactory, SceneRenderer,
};
use tf2tools_core::{MapMesh, load_map_mesh};

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
    super::init(cx);
    let mesh = Arc::new(load_map_mesh(&bsp(&BspOptions::default())).unwrap());
    let (renderer, handle) = stub(answer);
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(super::window_options(), cx, |window, cx| {
                let viewport =
                    cx.new(|cx| DemoViewport::new(mesh, renderer, frame_clock(), window, cx));
                cx.new(|_| ViewportHost(viewport))
            })
        })
        .unwrap();
    super::settle(cx, win);
    let viewport = cx.update(|cx| host.read(cx).0.clone());
    (win, viewport, handle)
}
