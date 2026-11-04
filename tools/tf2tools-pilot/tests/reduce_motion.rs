use std::ffi::OsStr;

use gpui_kit::TestAppContext;
use tf2tools_pilot::{REDUCE_MOTION_VAR, apply_reduce_motion};

// Headless, the kit never hears the system's setting, so the pilot's variable is the only
// way a QA run reaches the reduced-motion paths.
#[gpui_kit::test]
fn the_reduce_motion_variable_turns_reduced_motion_on(cx: &mut TestAppContext) {
    assert_eq!(REDUCE_MOTION_VAR, "TF2TOOLS_PILOT_REDUCE_MOTION");
    cx.update(|cx| {
        apply_reduce_motion(Some(OsStr::new("1")), cx);
        assert!(cx.reduce_motion());
    });
}

#[gpui_kit::test]
fn an_unset_or_other_value_leaves_motion_alone(cx: &mut TestAppContext) {
    cx.update(|cx| {
        for value in [None, Some(OsStr::new("")), Some(OsStr::new("0"))] {
            apply_reduce_motion(value, cx);
            assert!(!cx.reduce_motion(), "{value:?}");
        }
    });
}
