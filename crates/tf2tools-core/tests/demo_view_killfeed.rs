//! The killfeed at a tick: the hold window, the newest few, and the same answer after a seek.

use tf2tools_core::{DemoKill, DemoTestKill, DemoTracks, SnapshotPlayer, Team, TfClass};

// One soldier standing off the origin for ticks 0..=2000, at 66 tick.
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
        (0..=2000u32).map(|t| (t, vec![(1, p.clone())])).collect();
    DemoTracks::from_entity_frames(1. / 66., &frames, &[]).expect("tracks")
}

fn plain(tick: u32, victim: u32) -> DemoTestKill<'static> {
    (
        tick,
        Some((2, "killer", Team::Blue, TfClass::Soldier)),
        None,
        (victim, "victim", Team::Red, TfClass::Scout),
        "quake_rl",
        false,
        false,
    )
}

// Kills at ticks 100, 200, .., 700.
fn seven() -> DemoTracks {
    let kills: Vec<_> = (1..=7u32).map(|i| plain(i * 100, i)).collect();
    base().with_test_kills(&kills)
}

fn ticks(feed: &[DemoKill]) -> Vec<u32> {
    feed.iter().map(DemoKill::tick).collect()
}

#[test]
fn the_feed_holds_kills_for_the_hold_window() {
    let t = seven();
    assert_eq!(t.kill_count(), 7);
    assert_eq!(ticks(t.killfeed_at(700., 6., 10)), [400, 500, 600, 700]);
    assert_eq!(ticks(t.killfeed_at(699.9, 6., 10)), [400, 500, 600]);
    assert_eq!(ticks(t.killfeed_at(695., 6., 10)), [300, 400, 500, 600]);
    assert_eq!(ticks(t.killfeed_at(697., 6., 10)), [400, 500, 600]);
    assert_eq!(ticks(t.killfeed_at(100., 6., 10)), [100]);
    assert!(t.killfeed_at(99.99, 6., 10).is_empty());
}

#[test]
fn max_keeps_the_newest() {
    assert_eq!(ticks(seven().killfeed_at(700., 6., 2)), [600, 700]);
}

#[test]
fn seeking_back_and_forth_gives_the_same_feed() {
    let t = seven();
    let first = ticks(t.killfeed_at(500., 6., 5));
    let _ = t.killfeed_at(1000., 6., 5);
    let _ = t.killfeed_at(0., 6., 5);
    assert_eq!(ticks(t.killfeed_at(500., 6., 5)), first);
    assert_eq!(first, [200, 300, 400, 500]);
}

#[test]
fn nan_ticks_empty_holds_and_zero_max_give_nothing() {
    let t = seven();
    assert!(t.killfeed_at(f64::NAN, 6., 5).is_empty());
    assert!(t.killfeed_at(700., 0., 5).is_empty());
    assert!(t.killfeed_at(700., -1., 5).is_empty());
    assert!(t.killfeed_at(700., f32::NAN, 5).is_empty());
    assert!(t.killfeed_at(700., 6., 0).is_empty());
}

#[test]
fn same_tick_kills_keep_their_order() {
    let t = base().with_test_kills(&[plain(800, 8), plain(800, 9)]);
    let victims: Vec<u32> = t
        .killfeed_at(800., 6., 5)
        .iter()
        .map(|k| k.victim().id().entity())
        .collect();
    assert_eq!(victims, [8, 9]);
}

#[test]
fn the_fixture_cleans_like_a_parse_and_keeps_parties() {
    let kills: [DemoTestKill<'_>; 2] = [
        (
            10,
            Some((2, "Killer\u{202e}", Team::Blue, TfClass::Sniper)),
            Some((5, "helper", Team::Blue, TfClass::Medic)),
            (3, "victim", Team::Red, TfClass::Medic),
            "SniperRifle",
            true,
            true,
        ),
        (
            20,
            None,
            None,
            (4, "fell", Team::Red, TfClass::Scout),
            "world",
            false,
            false,
        ),
    ];
    let t = base().with_test_kills(&kills);
    let all = t.kills();
    assert_eq!(all.len(), 2);
    let killer = all[0].killer().expect("killer");
    assert_eq!(
        (
            killer.id().entity(),
            killer.name(),
            killer.team(),
            killer.class()
        ),
        (2, Some("Killer"), Some(Team::Blue), Some(TfClass::Sniper))
    );
    assert_eq!(all[0].victim().class(), Some(TfClass::Medic));
    assert_eq!(all[0].assister().and_then(|p| p.name()), Some("helper"));
    assert!(all[1].assister().is_none());
    assert_eq!(all[0].weapon(), Some("sniperrifle"));
    assert!(all[0].critical() && all[0].feigned());
    assert!(all[1].killer().is_none());
    assert_eq!(all[1].victim().name(), Some("fell"));
}
