//! Reading a demo's player tracks: the caps, unreadable files and cancelling.

use std::sync::atomic::AtomicBool;

use tf2tools_core::{
    DemoSnapshotError, DemoTracksError, MAX_DEMO_BYTES, MAX_TRACK_SAMPLES, PlaybackEvent,
    read_demo_playback, read_demo_tracks,
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
