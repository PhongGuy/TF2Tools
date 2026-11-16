//! The background job that drops old full logs.tf logs and measures what the cache takes. One
//! global entity runs at most one pass at a time; the first use, every finished sync and a new
//! keep choice each ask for a pass.

use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, Global};
use tf2tools_core::{KeepFullLogs, LogsSpace};

use crate::logs_sync::{LogsSyncEvent, logs_sync};
use crate::services::{OpError, run_blocking};

/// What observers hear from the trim job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogsTrimEvent {
    /// A pass ended, trimmed or failed; the cache may have changed.
    Landed,
}

/// The trim job: the keep choice and figures of its last pass, and whether one runs or waits.
#[derive(Default)]
pub struct LogsTrim {
    keep: Option<KeepFullLogs>,
    space: Option<LogsSpace>,
    running: bool,
    queued: bool,
    // A choice the player made that the next pass saves before it trims.
    pending: Option<KeepFullLogs>,
    passes: u64,
    failed: bool,
}

struct LogsTrimGlobal(Entity<LogsTrim>);

impl Global for LogsTrimGlobal {}

impl EventEmitter<LogsTrimEvent> for LogsTrim {}

impl LogsTrim {
    /// The app's job. The first call creates it, subscribes it to every
    /// `LogsSyncEvent::Finished` and runs its first pass. Needs the `Services` global.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(trim) = Self::try_global(cx) {
            return trim;
        }
        let trim = cx.new(|_| LogsTrim::default());
        cx.set_global(LogsTrimGlobal(trim.clone()));
        let weak = trim.downgrade();
        let sync = logs_sync(cx);
        cx.subscribe(&sync, move |_, event: &LogsSyncEvent, cx| {
            if matches!(event, LogsSyncEvent::Finished(_)) {
                let _ = weak.update(cx, |trim, cx| trim.request(cx));
            }
        })
        .detach();
        trim.update(cx, |trim, cx| trim.request(cx));
        trim
    }

    /// The app's job if anything has used it yet, for readers that must not create it.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<LogsTrimGlobal>().map(|g| g.0.clone())
    }

    /// Runs a pass now, or once more after the running one; requests during a run fold into one.
    pub fn request(&mut self, cx: &mut Context<Self>) {
        if self.running {
            self.queued = true;
            return;
        }
        self.running = true;
        cx.notify();
        let pending = self.pending.take();
        let pass = run_blocking(cx, move |s| {
            let cache = s.logs.cache();
            let fail = |e| OpError::core("Trim the logs cache", e);
            if let Some(keep) = pending {
                cache.set_keep_full_logs(keep).map_err(fail)?;
            }
            let keep = cache.keep_full_logs().map_err(fail)?;
            let now = chrono::Utc::now().timestamp();
            cache.trim(keep, now).map_err(fail)?;
            let space = cache.space(now).map_err(fail)?;
            Ok((keep, space))
        });
        cx.spawn(async move |trim, cx| {
            let landed = pass.await;
            let _ = trim.update(cx, |trim, cx| trim.land(landed, pending, cx));
        })
        .detach();
    }

    // A failed pass keeps the old figures and is only logged, as a background job has no window.
    // The choice it carried waits again for the next pass, unless a newer one replaced it.
    fn land(
        &mut self,
        landed: Result<(KeepFullLogs, LogsSpace), OpError>,
        carried: Option<KeepFullLogs>,
        cx: &mut Context<Self>,
    ) {
        match landed {
            Ok((keep, space)) => {
                // A choice made during the pass is newer than the one the pass read.
                if self.pending.is_none() {
                    self.keep = Some(keep);
                }
                self.space = Some(space);
                self.failed = false;
            }
            Err(err) => {
                tracing::warn!("the logs cache could not be trimmed: {err}");
                self.failed = true;
                self.pending = self.pending.or(carried);
            }
        }
        self.running = false;
        self.passes += 1;
        cx.emit(LogsTrimEvent::Landed);
        cx.notify();
        if std::mem::take(&mut self.queued) {
            self.request(cx);
        }
    }

    /// Takes the player's keep choice at once; the next pass saves it, then trims by it.
    pub fn set_keep(&mut self, keep: KeepFullLogs, cx: &mut Context<Self>) {
        self.keep = Some(keep);
        self.pending = Some(keep);
        cx.notify();
        self.request(cx);
    }

    /// The keep choice, None until the first pass lands or the player picks one.
    pub fn keep(&self) -> Option<KeepFullLogs> {
        self.keep
    }

    /// What the cache took at the last pass that succeeded.
    pub fn space(&self) -> Option<&LogsSpace> {
        self.space.as_ref()
    }

    /// How many passes have landed, failed ones included.
    pub fn passes(&self) -> u64 {
        self.passes
    }

    /// Whether the last pass that landed failed; a later pass that succeeds clears it.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Whether a pass is running now.
    pub fn is_running(&self) -> bool {
        self.running
    }
}
