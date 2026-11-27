//! The Demo Viewer bar's camera chip and players list, on tracks with names and a death.

use gpui_kit::{
    AppContext as _, ElementId, Entity, Modifiers, ScrollDelta, TestAppContext, point, px, size,
};
use tf2tools_app::ids;
use tf2tools_app::viewer::viewport::{CameraState, DemoViewport};
use tf2tools_core::PlayerId;
use tf2tools_core::{CameraMode, PlaybackEvent};

use crate::support;
use crate::support::demo_viewer::{
    DemoViewerShell, LockCall, camera_tracks, full_server_tracks, gated_playback, open_scripted,
    open_with, open_with_locked, walking,
};

fn viewport(cx: &mut TestAppContext, w: &DemoViewerShell) -> Entity<DemoViewport> {
    cx.update(|cx| {
        let page = w.s.shell.read(cx).demo_viewer_page(cx).expect("the viewer");
        page.read(cx).viewport().expect("a viewport").clone()
    })
}

fn state(cx: &mut TestAppContext, w: &DemoViewerShell) -> CameraState {
    let v = viewport(cx, w);
    cx.update(|cx| v.read(cx).camera_state())
}

fn seek(cx: &mut TestAppContext, w: &DemoViewerShell, tick: f64) {
    let v = viewport(cx, w);
    cx.update(|cx| v.update(cx, |v, cx| v.seek(tick, cx)));
    support::settle(cx, w.s.win);
}

fn label(cx: &mut TestAppContext, w: &DemoViewerShell, id: impl Into<ElementId>) -> Option<String> {
    support::label_of(cx, w.s.win, id)
}

#[gpui_kit::test]
fn the_camera_chip_names_its_controls(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with(cx, &env, camera_tracks());
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_CAMERA_MODE).as_deref(),
        Some("Camera: Free camera")
    );
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_CAMERA_TARGET_PREV).as_deref(),
        Some("Previous player")
    );
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_CAMERA_TARGET_NEXT).as_deref(),
        Some("Next player")
    );
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_CAMERA_TARGET).as_deref(),
        Some("No player")
    );
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_DEAD));
}

#[gpui_kit::test]
fn the_mode_button_cycles_and_names_the_target(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with(cx, &env, camera_tracks());
    let mut seen = Vec::new();
    for _ in 0..3 {
        support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_MODE);
        seen.push((
            label(cx, &w, ids::DEMO_VIEWER_CAMERA_MODE).unwrap(),
            label(cx, &w, ids::DEMO_VIEWER_CAMERA_TARGET).unwrap(),
        ));
    }
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    assert_eq!(
        seen,
        vec![
            pair("Camera: First person", "Alice"),
            pair("Camera: Chase camera", "Alice"),
            pair("Camera: Free camera", "Alice"),
        ]
    );
}

#[gpui_kit::test]
fn the_arrows_step_through_the_players_and_wrap(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with(cx, &env, camera_tracks());
    let mut seen = Vec::new();
    for id in [
        ids::DEMO_VIEWER_CAMERA_TARGET_NEXT,
        ids::DEMO_VIEWER_CAMERA_TARGET_NEXT,
        ids::DEMO_VIEWER_CAMERA_TARGET_NEXT,
        ids::DEMO_VIEWER_CAMERA_TARGET_NEXT,
        ids::DEMO_VIEWER_CAMERA_TARGET_PREV,
    ] {
        support::click_id(cx, w.s.win, id);
        seen.push(label(cx, &w, ids::DEMO_VIEWER_CAMERA_TARGET).unwrap());
    }
    assert_eq!(seen, ["Alice", "Bob", "Player 3", "Alice", "Player 3"]);
    assert_eq!(state(cx, &w).mode, CameraMode::FirstPerson);
}

