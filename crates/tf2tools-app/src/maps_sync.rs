//! The app-wide GameBanana maps sync: one `StoreClient::sync_gb_maps` run at a time on the
//! tokio bridge, sharing the store syncs' state and repaint pump, observed by the Maps page.

use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, Global};
use tf2tools_core::{StoreClient, StoreKind};

use crate::sync::{ProgressSink, SyncFuture, SyncState, SyncSummary, SyncTick, pump};
use crate::sync_messages::{SyncFailure, describe_sync};
use crate::tokio_bridge;

/// Starts one maps sync, forced or not, reporting into the sink; runs on tokio.
pub type MapsRunner = Arc<dyn Fn(bool, ProgressSink) -> SyncFuture + Send + Sync>;

/// Emitted when a run ends on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapsSyncEvent {
    Finished,
    Failed(SyncFailure),
}

/// The maps catalogue's sync and its outcome.
pub struct MapsSync {
    runner: MapsRunner,
    state: SyncState,
}

impl EventEmitter<MapsSyncEvent> for MapsSync {}

struct MapsSyncGlobal(Entity<MapsSync>);
impl Global for MapsSyncGlobal {}

impl MapsSync {
    /// Creates an idle maps sync over `runner` and makes it the app's.
    pub fn install(cx: &mut App, runner: MapsRunner) -> Entity<MapsSync> {
        let sync = cx.new(|_| MapsSync {
            runner,
            state: SyncState::Idle { last: None },
        });
        cx.set_global(MapsSyncGlobal(sync.clone()));
        sync
    }

    /// Starts a run, starting the tokio runtime if needed; false while one runs.
    pub fn start(&mut self, force: bool, cx: &mut Context<Self>) -> bool {
        if self.is_running() {
            return false;
        }
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let runner = Arc::clone(&self.runner);
        let task = pump(
            cx,
            move |sink| runner(force, sink),
            Self::set_tick,
            Self::finish,
        );
        // A retry keeps the error it replaces until a run succeeds.
        let last_error = match std::mem::replace(&mut self.state, SyncState::Idle { last: None }) {
            SyncState::Failed { error } => Some(error),
            SyncState::Idle { .. } | SyncState::Running { .. } => None,
        };
        self.state = SyncState::Running {
            tick: SyncTick::default(),
            last_error,
            task,
        };
        cx.notify();
        true
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, SyncState::Running { .. })
    }

    /// The latest published tick, only while a run is going.
    pub fn progress(&self) -> Option<SyncTick> {
        match &self.state {
            SyncState::Running { tick, .. } => Some(*tick),
            SyncState::Idle { .. } | SyncState::Failed { .. } => None,
        }
    }

    /// The player message of the last failure, kept through a retry until a run succeeds.
    pub fn error(&self) -> Option<&str> {
        match &self.state {
            SyncState::Failed { error } => Some(&error.message),
            SyncState::Running { last_error, .. } => {
                last_error.as_ref().map(|e| e.message.as_str())
            }
            SyncState::Idle { .. } => None,
        }
    }

    /// The last success's summary, only while idle after one.
    pub fn last(&self) -> Option<SyncSummary> {
        match &self.state {
            SyncState::Idle { last } => *last,
            SyncState::Running { .. } | SyncState::Failed { .. } => None,
        }
    }

    fn set_tick(&mut self, next: SyncTick, cx: &mut Context<Self>) {
        if let SyncState::Running { tick, .. } = &mut self.state {
            *tick = next;
            cx.notify();
        }
    }

    fn finish(&mut self, outcome: Result<SyncSummary, SyncFailure>, cx: &mut Context<Self>) {
        let (state, event) = match outcome {
            Ok(summary) => (
                SyncState::Idle {
                    last: Some(summary),
                },
                MapsSyncEvent::Finished,
            ),
            Err(failure) => {
                // The raw error is for developers only; the player reads `failure.message`.
                tracing::warn!(detail = %failure.detail, "GameBanana maps sync failed");
                (
                    SyncState::Failed {
                        error: failure.clone(),
                    },
                    MapsSyncEvent::Failed(failure),
                )
            }
        };
        // Replacing the state drops the pump task that called this, so it is the pump's last step.
        self.state = state;
        cx.emit(event);
        cx.notify();
    }
}

/// The app's maps sync, if one was installed.
pub fn maps_sync(cx: &App) -> Option<Entity<MapsSync>> {
    cx.try_global::<MapsSyncGlobal>().map(|g| g.0.clone())
}

/// The app's runner: `StoreClient::sync_gb_maps`, its failures worded as any GameBanana store's.
pub fn gb_maps_runner(client: StoreClient) -> MapsRunner {
    Arc::new(move |force, sink| {
        let client = client.clone();
        Box::pin(async move {
            client
                .sync_gb_maps(force, &|p| sink.report(p.fetched(), p.total()))
                .await
                .map(|o| SyncSummary {
                    fresh: o.is_fresh(),
                    fetched: o.fetched(),
                    rows: o.rows(),
                })
                .map_err(|e| describe_sync(StoreKind::GameBananaHuds, &e))
        })
    })
}
