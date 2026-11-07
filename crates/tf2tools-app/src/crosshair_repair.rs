//! Repairs the crosshair-only weapon scripts apply wrote before 6.0.2, which stop the weapon
//! from firing after the TF2 update of 2026-11-06. The first time this start finds TF2, the
//! managed folder is checked off the UI thread and, when it needs it, applied again.

use gpui_kit::{AnyWindowHandle, App, Global, SharedString};
use tf2tools_core::{LogLevel, NewLogEntry, Tf2Install, crosshair_scripts_need_repair};

use crate::activity;
use crate::library_events::LibraryEvents;
use crate::local_messages::describe_tf2_install;
use crate::pages::crosshairs::skipped_description;
use crate::services::{Tf2State, run_blocking, tf2_state};
use crate::shell::toast::{ToastKind, toast, toast_error};

const FIXED: &str = "Fixed your crosshairs for the latest TF2 update";
const FAILED: &str = "Couldn't fix your crosshairs for the latest TF2 update";

// Set once the check has run this start, so a later find of the game never checks again.
struct Checked;

impl Global for Checked {}

/// Watches the TF2 state and runs the check once, the first time it is `Found`; toasts go to
/// `win`. Needs the `Services` and `Tf2Global` globals.
pub fn install(win: AnyWindowHandle, cx: &mut App) {
    let state = tf2_state(cx);
    cx.observe(&state, move |state, cx| {
        if let Tf2State::Found(tf2) = state.read(cx) {
            let tf2 = tf2.clone();
            check(win, tf2, cx);
        }
    })
    .detach();
    if let Tf2State::Found(tf2) = state.read(cx).clone() {
        check(win, tf2, cx);
    }
}

// Checks and, when needed, applies the saved assignments as the Apply button does. A failure
// toasts once; there is no retry.
fn check(win: AnyWindowHandle, tf2: Tf2Install, cx: &mut App) {
    if cx.has_global::<Checked>() {
        return;
    }
    cx.set_global(Checked);
    let repairing = run_blocking(cx, move |s| {
        if !crosshair_scripts_need_repair(&tf2) {
            return Ok(None);
        }
        let report = s
            .library
            .apply_crosshairs(&tf2, &s.builtins)
            .map_err(|e| describe_tf2_install(&e).into_op(FAILED))?;
        let entry = NewLogEntry::new(
            LogLevel::Info,
            "crosshair",
            "repair",
            format!(
                "Repaired {} crosshair weapon scripts for the latest TF2 update",
                report.applied().len()
            ),
        );
        activity::write(s, &entry);
        Ok(Some(skipped_description(&report)))
    });
    cx.spawn(async move |cx| {
        let done = repairing.await;
        cx.update(|cx| {
            let repaired = matches!(done, Ok(Some(_)));
            let _ = win.update(cx, |_, window, cx| match done {
                Ok(None) => {}
                Ok(Some(None)) => toast(window, cx, ToastKind::Success, FIXED, ""),
                Ok(Some(Some(skipped))) => toast(
                    window,
                    cx,
                    ToastKind::Warning,
                    FIXED,
                    SharedString::from(skipped),
                ),
                Err(err) => toast_error(window, cx, &err),
            });
            if repaired {
                LibraryEvents::changed(cx);
            }
        });
    })
    .detach();
}
