//! Writes back mastercomfig's setup_hook.cfg and addons.cfg when another tool cleared them.
//! The first time this start finds TF2, the files are checked off the UI thread; a failure is
//! only logged, since the Graphics page retries the restore whenever it opens.

use gpui_kit::{App, Global};
use tf2tools_core::Tf2Install;

use crate::services::{Tf2State, run_blocking, tf2_state};

// Set once the restore has run this start, so a later find of the game never runs it again.
struct Restored;

impl Global for Restored {}

/// Watches the TF2 state and restores the files once, the first time it is `Found`. Needs the
/// `Services` and `Tf2Global` globals.
pub fn install(cx: &mut App) {
    let state = tf2_state(cx);
    cx.observe(&state, |state, cx| {
        if let Tf2State::Found(tf2) = state.read(cx) {
            let tf2 = tf2.clone();
            restore(tf2, cx);
        }
    })
    .detach();
    if let Tf2State::Found(tf2) = state.read(cx).clone() {
        restore(tf2, cx);
    }
}

fn restore(tf2: Tf2Install, cx: &mut App) {
    if cx.has_global::<Restored>() {
        return;
    }
    cx.set_global(Restored);
    run_blocking(cx, move |s| {
        if let Err(e) = s.library.restore_mastercomfig_files(&tf2) {
            tracing::warn!("mastercomfig restore at start: {e}");
        }
        Ok(())
    })
    .detach();
}
