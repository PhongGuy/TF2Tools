//! Reading a demo's player tracks: the caps, unreadable files and cancelling.

use std::sync::atomic::AtomicBool;

use tf2tools_core::{
    DemoSnapshot, DemoSnapshotError, DemoTracks, DemoTracksError, MAX_DEMO_BYTES,
    MAX_TRACK_SAMPLES, PlaybackEvent, SnapshotPlayer, Team, TfClass, TrackCut, read_demo_playback,
    read_demo_tracks,
};

#[test]
fn the_caps_hold_the_contract_values() {
    assert_eq!(MAX_DEMO_BYTES, 512 * 1024 * 1024);
    assert_eq!(MAX_TRACK_SAMPLES, 8_000_000);
}

#[test]
fn a_missing_file_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let got = read_demo_tracks(&dir.path().join("none.dem"), &AtomicBool::new(false));
    assert!(
        matches!(got, Err(DemoTracksError::Unreadable(_))),
        "{got:?}"
    );
}

#[test]
fn a_file_shorter_than_a_header_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.dem");
    std::fs::write(&path, b"HL2DEMO\0garbage").unwrap();
    let got = read_demo_tracks(&path, &AtomicBool::new(false));
    assert!(
        matches!(got, Err(DemoTracksError::Unreadable(_))),
        "{got:?}"
    );
}

#[test]
fn a_cancel_before_the_read_is_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.dem");
    std::fs::write(&path, b"HL2DEMO\0garbage").unwrap();
    let got = read_demo_tracks(&path, &AtomicBool::new(true));
    assert!(matches!(got, Err(DemoTracksError::Cancelled)), "{got:?}");
}

fn events_for(path: &std::path::Path, cancel: bool) -> Vec<PlaybackEvent> {
    let mut events = Vec::new();
    read_demo_playback(path, &AtomicBool::new(cancel), |e| {
        events.push(e);
        std::ops::ControlFlow::Continue(())
    });
    events
}

#[test]
fn a_garbage_demo_reports_both_failures() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.dem");
    std::fs::write(&path, b"HL2DEMO\0garbage").unwrap();
    let events = events_for(&path, false);
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(
        matches!(
            events[0],
            PlaybackEvent::Snapshot(Err(DemoSnapshotError::Unreadable(_)))
        ),
        "{events:?}"
    );
    assert!(
        matches!(
            events[1],
            PlaybackEvent::TracksFailed(DemoTracksError::Unreadable(_))
        ),
        "{events:?}"
    );
}

#[test]
fn a_missing_demo_reports_both_failures() {
    let dir = tempfile::tempdir().unwrap();
    let events = events_for(&dir.path().join("none.dem"), false);
    assert_eq!(events.len(), 2, "{events:?}");
}

#[test]
fn a_cancel_before_the_read_sends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.dem");
    std::fs::write(&path, b"HL2DEMO\0garbage").unwrap();
    assert!(events_for(&path, true).is_empty());
}

fn scout_at(tick: u32, x: f32) -> DemoSnapshot {
    DemoSnapshot::new(
        tick,
        vec![SnapshotPlayer::new(
            Team::Blue,
            TfClass::Scout,
            [x, 0., 64.],
            90.,
            0.,
            65.,
            false,
        )],
    )
}

#[test]
fn keyframes_build_one_run_that_interpolates_between_them() {
    let cut = Some(TrackCut::EndedEarly { at_tick: 200 });
    let tracks = DemoTracks::from_keyframes(
        0.015,
        &[scout_at(100, 0.), scout_at(200, 100.)],
        vec![150],
        cut,
    )
    .unwrap();
    assert_eq!((tracks.first_tick(), tracks.last_tick()), (100, 200));
    assert_eq!(tracks.seconds_per_tick(), 0.015);
    assert_eq!(tracks.round_starts(), &[150]);
    assert_eq!(tracks.cut(), cut);
    assert_eq!(
        (
            tracks.player_count(),
            tracks.run_count(),
            tracks.sample_count()
        ),
        (1, 1, 2)
    );
    let mid = tracks.at(150.);
    assert_eq!(mid.tick(), 150);
    let scout = &mid.players()[0];
    assert!((scout.feet()[0] - 50.).abs() < 1e-3, "{:?}", scout.feet());
    assert_eq!((scout.team(), scout.class()), (Team::Blue, TfClass::Scout));
    assert!((scout.yaw() - 90.).abs() < 0.01, "{}", scout.yaw());
}

#[test]
fn keyframes_skip_a_tick_that_does_not_move_on() {
    let tracks = DemoTracks::from_keyframes(
        0.015,
        &[scout_at(100, 0.), scout_at(100, 50.), scout_at(200, 100.)],
        vec![],
        None,
    )
    .unwrap();
    assert_eq!(tracks.sample_count(), 2);
    assert!((tracks.at(150.).players()[0].feet()[0] - 50.).abs() < 1e-3);
}

#[test]
fn keyframes_with_nobody_build_no_tracks() {
    assert!(DemoTracks::from_keyframes(0.015, &[], vec![], None).is_none());
    assert!(
        DemoTracks::from_keyframes(0.015, &[DemoSnapshot::new(5, vec![])], vec![], None).is_none()
    );
}
