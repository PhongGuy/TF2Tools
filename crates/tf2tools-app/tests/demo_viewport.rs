//! The demo viewport: device-pixel draws, frame swaps, drag look, WASD flight with Shift, focus,
//! resize and renderer failures, against a stub renderer.

use std::sync::Arc;
use std::sync::Mutex;

use gpui_kit::test::TestEventExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, Pixels, Point,
    TestAppContext, VisualTestContext, point, px, size,
};
use tf2tools_app::ids;
use tf2tools_app::viewer::viewport::DemoViewport;
use tf2tools_app::viewer::viewport::{PlaybackMoved, PlaybackState};
use tf2tools_app::viewer::{RenderEvent, RendererError};
use tf2tools_core::DemoTracks;
use tf2tools_core::{DemoSnapshot, SnapshotPlayer, Team, TfClass};

use crate::support;
use tf2tools_app::viewer::pointer::LockMode;

use crate::support::demo_viewer::viewport_window_unsettled;
use crate::support::demo_viewer::{LockCall, viewport_window_locked, viewport_window_with};
use crate::support::demo_viewer::{StubAnswer, fake_lock, one_pixel, viewport_window};

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

// Drives only the frame loop a real window has, with no forced re-render and no input, so a
// first frame that waits on a wake-up which never comes stays unshown here too (#1405).
#[gpui_kit::test]
fn the_first_frame_shows_with_no_input_and_no_forced_render(cx: &mut TestAppContext) {
    let (win, viewport, stub) =
        viewport_window_unsettled(cx, StubAnswer::Frames, None, fake_lock(LockMode::Grabbed).0);
    cx.run_until_parked();
    for _ in 0..4 {
        cx.update_window(win, |_, w, cx| {
            w.simulate_next_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    let draws = stub.draws();
    assert_eq!(draws.len(), 1, "{draws:?}");
    assert_eq!(draws[0].seq, 1);
    assert_eq!(cx.update(|cx| viewport.read(cx).shown_seq()), Some(1));
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
    let (_win, viewport, stub) = viewport_window_with(
        cx,
        StubAnswer::Frames,
        Some(snapshot),
        fake_lock(LockMode::Grabbed).0,
    );
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

#[gpui_kit::test]
fn every_draw_carries_the_snapshot_boxes(cx: &mut TestAppContext) {
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
    let (win, _viewport, stub) = viewport_window_with(
        cx,
        StubAnswer::Frames,
        Some(snapshot),
        fake_lock(LockMode::Grabbed).0,
    );
    let at = centre(cx, win);
    cx.update_window(win, |_, w, cx| {
        w.pointer_down(at, MouseButton::Left, cx);
        w.pointer_move(at + point(px(30.), px(0.)), Some(MouseButton::Left), cx);
        w.pointer_move(at + point(px(60.), px(0.)), Some(MouseButton::Left), cx);
        w.pointer_up(at + point(px(60.), px(0.)), MouseButton::Left, cx);
    })
    .unwrap();
    support::settle(cx, win);
    let draws = stub.draws();
    assert!(draws.len() >= 2, "{draws:?}");
    for draw in &draws {
        let teams: Vec<Team> = draw.players.iter().map(|b| b.team).collect();
        assert_eq!(teams, [Team::Blue, Team::Red], "draw {}", draw.seq);
    }
    // One set of boxes, shared by every draw rather than rebuilt for each.
    assert_eq!(stub.players().len(), 1);
}

fn yaw_pitch(cx: &mut TestAppContext, viewport: &Entity<DemoViewport>) -> (f32, f32) {
    cx.update(|cx| {
        let camera = viewport.read(cx).camera();
        (camera.yaw(), camera.pitch())
    })
}

#[gpui_kit::test]
fn a_drag_that_leaves_the_viewport_keeps_turning(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = viewport_window(cx, StubAnswer::Frames);
    let bounds = support::bounds_of(cx, win, ids::DEMO_VIEWER_VIEWPORT).expect("the viewport");
    let from = bounds.center();
    // Past the right edge and below the bottom edge, still inside the window.
    let out = point(bounds.right() + px(100.), bounds.bottom() + px(50.));
    let further = point(out.x + px(100.), out.y);
    cx.update_window(win, |_, w, cx| w.pointer_down(from, MouseButton::Right, cx))
        .unwrap();
    support::settle(cx, win);
    cx.update_window(win, |_, w, cx| {
        w.pointer_move(out, Some(MouseButton::Right), cx)
    })
    .unwrap();
    support::settle(cx, win);
    let (yaw, _) = yaw_pitch(cx, &viewport);
    cx.update_window(win, |_, w, cx| {
        w.pointer_move(further, Some(MouseButton::Right), cx);
        w.pointer_up(further, MouseButton::Right, cx);
    })
    .unwrap();
    support::settle(cx, win);
    let (yaw2, _) = yaw_pitch(cx, &viewport);
    assert!((yaw2 - (yaw - 22.)).abs() < 1e-3, "{yaw} -> {yaw2}");
}

#[gpui_kit::test]
fn a_release_outside_the_viewport_ends_the_drag(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = viewport_window(cx, StubAnswer::Frames);
    let bounds = support::bounds_of(cx, win, ids::DEMO_VIEWER_VIEWPORT).expect("the viewport");
    let out = point(bounds.right() + px(100.), bounds.bottom() + px(50.));
    cx.update_window(win, |_, w, cx| {
        w.pointer_down(bounds.center(), MouseButton::Left, cx)
    })
    .unwrap();
    support::settle(cx, win);
    cx.update_window(win, |_, w, cx| w.pointer_up(out, MouseButton::Left, cx))
        .unwrap();
    support::settle(cx, win);
    let before = yaw_pitch(cx, &viewport);
    // A later button-held move over the viewport must not turn: the drag ended at the release outside.
    let to = point(bounds.center().x + px(100.), bounds.center().y);
    cx.update_window(win, |_, w, cx| {
        w.pointer_move(to, Some(MouseButton::Left), cx)
    })
    .unwrap();
    support::settle(cx, win);
    assert_eq!(yaw_pitch(cx, &viewport), before);
}

#[gpui_kit::test]
fn escape_ends_a_drag(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = viewport_window(cx, StubAnswer::Frames);
    let from = centre(cx, win);
    cx.update_window(win, |_, w, cx| w.pointer_down(from, MouseButton::Right, cx))
        .unwrap();
    support::settle(cx, win);
    cx.update_window(win, |_, w, cx| w.key_down("escape", false, cx))
        .unwrap();
    support::settle(cx, win);
    let before = yaw_pitch(cx, &viewport);
    let to = point(from.x + px(100.), from.y);
    cx.update_window(win, |_, w, cx| {
        w.pointer_move(to, Some(MouseButton::Right), cx)
    })
    .unwrap();
    support::settle(cx, win);
    assert_eq!(yaw_pitch(cx, &viewport), before);
}

fn press(cx: &mut TestAppContext, win: AnyWindowHandle, at: Point<Pixels>) {
    cx.update_window(win, |_, w, cx| w.pointer_down(at, MouseButton::Right, cx))
        .unwrap();
    support::settle(cx, win);
}

#[gpui_kit::test]
fn a_press_locks_once_and_a_release_unlocks(cx: &mut TestAppContext) {
    let (win, _viewport, _stub, lock) = viewport_window_locked(cx, LockMode::Locked);
    let at = centre(cx, win);
    press(cx, win, at);
    assert_eq!(lock.calls(), vec![LockCall::Lock]);
    cx.update_window(win, |_, w, cx| w.pointer_up(at, MouseButton::Right, cx))
        .unwrap();
    support::settle(cx, win);
    assert_eq!(lock.calls(), vec![LockCall::Lock, LockCall::Unlock]);
}

#[gpui_kit::test]
fn a_second_button_mid_drag_does_not_lock_again(cx: &mut TestAppContext) {
    let (win, _viewport, _stub, lock) = viewport_window_locked(cx, LockMode::Locked);
    let at = centre(cx, win);
    press(cx, win, at);
    cx.update_window(win, |_, w, cx| w.pointer_down(at, MouseButton::Left, cx))
        .unwrap();
    support::settle(cx, win);
    assert_eq!(lock.calls(), vec![LockCall::Lock]);
}

#[gpui_kit::test]
fn a_move_with_no_button_held_unlocks(cx: &mut TestAppContext) {
    let (win, _viewport, _stub, lock) = viewport_window_locked(cx, LockMode::Locked);
    let at = centre(cx, win);
    press(cx, win, at);
    cx.update_window(win, |_, w, cx| w.pointer_move(at, None, cx))
        .unwrap();
    support::settle(cx, win);
    assert_eq!(lock.calls(), vec![LockCall::Lock, LockCall::Unlock]);
}

#[gpui_kit::test]
fn escape_unlocks(cx: &mut TestAppContext) {
    let (win, _viewport, _stub, lock) = viewport_window_locked(cx, LockMode::Locked);
    let at = centre(cx, win);
    press(cx, win, at);
    cx.update_window(win, |_, w, cx| w.key_down("escape", false, cx))
        .unwrap();
    support::settle(cx, win);
    assert_eq!(lock.calls(), vec![LockCall::Lock, LockCall::Unlock]);
}

#[gpui_kit::test]
fn leaving_the_window_unlocks(cx: &mut TestAppContext) {
    let (win, _viewport, _stub, lock) = viewport_window_locked(cx, LockMode::Locked);
    cx.update_window(win, |_, w, _| w.activate_window())
        .unwrap();
    support::settle(cx, win);
    let at = centre(cx, win);
    press(cx, win, at);
    VisualTestContext::from_window(win, cx).deactivate_window();
    support::settle(cx, win);
    assert_eq!(lock.calls(), vec![LockCall::Lock, LockCall::Unlock]);
}

#[gpui_kit::test]
fn closing_the_window_mid_drag_unlocks(cx: &mut TestAppContext) {
    // The test must hold no handle to the viewport, or it outlives the window and never releases.
    let (win, viewport, _stub, lock) = viewport_window_locked(cx, LockMode::Locked);
    drop(viewport);
    let at = centre(cx, win);
    press(cx, win, at);
    cx.update_window(win, |_, w, _| w.remove_window()).unwrap();
    cx.run_until_parked();
    assert_eq!(lock.calls(), vec![LockCall::Lock, LockCall::Unlock]);
}

#[gpui_kit::test]
fn a_locked_drag_turns_by_the_lock_motion(cx: &mut TestAppContext) {
    let (win, viewport, _stub, lock) = viewport_window_locked(cx, LockMode::Locked);
    let at = centre(cx, win);
    press(cx, win, at);
    let (yaw, pitch) = yaw_pitch(cx, &viewport);
    // The event position stays at the press point, as a warped Windows cursor does.
    lock.push_motion(point(px(100.), px(50.)));
    cx.update_window(win, |_, w, cx| {
        w.pointer_move(at, Some(MouseButton::Right), cx)
    })
    .unwrap();
    support::settle(cx, win);
    let (yaw2, pitch2) = yaw_pitch(cx, &viewport);
    assert!((yaw2 - (yaw - 22.)).abs() < 1e-3, "{yaw} -> {yaw2}");
    assert!((pitch2 - (pitch + 11.)).abs() < 1e-3, "{pitch} -> {pitch2}");
}

/// The yaw change, wrapped to [-180, 180), of one 100 px rightward drag at `scale`; the lock
/// supplies the motion when `locked`, otherwise the viewport falls back to the event positions.
fn yaw_turn_at_scale(cx: &mut TestAppContext, scale: f32, locked: bool) -> f32 {
    let (win, viewport, _stub, lock) = viewport_window_locked(cx, LockMode::Locked);
    viewport.update(cx, |v, _| v.set_look_scale(scale));
    let at = centre(cx, win);
    press(cx, win, at);
    let (yaw, _) = yaw_pitch(cx, &viewport);
    let to = if locked {
        lock.push_motion(point(px(100.), px(0.)));
        at
    } else {
        point(at.x + px(100.), at.y)
    };
    cx.update_window(win, |_, w, cx| {
        w.pointer_move(to, Some(MouseButton::Right), cx)
    })
    .unwrap();
    support::settle(cx, win);
    let (yaw2, _) = yaw_pitch(cx, &viewport);
    (yaw2 - yaw + 540.).rem_euclid(360.) - 180.
}

#[gpui_kit::test]
fn a_look_scale_of_two_turns_twice_as_far_with_the_lock_motion(cx: &mut TestAppContext) {
    let once = yaw_turn_at_scale(cx, 1., true);
    let twice = yaw_turn_at_scale(cx, 2., true);
    assert!((once + 22.).abs() < 1e-3, "{once}");
    assert!((twice - 2. * once).abs() < 1e-3, "{once} vs {twice}");
}

#[gpui_kit::test]
fn a_look_scale_of_two_turns_twice_as_far_without_the_lock_motion(cx: &mut TestAppContext) {
    let once = yaw_turn_at_scale(cx, 1., false);
    let twice = yaw_turn_at_scale(cx, 2., false);
    assert!((once + 22.).abs() < 1e-3, "{once}");
    assert!((twice - 2. * once).abs() < 1e-3, "{once} vs {twice}");
}

// A scale that is not a finite positive number must fall back to the neutral turn rather than
// freeze, reverse or blow up the camera.
#[gpui_kit::test]
fn a_scale_that_is_not_finite_and_positive_turns_at_the_neutral_rate(cx: &mut TestAppContext) {
    for locked in [true, false] {
        let neutral = yaw_turn_at_scale(cx, 1., locked);
        for bad in [f32::NAN, 0., -0., -2., f32::INFINITY, f32::NEG_INFINITY] {
            let turn = yaw_turn_at_scale(cx, bad, locked);
            assert!(
                (turn - neutral).abs() < 1e-3,
                "{bad} {locked}: {turn} vs {neutral}"
            );
        }
    }
}

#[gpui_kit::test]
fn a_zero_lock_motion_leaves_the_camera_and_draws_nothing(cx: &mut TestAppContext) {
    let (win, viewport, stub, lock) = viewport_window_locked(cx, LockMode::Locked);
    let at = centre(cx, win);
    press(cx, win, at);
    let before = (yaw_pitch(cx, &viewport), stub.draws().len());
    lock.push_motion(point(px(0.), px(0.)));
    cx.update_window(win, |_, w, cx| {
        w.pointer_move(at, Some(MouseButton::Right), cx)
    })
    .unwrap();
    support::settle(cx, win);
    assert_eq!((yaw_pitch(cx, &viewport), stub.draws().len()), before);
}

#[gpui_kit::test]
fn a_grabbed_drag_outside_the_viewport_still_turns(cx: &mut TestAppContext) {
    let (win, viewport, _stub, _lock) = viewport_window_locked(cx, LockMode::Grabbed);
    let bounds = support::bounds_of(cx, win, ids::DEMO_VIEWER_VIEWPORT).expect("the viewport");
    let out = point(bounds.right() + px(50.), bounds.center().y);
    press(cx, win, bounds.center());
    cx.update_window(win, |_, w, cx| {
        w.pointer_move(out, Some(MouseButton::Right), cx)
    })
    .unwrap();
    support::settle(cx, win);
    let (yaw, _) = yaw_pitch(cx, &viewport);
    let further = point(out.x + px(100.), out.y);
    cx.update_window(win, |_, w, cx| {
        w.pointer_move(further, Some(MouseButton::Right), cx)
    })
    .unwrap();
    support::settle(cx, win);
    let (yaw2, _) = yaw_pitch(cx, &viewport);
    assert!((yaw2 - (yaw - 22.)).abs() < 1e-3, "{yaw} -> {yaw2}");
}

// BLU walks +x from 0 to 200 over ticks 1000..=1200; RED stands at x 500. A round starts at 1100.
fn walking_tracks() -> Arc<DemoTracks> {
    let at = |tick, x| {
        DemoSnapshot::new(
            tick,
            vec![
                SnapshotPlayer::new(Team::Blue, TfClass::Scout, [x, 0., 64.], 0., 0., 65., false),
                SnapshotPlayer::new(
                    Team::Red,
                    TfClass::Medic,
                    [500., 0., 64.],
                    0.,
                    0.,
                    75.,
                    false,
                ),
            ],
        )
    };
    Arc::new(
        DemoTracks::from_keyframes(1. / 66., &[at(1000, 0.), at(1200, 200.)], vec![1100], None)
            .unwrap(),
    )
}

fn playback(cx: &mut TestAppContext, viewport: &Entity<DemoViewport>) -> PlaybackState {
    cx.update(|cx| viewport.read(cx).playback())
        .expect("tracks")
}

fn on(
    cx: &mut TestAppContext,
    viewport: &Entity<DemoViewport>,
    f: impl FnOnce(&mut DemoViewport, &mut gpui_kit::Context<DemoViewport>),
) {
    cx.update(|cx| viewport.update(cx, |v, cx| f(v, cx)));
}

// The frame loop a real window has, with no forced re-render.
fn next_frames(cx: &mut TestAppContext, win: AnyWindowHandle, n: usize) {
    for _ in 0..n {
        cx.update_window(win, |_, w, cx| w.simulate_next_frame(cx))
            .unwrap();
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn tracks_start_paused_at_the_first_tick_and_draw_its_boxes(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    on(cx, &viewport, |v, cx| v.set_tracks(walking_tracks(), cx));
    support::settle(cx, win);
    let state = playback(cx, &viewport);
    assert_eq!(state.tick, 1000.);
    assert!(!state.playing);
    assert_eq!(state.speed, 1.);
    assert!((state.total_seconds - 200. / 66.).abs() < 1e-6);
    let last = stub.draws().last().cloned().expect("a draw");
    assert_eq!(last.players.len(), 2);
    assert_eq!(last.players[0].feet[0], 0.);
}

#[gpui_kit::test]
fn tracks_start_at_the_shown_snapshot_tick(cx: &mut TestAppContext) {
    let still = DemoSnapshot::new(
        1100,
        vec![SnapshotPlayer::new(
            Team::Blue,
            TfClass::Scout,
            [100., 0., 64.],
            0.,
            0.,
            65.,
            false,
        )],
    );
    let (win, viewport, stub) = viewport_window_with(
        cx,
        StubAnswer::Frames,
        Some(still),
        fake_lock(LockMode::Grabbed).0,
    );
    on(cx, &viewport, |v, cx| v.set_tracks(walking_tracks(), cx));
    support::settle(cx, win);
    assert_eq!(playback(cx, &viewport).tick, 1100.);
    let x = stub.draws().last().unwrap().players[0].feet[0];
    assert!((x - 100.).abs() < 0.5, "{x}");
}

#[gpui_kit::test]
fn play_advances_the_tick_and_moves_the_boxes_on_each_frame(cx: &mut TestAppContext) {
    let (win, viewport, stub) =
        viewport_window_unsettled(cx, StubAnswer::Frames, None, fake_lock(LockMode::Grabbed).0);
    cx.run_until_parked();
    next_frames(cx, win, 2);
    on(cx, &viewport, |v, cx| {
        v.set_tracks(walking_tracks(), cx);
        v.toggle_play(cx);
    });
    next_frames(cx, win, 10);
    let state = playback(cx, &viewport);
    assert!(state.playing);
    assert!(state.tick > 1000. && state.tick < 1200., "{state:?}");
    let xs: Vec<f32> = stub
        .draws()
        .iter()
        .filter(|d| !d.players.is_empty())
        .map(|d| d.players[0].feet[0])
        .collect();
    assert!(xs.len() >= 5, "{xs:?}");
    assert!(xs.windows(2).all(|w| w[0] <= w[1]), "{xs:?}");
    assert!(xs.last() > xs.first(), "{xs:?}");
}

#[gpui_kit::test]
fn playing_to_the_end_pauses_on_the_last_tick(cx: &mut TestAppContext) {
    let (win, viewport, _stub) =
        viewport_window_unsettled(cx, StubAnswer::Frames, None, fake_lock(LockMode::Grabbed).0);
    cx.run_until_parked();
    next_frames(cx, win, 2);
    on(cx, &viewport, |v, cx| {
        v.set_tracks(walking_tracks(), cx);
        v.set_speed(4., cx);
        v.seek(1190., cx);
        v.toggle_play(cx);
    });
    next_frames(cx, win, 30);
    let state = playback(cx, &viewport);
    assert!(!state.playing, "{state:?}");
    assert_eq!(state.tick, 1200.);
}

#[gpui_kit::test]
fn a_paused_seek_draws_once_at_the_new_tick(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    on(cx, &viewport, |v, cx| v.set_tracks(walking_tracks(), cx));
    support::settle(cx, win);
    let before = stub.draws().len();
    on(cx, &viewport, |v, cx| v.seek(1150., cx));
    next_frames(cx, win, 4);
    let draws = stub.draws();
    assert_eq!(draws.len(), before + 1, "{:?}", &draws[before..]);
    let x = draws.last().unwrap().players[0].feet[0];
    assert!((x - 150.).abs() < 0.5, "{x}");
    let state = playback(cx, &viewport);
    assert_eq!(state.tick, 1150.);
    assert!(!state.playing);
}

#[gpui_kit::test]
fn a_step_pauses_on_a_whole_tick(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = viewport_window(cx, StubAnswer::Frames);
    on(cx, &viewport, |v, cx| {
        v.set_tracks(walking_tracks(), cx);
        v.seek(1150.5, cx);
        v.toggle_play(cx);
        v.step(1, cx);
    });
    support::settle(cx, win);
    let state = playback(cx, &viewport);
    assert!(!state.playing);
    assert_eq!(state.tick, 1151.);
}

#[gpui_kit::test]
fn controls_report_each_change_and_do_nothing_without_tracks(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    let seen: Arc<Mutex<Vec<PlaybackState>>> = Arc::default();
    let record = Arc::clone(&seen);
    cx.update(|cx| {
        cx.subscribe(&viewport, move |_, ev: &PlaybackMoved, _| {
            record.lock().unwrap().push(ev.0)
        })
        .detach();
    });
    let before = stub.draws().len();
    on(cx, &viewport, |v, cx| {
        v.toggle_play(cx);
        v.seek(5., cx);
        v.step(1, cx);
        v.set_speed(2., cx);
    });
    support::settle(cx, win);
    assert!(cx.update(|cx| viewport.read(cx).playback()).is_none());
    assert!(seen.lock().unwrap().is_empty());
    assert_eq!(stub.draws().len(), before);
    on(cx, &viewport, |v, cx| {
        v.set_tracks(walking_tracks(), cx);
        v.set_speed(2., cx);
        v.seek(1100., cx);
    });
    support::settle(cx, win);
    let seen = seen.lock().unwrap();
    assert!(seen.iter().any(|s| s.speed == 2.), "{seen:?}");
    assert_eq!(seen.last().unwrap().tick, 1100.);
}

#[gpui_kit::test]
fn a_seek_to_the_last_tick_while_playing_reports_and_draws_the_pause(cx: &mut TestAppContext) {
    let (win, viewport, stub) =
        viewport_window_unsettled(cx, StubAnswer::Frames, None, fake_lock(LockMode::Grabbed).0);
    cx.run_until_parked();
    next_frames(cx, win, 2);
    let seen: Arc<Mutex<Vec<PlaybackState>>> = Arc::default();
    let record = Arc::clone(&seen);
    cx.update(|cx| {
        cx.subscribe(&viewport, move |_, ev: &PlaybackMoved, _| {
            record.lock().unwrap().push(ev.0)
        })
        .detach();
    });
    on(cx, &viewport, |v, cx| {
        v.set_tracks(walking_tracks(), cx);
        v.toggle_play(cx);
        v.seek(1200., cx);
    });
    next_frames(cx, win, 4);
    let last = *seen.lock().unwrap().last().expect("an event");
    assert!(!last.playing, "{last:?}");
    assert_eq!(last.tick, 1200.);
    let x = stub.draws().last().expect("a draw").players[0].feet[0];
    assert!((x - 200.).abs() < 1e-3, "{x}");
}
