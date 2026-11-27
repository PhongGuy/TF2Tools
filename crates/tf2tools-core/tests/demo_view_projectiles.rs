//! Projectiles at a tick: the curve, the hold, the heading and the order.

use tf2tools_core::{
    DemoTestShot, DemoTracks, MAX_PROJECTILE_SAMPLES, ProjectileKind, SnapshotPlayer, Team, TfClass,
};

// One soldier standing off the origin for ticks 0..=200, at 66 tick.
fn base() -> DemoTracks {
    let p = SnapshotPlayer::new(
        Team::Red,
        TfClass::Soldier,
        [10., 10., 10.],
        0.,
        0.,
        68.,
        false,
    );
    let frames: Vec<(u32, Vec<(u32, SnapshotPlayer)>)> =
        (0..=200u32).map(|t| (t, vec![(1, p.clone())])).collect();
    DemoTracks::from_entity_frames(1. / 66., &frames, &[]).expect("tracks")
}

fn rocket(serial: u32, ticks: std::ops::RangeInclusive<u32>) -> Vec<DemoTestShot> {
    ticks
        .map(|t| {
            let x = 100. + 20. * (t as f32 - 10.);
            (
                t,
                serial,
                ProjectileKind::Rocket,
                Team::Blue,
                true,
                [x, 50., 100.],
            )
        })
        .collect()
}

#[test]
fn the_cap_holds_the_contract_value() {
    assert_eq!(MAX_PROJECTILE_SAMPLES, 4_000_000);
}

#[test]
fn a_straight_shot_lies_on_its_line_at_a_fractional_tick() {
    let tracks = base().with_test_projectiles(&rocket(1, 10..=20));
    assert_eq!(
        (tracks.projectile_count(), tracks.projectile_sample_count()),
        (1, 11)
    );
    let got = tracks.projectiles_at(15.5);
    assert_eq!(got.len(), 1);
    let v = got[0];
    assert_eq!(
        (v.kind(), v.team(), v.critical()),
        (ProjectileKind::Rocket, Team::Blue, true)
    );
    let p = v.position();
    assert!(
        (p[0] - 210.).abs() < 1e-3 && p[1] == 50. && p[2] == 100.,
        "{p:?}"
    );
    let h = v.heading().expect("moving");
    assert!(
        (h[0] - 1.).abs() < 1e-5 && h[1].abs() < 1e-5 && h[2].abs() < 1e-5,
        "{h:?}"
    );
}

#[test]
fn the_last_sample_is_held_one_tick_then_gone() {
    let tracks = base().with_test_projectiles(&rocket(1, 10..=20));
    let x = |t: f64| tracks.projectiles_at(t).first().map(|v| v.position()[0]);
    assert_eq!(x(9.99), None);
    assert_eq!(x(10.), Some(100.));
    assert_eq!(x(20.), Some(300.));
    assert_eq!(x(20.9), Some(300.));
    assert_eq!(x(21.), None);
    assert!(tracks.projectiles_at(f64::NAN).is_empty());
}

#[test]
fn a_resting_sticky_keeps_its_landing_heading_and_a_still_one_has_none() {
    let mut rows: Vec<DemoTestShot> = (30..=60u32)
        .map(|t| {
            let y = 500. - 10. * (t.min(33) - 30) as f32;
            (t, 2, ProjectileKind::Sticky, Team::Red, false, [5., y, 5.])
        })
        .collect();
    rows.extend((30..=40u32).map(|t| (t, 3, ProjectileKind::Pipe, Team::Red, false, [7., 7., 7.])));
    let tracks = base().with_test_projectiles(&rows);
    let at = tracks.projectiles_at(50.);
    let sticky = at
        .iter()
        .find(|v| v.kind() == ProjectileKind::Sticky)
        .expect("sticky");
    assert_eq!(sticky.position(), [5., 470., 5.]);
    let h = sticky.heading().expect("it moved before landing");
    assert!(h[0].abs() < 1e-5 && (h[1] + 1.).abs() < 1e-5, "{h:?}");
    let still = tracks.projectiles_at(35.);
    let pipe = still
        .iter()
        .find(|v| v.kind() == ProjectileKind::Pipe)
        .expect("pipe");
    assert_eq!(pipe.heading(), None);
}

#[test]
fn a_bounce_never_overshoots() {
    let zs = [100., 150., 180., 190., 180., 150.];
    let rows: Vec<DemoTestShot> = zs
        .iter()
        .zip(0u32..)
        .map(|(&z, t)| (t, 4, ProjectileKind::Pipe, Team::Red, false, [1., 1., z]))
        .collect();
    let tracks = base().with_test_projectiles(&rows);
    for i in 0..=500 {
        let t = f64::from(i) / 100.;
        let z = tracks.projectiles_at(t)[0].position()[2];
        assert!((100. ..=190.001).contains(&z), "{t} {z}");
    }
}

#[test]
fn projectiles_come_in_first_tick_order_and_the_origin_makes_none() {
    let mut rows = Vec::new();
    rows.extend((2..=10u32).map(|t| (t, 4, ProjectileKind::Flare, Team::Red, false, [1., 2., 3.])));
    rows.extend(
        (0..=10u32).map(|t| (t, 5, ProjectileKind::Rocket, Team::Red, false, [4., 5., 6.])),
    );
    rows.extend((0..=10u32).map(|t| (t, 6, ProjectileKind::Arrow, Team::Red, false, [0., 0., 0.])));
    let tracks = base().with_test_projectiles(&rows);
    let kinds: Vec<ProjectileKind> = tracks.projectiles_at(5.).iter().map(|v| v.kind()).collect();
    assert_eq!(kinds, [ProjectileKind::Rocket, ProjectileKind::Flare]);
    assert_eq!(tracks.projectile_count(), 2);
}
