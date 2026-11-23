//! The demo viewer's 3D view as a GPUI element: drag to look, WASD to fly, Shift to slow down.
//! It asks its renderer for a frame only when the camera moves or the device size changes.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::{
    Bounds, Context, DispatchPhase, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement as _, Pixels, Point, Render, RenderImage, Styled as _, TestSupportExt as _,
    Window, canvas, div, img,
};
use tf2tools_core::{DemoSnapshot, FlyInput, FreeFly, MapMesh};

use super::pointer::PointerLock;
use super::{DrawRequest, RenderEvent, RendererError, SceneRenderer, math};
use crate::ids;

/// The time source the viewport steps its camera by; read once per render.
pub type Clock = Arc<dyn Fn() -> Instant>;

#[derive(Debug, Default, Clone, Copy)]
struct HeldKeys {
    w: bool,
    a: bool,
    s: bool,
    d: bool,
}

impl HeldKeys {
    fn any(self) -> bool {
        self.w || self.a || self.s || self.d
    }

    /// The flag for a movement key, or `None` for any other key.
    fn slot(&mut self, key: &str) -> Option<&mut bool> {
        match key {
            "w" => Some(&mut self.w),
            "a" => Some(&mut self.a),
            "s" => Some(&mut self.s),
            "d" => Some(&mut self.d),
            _ => None,
        }
    }
}

fn axis(plus: bool, minus: bool) -> f32 {
    f32::from(u8::from(plus)) - f32::from(u8::from(minus))
}

/// A free-fly view of one map, drawn by a `SceneRenderer` and shown as its latest frame.
pub struct DemoViewport {
    renderer: Box<dyn SceneRenderer>,
    lock: Box<dyn PointerLock>,
    camera: FreeFly,
    keys: HeldKeys,
    shown: Option<(Arc<RenderImage>, u64)>,
    last_seq: u64,
    /// The element's size in device pixels, as the last prepaint measured it.
    device_size: Rc<Cell<(u32, u32)>>,
    /// The element's window bounds in logical pixels, as the last prepaint measured them.
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    drawn_size: Option<(u32, u32)>,
    /// When the last moving frame was stepped; `None` while the camera rests.
    last_tick: Option<Instant>,
    /// A next-frame tick is scheduled; renders before it leave the camera be.
    tick_pending: bool,
    drag: Option<Point<Pixels>>,
    focus: FocusHandle,
    failure: Option<RendererError>,
    clock: Clock,
    /// When the shown frame was taken off the channel and its seq, until its paint logs `ui_us`.
    taken: Rc<Cell<Option<(Instant, u64)>>>,
}

impl DemoViewport {
    /// Loads `mesh` and the snapshot's boxes into `renderer`, starts the camera at
    /// `start_camera` and listens for frames.
    pub fn new(
        mesh: Arc<MapMesh>,
        snapshot: Option<&DemoSnapshot>,
        renderer: Box<dyn SceneRenderer>,
        lock: Box<dyn PointerLock>,
        clock: Clock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (origin, yaw, pitch) = math::start_camera(&mesh, snapshot);
        renderer.load_scene(mesh);
        renderer.set_players(math::player_boxes(snapshot));
        let events = renderer.events();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if this.update(cx, |v, cx| v.handle_event(event, cx)).is_err() {
                    return;
                }
            }
            // A worker that stops after reporting why keeps that reason.
            let _ = this.update(cx, |v, cx| {
                if v.failure.is_none() {
                    v.failure = Some(RendererError::Device("the renderer stopped".into()));
                    cx.notify();
                }
            });
        })
        .detach();
        // Focus survives an alt-tab, but the key-ups go to the other app, so let go on leaving.
        cx.observe_window_activation(window, |v, window, cx| {
            if !window.is_window_active() {
                v.keys = HeldKeys::default();
                v.end_drag(cx);
                cx.notify();
            }
        })
        .detach();
        // The shown frame's atlas tile outlives the viewport unless it is dropped here, and a
        // window closed mid-drag must not leave the pointer locked.
        cx.on_release(|v, cx| {
            v.lock.unlock();
            if let Some((image, _)) = v.shown.take() {
                cx.drop_image(image, None);
            }
        })
        .detach();
        Self {
            renderer,
            lock,
            camera: FreeFly::new(origin, yaw, pitch),
            keys: HeldKeys::default(),
            shown: None,
            last_seq: 0,
            device_size: Rc::new(Cell::new((0, 0))),
            bounds: Rc::new(Cell::new(None)),
            drawn_size: None,
            last_tick: None,
            tick_pending: false,
            drag: None,
            focus: cx.focus_handle(),
            failure: None,
            clock,
            taken: Rc::new(Cell::new(None)),
        }
    }

    pub fn camera(&self) -> &FreeFly {
        &self.camera
    }

    /// The seq of the frame on screen, if any has arrived.
    pub fn shown_seq(&self) -> Option<u64> {
        self.shown.as_ref().map(|(_, seq)| *seq)
    }

    /// Why the renderer stopped, once it has.
    pub fn failure(&self) -> Option<&RendererError> {
        self.failure.as_ref()
    }

    fn handle_event(&mut self, event: RenderEvent, cx: &mut Context<Self>) {
        match event {
            RenderEvent::Ready => {}
            // Frames can overtake each other's delivery; only a newer one replaces the shown one.
            RenderEvent::Frame { image, seq, .. } => {
                if self.shown_seq().is_some_and(|shown| seq <= shown) {
                    return cx.drop_image(image, None);
                }
                if super::stats::enabled() {
                    self.taken.set(Some((Instant::now(), seq)));
                }
                if let Some((old, _)) = self.shown.replace((image, seq)) {
                    cx.drop_image(old, None);
                }
                cx.notify();
            }
            RenderEvent::Failed(err) => {
                self.failure.get_or_insert(err);
                cx.notify();
            }
        }
    }

    /// Asks for a frame of the current camera at the measured size; nothing before a measure.
    fn draw(&mut self) {
        let (width, height) = self.device_size.get();
        if width == 0 || height == 0 || self.failure.is_some() {
            return;
        }
        self.last_seq += 1;
        let aspect = width as f32 / height as f32;
        let view_proj = math::view_proj(
            self.camera.origin(),
            self.camera.yaw(),
            self.camera.pitch(),
            aspect,
        );
        self.renderer.draw(DrawRequest {
            width,
            height,
            view_proj,
            seq: self.last_seq,
        });
        self.drawn_size = Some((width, height));
    }

    fn press(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        // A second button joining a drag keeps the lock the first one took.
        if self.drag.is_some() {
            return;
        }
        // Before the first prepaint there is no measured centre, so the press point stands in.
        let centre = self.bounds.get().map_or(ev.position, |b| b.center());
        self.lock.lock(ev.position, centre, window);
        self.drag = Some(ev.position);
        cx.notify();
    }

    /// Ends a drag, whichever way it ended; does nothing when none is under way.
    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            self.lock.unlock();
            cx.notify();
        }
    }

    /// Turns the camera by the drag since the last move, as the lock measures it when it can;
    /// a move with no button held ends it.
    fn drag_to(&mut self, ev: &MouseMoveEvent, window: &Window, cx: &mut Context<Self>) {
        let Some(last) = self.drag else {
            return;
        };
        if ev.pressed_button.is_none() {
            return self.end_drag(cx);
        }
        self.drag = Some(ev.position);
        let by = self
            .lock
            .motion(window)
            .unwrap_or_else(|| ev.position - last);
        if by == Point::default() {
            return;
        }
        let (d_yaw, d_pitch) = math::drag_look(by.x.as_f32(), by.y.as_f32());
        self.camera.look(d_yaw, d_pitch);
        self.draw();
        cx.notify();
    }

    fn set_key(&mut self, key: &str, held: bool, cx: &mut Context<Self>) {
        if let Some(slot) = self.keys.slot(key)
            && *slot != held
        {
            *slot = held;
            cx.notify();
        }
    }
}

