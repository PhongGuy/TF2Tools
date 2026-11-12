//! Whether an opened log names tracked players by their nicknames. One global entity holds the
//! choice, so the Settings switch and the logs.tf page agree; it is saved the moment it changes.

use gpui_kit::{App, AppContext as _, Context, Entity, Global};

use crate::local_messages::describe_settings;
use crate::services::run_blocking;

/// The "Show nicknames in match logs" choice: on until the saved value says otherwise.
pub struct LogNicknames {
    show: bool,
    known: bool,
}

struct LogNicknamesGlobal(Entity<LogNicknames>);

impl Global for LogNicknamesGlobal {}

impl LogNicknames {
    /// The app's choice. The first call creates it and starts reading the saved value; a
    /// failed read keeps the default, on. Needs the `Services` global.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<LogNicknamesGlobal>() {
            return global.0.clone();
        }
        let nicknames = cx.new(|_| LogNicknames {
            show: true,
            known: false,
        });
        cx.set_global(LogNicknamesGlobal(nicknames.clone()));
        let load = run_blocking(cx, |s| {
            s.db.settings()
                .map(|settings| settings.show_log_nicknames())
                .map_err(|e| describe_settings(&e).into_op("Load settings"))
        });
        let weak = nicknames.downgrade();
        cx.spawn(async move |cx| {
            let show = load.await.unwrap_or_else(|err| {
                tracing::warn!("the log nicknames setting could not be read: {err}");
                true
            });
            // A `set` that ran first already decided; the read only fills an unknown value.
            let _ = weak.update(cx, |n, cx| {
                if !n.known {
                    n.show = show;
                    n.known = true;
                    cx.notify();
                }
            });
        })
        .detach();
        nicknames
    }

    /// Whether opened logs show nicknames.
    pub fn show(&self) -> bool {
        self.show
    }

    /// Whether the saved value was read or the player chose one.
    pub fn known(&self) -> bool {
        self.known
    }

    /// Takes the player's choice at once and saves it off the UI thread; a failed save is
    /// only logged, as there is no window here.
    pub fn set(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.known && self.show == on {
            return;
        }
        self.show = on;
        self.known = true;
        cx.notify();
        let save = run_blocking(cx, move |s| {
            s.db.set_show_log_nicknames(on)
                .map_err(|e| describe_settings(&e).into_op("Save log nicknames"))
        });
        cx.spawn(async move |_, _| {
            if let Err(err) = save.await {
                tracing::warn!("the log nicknames setting could not be saved: {err}");
            }
        })
        .detach();
    }
}
