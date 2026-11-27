//! The demo viewer's 3D view as a GPUI element: drag to look, WASD to fly, Shift to slow down,
//! or follow a player in first person or chase. It asks its renderer for a frame only when the
//! camera or the playback tick moves, or the device size changes.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::{
    Bounds, Context, DispatchPhase, EventEmitter, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Point, Render, RenderImage, Styled as _,
    TestSupportExt as _, Window, canvas, div, img,
};
use tf2tools_core::{
    CameraMode, CameraView, DemoSnapshot, DemoTracks, FlyInput, FreeFly, MapMesh, PlaybackClock,
    PlayerId, SpectatorCamera, TargetLife,
};

use super::pointer::PointerLock;
use super::{DrawRequest, PlayerBox, RenderEvent, RendererError, SceneRenderer, math};
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

/// A look scale the camera can use: finite and positive, else the neutral 1.0.
fn sane_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0. {
        scale
    } else {
        1.
    }
}

/// Where playback stands, for the bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackState {
    pub tick: f64,
    pub playing: bool,
    pub speed: f32,
    pub elapsed_seconds: f64,
    pub total_seconds: f64,
}

/// Sent whenever the playback state changes: a frame that moved the tick, a play or pause,
/// a seek, a step or a new speed.
pub struct PlaybackMoved(pub PlaybackState);

impl EventEmitter<PlaybackMoved> for DemoViewport {}

/// What the bar shows of the camera: the mode, the kept target, and whether a followed target is dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraState {
    pub mode: CameraMode,
    pub target: Option<PlayerId>,
    pub dead: bool,
}

/// Sent whenever `camera_state()` changes.
pub struct CameraMoved(pub CameraState);

impl EventEmitter<CameraMoved> for DemoViewport {}

/// The demo's tracks and the clock that walks them.
struct Playback {
    tracks: Arc<DemoTracks>,
    clock: PlaybackClock,
}