#[gpui_kit::test]
fn a_dead_target_shows_the_dead_badge_until_respawn(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with(cx, &env, camera_tracks());
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET_NEXT);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET_NEXT);
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_DEAD));
    seek(cx, &w, 4140.);
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_CAMERA_DEAD).as_deref(),
        Some("Dead")
    );
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_CAMERA_TARGET).as_deref(),
        Some("Bob, dead")
    );
    seek(cx, &w, 4180.);
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_DEAD));
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_CAMERA_TARGET).as_deref(),
        Some("Bob")
    );
}

#[gpui_kit::test]
fn the_chip_waits_for_the_tracks(cx: &mut TestAppContext) {
    let env = support::env();
    // The still picture and some progress arrive; the tracks wait behind the gate.
    let (reader, gate) = gated_playback(
        || {
            vec![
                PlaybackEvent::Snapshot(Ok(walking(4100, 100.))),
                PlaybackEvent::Progress(0.4),
            ]
        },
        || vec![PlaybackEvent::Tracks(camera_tracks())],
    );
    let w = open_scripted(cx, &env, reader);
    support::wait_real(cx, "the reading note", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_READING)
    });
    for id in [
        ids::DEMO_VIEWER_CAMERA_MODE,
        ids::DEMO_VIEWER_CAMERA_TARGET_PREV,
        ids::DEMO_VIEWER_CAMERA_TARGET_NEXT,
        ids::DEMO_VIEWER_CAMERA_TARGET,
    ] {
        assert!(
            support::disabled_of(cx, w.s.win, id),
            "{id} is live before the tracks"
        );
    }
    gate.open();
    support::wait_real(cx, "the tracks", |cx| {
        !support::disabled_of(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_MODE)
    });
}

#[gpui_kit::test]
fn the_players_list_groups_red_then_blu_and_follows_the_pick(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with(cx, &env, camera_tracks());
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET);
    assert!(support::present(cx, w.s.win, ids::DEMO_VIEWER_PLAYERS));
    assert_eq!(
        label(cx, &w, ids::demo_viewer_player(1)).as_deref(),
        Some("Alice")
    );
    assert_eq!(
        label(cx, &w, ids::demo_viewer_player(2)).as_deref(),
        Some("Bob")
    );
    assert_eq!(
        label(cx, &w, ids::demo_viewer_player(3)).as_deref(),
        Some("Player 3")
    );
    let top = |cx: &mut TestAppContext, e: u32| {
        support::bounds_of(cx, w.s.win, ids::demo_viewer_player(e))
            .unwrap()
            .top()
    };
    let (alice, bob, three) = (top(cx, 1), top(cx, 2), top(cx, 3));
    assert!(
        bob < three && three < alice,
        "RED rows first: {bob:?} {three:?} {alice:?}"
    );
    support::click_id(cx, w.s.win, ids::demo_viewer_player(3));
    let s = state(cx, &w);
    assert_eq!(
        (s.mode, s.target.map(PlayerId::entity)),
        (CameraMode::FirstPerson, Some(3))
    );
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER_PLAYERS));
}

#[gpui_kit::test]
fn the_players_list_marks_the_dead(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with(cx, &env, camera_tracks());
    seek(cx, &w, 4140.);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET);
    assert_eq!(
        label(cx, &w, ids::demo_viewer_player(2)).as_deref(),
        Some("Bob, dead")
    );
    assert_eq!(
        label(cx, &w, ids::demo_viewer_player(1)).as_deref(),
        Some("Alice")
    );
}

#[gpui_kit::test]
fn a_players_list_pick_never_reaches_the_map(cx: &mut TestAppContext) {
    let env = support::env();
    let (w, lock) = open_with_locked(cx, &env, camera_tracks());
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET);
    support::click_id(cx, w.s.win, ids::demo_viewer_player(3));
    assert!(
        !lock.calls().contains(&LockCall::Lock),
        "a pick in the players list reached the map"
    );
    // A click on the map would step from Alice to Bob, so entity 3 means only the pick landed.
    assert_eq!(state(cx, &w).target.map(PlayerId::entity), Some(3));
}

