//! Whether mature GameBanana add-ons are listed. One global entity holds the choice, so the
//! Settings switch and every add-on tab see the same value; it is saved the moment it changes.

use gpui_kit::{App, AppContext as _, Context, Entity, Global};

use crate::local_messages::describe_settings;
use crate::services::run_blocking;

/// The "Show mature add-ons" choice, unknown until the saved value is read.
pub struct MatureAddons {
    show: bool,
    known: bool,
}

struct MatureGlobal(Entity<MatureAddons>);

impl Global for MatureGlobal {}

impl MatureAddons {
    /// The app's choice. The first call creates it unknown and starts reading the saved value;
    /// a failed read counts as off. Needs the `Services` global.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<MatureGlobal>() {
            return global.0.clone();
        }
        let mature = cx.new(|_| MatureAddons {
            show: false,
            known: false,
        });
        cx.set_global(MatureGlobal(mature.clone()));
        let load = run_blocking(cx, |s| {
            s.db.settings()
                .map(|settings| settings.show_mature_addons())
                .map_err(|e| describe_settings(&e).into_op("Load settings"))
        });
        let weak = mature.downgrade();
        cx.spawn(async move |cx| {
            let show = load.await.unwrap_or_else(|err| {
                tracing::warn!("the mature add-ons setting could not be read: {err}");
                false
            });
            // A `set` that ran first already decided; the read only fills an unknown value.
            let _ = weak.update(cx, |m, cx| {
                if !m.known {
                    m.show = show;
                    m.known = true;
                    cx.notify();
                }
            });
        })
        .detach();
        mature
    }

    /// Whether mature add-ons are listed; false while unknown.
    pub fn show(&self) -> bool {
        self.show
    }

    /// Whether the saved value was read or the player chose one.
    pub fn known(&self) -> bool {
        self.known
    }

    /// Takes the player's choice at once and saves it off the UI thread. There is no window
    /// here, so a failed save is only logged.
    pub fn set(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.known && self.show == on {
            return;
        }
        self.show = on;
        self.known = true;
        cx.notify();
        let save = run_blocking(cx, move |s| {
            s.db.set_show_mature_addons(on)
                .map_err(|e| describe_settings(&e).into_op("Save mature add-ons"))
        });
        cx.spawn(async move |_, _| {
            if let Err(err) = save.await {
                tracing::warn!("the mature add-ons setting could not be saved: {err}");
            }
        })
        .detach();
    }
}
