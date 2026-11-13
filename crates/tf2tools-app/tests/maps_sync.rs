//! The app-wide maps sync: one run at a time, its progress, its failure and its banner text.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::maps_sync::{MapsSync, MapsSyncEvent};
use tf2tools_app::sync::{SyncSummary, SyncTick};
use tf2tools_app::widgets::sync_banner::maps_banner_text;

use crate::support;
use crate::support::maps::fake_maps_sync;

fn start(cx: &mut TestAppContext, sync: &Entity<MapsSync>, force: bool) -> bool {
    cx.update(|cx| sync.update(cx, |s, cx| s.start(force, cx)))
}

fn read<R>(cx: &mut TestAppContext, sync: &Entity<MapsSync>, f: impl FnOnce(&MapsSync) -> R) -> R {
    cx.update(|cx| f(sync.read(cx)))
}

fn events(cx: &mut TestAppContext, sync: &Entity<MapsSync>) -> Rc<RefCell<Vec<MapsSyncEvent>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    cx.update(|cx| {
        cx.subscribe(sync, move |_, ev: &MapsSyncEvent, _| {
            sink.borrow_mut().push(ev.clone())
        })
        .detach()
    });
    seen
}

const SUMMARY: SyncSummary = SyncSummary {
    fresh: false,
    fetched: 6802,
    rows: 6802,
};

#[gpui_kit::test]
fn a_second_start_while_one_runs_is_ignored(cx: &mut TestAppContext) {
    let (sync, runs) = fake_maps_sync(cx);
    assert!(start(cx, &sync, false));
    assert!(!start(cx, &sync, true));
    cx.run_until_parked();
    assert!(!runs.try_recv().expect("one run").force);
    assert!(
        runs.try_recv().is_err(),
        "the second start must not reach the runner"
    );
    assert!(read(cx, &sync, |s| s.is_running()));
}

#[gpui_kit::test]
fn progress_shows_while_running_and_a_finish_keeps_the_summary(cx: &mut TestAppContext) {
    let (sync, runs) = fake_maps_sync(cx);
    let seen = events(cx, &sync);
    start(cx, &sync, false);
    let run = runs.try_recv().unwrap();
    run.report(50, 6802);
    support::next_repaint(cx);
    assert_eq!(
        read(cx, &sync, |s| s.progress()),
        Some(SyncTick {
            fetched: 50,
            total: 6802
        })
    );
    run.finish(Ok(SUMMARY));
    support::wait_hub(cx, "the maps sync to finish", |cx| {
        !read(cx, &sync, |s| s.is_running())
    });
    assert_eq!(read(cx, &sync, |s| s.last()), Some(SUMMARY));
    assert_eq!(read(cx, &sync, |s| s.progress()), None);
    assert_eq!(*seen.borrow(), vec![MapsSyncEvent::Finished]);
}

#[gpui_kit::test]
fn a_failure_stays_through_a_retry_until_a_run_succeeds(cx: &mut TestAppContext) {
    let (sync, runs) = fake_maps_sync(cx);
    let seen = events(cx, &sync);
    let error = |cx: &mut TestAppContext| read(cx, &sync, |s| s.error().map(str::to_owned));
    start(cx, &sync, false);
    runs.try_recv()
        .unwrap()
        .finish(Err(support::offline_failure()));
    support::wait_hub(cx, "the failure", |cx| !read(cx, &sync, |s| s.is_running()));
    let offline = support::offline_failure();
    assert_eq!(error(cx), Some(offline.message.clone()));
    assert_eq!(*seen.borrow(), vec![MapsSyncEvent::Failed(offline.clone())]);
    assert!(start(cx, &sync, false));
    assert_eq!(
        error(cx),
        Some(offline.message),
        "a retry keeps the message"
    );
    cx.run_until_parked();
    runs.try_recv().unwrap().finish(Ok(SUMMARY));
    support::wait_hub(cx, "the retry", |cx| !read(cx, &sync, |s| s.is_running()));
    assert_eq!(error(cx), None);
}

#[test]
fn the_banner_names_maps() {
    assert_eq!(maps_banner_text(false), "Syncing maps from GameBanana...");
    assert_eq!(
        maps_banner_text(true),
        "Syncing maps from GameBanana... Not all items loaded yet."
    );
}