#[gpui_kit::test]
fn space_changes_the_camera_after_a_click_on_next_player(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with(cx, &env, camera_tracks());
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET_NEXT);
    assert_eq!(state(cx, &w).mode, CameraMode::FirstPerson);
    support::press(cx, w.s.win, "space");
    assert_eq!(state(cx, &w).mode, CameraMode::Chase);
}

#[gpui_kit::test]
fn space_still_cycles_after_a_click_on_play(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with(cx, &env, camera_tracks());
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_PLAY);
    support::press(cx, w.s.win, "space");
    assert_eq!(state(cx, &w).mode, CameraMode::FirstPerson);
}

// Opens the players list of a full server at `width` x `height` and checks it stays inside the
// page and above the bar, then wheels its last BLU row into view and follows that player.
fn full_list_fits(cx: &mut TestAppContext, width: f32, height: f32) {
    let env = support::env();
    let w = open_with(cx, &env, full_server_tracks());
    cx.simulate_window_resize(w.s.win, size(px(width), px(height)));
    support::settle(cx, w.s.win);
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET);
    let page = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER).expect("the page");
    let bar = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_BAR).expect("the bar");
    let list = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_PLAYERS).expect("the list");
    assert!(
        support::demo_viewer::inside(list, page),
        "{width}: the list {list:?} leaves the page {page:?}"
    );
    assert!(
        list.bottom() <= bar.top() + px(0.5),
        "{width}: the list {list:?} runs into the bar {bar:?}"
    );
    let last = ids::demo_viewer_player(18);
    for _ in 0..20 {
        let row = support::bounds_of(cx, w.s.win, last.clone()).expect("the last BLU row");
        if row.bottom() <= list.bottom() + px(0.5) {
            break;
        }
        cx.update_window(w.s.win, |_, win, cx| {
            let wheel = gpui_kit::ScrollEvent {
                position: list.center(),
                delta: ScrollDelta::Lines(point(0., -3.)),
                modifiers: Modifiers::default(),
                touch_phase: gpui_kit::TouchPhase::Moved,
                source: Default::default(),
            };
            let _ = win.dispatch_event(gpui_kit::InputEvent::to_platform_input(wheel), cx);
        })
        .unwrap();
        support::settle(cx, w.s.win);
    }
    let row = support::bounds_of(cx, w.s.win, last.clone()).expect("the last BLU row");
    assert!(
        support::demo_viewer::inside(row, list),
        "{width}: the last BLU row {row:?} never scrolls into {list:?}"
    );
    support::click_id(cx, w.s.win, last);
    assert_eq!(
        state(cx, &w).target.map(PlayerId::entity),
        Some(18),
        "{width}: the last BLU row's pick"
    );
}

#[gpui_kit::test]
fn a_full_players_list_fits_above_the_bar_and_scrolls_at_1280(cx: &mut TestAppContext) {
    full_list_fits(cx, 1280., 800.);
}

#[gpui_kit::test]
fn a_full_players_list_fits_above_the_bar_and_scrolls_at_800(cx: &mut TestAppContext) {
    full_list_fits(cx, 800., 600.);
}

#[gpui_kit::test]
fn the_hint_follows_the_camera(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with(cx, &env, camera_tracks());
    let free = "Drag to look, WASD to move, Shift to slow down, Space to follow a player.";
    let follow =
        "Space to change camera, click for the next player, right-click for the previous one.";
    assert_eq!(label(cx, &w, ids::DEMO_VIEWER_HINT).as_deref(), Some(free));
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET_NEXT);
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_HINT).as_deref(),
        Some(follow)
    );
    support::press(cx, w.s.win, "space");
    assert_eq!(state(cx, &w).mode, CameraMode::Chase);
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_HINT).as_deref(),
        Some(follow)
    );
    support::press(cx, w.s.win, "space");
    assert_eq!(state(cx, &w).mode, CameraMode::FreeFly);
    assert_eq!(label(cx, &w, ids::DEMO_VIEWER_HINT).as_deref(), Some(free));
}
