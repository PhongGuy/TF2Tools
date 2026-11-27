//! The viewport's projectiles: built at the shown tick, moved by seeks and playback, and
//! hidden beside the eye in first person.

use std::sync::Arc;

use gpui_kit::test::TestEventExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::viewer::ProjectileShot;
use tf2tools_app::viewer::viewport::DemoViewport;
use tf2tools_core::{DemoTracks, ProjectileKind, SnapshotPlayer, Team, TfClass};

use crate::support;
use crate::support::demo_viewer::{StubAnswer, StubHandle, viewport_window};

// Alice (1, BLU) walks +x from x 0 at tick 1000, eye 65 over feet at z 64. A RED rocket flies
// +x from (100, 0, 100) at 2 u a tick until 1100; a BLU sticky rests at (300, 50, 70) and a RED
// pipe rests 10 u ahead of Alice's first eye, at (10, 0, 129), both all along.
fn shot_tracks() -> Arc<DemoTracks> {
    let frames: Vec<(u32, Vec<(u32, SnapshotPlayer)>)> = (1000..=1200u32)
        .map(|t| {
            let x = (t - 1000) as f32;
            let alice =
                SnapshotPlayer::new(Team::Blue, TfClass::Scout, [x, 0., 64.], 0., 0., 65., false);
            (t, vec![(1, alice)])
        })
        .collect();
    let mut shots = Vec::new();
    for t in 1000..=1200u32 {
        if t <= 1100 {
            let x = 100. + 2. * (t - 1000) as f32;
            shots.push((
                t,
                1,
                ProjectileKind::Rocket,
                Team::Red,
                false,
                [x, 0., 100.],
            ));
        }
        shots.push((
            t,
            2,
            ProjectileKind::Sticky,
            Team::Blue,
            false,
            [300., 50., 70.],
        ));
        shots.push((
            t,
            3,
            ProjectileKind::Pipe,
            Team::Red,
            false,
            [10., 0., 129.],
        ));
    }
    let tracks =
        DemoTracks::from_entity_frames(1. / 66., &frames, &[(1, "Alice")]).expect("tracks");
    Arc::new(tracks.with_test_projectiles(&shots))
}

fn on(
    cx: &mut TestAppContext,
    viewport: &Entity<DemoViewport>,
    f: impl FnOnce(&mut DemoViewport, &mut gpui_kit::Context<DemoViewport>),
) {
    cx.update(|cx| viewport.update(cx, |v, cx| f(v, cx)));
}

fn shots(stub: &StubHandle) -> Arc<[ProjectileShot]> {
    Arc::clone(&stub.draws().last().expect("a draw").projectiles)
}

fn rocket(stub: &StubHandle) -> Option<ProjectileShot> {
    shots(stub).iter().copied().find(|s| s.long)
}

#[gpui_kit::test]
fn every_draw_carries_the_shots_at_the_shown_tick(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    assert!(shots(&stub).is_empty(), "shots before any tracks");
    on(cx, &viewport, |v, cx| v.set_tracks(shot_tracks(), cx));
    support::settle(cx, win);
    assert_eq!(shots(&stub).len(), 3);
    let r = rocket(&stub).expect("the rocket");
    assert_eq!(r.team, Team::Red);
    assert!((r.position[0] - 100.).abs() < 0.5, "{r:?}");
}

#[gpui_kit::test]
fn a_seek_moves_the_rocket_ends_it_and_comes_back(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    on(cx, &viewport, |v, cx| v.set_tracks(shot_tracks(), cx));
    on(cx, &viewport, |v, cx| v.seek(1050.5, cx));
    support::settle(cx, win);
    let x = rocket(&stub).expect("the rocket").position[0];
    assert!((x - 201.).abs() < 0.5, "rocket x {x} at 1050.5");
    on(cx, &viewport, |v, cx| v.seek(1150., cx));
    support::settle(cx, win);
    assert!(rocket(&stub).is_none(), "the rocket outlived its track");
    assert_eq!(shots(&stub).len(), 2);
    on(cx, &viewport, |v, cx| v.seek(1050.5, cx));
    support::settle(cx, win);
    let again = rocket(&stub)
        .expect("the rocket after seeking back")
        .position[0];
    assert!(
        (again - x).abs() < 1e-3,
        "{again} after seeking back, {x} before"
    );
}

