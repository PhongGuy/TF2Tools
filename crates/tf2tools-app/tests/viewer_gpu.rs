//! The real wgpu worker on this machine's GPU: lavapipe in CI's Linux jobs, WARP on Windows.
//! No adapter is a failure to fix, never a skip.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::support::demo_view_fixtures::{BspOptions, bsp};
use tf2tools_app::viewer::PlayerBox;
use tf2tools_app::viewer::renderer::join_render_workers;
use tf2tools_app::viewer::{DrawRequest, RenderEvent, Renderer, SceneRenderer, math};
use tf2tools_core::Team;
use tf2tools_core::load_map_mesh;

// The GPU tests take turns: the NVIDIA Vulkan/EGL driver deadlocks when one device is torn
// down while another comes up, and a quit join stops every live worker in the process.
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    TURN.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn next_event(events: &async_channel::Receiver<RenderEvent>, what: &str) -> RenderEvent {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match events.try_recv() {
            Ok(event) => return event,
            Err(async_channel::TryRecvError::Closed) => panic!("the worker stopped before {what}"),
            Err(async_channel::TryRecvError::Empty) => {}
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn ready_renderer() -> (Renderer, async_channel::Receiver<RenderEvent>) {
    let renderer = Renderer::spawn();
    let events = renderer.events();
    match next_event(&events, "the device") {
        RenderEvent::Ready => {}
        RenderEvent::Failed(error) => panic!("this machine has no usable GPU adapter: {error}"),
        RenderEvent::Frame { .. } => panic!("a frame before Ready"),
    }
    renderer.load_scene(Arc::new(
        load_map_mesh(&bsp(&BspOptions::default())).unwrap(),
    ));
    (renderer, events)
}

// Above the floors looking straight down, so the grey floors fill part of the view.
fn above(aspect: f32) -> [[f32; 4]; 4] {
    math::view_proj([50., 32., 200.], 0., 89., aspect)
}

#[test]
fn the_worker_renders_the_synthetic_map_and_stops_when_dropped() {
    let _turn = one_at_a_time();
    let (renderer, events) = ready_renderer();
    renderer.draw(DrawRequest {
        width: 64,
        height: 48,
        view_proj: above(64. / 48.),
        seq: 7,
        players: Arc::from([]),
    });
    let RenderEvent::Frame { image, seq, .. } = next_event(&events, "the frame") else {
        panic!("expected a frame");
    };
    assert_eq!(seq, 7);
    let bytes = image.as_bytes(0).expect("one frame");
    assert_eq!(bytes.len(), 64 * 48 * 4);
    assert!(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .any(|px| px[..] != bytes[..4]),
        "the image is one colour"
    );
    drop(renderer);
    wait_closed(&events);
}

// The worker closes its events only after destroying its device; waiting for that keeps the
// test process from exiting while a driver is still tearing down.
fn wait_closed(events: &async_channel::Receiver<RenderEvent>) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !events.is_closed() {
        assert!(
            Instant::now() < deadline,
            "the worker outlived its Renderer"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_burst_of_draws_ends_on_the_newest_at_an_unaligned_width() {
    let _turn = one_at_a_time();
    let (renderer, events) = ready_renderer();
    // 30 px is 120 bytes a row, so every row comes back padded to 256.
    for seq in 1..=20 {
        renderer.draw(DrawRequest {
            width: 30,
            height: 20,
            view_proj: above(1.5),
            seq,
            players: Arc::from([]),
        });
    }
    let mut seen = Vec::new();
    while seen.last() != Some(&20) {
        let RenderEvent::Frame { image, seq, .. } = next_event(&events, "frame 20") else {
            panic!("expected frames");
        };
        assert_eq!(image.as_bytes(0).map(<[u8]>::len), Some(30 * 20 * 4));
        seen.push(seq);
    }
    assert!(
        seen.windows(2).all(|w| w[0] < w[1]),
        "frames out of order: {seen:?}"
    );
    drop(renderer);
    wait_closed(&events);
}

#[test]
fn a_dropped_renderer_leaves_no_worker_to_join() {
    let _turn = one_at_a_time();
    drop(Renderer::spawn());
    assert!(
        join_render_workers(Duration::from_secs(10)),
        "a render worker is still running 10 s after its Renderer dropped"
    );
}

// One frame of `view_proj` at 64x48 with a RED box standing at (50, 32, 100); the centre
// pixel as (r, g, b) from the BGRA readback.
fn centre_with_a_red_box(view_proj: [[f32; 4]; 4]) -> (u8, u8, u8) {
    let (renderer, events) = ready_renderer();
    renderer.draw(DrawRequest {
        width: 64,
        height: 48,
        view_proj,
        seq: 1,
        players: Arc::from([PlayerBox {
            feet: [50., 32., 100.],
            yaw_deg: 0.,
            ducked: false,
            team: Team::Red,
        }]),
    });
    let RenderEvent::Frame { image, .. } = next_event(&events, "the frame") else {
        panic!("expected a frame");
    };
    let bytes = image.as_bytes(0).expect("one frame");
    let at = (24 * 64 + 32) * 4;
    let pixel = (bytes[at + 2], bytes[at + 1], bytes[at]);
    drop(renderer);
    wait_closed(&events);
    pixel
}

#[test]
fn a_red_box_under_the_camera_fills_the_centre_in_red() {
    let _turn = one_at_a_time();
    // From 18 units above the box's top (z 182), looking down.
    let (r, g, b) = centre_with_a_red_box(math::view_proj([50., 32., 200.], 0., 89., 64. / 48.));
    assert!(r > g && r > b, "centre pixel {:?}", (r, g, b));
}

#[test]
fn a_camera_inside_a_box_sees_out_of_it() {
    let _turn = one_at_a_time();
    // At a standing eye inside the box, looking down: back faces are culled.
    let (r, g, b) = centre_with_a_red_box(math::view_proj([50., 32., 168.], 0., 89., 64. / 48.));
    assert!(
        !(r > g && r > b),
        "the inside of the box showed: {:?}",
        (r, g, b)
    );
}

// One frame of the top-down view at 64x48 with `boxes`; the centre pixel as (r, g, b).
fn centre_of(
    renderer: &Renderer,
    events: &async_channel::Receiver<RenderEvent>,
    boxes: Arc<[PlayerBox]>,
    seq: u64,
) -> (u8, u8, u8) {
    renderer.draw(DrawRequest {
        width: 64,
        height: 48,
        view_proj: math::view_proj([50., 32., 200.], 0., 89., 64. / 48.),
        seq,
        players: boxes,
    });
    let RenderEvent::Frame { image, .. } = next_event(events, "the frame") else {
        panic!("expected a frame");
    };
    let bytes = image.as_bytes(0).expect("one frame");
    let at = (24 * 64 + 32) * 4;
    (bytes[at + 2], bytes[at + 1], bytes[at])
}

fn standing(team: Team, x: f32) -> PlayerBox {
    PlayerBox {
        feet: [x, 32., 100.],
        yaw_deg: 0.,
        ducked: false,
        team,
    }
}

#[test]
fn the_box_buffer_grows_shrinks_and_clears_between_draws() {
    let _turn = one_at_a_time();
    let (renderer, events) = ready_renderer();
    let (r, g, b) = centre_of(&renderer, &events, Arc::from([standing(Team::Red, 50.)]), 1);
    assert!(r > g && r > b, "one red box: {:?}", (r, g, b));
    // 40 boxes outgrow any buffer sized for one; the BLU one under the camera must show.
    let mut many: Vec<PlayerBox> = (0..39)
        .map(|i| standing(Team::Red, 1000. + 60. * i as f32))
        .collect();
    many.push(standing(Team::Blue, 50.));
    let (r, _, b) = centre_of(&renderer, &events, Arc::from(many), 2);
    assert!(b > r, "the BLU box after growing: {:?}", (r, b));
    let (r, g, b) = centre_of(&renderer, &events, Arc::from([]), 3);
    assert!(
        !(r > g && r > b) && !(b > r && b > g),
        "no boxes left a box: {:?}",
        (r, g, b)
    );
    drop(renderer);
    wait_closed(&events);
}

#[test]
fn quit_stops_a_live_renderer_and_joins_it_quickly() {
    let _turn = one_at_a_time();
    let (renderer, events) = ready_renderer();
    let started = Instant::now();
    assert!(
        join_render_workers(Duration::from_secs(2)),
        "a worker outlived the quit join"
    );
    let took = started.elapsed();
    assert!(
        took < Duration::from_millis(500),
        "the quit join took {took:?}"
    );
    assert!(events.is_closed(), "the worker's events are still open");
    // A renderer whose worker stopped takes draws without panicking.
    renderer.draw(DrawRequest {
        width: 8,
        height: 8,
        view_proj: above(1.),
        seq: 1,
        players: Arc::from([]),
    });
    drop(renderer);
}

#[test]
fn a_renderer_opened_after_one_closes_waits_for_its_teardown() {
    let _turn = one_at_a_time();
    for _ in 0..3 {
        let (first, first_events) = ready_renderer();
        drop(first);
        // The second device must not come up while the first is still being destroyed.
        let (second, events) = ready_renderer();
        assert!(
            first_events.is_closed(),
            "the new device came up before the old one was torn down"
        );
        drop(second);
        wait_closed(&events);
    }
}

#[test]
fn a_dropped_renderer_ends_its_worker_without_any_join() {
    let _turn = one_at_a_time();
    let (renderer, events) = ready_renderer();
    drop(renderer);
    wait_closed(&events);
}
