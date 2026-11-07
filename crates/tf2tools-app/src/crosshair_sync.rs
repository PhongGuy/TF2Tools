//! Keeps `tf/custom` matching the Crosshairs board: every assignment change asks for an apply,
//! which runs off the UI thread with at most one in flight and one rerun queued behind it.
//! Without TF2 the change waits and applies once the game is found. A failure toasts once.

use gpui_kit::{AnyWindowHandle, App, AppContext as _, Context, Entity, Global, Subscription};
use tf2tools_core::{LogLevel, NewLogEntry};

use crate::activity;
use crate::local_messages::describe_tf2_install;
use crate::pages::crosshairs::skipped_description;
use crate::pages::profiles::probe::Tf2Probe;
use crate::services::{Tf2State, run_blocking, tf2_state};
use crate::shell::toast::toast_error;

/// Every failed apply toasts under this action.
const FAILED: &str = "Couldn't save your crosshairs to TF2";

/// Where the last change stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SyncStatus {
    /// Nothing applied this session.
    #[default]
    Idle,
    /// An apply runs.
    Saving,
    /// The game folder matches the board.
    InGame,
    /// Written while TF2 runs, so the game picks it up on its next start.
    InGameWhileRunning,
    /// Written, but these weapons were left out: the apply report's skip line.
    Skipped(String),
    /// Saved to the library; waits for TF2 to be found.
    WaitingForTf2,
    /// The player took every crosshair out of the game; the board choices stay.
    Removed,
    /// The last apply failed; it already toasted.
    Failed,
}

impl SyncStatus {
    /// The page's quiet status line. `assigned` says whether any weapon has a crosshair and
    /// `waiting` how many weapons an apply would change, both read when nothing applied yet.
    pub fn line(&self, assigned: bool, waiting: usize) -> String {
        match self {
            SyncStatus::Saving => "Saving to TF2...".into(),
            SyncStatus::InGame => "In TF2".into(),
            SyncStatus::InGameWhileRunning => {
                "Saved. TF2 is running, so it takes effect next time TF2 starts".into()
            }
            SyncStatus::Skipped(skipped) => format!("In TF2. {skipped}"),
            SyncStatus::WaitingForTf2 => "Saved. It goes into TF2 once TF2 is found".into(),
            SyncStatus::Removed => "Removed from TF2. Your next change puts them back".into(),
            SyncStatus::Failed => "Couldn't save to TF2".into(),
            SyncStatus::Idle if waiting > 0 => {
                "Not in TF2 yet. Your next change puts them in".into()
            }
            SyncStatus::Idle if !assigned => "TF2 is using its own crosshairs".into(),
            SyncStatus::Idle => "In TF2".into(),
        }
    }
}

/// The app's one crosshair apply queue.
pub struct CrosshairSync {
    win: AnyWindowHandle,
    status: SyncStatus,
    in_flight: bool,
    /// A change landed while an apply ran; one more apply follows it with the latest table.
    queued: bool,
    /// A change landed without TF2; the apply runs once the game is found.
    waiting_for_tf2: bool,
    /// Applies started, for tests and the page's reload.
    started: usize,
    /// Applies finished, successful or not.
    finished: usize,
    _tf2: Subscription,
}

struct SyncGlobal(Entity<CrosshairSync>);

impl Global for SyncGlobal {}

impl CrosshairSync {
    /// The installed queue, installing one that toasts into `win` if there is none. Needs the
    /// `Services` and `Tf2Global` globals.
    pub fn get_or_install(win: AnyWindowHandle, cx: &mut App) -> Entity<Self> {
        if let Some(sync) = Self::global(cx) {
            return sync;
        }
        let state = tf2_state(cx);
        let sync = cx.new(|cx| {
            // Finding the game runs the apply a change without it was waiting for.
            let tf2 = cx.observe(&state, |sync: &mut Self, state, cx| {
                if sync.waiting_for_tf2 && matches!(state.read(cx), Tf2State::Found(_)) {
                    sync.waiting_for_tf2 = false;
                    sync.request(cx);
                }
            });
            Self {
                win,
                status: SyncStatus::Idle,
                in_flight: false,
                queued: false,
                waiting_for_tf2: false,
                started: 0,
                finished: 0,
                _tf2: tf2,
            }
        });
        cx.set_global(SyncGlobal(sync.clone()));
        sync
    }

    /// The installed queue, if any.
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<SyncGlobal>().map(|g| g.0.clone())
    }

    /// Asks the installed queue for an apply; does nothing when none is installed.
    pub fn request_global(cx: &mut App) {
        if let Some(sync) = Self::global(cx) {
            sync.update(cx, |sync, cx| sync.request(cx));
        }
    }

    pub fn status(&self) -> &SyncStatus {
        &self.status
    }

    /// Whether an apply runs now.
    pub fn is_saving(&self) -> bool {
        self.in_flight
    }

    pub fn started(&self) -> usize {
        self.started
    }

    pub fn finished(&self) -> usize {
        self.finished
    }

    /// Records that the player took every crosshair out of the game.
    pub fn mark_removed(&mut self, cx: &mut Context<Self>) {
        self.status = SyncStatus::Removed;
        cx.notify();
    }

    /// Brings the game folder up to the assignment table: now, after the apply in flight, or
    /// once TF2 is found.
    pub fn request(&mut self, cx: &mut Context<Self>) {
        if self.in_flight {
            self.queued = true;
            return;
        }
        let Some(tf2) = tf2_state(cx).read(cx).install().cloned() else {
            self.waiting_for_tf2 = true;
            self.status = SyncStatus::WaitingForTf2;
            cx.notify();
            return;
        };
        self.in_flight = true;
        self.started += 1;
        self.status = SyncStatus::Saving;
        cx.notify();
        let running = Tf2Probe::get(cx);
        let applying = run_blocking(cx, move |s| {
            let report = s
                .library
                .apply_crosshairs(&tf2, &s.builtins)
                .map_err(|e| describe_tf2_install(&e).into_op(FAILED))?;
            if !report.applied().is_empty() {
                let entry = NewLogEntry::new(
                    LogLevel::Info,
                    "crosshair",
                    "install",
                    format!("Installed {} crosshair assignments", report.applied().len()),
                );
                activity::write(s, &entry);
            }
            Ok((skipped_description(&report), running()))
        });
        cx.spawn(async move |sync, cx| {
            let done = applying.await;
            let _ = sync.update(cx, |sync, cx| {
                sync.in_flight = false;
                sync.finished += 1;
                sync.status = match &done {
                    Ok((Some(skipped), _)) => SyncStatus::Skipped(skipped.clone()),
                    Ok((None, true)) => SyncStatus::InGameWhileRunning,
                    Ok((None, false)) => SyncStatus::InGame,
                    Err(_) => SyncStatus::Failed,
                };
                if let Err(err) = &done {
                    let _ = sync
                        .win
                        .update(cx, |_, window, cx| toast_error(window, cx, err));
                }
                cx.notify();
                if std::mem::take(&mut sync.queued) {
                    sync.request(cx);
                }
            });
        })
        .detach();
    }
}
