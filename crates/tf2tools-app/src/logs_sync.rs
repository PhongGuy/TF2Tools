//! The app-wide logs.tf sync: one `LogsClient::sync_player` run on the tokio bridge, observed
//! by the logs.tf page and the shell header, so a sync keeps going while the page is hidden.

use std::time::Duration;

use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use tf2tools_core::{
    LogState, LogsCache, LogsCancel, LogsProgress, LogsRange, LogsSyncError, LogsSyncSummary,
    SteamId,
};

use crate::pages::logs_tf::text::{LOGS_TF_DOWN, sync_message};
use crate::services::Services;
use crate::tokio_bridge;

/// Where a running sync stands, in the counts the page and header show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncProgress {
    /// Reading the player's list from logs.tf.
    Finding { listed: usize, total: usize },
    /// Downloading log bodies, counting those an earlier run already saved.
    Downloading {
        done: usize,
        total: usize,
        left: Duration,
    },
}

/// `done` and `total` from core plus `base` logs already saved; `left` = spacing x (total - done).
pub fn downloading(done: usize, total: usize, spacing: Duration, base: usize) -> SyncProgress {
    let remaining = u32::try_from(total.saturating_sub(done)).unwrap_or(u32::MAX);
    SyncProgress::Downloading {
        done: done.saturating_add(base),
        total: total.saturating_add(base),
        left: spacing.saturating_mul(remaining),
    }
}

/// What observers hear from the sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogsSyncEvent {
    /// The running player's progress moved.
    Progress(SteamId),
    /// The player's run ended, finished, failed or stopped.
    Finished(SteamId),
}

/// One sync request: whose logs, how far back, and whether to skip the freshness check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Request {
    id: SteamId,
    range: LogsRange,
    force: bool,
}

/// The running sync; dropping `_task` aborts the tokio job with it. `ended` closes when the
/// sync future is gone, which is what `stop` waits for.
struct Run {
    request: Request,
    progress: Option<SyncProgress>,
    cancel: LogsCancel,
    ended: async_channel::Receiver<()>,
    _task: Task<()>,
}

/// The one logs.tf sync the app runs, the one request waiting behind it, and the last failure
/// kept for the page's error line.
#[derive(Default)]
pub struct LogsSync {
    run: Option<Run>,
    queued: Option<Request>,
    error: Option<(SteamId, &'static str)>,
}

struct LogsSyncGlobal(Entity<LogsSync>);
impl Global for LogsSyncGlobal {}

/// The app's logs.tf sync, created on first call.
pub fn logs_sync(cx: &mut App) -> Entity<LogsSync> {
    if let Some(sync) = try_logs_sync(cx) {
        return sync;
    }
    let sync = cx.new(|_| LogsSync::default());
    cx.set_global(LogsSyncGlobal(sync.clone()));
    sync
}

/// The app's logs.tf sync if anything has used it yet, for readers that must not create it.
pub fn try_logs_sync(cx: &App) -> Option<Entity<LogsSync>> {
    cx.try_global::<LogsSyncGlobal>().map(|g| g.0.clone())
}

impl EventEmitter<LogsSyncEvent> for LogsSync {}

impl LogsSync {
    /// Starts syncing `id` over `range`. A run of this same request carries on; any other
    /// request cancels the run and waits in the queue, replacing whatever waited there.
    pub fn start(&mut self, id: SteamId, range: LogsRange, force: bool, cx: &mut Context<Self>) {
        let request = Request { id, range, force };
        match &self.run {
            None => self.spawn(request, cx),
            Some(run) if run.request == request && !force && !run.cancel.is_cancelled() => {
                self.queued = None;
            }
            // Cancel rather than abort, so a list page already in flight is still recorded.
            Some(run) => {
                run.cancel.cancel();
                self.queued = Some(request);
            }
        }
        cx.notify();
    }

    /// Drops `id` from the queue and cancels their run; the task resolves once it has ended.
    pub fn stop(&mut self, id: SteamId, cx: &mut Context<Self>) -> Task<()> {
        if self.queued.is_some_and(|queued| queued.id == id) {
            self.queued = None;
            cx.notify();
        }
        match self.run.as_ref().filter(|run| run.request.id == id) {
            Some(run) => {
                run.cancel.cancel();
                let ended = run.ended.clone();
                cx.background_spawn(async move {
                    let _ = ended.recv().await;
                })
            }
            None => Task::ready(()),
        }
    }

