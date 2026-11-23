//! The demo snapshot: the header's tick count, the snapshot types, and reading players.

use crate::common::demo_view::demo_header;
use tf2tools_core::{
    DemoSnapshot, DemoSnapshotError, MAX_SNAPSHOT_BYTES, SnapshotPlayer, Team, TfClass,
    read_demo_header,
};

#[test]
fn the_header_counts_its_ticks() {
    // The fixture writes 66 ticks a second.
    let header = read_demo_header(&demo_header("cp_process_final", 60.0)).unwrap();
    assert_eq!(header.ticks(), 3960);
}

#[test]
fn a_negative_tick_count_reads_as_zero() {
    let mut dem = demo_header("cp_process_final", 60.0);
    dem[1060..1064].copy_from_slice(&(-5i32).to_le_bytes());
    assert_eq!(read_demo_header(&dem).unwrap().ticks(), 0);
}

#[test]
fn a_snapshot_keeps_its_tick_and_its_players_in_order() {
    let red = SnapshotPlayer::new(
        Team::Red,
        TfClass::Scout,
        [1., 2., 3.],
        90.,
        -10.,
        65.,
        false,
    );
    let blu = SnapshotPlayer::new(
        Team::Blue,
        TfClass::Heavy,
        [4., 5., 6.],
        270.,
        5.,
        45.,
        true,
    );
    let snapshot = DemoSnapshot::new(1733, vec![red.clone(), blu.clone()]);
    assert_eq!(snapshot.tick(), 1733);
    assert_eq!(snapshot.players(), [red, blu]);
    let p = &snapshot.players()[1];
    assert_eq!(
        (
            p.team(),
            p.class(),
            p.feet(),
            p.yaw(),
            p.pitch(),
            p.eye_height(),
            p.ducked()
        ),
        (
            Team::Blue,
            TfClass::Heavy,
            [4., 5., 6.],
            270.,
            5.,
            45.,
            true
        )
    );
}

#[test]
fn the_snapshot_errors_read_plainly() {
    assert_eq!(
        DemoSnapshotError::NoPlayers.to_string(),
        "the demo shows no players to draw"
    );
    assert_eq!(
        DemoSnapshotError::Unreadable("bad packet".into()).to_string(),
        "the demo's players cannot be read: bad packet"
    );
}

#[test]
fn at_most_64_mib_of_a_demo_is_read_for_players() {
    assert_eq!(MAX_SNAPSHOT_BYTES, 64 * 1024 * 1024);
}