impl Render for DemoViewport {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = (self.clock)();
        if !self.focus.is_focused(window) {
            self.keys = HeldKeys::default();
        }
        if self.keys.any() || self.camera.is_moving() {
            // Flight steps once per display frame. A render between ticks, such as one for an
            // arriving frame, must not step, or each frame would draw the next at once.
            if !self.tick_pending {
                // The first moving frame steps by nothing, so time spent at rest is not flown.
                let dt = self.last_tick.map_or(0.0, |t| (now - t).as_secs_f32());
                self.last_tick = Some(now);
                let k = self.keys;
                let shift = window.modifiers().shift;
                self.camera
                    .advance(dt, FlyInput::new(axis(k.w, k.s), axis(k.d, k.a), shift));
                self.draw();
                self.tick_pending = true;
                cx.on_next_frame(window, |v, _, cx| {
                    v.tick_pending = false;
                    cx.notify();
                });
            }
        } else {
            self.last_tick = None;
            if self.drawn_size != Some(self.device_size.get()) {
                self.draw();
            }
        }

        // The canvas measures the element in device pixels every prepaint, so a resize or a
        // scale change asks for one more frame, which draws at the new size.
        let measured = Rc::clone(&self.device_size);
        let logical = Rc::clone(&self.bounds);
        let paint_taken = Rc::clone(&self.taken);
        let dragging = self.drag.is_some().then(|| cx.entity().downgrade());
        let watch = canvas(
            move |bounds, window, _| {
                logical.set(Some(bounds));
                let scale = window.scale_factor();
                let device = |v: Pixels| (v.as_f32() * scale).round().max(1.0) as u32;
                let size = (device(bounds.size.width), device(bounds.size.height));
                if measured.get() != size {
                    measured.set(size);
                    window.request_animation_frame();
                }
            },
            move |_, _, window, _| {
                // A drag follows the pointer anywhere in the window, so moves and the release
                // past the viewport's edge still reach it.
                if let Some(view) = dragging {
                    let moved = view.clone();
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = moved.update(cx, |v, cx| v.drag_to(ev, window, cx));
                        }
                    });
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = view.update(cx, |v, cx| v.end_drag(cx));
                        }
                    });
                }
                if let Some((taken, seq)) = paint_taken.take() {
                    tracing::info!(
                        target: super::stats::TARGET,
                        seq,
                        ui_us = taken.elapsed().as_micros(),
                        "shown"
                    );
                }
            },
        )
        .absolute()
        .size_full();
        let frame = self
            .shown
            .as_ref()
            .map(|(image, _)| img(Arc::clone(image)).absolute().size_full());

        let mut view = div()
            .id(ids::DEMO_VIEWER_VIEWPORT)
            .test_support()
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .child(watch)
            .children(frame)
            .on_key_down(cx.listener(|v, ev: &KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    v.end_drag(cx);
                }
                v.set_key(&ev.keystroke.key, true, cx);
            }))
            .on_key_up(cx.listener(|v, ev: &KeyUpEvent, _, cx| {
                v.set_key(&ev.keystroke.key, false, cx);
            }));
        for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            view = view.on_mouse_down(button, cx.listener(Self::press));
        }
        view
    }
}