    /// The player waiting for the running sync to end.
    pub fn queued(&self) -> Option<SteamId> {
        self.queued.map(|request| request.id)
    }

    /// The player whose sync is running.
    pub fn running(&self) -> Option<SteamId> {
        self.run.as_ref().map(|run| run.request.id)
    }

    /// Whether a sync is running or waiting to.
    pub fn busy(&self) -> bool {
        self.run.is_some() || self.queued.is_some()
    }

    /// The running player's progress, once core has reported any.
    pub fn progress(&self) -> Option<SyncProgress> {
        self.run.as_ref().and_then(|run| run.progress)
    }

    /// Why `id`'s last sync failed, in player words.
    pub fn error(&self, id: SteamId) -> Option<&'static str> {
        self.error
            .and_then(|(failed, message)| (failed == id).then_some(message))
    }

    fn spawn(&mut self, request: Request, cx: &mut Context<Self>) {
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        if self.error.is_some_and(|(id, _)| id == request.id) {
            self.error = None;
        }
        let client = cx.global::<Services>().logs.clone();
        let cancel = LogsCancel::new();
        let run_cancel = cancel.clone();
        let (ended_tx, ended) = async_channel::bounded::<()>(1);
        // Only the newest progress is kept, so a slow frame skips counts rather than queueing them.
        let (tx, mut rx) = tokio::sync::watch::channel(None::<SyncProgress>);
        let job = tokio_bridge::spawn(cx, async move {
            let _ended = ended_tx;
            let Request { id, range, force } = request;
            let cache = client.cache();
            // Read once when downloads start: logs in the window an earlier run already handled.
            let base = std::sync::OnceLock::new();
            let report = move |progress: LogsProgress| {
                let shown = match progress {
                    LogsProgress::Listing { listed, total, .. } => {
                        SyncProgress::Finding { listed, total }
                    }
                    LogsProgress::Downloading {
                        done,
                        total,
                        spacing,
                        ..
                    } => {
                        let base = *base.get_or_init(|| saved(cache, id, range));
                        downloading(done, total, spacing, base)
                    }
                    _ => return,
                };
                tx.send_replace(Some(shown));
            };
            client.sync_player(id, range, force, &cancel, &report).await
        });
        let task = cx.spawn(async move |this, cx| {
            while rx.changed().await.is_ok() {
                let progress = *rx.borrow_and_update();
                if this
                    .update(cx, |sync, cx| sync.set_progress(request.id, progress, cx))
                    .is_err()
                {
                    return;
                }
            }
            let joined = job.await;
            let _ = this.update(cx, |sync, cx| sync.finished(request.id, joined, cx));
        });
        self.run = Some(Run {
            request,
            progress: None,
            cancel: run_cancel,
            ended,
            _task: task,
        });
    }

    fn set_progress(
        &mut self,
        id: SteamId,
        progress: Option<SyncProgress>,
        cx: &mut Context<Self>,
    ) {
        if let Some(run) = self.run.as_mut().filter(|run| run.request.id == id) {
            run.progress = progress;
            cx.emit(LogsSyncEvent::Progress(id));
            cx.notify();
        }
    }

    // Keeps or clears id's error line as the page did; a player removed while their sync
    // waited is no failure. Dropping the run here also drops this very task, as SyncHub does.
    fn finished(
        &mut self,
        id: SteamId,
        joined: Result<Result<LogsSyncSummary, LogsSyncError>, tokio_bridge::JoinError>,
        cx: &mut Context<Self>,
    ) {
        match joined {
            Ok(Ok(_) | Err(LogsSyncError::NotTracked { .. })) => {
                if self.error.is_some_and(|(failed, _)| failed == id) {
                    self.error = None;
                }
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "syncing logs.tf logs failed");
                self.error = Some((id, sync_message(&error)));
            }
            Err(error) => {
                tracing::warn!(%error, "the logs.tf sync task stopped");
                self.error = Some((id, LOGS_TF_DOWN));
            }
        }
        self.run = None;
        cx.emit(LogsSyncEvent::Finished(id));
        if let Some(next) = self.queued.take() {
            self.spawn(next, cx);
        }
        cx.notify();
    }
}

// Logs in the window no longer waiting for a download; a failed read counts none.
fn saved(cache: &LogsCache, id: SteamId, range: LogsRange) -> usize {
    cache.logs(id, range).map_or(0, |rows| {
        rows.iter()
            .filter(|row| row.state() != LogState::Pending)
            .count()
    })
}
