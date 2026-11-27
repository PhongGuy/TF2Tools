//! The viewport's spectator camera: follow modes, the hidden box, a dead target, the state
//! events the bar reads, and (Task 3) the clicks, drags and Space that drive it.

use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use tf2tools_app::viewer::viewport::{CameraMoved, CameraState, DemoViewport};
use tf2tools_core::{CameraMode, CameraView, DemoTracks, PlayerId, SnapshotPlayer, Team, TfClass};

use crate::support;
use crate::support::demo_viewer::{StubAnswer, StubHandle, viewport_window};

// Alice (entity 1, BLU) walks +x from x 0 at tick 1000, looking along +x, eye 65 over feet at
// z 64. Bob (entity 2, RED) stands at x 500 and is dead from 1080 to 1139 (0.9 s at 66 tick).
fn two_players() -> Arc<DemoTracks> {
    let frames: Vec<(u32, Vec<(u32, SnapshotPlayer)>)> = (1000..=1200u32)
        .map(|t| {
            let x = (t - 1000) as f32;
            let alice =
                SnapshotPlayer::new(Team::Blue, TfClass::Scout, [x, 0., 64.], 0., 0., 65., false);
            let mut rows = vec![(1, alice)];
            if !(1080..1140).contains(&t) {
                let bob = SnapshotPlayer::new(
                    Team::Red,
                    TfClass::Medic,
                    [500., 0., 64.],
                    180.,
                    0.,
                    75.,
                    false,
                );
                rows.push((2, bob));
            }
            (t, rows)
        })
        .collect();
    let names = [(1, "Alice"), (2, "Bob")];
    Arc::new(DemoTracks::from_entity_frames(1. / 66., &frames, &names).expect("tracks"))
}

fn on(
    cx: &mut TestAppContext,
    viewport: &Entity<DemoViewport>,
    f: impl FnOnce(&mut DemoViewport, &mut gpui_kit::Context<DemoViewport>),
) {
    cx.update(|cx| viewport.update(cx, |v, cx| f(v, cx)));
}

fn with_tracks(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<DemoViewport>, StubHandle) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    on(cx, &viewport, |v, cx| v.set_tracks(two_players(), cx));
    support::settle(cx, win);
    (win, viewport, stub)
}

fn state(cx: &mut TestAppContext, viewport: &Entity<DemoViewport>) -> CameraState {
    cx.update(|cx| viewport.read(cx).camera_state())
}

fn followed(s: CameraState) -> (CameraMode, Option<u32>, bool) {
    (s.mode, s.target.map(PlayerId::entity), s.dead)
}

fn view(cx: &mut TestAppContext, viewport: &Entity<DemoViewport>) -> CameraView {
    cx.update(|cx| viewport.read(cx).camera_view().clone())
}

fn boxes(stub: &StubHandle) -> usize {
    stub.draws().last().expect("a draw").players.len()
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3)
}

#[gpui_kit::test]
fn first_person_follows_the_first_alive_player_and_hides_their_box(cx: &mut TestAppContext) {
    let (win, viewport, stub) = with_tracks(cx);
    assert_eq!(
        followed(state(cx, &viewport)),
        (CameraMode::FreeFly, None, false)
    );
    assert_eq!(boxes(&stub), 2);
    on(cx, &viewport, |v, cx| v.cycle_camera(cx));
    support::settle(cx, win);
    assert_eq!(
        followed(state(cx, &viewport)),
        (CameraMode::FirstPerson, Some(1), false)
    );
    let shown = view(cx, &viewport);
    assert_eq!(shown.hidden().map(PlayerId::entity), Some(1));
    assert!(near(shown.origin(), [0., 0., 129.]), "{:?}", shown.origin());
    let last = stub.draws().last().cloned().expect("a draw");
    assert_eq!(last.players.len(), 1);
    assert_eq!(last.players[0].team, Team::Red);
}

