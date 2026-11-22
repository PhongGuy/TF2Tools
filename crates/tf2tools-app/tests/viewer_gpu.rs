//! The real wgpu worker on this machine's GPU: lavapipe in CI's Linux jobs, WARP on Windows.
//! No adapter is a failure to fix, never a skip.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::support::demo_view_fixtures::{BspOptions, bsp};
use tf2tools_app::viewer::renderer::join_render_workers;
use tf2tools_app::viewer::{DrawRequest, RenderEvent, Renderer, SceneRenderer, math};
use tf2tools_core::load_map_mesh;

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
    let (renderer, events) = ready_renderer();
    renderer.draw(DrawRequest {
        width: 64,
        height: 48,
        view_proj: above(64. / 48.),
        seq: 7,
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
    let (renderer, events) = ready_renderer();
    // 30 px is 120 bytes a row, so every row comes back padded to 256.
    for seq in 1..=20 {
        renderer.draw(DrawRequest {
            width: 30,
            height: 20,
            view_proj: above(1.5),
            seq,
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
    drop(Renderer::spawn());
    assert!(
        join_render_workers(Duration::from_secs(10)),
        "a render worker is still running 10 s after its Renderer dropped"
    );
}
