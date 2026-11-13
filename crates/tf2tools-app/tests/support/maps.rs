//! The Maps page's GameBanana helpers: a maps sync whose runs the test drives.

use std::sync::{Arc, Mutex};

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::maps_sync::{MapsRunner, MapsSync};
use tf2tools_app::sync::{ProgressSink, SyncSummary};
use tf2tools_app::sync_messages::{SyncFailure, sync_stopped};

/// One maps sync the fake runner was asked for; the test reports progress and ends it.
pub struct FakeMapsRun {
    pub force: bool,
    sink: Option<ProgressSink>,
    finish: Option<tokio::sync::oneshot::Sender<Result<SyncSummary, SyncFailure>>>,
}

impl FakeMapsRun {
    pub fn report(&self, fetched: u32, total: u32) {
        self.sink
            .as_ref()
            .expect("a live run")
            .report(fetched, total);
    }

    /// Drops the progress sink first, as a finished sync future does, then answers.
    pub fn finish(mut self, result: Result<SyncSummary, SyncFailure>) {
        self.sink.take();
        let _ = self.finish.take().expect("a live run").send(result);
    }
}

pub fn fake_maps_runner() -> (MapsRunner, std::sync::mpsc::Receiver<FakeMapsRun>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    let runner: MapsRunner = Arc::new(move |force, sink| {
        let (finish, done) = tokio::sync::oneshot::channel();
        let _ = tx.lock().unwrap().send(FakeMapsRun {
            force,
            sink: Some(sink),
            finish: Some(finish),
        });
        Box::pin(async move {
            done.await
                .unwrap_or_else(|_| Err(sync_stopped("the test dropped the run".to_owned())))
        })
    });
    (runner, rx)
}

/// Installs a maps sync over the fake runner as the app's; its runs cross tokio, so parking is allowed.
pub fn fake_maps_sync(
    cx: &mut TestAppContext,
) -> (Entity<MapsSync>, std::sync::mpsc::Receiver<FakeMapsRun>) {
    cx.executor().allow_parking();
    let (runner, runs) = fake_maps_runner();
    let sync = cx.update(|cx| MapsSync::install(cx, runner));
    (sync, runs)
}