#[gpui_kit::test]
fn the_cycle_goes_chase_then_free_fly_at_the_drawn_pose(cx: &mut TestAppContext) {
    let (win, viewport, stub) = with_tracks(cx);
    on(cx, &viewport, |v, cx| {
        v.cycle_camera(cx);
        v.cycle_camera(cx);
    });
    support::settle(cx, win);
    assert_eq!(
        followed(state(cx, &viewport)),
        (CameraMode::Chase, Some(1), false)
    );
    let chase = view(cx, &viewport);
    assert_eq!(chase.hidden(), None);
    let o = chase.origin();
    // Behind Alice's eye at x 0, never farther than the 96-unit chase.
    assert!(o[0] <= 1e-3 && o[0] >= -96.001, "{o:?}");
    assert!(o[1].abs() < 1e-3 && (o[2] - 129.).abs() < 1e-3, "{o:?}");
    assert_eq!(boxes(&stub), 2);
    on(cx, &viewport, |v, cx| v.cycle_camera(cx));
    support::settle(cx, win);
    assert_eq!(
        followed(state(cx, &viewport)),
        (CameraMode::FreeFly, Some(1), false)
    );
    let fly = cx.update(|cx| viewport.read(cx).camera().origin());
    assert!(near(fly, o), "{fly:?} vs {o:?}");
}

#[gpui_kit::test]
fn next_and_prev_step_through_the_players_and_wrap(cx: &mut TestAppContext) {
    let (_win, viewport, _stub) = with_tracks(cx);
    let mut seen = Vec::new();
    for next in [true, true, true, false] {
        on(cx, &viewport, move |v, cx| {
            if next {
                v.next_target(cx);
            } else {
                v.prev_target(cx);
            }
        });
        seen.push(followed(state(cx, &viewport)));
    }
    let fp = CameraMode::FirstPerson;
    assert_eq!(
        seen,
        vec![
            (fp, Some(1), false),
            (fp, Some(2), false),
            (fp, Some(1), false),
            (fp, Some(2), false)
        ]
    );
}

#[gpui_kit::test]
fn a_dead_target_shows_its_chase_and_comes_back_at_respawn(cx: &mut TestAppContext) {
    let (win, viewport, stub) = with_tracks(cx);
    let bob = cx.update(|cx| viewport.read(cx).tracks().unwrap().players()[1].id());
    on(cx, &viewport, move |v, cx| v.follow(bob, cx));
    on(cx, &viewport, |v, cx| v.seek(1100., cx));
    support::settle(cx, win);
    assert_eq!(
        followed(state(cx, &viewport)),
        (CameraMode::FirstPerson, Some(2), true)
    );
    assert_eq!(view(cx, &viewport).hidden(), None);
    // Only Alice is in the tracks at 1100.
    assert_eq!(boxes(&stub), 1);
    on(cx, &viewport, |v, cx| v.seek(1150., cx));
    support::settle(cx, win);
    assert_eq!(
        followed(state(cx, &viewport)),
        (CameraMode::FirstPerson, Some(2), false)
    );
    assert_eq!(view(cx, &viewport).hidden().map(PlayerId::entity), Some(2));
    assert_eq!(boxes(&stub), 1);
}

// Carol (entity 1) looks 70 degrees up from feet at (60, 32, 0) until tick 1099, so her chase
// ray runs down and back into the synthetic map's 64x64 floor at z 0, about 69 units out.
// From 1100 she stands at x 1000, off the floor, where nothing stops the 96-unit chase.
fn over_the_floor_then_off_it() -> Arc<DemoTracks> {
    let frames: Vec<(u32, Vec<(u32, SnapshotPlayer)>)> = (1000..=1200u32)
        .map(|t| {
            let x = if t < 1100 { 60. } else { 1000. };
            let carol = SnapshotPlayer::new(
                Team::Blue,
                TfClass::Soldier,
                [x, 32., 0.],
                0.,
                -70.,
                65.,
                false,
            );
            (t, vec![(1, carol)])
        })
        .collect();
    Arc::new(DemoTracks::from_entity_frames(1. / 66., &frames, &[(1, "Carol")]).expect("tracks"))
}

// How far the drawn chase sits from Carol's eye, 65 over her feet at `x`.
fn chase_distance(cx: &mut TestAppContext, viewport: &Entity<DemoViewport>, x: f32) -> f32 {
    let o = view(cx, viewport).origin();
    let eye = [x, 32., 65.];
    (0..3).map(|i| (o[i] - eye[i]).powi(2)).sum::<f32>().sqrt()
}

fn tick(cx: &mut TestAppContext, viewport: &Entity<DemoViewport>) -> f64 {
    cx.update(|cx| viewport.read(cx).playback())
        .expect("tracks")
        .tick
}

