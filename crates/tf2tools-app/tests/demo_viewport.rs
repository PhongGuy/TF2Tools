//! The demo viewport: device-pixel draws, frame swaps, drag look, WASD flight with Shift, focus,
//! resize and renderer failures, against a stub renderer.

use gpui_kit::test::TestEventExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, Pixels, Point,
    TestAppContext, VisualTestContext, point, px, size,
};
use tf2tools_app::ids;
use tf2tools_app::viewer::viewport::DemoViewport;
use tf2tools_app::viewer::{RenderEvent, RendererError};
use tf2tools_core::{DemoSnapshot, SnapshotPlayer, Team, TfClass};

use crate::support;
use crate::support::demo_viewer::viewport_window_with;
use crate::support::demo_viewer::{StubAnswer, one_pixel, viewport_window};

fn centre(cx: &mut TestAppContext, win: AnyWindowHandle) -> Point<Pixels> {
    support::bounds_of(cx, win, ids::DEMO_VIEWER_VIEWPORT)
        .expect("the viewport")
        .center()
}

fn device_size(cx: &mut TestAppContext, win: AnyWindowHandle) -> (u32, u32) {
    let b = support::bounds_of(cx, win, ids::DEMO_VIEWER_VIEWPORT).expect("the viewport");
    let scale = cx.update_window(win, |_, w, _| w.scale_factor()).unwrap();
    (
        (b.size.width.as_f32() * scale).round() as u32,
        (b.size.height.as_f32() * scale).round() as u32,
    )
}

fn speed(cx: &mut TestAppContext, viewport: &Entity<DemoViewport>) -> f32 {
    cx.update(|cx| viewport.read(cx).camera().speed())
}