/// A spectator view of one map, drawn by a `SceneRenderer` and shown as its latest frame.
pub struct DemoViewport {
    renderer: Box<dyn SceneRenderer>,
    lock: Box<dyn PointerLock>,
    camera: SpectatorCamera,
    /// The map the chase is kept out of.
    mesh: Arc<MapMesh>,
    /// The pose of the latest camera step, which every draw uses.
    view: CameraView,
    /// The last step moved the drawn pose, so the frame loop keeps running until it rests.
    settling: bool,
    /// The state last reported by `CameraMoved`.
    reported: CameraState,
    /// The boxes at the shown tick, shared by every draw so the worker uploads them once.
    boxes: Arc<[PlayerBox]>,
    /// The tick and hidden player the boxes were built for, once tracks drive them.
    boxes_for: Option<(f64, Option<PlayerId>)>,
    /// The still snapshot's tick, where playback starts so the picture does not jump.
    snapshot_tick: Option<u32>,
    playback: Option<Playback>,
    /// Multiplies the turn per dragged pixel; always finite and positive.
    look_scale: f32,
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
    /// Loads `mesh` into `renderer`, keeps the snapshot's boxes for every draw, starts the camera
    /// at `start_camera` and listens for frames.
    #[expect(
        clippy::too_many_arguments,
        reason = "each seam a test swaps plus GPUI's window and context"
    )]
    pub fn new(
        mesh: Arc<MapMesh>,
        snapshot: Option<&DemoSnapshot>,
        renderer: Box<dyn SceneRenderer>,
        lock: Box<dyn PointerLock>,
        clock: Clock,
        look_scale: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (origin, yaw, pitch) = math::start_camera(&mesh, snapshot);
        let mut camera = SpectatorCamera::new(FreeFly::new(origin, yaw, pitch));
        let view = camera.view(None, None, 0., 0., FlyInput::default());
        renderer.load_scene(Arc::clone(&mesh));
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
            reported: CameraState {
                mode: camera.mode(),
                target: camera.target(),
                dead: false,
            },
            camera,
            mesh,
            view,
            settling: false,
            boxes: math::player_boxes(snapshot),
            boxes_for: None,
            snapshot_tick: snapshot.map(DemoSnapshot::tick),
            playback: None,
            look_scale: sane_scale(look_scale),
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

    /// Sets how far a dragged pixel turns the camera; a scale that is not a finite positive
    /// number falls back to 1.0.
    pub fn set_look_scale(&mut self, scale: f32) {
        self.look_scale = sane_scale(scale);
    }

    /// The free-fly pose; stale while following a player.
    pub fn camera(&self) -> &FreeFly {
        self.camera.fly()
    }

    pub fn spectator(&self) -> &SpectatorCamera {
        &self.camera
    }

    /// The pose of the latest step.
    pub fn camera_view(&self) -> &CameraView {
        &self.view
    }

    pub fn camera_state(&self) -> CameraState {
        CameraState {
            mode: self.camera.mode(),
            target: self.camera.target(),
            dead: matches!(self.view.target(), Some((_, TargetLife::Dead))),
        }
    }

    pub fn tracks(&self) -> Option<&Arc<DemoTracks>> {
        self.playback.as_ref().map(|p| &p.tracks)
    }

    /// Space: first person, chase, free-fly.
    pub fn cycle_camera(&mut self, cx: &mut Context<Self>) {
        self.control(cx, SpectatorCamera::cycle);
    }

    /// Mouse1: the next alive player.
    pub fn next_target(&mut self, cx: &mut Context<Self>) {
        self.control(cx, SpectatorCamera::next);
    }

    /// Mouse2: the previous alive player.
    pub fn prev_target(&mut self, cx: &mut Context<Self>) {
        self.control(cx, SpectatorCamera::prev);
    }

    pub fn follow(&mut self, id: PlayerId, cx: &mut Context<Self>) {
        self.control(cx, |camera, _, _| camera.follow(id));
    }

    /// Applies a camera control at the shown tick and draws its pose at once; without tracks
    /// there is nobody to follow, so it does nothing.
    fn control(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut SpectatorCamera, &DemoTracks, f64),
    ) {
        let Some(p) = &self.playback else {
            return;
        };
        f(&mut self.camera, &p.tracks, p.clock.tick());
        self.step_camera(0., FlyInput::default(), cx);
        self.draw();
        cx.notify();
    }

    /// The seq of the frame on screen, if any has arrived.
    pub fn shown_seq(&self) -> Option<u64> {
        self.shown.as_ref().map(|(_, seq)| *seq)
    }

    /// Why the renderer stopped, once it has.
    pub fn failure(&self) -> Option<&RendererError> {
        self.failure.as_ref()
    }

    /// Takes the tracks and starts a paused clock at the shown snapshot's tick (clamped), or at
    /// the first tick without one; the boxes become `tracks.at(tick)`'s and one frame draws.
    pub fn set_tracks(&mut self, tracks: Arc<DemoTracks>, cx: &mut Context<Self>) {
        let mut clock = PlaybackClock::new(
            tracks.first_tick(),
            tracks.last_tick(),
            tracks.seconds_per_tick(),
        );
        if let Some(tick) = self.snapshot_tick {
            clock.seek(f64::from(tick));
        }
        self.playback = Some(Playback { tracks, clock });
        self.camera.snap();
        self.moved_while_paused(cx);
    }

    pub fn playback(&self) -> Option<PlaybackState> {
        self.playback.as_ref().map(|p| PlaybackState {
            tick: p.clock.tick(),
            playing: p.clock.is_playing(),
            speed: p.clock.speed(),
            elapsed_seconds: p.clock.elapsed_seconds(),
            total_seconds: p.clock.total_seconds(),
        })
    }

    /// Play from paused (from the last tick it restarts), pause from playing.
    pub fn toggle_play(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.playback.as_mut() else {
            return;
        };
        if p.clock.is_playing() {
            p.clock.pause();
            self.moved_while_paused(cx);
        } else {
            let from = p.clock.tick();
            p.clock.play();
            // A restart from the last tick jumps, so the camera and boxes follow before the loop runs.
            if p.clock.tick() != from {
                self.camera.snap();
            }
            self.step_camera(0., FlyInput::default(), cx);
            self.draw();
            self.emit_playback(cx);
            cx.notify();
        }
    }

    pub fn seek(&mut self, tick: f64, cx: &mut Context<Self>) {
        self.with_clock(cx, |clock| clock.seek(tick));
    }

    pub fn step(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.with_clock(cx, |clock| clock.step(delta));
    }

    pub fn set_speed(&mut self, speed: f32, cx: &mut Context<Self>) {
        let Some(p) = self.playback.as_mut() else {
            return;
        };
        let before = p.clock.speed();
        p.clock.set_speed(speed);
        if p.clock.speed() != before {
            self.emit_playback(cx);
        }
    }

    /// Moves the clock outside the frame loop and draws the new tick at once; a playing clock
    /// keeps the frame loop it already has.
    fn with_clock(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut PlaybackClock)) {
        let Some(p) = self.playback.as_mut() else {
            return;
        };
        f(&mut p.clock);
        // A jump in the tick cuts the chase straight to its new distance.
        self.camera.snap();
        if p.clock.is_playing() {
            self.step_camera(0., FlyInput::default(), cx);
            self.draw();
            self.emit_playback(cx);
        } else {
            self.moved_while_paused(cx);
        }
    }

    /// Steps the camera by nothing, draws once and reports, without starting the frame loop.
    fn moved_while_paused(&mut self, cx: &mut Context<Self>) {
        self.step_camera(0., FlyInput::default(), cx);
        self.draw();
        self.emit_playback(cx);
        cx.notify();
    }

    /// Advances the camera to the clock's tick and keeps the boxes and the reported state in
    /// step with its pose; never draws, so each caller draws once. True when the pose moved.
    fn step_camera(&mut self, dt: f32, input: FlyInput, cx: &mut Context<Self>) -> bool {
        let tick = self.playback.as_ref().map_or(0., |p| p.clock.tick());
        let tracks = self.playback.as_ref().map(|p| Arc::clone(&p.tracks));
        let before = (self.view.origin(), self.view.yaw(), self.view.pitch());
        self.view = self
            .camera
            .view(tracks.as_deref(), Some(&self.mesh), tick, dt, input);
        let moved = before != (self.view.origin(), self.view.yaw(), self.view.pitch());
        // A step of no time cannot tell a resting chase from one still growing, so only a
        // timed step may end the settling.
        self.settling = moved || (dt == 0. && self.settling);
        if let Some(tracks) = tracks {
            let key = (tick, self.view.hidden());
            if self.boxes_for != Some(key) {
                self.boxes = math::player_boxes_except(Some(&tracks.at(tick)), key.1);
                self.boxes_for = Some(key);
            }
        }
        let state = self.camera_state();
        if state != self.reported {
            self.reported = state;
            cx.emit(CameraMoved(state));
        }
        moved
    }

    fn emit_playback(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.playback() {
            cx.emit(PlaybackMoved(state));
        }
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
            self.view.origin(),
            self.view.yaw(),
            self.view.pitch(),
            aspect,
        );
        self.renderer.draw(DrawRequest {
            width,
            height,
            view_proj,
            seq: self.last_seq,
            players: Arc::clone(&self.boxes),
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
        let (d_yaw, d_pitch) = math::drag_look(by.x.as_f32(), by.y.as_f32(), self.look_scale);
        self.camera.look(d_yaw, d_pitch);
        self.step_camera(0., FlyInput::default(), cx);
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
        let flying = self.camera.mode() == CameraMode::FreeFly
            && (self.keys.any() || self.camera.fly().is_moving());
        let playing = self.playback.as_ref().is_some_and(|p| p.clock.is_playing());
        if flying || playing || self.settling {
            // Flight and playback step once per display frame. A render between ticks, such as
            // one for an arriving frame, must not step, or each frame would draw the next at once.
            if !self.tick_pending {
                // The first moving frame steps by nothing, so time spent at rest is not flown
                // or played.
                let dt = self.last_tick.map_or(0.0, |t| (now - t).as_secs_f32());
                self.last_tick = Some(now);
                // A clock already on its last tick pauses without moving; that pause still counts.
                let ticked = self.playback.as_mut().is_some_and(|p| {
                    let was_playing = p.clock.is_playing();
                    p.clock.advance(dt) || (was_playing && !p.clock.is_playing())
                });
                let k = self.keys;
                let input = if flying {
                    FlyInput::new(axis(k.w, k.s), axis(k.d, k.a), window.modifiers().shift)
                } else {
                    FlyInput::default()
                };
                let moved = self.step_camera(dt, input, cx);
                if ticked {
                    self.emit_playback(cx);
                }
                if flying || ticked || moved {
                    self.draw();
                }
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