#[gpui_kit::test]
fn the_chase_stops_short_of_the_floor_and_a_seek_snaps_it_back_out(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = viewport_window(cx, StubAnswer::Frames);
    on(cx, &viewport, |v, cx| {
        v.set_tracks(over_the_floor_then_off_it(), cx);
        v.cycle_camera(cx);
        v.cycle_camera(cx);
    });
    support::settle(cx, win);
    let near_floor = chase_distance(cx, &viewport, 60.);
    assert!((50. ..80.).contains(&near_floor), "{near_floor}");
    // Without the snap the distance would still read about 63 here and only grow later.
    on(cx, &viewport, |v, cx| v.seek(1150., cx));
    let off_floor = chase_distance(cx, &viewport, 1000.);
    assert!((off_floor - 96.).abs() < 0.01, "{off_floor}");
}

#[gpui_kit::test]
fn a_paused_chase_keeps_growing_back_then_stops_drawing(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    on(cx, &viewport, |v, cx| {
        v.set_tracks(over_the_floor_then_off_it(), cx);
        v.seek(1096., cx);
        v.cycle_camera(cx);
        v.cycle_camera(cx);
        v.toggle_play(cx);
    });
    for _ in 0..40 {
        support::settle(cx, win);
        if tick(cx, &viewport) >= 1101. {
            break;
        }
    }
    on(cx, &viewport, |v, cx| v.toggle_play(cx));
    assert!(tick(cx, &viewport) >= 1101.);
    let paused = chase_distance(cx, &viewport, 1000.);
    assert!(paused < 90., "{paused}");
    for _ in 0..5 {
        support::settle(cx, win);
    }
    let growing = chase_distance(cx, &viewport, 1000.);
    assert!(growing > paused + 1., "{paused} -> {growing} while paused");
    for _ in 0..60 {
        support::settle(cx, win);
    }
    assert!((chase_distance(cx, &viewport, 1000.) - 96.).abs() < 0.01);
    let drawn = stub.draws().len();
    for _ in 0..5 {
        support::settle(cx, win);
    }
    assert_eq!(
        stub.draws().len(),
        drawn,
        "a settled paused chase kept drawing"
    );
}

#[gpui_kit::test]
fn camera_changes_are_reported_once_each(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = with_tracks(cx);
    let seen: Arc<Mutex<Vec<CameraState>>> = Arc::default();
    let record = Arc::clone(&seen);
    cx.update(|cx| {
        cx.subscribe(&viewport, move |_, ev: &CameraMoved, _| {
            record.lock().unwrap().push(ev.0)
        })
        .detach();
    });
    on(cx, &viewport, |v, cx| {
        v.next_target(cx);
        v.next_target(cx);
        v.seek(1100., cx);
        v.seek(1101., cx);
        v.cycle_camera(cx);
    });
    support::settle(cx, win);
    let got: Vec<_> = seen.lock().unwrap().iter().map(|s| followed(*s)).collect();
    let (fp, chase) = (CameraMode::FirstPerson, CameraMode::Chase);
    assert_eq!(
        got,
        vec![
            (fp, Some(1), false),
            (fp, Some(2), false),
            (fp, Some(2), true),
            (chase, Some(2), true)
        ]
    );
}

#[gpui_kit::test]
fn without_tracks_the_camera_controls_do_nothing(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = viewport_window(cx, StubAnswer::Frames);
    let seen: Arc<Mutex<usize>> = Arc::default();
    let record = Arc::clone(&seen);
    cx.update(|cx| {
        cx.subscribe(&viewport, move |_, _: &CameraMoved, _| {
            *record.lock().unwrap() += 1
        })
        .detach();
    });
    on(cx, &viewport, |v, cx| {
        v.cycle_camera(cx);
        v.next_target(cx);
        v.prev_target(cx);
    });
    support::settle(cx, win);
    assert_eq!(
        followed(state(cx, &viewport)),
        (CameraMode::FreeFly, None, false)
    );
    assert_eq!(*seen.lock().unwrap(), 0);
}

#[gpui_kit::test]
fn playing_in_first_person_moves_the_view_with_the_eye(cx: &mut TestAppContext) {
    let (win, viewport, _stub) = with_tracks(cx);
    on(cx, &viewport, |v, cx| {
        v.next_target(cx);
        v.toggle_play(cx);
    });
    for _ in 0..10 {
        support::settle(cx, win);
    }
    let tick = cx.update(|cx| viewport.read(cx).playback()).unwrap().tick;
    let x = view(cx, &viewport).origin()[0];
    assert!(tick > 1001., "{tick}");
    assert!(
        (f64::from(x) - (tick - 1000.)).abs() < 0.5,
        "eye x {x} at tick {tick}"
    );
}
