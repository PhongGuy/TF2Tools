//! The vendored gpui-pre-linux pointer lock's pure parts: virtual position and X11 warp echo filtering.

use gpui_linux::pointer_lock::{
    PointerLockState, VirtualPointer, WarpTracker, lock_pointer, unlock_pointer,
};

// Deltas come from float subtraction, so compare within a hundredth of a pixel.
fn near(got: (f32, f32), want: (f32, f32), what: &str) {
    assert!(
        (got.0 - want.0).abs() < 0.01 && (got.1 - want.1).abs() < 0.01,
        "{what}: {got:?} != {want:?}"
    );
}

#[test]
fn the_virtual_pointer_adds_deltas_without_bounds() {
    let mut p = VirtualPointer::new((100.0, 50.0));
    assert_eq!(p.apply(10.0, -5.0), (110.0, 45.0), "first delta");
    assert_eq!(
        p.apply(-500.0, 0.0),
        (-390.0, 45.0),
        "past the window's left edge"
    );
    assert_eq!(
        p.position(),
        (-390.0, 45.0),
        "position matches the last apply"
    );
}

#[test]
fn a_tracker_returns_motion_as_deltas() {
    let mut t = WarpTracker::new((400.0, 300.0));
    assert_eq!(t.motion((410.0, 295.0)), (10.0, -5.0), "first motion");
    assert_eq!(t.motion((412.0, 295.0)), (2.0, 0.0), "second motion");
}

#[test]
fn the_warps_own_motion_is_ignored_and_later_motion_is_from_the_centre() {
    let mut t = WarpTracker::new((400.0, 300.0));
    assert_eq!(t.motion((450.0, 300.0)), (50.0, 0.0), "away from centre");
    assert!(
        t.should_warp((400.0, 300.0)),
        "away from centre asks for a warp"
    );
    assert!(
        !t.should_warp((400.0, 300.0)),
        "no second warp while one is in flight"
    );
    // A motion queued before the warp still counts from the old position.
    assert_eq!(t.motion((455.0, 300.0)), (5.0, 0.0), "pre-warp motion");
    // The echo, rounded by the client's FP16.16 conversion.
    near(
        t.motion((400.4, 299.6)),
        (0.0, 0.0),
        "the echo turns nothing",
    );
    near(
        t.motion((403.0, 300.0)),
        (2.6, 0.4),
        "after the echo, deltas count from the echo",
    );
}

#[test]
fn a_pointer_at_the_centre_needs_no_warp() {
    let mut t = WarpTracker::new((400.0, 300.0));
    assert!(!t.should_warp((400.0, 300.0)), "already at the centre");
}

#[test]
fn without_a_running_client_locking_is_unsupported() {
    assert_eq!(
        lock_pointer(),
        PointerLockState::Unsupported,
        "no client on this thread"
    );
    unlock_pointer();
}
