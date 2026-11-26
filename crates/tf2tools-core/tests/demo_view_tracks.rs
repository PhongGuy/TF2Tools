//! Reading a demo's player tracks: the caps, unreadable files and cancelling.

use std::sync::atomic::AtomicBool;

use tf2tools_core::{DemoTracksError, MAX_DEMO_BYTES, MAX_TRACK_SAMPLES, read_demo_tracks};

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
