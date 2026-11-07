//! Store syncs run on the tokio bridge, at most one per store kind, with progress
//! coalesced to one repaint per `REPAINT_INTERVAL` and the outcome kept per kind.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use gpui_kit::{App, AppContext, Context, Entity, EventEmitter, Global, Task};
use tf2tools_core::{StoreClient, StoreKind};

use crate::sync_messages::{SyncFailure, describe_sync, sync_stopped};
use crate::tokio_bridge;

/// The longest a sync's progress waits before it reaches the UI: ten repaints a second.
pub const REPAINT_INTERVAL: Duration = Duration::from_millis(100);

/// How far a running sync has got: items fetched of the total (0 while unknown).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncTick {
    pub fetched: u32,
    pub total: u32,
}

/// What a successful sync did: `fresh` when the cache was already current.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncSummary {
    pub fresh: bool,
    pub fetched: u32,
    pub rows: u64,
}

/// Where a sync reports progress. Reporting never blocks; only the newest tick is kept.
pub struct ProgressSink(tokio::sync::watch::Sender<SyncTick>);

impl ProgressSink {
    pub fn report(&self, fetched: u32, total: u32) {
        self.0.send_replace(SyncTick { fetched, total });
    }
}

pub type SyncFuture = Pin<Box<dyn Future<Output = Result<SyncSummary, SyncFailure>> + Send>>;

/// Starts one sync of a kind, forced or not, reporting into the sink; runs on tokio.
pub type SyncRunner = Arc<dyn Fn(StoreKind, bool, ProgressSink) -> SyncFuture + Send + Sync>;

/// One kind's sync state. Dropping `Running`'s task aborts its sync.
pub enum SyncState {
    Idle {
        last: Option<SyncSummary>,
    },
    Running {
        tick: SyncTick,
        last_error: Option<SyncFailure>,
        task: Task<()>,
    },
    Failed {
        error: SyncFailure,
    },
}

/// Emitted when a run ends on its own; a cancelled run emits nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncEvent {
    Finished {
        kind: StoreKind,
    },
    Failed {
        kind: StoreKind,
        failure: SyncFailure,
    },
}

/// The app's sync orchestrator: one `SyncState` per store kind.
pub struct SyncHub {
    runner: SyncRunner,
    states: HashMap<StoreKind, SyncState>,
}

impl EventEmitter<SyncEvent> for SyncHub {}

struct HubGlobal(Entity<SyncHub>);
impl Global for HubGlobal {}

const IDLE: SyncState = SyncState::Idle { last: None };

impl SyncHub {
    /// Creates a hub over `runner` with every kind idle and makes it the app's hub.
    pub fn install(cx: &mut App, runner: SyncRunner) -> Entity<SyncHub> {
        let states = StoreKind::ALL.iter().map(|k| (*k, IDLE)).collect();
        let hub = cx.new(|_| SyncHub { runner, states });
        cx.set_global(HubGlobal(hub.clone()));
        hub
    }

    /// Starts a sync of `kind`, starting the tokio runtime if needed; false while one runs.
    pub fn start(&mut self, kind: StoreKind, force: bool, cx: &mut Context<Self>) -> bool {
        if self.is_running(kind) {
            return false;
        }
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let (tx, mut rx) = tokio::sync::watch::channel(SyncTick::default());
        let mut job = tokio_bridge::spawn(cx, (self.runner)(kind, force, ProgressSink(tx)));
        // Publish the newest tick, then hold off one interval: at most ten repaints a second.
        // The sink drops with the sync future, which ends the loop; a run that ends inside the
        // hold-off cuts it short, so its outcome is recorded at once.
        let task = cx.spawn(async move |this, cx| {
            let joined = loop {
                if rx.changed().await.is_err() {
                    break job.await;
                }
                let tick = *rx.borrow_and_update();
                if this
                    .update(cx, |hub, cx| hub.set_tick(kind, tick, cx))
                    .is_err()
                {
                    return;
                }
                let mut hold = cx.background_executor().timer(REPAINT_INTERVAL);
                let ended = std::future::poll_fn(|waker| {
                    if let Poll::Ready(joined) = Pin::new(&mut job).poll(waker) {
                        return Poll::Ready(Some(joined));
                    }
                    Pin::new(&mut hold).poll(waker).map(|()| None)
                })
                .await;
                if let Some(joined) = ended {
                    break joined;
                }
            };
            let outcome =
                joined.unwrap_or_else(|e| Err(sync_stopped(format!("the sync stopped: {e}"))));
            let _ = this.update(cx, |hub, cx| hub.finish(kind, outcome, cx));
        });
        // A retry keeps the error it replaces, so the Settings card still shows it.
        let last_error = match self.states.remove(&kind) {
            Some(SyncState::Failed { error }) => Some(error),
            _ => None,
        };
        self.states.insert(
            kind,
            SyncState::Running {
                tick: SyncTick::default(),
                last_error,
                task,
            },
        );
        cx.notify();
        true
    }