// Clicks the viewport so it takes the keyboard.
fn focus(cx: &mut TestAppContext, win: AnyWindowHandle) {
    let at = centre(cx, win);
    cx.update_window(win, |_, w, cx| {
        w.pointer_down(at, MouseButton::Left, cx);
        w.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
}

// Each settle renders twice, so `settles` settles advance the 16 ms clock 2 x `settles` times.
fn frames(cx: &mut TestAppContext, win: AnyWindowHandle, settles: usize) {
    for _ in 0..settles {
        support::settle(cx, win);
    }
}

#[gpui_kit::test]
fn the_scene_loads_and_one_draw_goes_out_at_device_pixels(cx: &mut TestAppContext) {
    let (win, _viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    support::settle(cx, win);
    assert_eq!(stub.scenes(), 1);
    let draws = stub.draws();
    assert_eq!(draws.len(), 1, "{draws:?}");
    assert_eq!((draws[0].width, draws[0].height), device_size(cx, win));
    assert_eq!(draws[0].seq, 1);
}

#[gpui_kit::test]
fn a_newer_frame_is_shown_and_an_older_one_is_ignored(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    let newest = stub.draws().last().expect("a draw").seq;
    assert_eq!(cx.update(|cx| viewport.read(cx).shown_seq()), Some(newest));
    let stale = RenderEvent::Frame {
        image: one_pixel(),
        seq: newest - 1,
        stats: Default::default(),
    };
    stub.events.try_send(stale).unwrap();
    support::settle(cx, win);
    assert_eq!(cx.update(|cx| viewport.read(cx).shown_seq()), Some(newest));
}

#[gpui_kit::test]
fn dragging_turns_0_22_degrees_a_pixel_and_draws(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    let (yaw, pitch) = cx.update(|cx| {
        let camera = viewport.read(cx).camera();
        (camera.yaw(), camera.pitch())
    });
    let before = stub.draws().len();
    let from = centre(cx, win);
    let to = point(from.x + px(100.), from.y + px(50.));
    cx.update_window(win, |_, w, cx| {
        w.pointer_down(from, MouseButton::Right, cx);
        w.pointer_move(to, Some(MouseButton::Right), cx);
        w.pointer_up(to, MouseButton::Right, cx);
    })
    .unwrap();
    support::settle(cx, win);
    let (yaw2, pitch2) = cx.update(|cx| {
        let camera = viewport.read(cx).camera();
        (camera.yaw(), camera.pitch())
    });
    assert!((yaw2 - (yaw - 22.)).abs() < 1e-3, "{yaw} -> {yaw2}");
    assert!((pitch2 - (pitch + 11.)).abs() < 1e-3, "{pitch} -> {pitch2}");
    assert!(stub.draws().len() > before);
}

#[gpui_kit::test]
fn holding_w_flies_then_coasts_to_a_stop_and_drawing_stops(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    let start = cx.update(|cx| viewport.read(cx).camera().origin());
    focus(cx, win);
    cx.update_window(win, |_, w, cx| w.key_down("w", false, cx))
        .unwrap();
    frames(cx, win, 60);
    assert!(
        (speed(cx, &viewport) - 944.).abs() < 2.,
        "{}",
        speed(cx, &viewport)
    );
    cx.update_window(win, |_, w, cx| w.key_up("w", cx)).unwrap();
    frames(cx, win, 60);
    assert_eq!(speed(cx, &viewport), 0.);
    let end = cx.update(|cx| viewport.read(cx).camera().origin());
    let travelled: f32 = (0..3)
        .map(|i| (end[i] - start[i]).powi(2))
        .sum::<f32>()
        .sqrt();
    assert!(travelled > 500., "{travelled}");
    let drawn = stub.draws().len();
    frames(cx, win, 5);
    assert_eq!(stub.draws().len(), drawn, "a camera at rest kept drawing");
}

#[gpui_kit::test]
fn shift_slows_the_flight(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = viewport_window(cx, StubAnswer::Frames);
    focus(cx, win);
    cx.update_window(win, |_, w, cx| {
        w.change_modifiers(Modifiers::shift(), cx);
        w.key_down("w", false, cx);
    })
    .unwrap();
    frames(cx, win, 60);
    assert!(
        (speed(cx, &viewport) - 663.75).abs() < 2.,
        "{}",
        speed(cx, &viewport)
    );
}

#[gpui_kit::test]
fn losing_focus_lets_go_of_held_keys(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = viewport_window(cx, StubAnswer::Frames);
    focus(cx, win);
    cx.update_window(win, |_, w, cx| w.key_down("d", false, cx))
        .unwrap();
    frames(cx, win, 10);
    assert!(speed(cx, &viewport) > 100.);
    cx.update_window(win, |_, w, cx| w.blur(cx)).unwrap();
    frames(cx, win, 60);
    assert_eq!(speed(cx, &viewport), 0.);
}

#[gpui_kit::test]
fn leaving_the_window_lets_go_of_held_keys_and_drawing_stops(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    // Test windows open inactive, and deactivating one that never was active does nothing.
    cx.update_window(win, |_, w, _| w.activate_window())
        .unwrap();
    focus(cx, win);
    cx.update_window(win, |_, w, cx| w.key_down("w", false, cx))
        .unwrap();
    frames(cx, win, 10);
    assert!(speed(cx, &viewport) > 100.);
    // Alt-tab with W held: the key-up goes to the other app.
    VisualTestContext::from_window(win, cx).deactivate_window();
    frames(cx, win, 60);
    assert_eq!(speed(cx, &viewport), 0.);
    let drawn = stub.draws().len();
    frames(cx, win, 5);
    assert_eq!(stub.draws().len(), drawn, "an inactive window kept drawing");
}

#[gpui_kit::test]
fn a_resize_sends_one_draw_at_the_new_device_size(cx: &mut TestAppContext) {
    let (win, _viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    let before = stub.draws().len();
    cx.simulate_window_resize(win, size(px(800.), px(600.)));
    support::settle(cx, win);
    let draws = stub.draws();
    assert_eq!(draws.len(), before + 1, "{draws:?}");
    let last = draws.last().unwrap();
    assert_eq!((last.width, last.height), device_size(cx, win));
}

#[gpui_kit::test]
fn at_rest_nothing_redraws(cx: &mut TestAppContext) {
    let (win, _viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    let drawn = stub.draws().len();
    frames(cx, win, 5);
    assert_eq!(stub.draws().len(), drawn);
}

#[gpui_kit::test]
fn a_stopped_worker_reads_as_a_device_failure(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    stub.events.close();
    support::settle(cx, win);
    let failure = cx.update(|cx| viewport.read(cx).failure().cloned());
    assert!(
        matches!(failure, Some(RendererError::Device(_))),
        "{failure:?}"
    );
}

#[gpui_kit::test]
fn no_adapter_is_reported(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = viewport_window(cx, StubAnswer::NoAdapter);
    support::settle(cx, win);
    assert_eq!(
        cx.update(|cx| viewport.read(cx).failure().cloned()),
        Some(RendererError::NoAdapter)
    );
}

#[gpui_kit::test]
fn a_worker_that_stops_after_no_adapter_still_reads_as_no_adapter(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = viewport_window(cx, StubAnswer::NoAdapterThenStop);
    support::settle(cx, win);
    assert_eq!(
        cx.update(|cx| viewport.read(cx).failure().cloned()),
        Some(RendererError::NoAdapter)
    );
}

#[gpui_kit::test]
fn a_viewport_without_players_clears_the_boxes_once(cx: &mut TestAppContext) {
    let (_win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    let sent = stub.players();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].is_empty());
    assert_eq!(cx.update(|cx| viewport.read(cx).camera().pitch()), 0.);
}

#[gpui_kit::test]
fn a_viewport_with_players_sends_their_boxes_and_starts_at_the_red_eye(cx: &mut TestAppContext) {
    let snapshot = DemoSnapshot::new(
        1668,
        vec![
            SnapshotPlayer::new(
                Team::Blue,
                TfClass::Scout,
                [0., 0., 64.],
                90.,
                0.,
                65.,
                false,
            ),
            SnapshotPlayer::new(
                Team::Red,
                TfClass::Medic,
                [100., 200., 64.],
                270.,
                12.,
                75.,
                false,
            ),
        ],
    );
    let (_win, viewport, stub) = viewport_window_with(cx, StubAnswer::Frames, Some(snapshot));
    let sent = stub.players();
    assert_eq!(sent.len(), 1);
    let teams: Vec<Team> = sent[0].iter().map(|b| b.team).collect();
    assert_eq!(teams, [Team::Blue, Team::Red]);
    let (origin, yaw, pitch) = cx.update(|cx| {
        let camera = viewport.read(cx).camera();
        (camera.origin(), camera.yaw(), camera.pitch())
    });
    assert_eq!((origin, yaw, pitch), ([100., 200., 139.], 270., 12.));
}