#[gpui_kit::test]
fn playing_moves_the_rocket_with_the_tick(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    on(cx, &viewport, |v, cx| {
        v.set_tracks(shot_tracks(), cx);
        v.toggle_play(cx);
    });
    for _ in 0..10 {
        support::settle(cx, win);
    }
    // Each frame steps the clock, then draws at that tick, so the last draw is at `tick`.
    let tick = cx.update(|cx| viewport.read(cx).playback()).unwrap().tick;
    let x = rocket(&stub).expect("the rocket").position[0];
    assert!(tick > 1001. && tick < 1100., "{tick}");
    assert!(
        (f64::from(x) - (100. + 2. * (tick - 1000.))).abs() < 0.5,
        "rocket x {x} at tick {tick}"
    );
}

#[gpui_kit::test]
fn first_person_hides_the_shot_at_the_eye_and_chase_shows_it(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    on(cx, &viewport, |v, cx| v.set_tracks(shot_tracks(), cx));
    support::settle(cx, win);
    assert_eq!(shots(&stub).len(), 3, "free fly hides nothing");
    on(cx, &viewport, |v, cx| v.cycle_camera(cx));
    support::settle(cx, win);
    let first = shots(&stub);
    assert_eq!(first.len(), 2, "first person: {first:?}");
    assert!(
        first.iter().all(|s| (s.position[0] - 10.).abs() > 1.),
        "{first:?}"
    );
    on(cx, &viewport, |v, cx| v.cycle_camera(cx));
    support::settle(cx, win);
    assert_eq!(shots(&stub).len(), 3, "chase hides nothing");
}

fn sticky_scale(shots: &[ProjectileShot]) -> f32 {
    shots
        .iter()
        .find(|s| s.team == Team::Blue)
        .expect("the sticky")
        .scale
}

// A paused seek to the shown tick: one more draw with the tick and the camera unchanged.
fn redraw(cx: &mut TestAppContext, viewport: &Entity<DemoViewport>, stub: &StubHandle) {
    let (tick, drawn) = (
        cx.update(|cx| viewport.read(cx).playback()).unwrap().tick,
        stub.draws().len(),
    );
    on(cx, viewport, |v, cx| v.seek(tick, cx));
    assert!(stub.draws().len() > drawn, "the seek drew nothing");
}

#[gpui_kit::test]
fn backing_away_grows_the_shots_and_a_still_camera_reuses_them(cx: &mut TestAppContext) {
    let (win, viewport, stub) = viewport_window(cx, StubAnswer::Frames);
    on(cx, &viewport, |v, cx| v.set_tracks(shot_tracks(), cx));
    support::settle(cx, win);
    let near = shots(&stub);
    redraw(cx, &viewport, &stub);
    assert!(
        Arc::ptr_eq(&near, &shots(&stub)),
        "a still camera rebuilt the shots"
    );
    // Hold S to fly back thousands of units, then let the camera coast to rest. A click would
    // follow a player, so the viewport takes the keyboard directly.
    cx.update_window(win, |_, w, cx| {
        viewport.update(cx, |v, cx| v.focus(w, cx));
        w.key_down("s", false, cx);
    })
    .unwrap();
    for _ in 0..120 {
        support::settle(cx, win);
    }
    cx.update_window(win, |_, w, cx| w.key_up("s", cx)).unwrap();
    for _ in 0..60 {
        support::settle(cx, win);
    }
    let far = shots(&stub);
    assert!(
        sticky_scale(&far) > 1. && sticky_scale(&far) > sticky_scale(&near),
        "{near:?} then {far:?}"
    );
    redraw(cx, &viewport, &stub);
    assert!(
        Arc::ptr_eq(&far, &shots(&stub)),
        "a still camera rebuilt the far shots"
    );
}