    /// Starts every kind that is not already running, without forcing.
    pub fn start_all(&mut self, cx: &mut Context<Self>) {
        for kind in StoreKind::ALL {
            self.start(kind, false, cx);
        }
    }

    /// Drops `kind`'s running sync, aborting it; false when none runs.
    pub fn cancel(&mut self, kind: StoreKind, cx: &mut Context<Self>) -> bool {
        if !self.is_running(kind) {
            return false;
        }
        self.states.insert(kind, IDLE);
        cx.notify();
        true
    }

    pub fn state(&self, kind: StoreKind) -> &SyncState {
        self.states.get(&kind).unwrap_or(&IDLE)
    }

    pub fn is_running(&self, kind: StoreKind) -> bool {
        matches!(self.state(kind), SyncState::Running { .. })
    }

    /// The latest published tick, only while `kind` runs.
    pub fn progress(&self, kind: StoreKind) -> Option<SyncTick> {
        match self.state(kind) {
            SyncState::Running { tick, .. } => Some(*tick),
            SyncState::Idle { .. } | SyncState::Failed { .. } => None,
        }
    }

    /// The last success's summary, only while `kind` is idle after one.
    pub fn last(&self, kind: StoreKind) -> Option<SyncSummary> {
        match self.state(kind) {
            SyncState::Idle { last } => *last,
            SyncState::Running { .. } | SyncState::Failed { .. } => None,
        }
    }

    /// The player message to show: a failed kind's, or the one a running retry replaced.
    pub fn failure(&self, kind: StoreKind) -> Option<&str> {
        match self.state(kind) {
            SyncState::Failed { error } => Some(&error.message),
            SyncState::Running { last_error, .. } => {
                last_error.as_ref().map(|e| e.message.as_str())
            }
            SyncState::Idle { .. } => None,
        }
    }

    fn set_tick(&mut self, kind: StoreKind, next: SyncTick, cx: &mut Context<Self>) {
        if let Some(SyncState::Running { tick, .. }) = self.states.get_mut(&kind) {
            *tick = next;
            cx.notify();
        }
    }

    fn finish(
        &mut self,
        kind: StoreKind,
        outcome: Result<SyncSummary, SyncFailure>,
        cx: &mut Context<Self>,
    ) {
        let (state, event) = match outcome {
            Ok(summary) => (
                SyncState::Idle {
                    last: Some(summary),
                },
                SyncEvent::Finished { kind },
            ),
            Err(failure) => {
                // The raw error is for developers only; the player reads `failure.message`.
                tracing::warn!(store = store_name(kind), detail = %failure.detail, "store sync failed");
                (
                    SyncState::Failed {
                        error: failure.clone(),
                    },
                    SyncEvent::Failed { kind, failure },
                )
            }
        };
        // This drops the Running state's pump task, which is the caller; safe only while finish is
        // the pump's last step, so add nothing after the call in start.
        self.states.insert(kind, state);
        cx.emit(event);
        cx.notify();
    }
}

/// The app's runner: `StoreClient::sync`, with core's outcome and error mapped to app types.
pub fn store_runner(client: StoreClient) -> SyncRunner {
    Arc::new(move |kind, force, sink| {
        let client = client.clone();
        Box::pin(async move {
            client
                .sync(kind, force, &|p| sink.report(p.fetched(), p.total()))
                .await
                .map(|o| SyncSummary {
                    fresh: o.is_fresh(),
                    fetched: o.fetched(),
                    rows: o.rows(),
                })
                .map_err(|e| describe_sync(kind, &e))
        })
    })
}

/// The app's hub, if one was installed.
pub fn sync_hub(cx: &App) -> Option<Entity<SyncHub>> {
    cx.try_global::<HubGlobal>().map(|g| g.0.clone())
}

/// The store's name as v5 shows it.
pub fn store_name(kind: StoreKind) -> &'static str {
    match kind {
        StoreKind::CuratedHuds => "Curated HUDs",
        StoreKind::GameBananaHuds => "GameBanana HUDs",
        StoreKind::Sounds => "Sounds",
        StoreKind::Crosshairs => "Crosshairs",
        StoreKind::Addons => "Addons",
        _ => "Store",
    }
}

/// The store's kebab-case slug, for element ids.
pub fn store_slug(kind: StoreKind) -> &'static str {
    match kind {
        StoreKind::CuratedHuds => "curated-huds",
        StoreKind::GameBananaHuds => "gamebanana-huds",
        StoreKind::Sounds => "sounds",
        StoreKind::Crosshairs => "crosshairs",
        StoreKind::Addons => "addons",
        _ => "store",
    }
}
