use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::sync::{self, SyncEvent, SyncHub, SyncSummary, SyncTick};
use tf2tools_core::StoreKind;

use crate::support;

fn start(cx: &mut TestAppContext, hub: &Entity<SyncHub>, kind: StoreKind, force: bool) -> bool {
    cx.update(|cx| hub.update(cx, |h, cx| h.start(kind, force, cx)))
}

fn read<R>(cx: &mut TestAppContext, hub: &Entity<SyncHub>, f: impl FnOnce(&SyncHub) -> R) -> R {
    cx.update(|cx| f(hub.read(cx)))
}

fn events(cx: &mut TestAppContext, hub: &Entity<SyncHub>) -> Rc<RefCell<Vec<SyncEvent>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    cx.update(|cx| {
        cx.subscribe(hub, move |_, ev: &SyncEvent, _| {
            sink.borrow_mut().push(ev.clone())
        })
        .detach()
    });
    seen
}

#[test]
fn stores_have_v5_names_and_kebab_slugs() {
    let names: Vec<_> = StoreKind::ALL
        .iter()
        .map(|k| sync::store_name(*k))
        .collect();
    assert_eq!(
        names,
        [
            "Curated HUDs",
            "GameBanana HUDs",
            "Sounds",
            "Crosshairs",
            "Addons"
        ]
    );
    let slugs: Vec<_> = StoreKind::ALL
        .iter()
        .map(|k| sync::store_slug(*k))
        .collect();
    assert_eq!(
        slugs,
        [
            "curated-huds",
            "gamebanana-huds",
            "sounds",
            "crosshairs",
            "addons"
        ]
    );
}

#[gpui_kit::test]
fn a_second_start_of_a_running_kind_is_ignored(cx: &mut TestAppContext) {
    let (hub, runs) = support::fake_hub(cx);
    assert!(start(cx, &hub, StoreKind::Sounds, false));
    assert!(!start(cx, &hub, StoreKind::Sounds, true));
    cx.run_until_parked();
    let run = runs.try_recv().expect("one run");
    assert_eq!((run.kind, run.force), (StoreKind::Sounds, false));
    assert!(
        runs.try_recv().is_err(),
        "the second start must not reach the runner"
    );
    assert!(read(cx, &hub, |h| h.is_running(StoreKind::Sounds)));
    // Other kinds run alongside, as v5's GameBanana syncs do.
    assert!(start(cx, &hub, StoreKind::Crosshairs, false));
    assert_eq!(runs.try_recv().unwrap().kind, StoreKind::Crosshairs);
}

#[gpui_kit::test]
fn progress_reaches_the_hub_at_most_once_per_repaint_interval(cx: &mut TestAppContext) {
    let (hub, runs) = support::fake_hub(cx);
    start(cx, &hub, StoreKind::Sounds, false);
    let run = runs.try_recv().unwrap();
    cx.run_until_parked();
    let notes = Rc::new(Cell::new(0));
    let n = notes.clone();
    cx.update(|cx| cx.observe(&hub, move |_, _| n.set(n.get() + 1)).detach());

    for fetched in 1..=500 {
        run.report(fetched, 1000);
    }
    cx.run_until_parked();
    let tick = |cx: &mut TestAppContext| read(cx, &hub, |h| h.progress(StoreKind::Sounds));
    assert_eq!(
        tick(cx),
        Some(SyncTick {
            fetched: 500,
            total: 1000
        })
    );
    assert_eq!(notes.get(), 1, "a burst of 500 reports is one repaint");

    run.report(600, 1000);
    cx.run_until_parked();
    assert_eq!(
        tick(cx),
        Some(SyncTick {
            fetched: 500,
            total: 1000
        }),
        "held inside the interval"
    );
    assert_eq!(notes.get(), 1);

    support::next_repaint(cx);
    assert_eq!(
        tick(cx),
        Some(SyncTick {
            fetched: 600,
            total: 1000
        })
    );
    assert_eq!(notes.get(), 2);
}

#[gpui_kit::test]
fn a_finished_run_goes_idle_with_its_summary(cx: &mut TestAppContext) {
    let (hub, runs) = support::fake_hub(cx);
    let seen = events(cx, &hub);
    start(cx, &hub, StoreKind::Sounds, false);
    let run = runs.try_recv().unwrap();
    run.report(10, 30);
    let summary = SyncSummary {
        fresh: false,
        fetched: 30,
        rows: 30,
    };
    run.finish(Ok(summary));
    support::wait_hub(cx, "the run to finish", |cx| {
        !read(cx, &hub, |h| h.is_running(StoreKind::Sounds))
    });
    assert_eq!(read(cx, &hub, |h| h.last(StoreKind::Sounds)), Some(summary));
    assert_eq!(read(cx, &hub, |h| h.progress(StoreKind::Sounds)), None);
    assert_eq!(
        read(cx, &hub, |h| h
            .failure(StoreKind::Sounds)
            .map(str::to_owned)),
        None
    );
    assert_eq!(
        *seen.borrow(),
        vec![SyncEvent::Finished {
            kind: StoreKind::Sounds
        }]
    );
}

#[gpui_kit::test]
fn a_run_that_ends_inside_the_repaint_interval_finishes_without_waiting_it_out(
    cx: &mut TestAppContext,
) {
    let (hub, runs) = support::fake_hub(cx);
    let seen = events(cx, &hub);
    start(cx, &hub, StoreKind::Sounds, false);
    let run = runs.try_recv().unwrap();
    run.report(10, 30);
    cx.run_until_parked();
    // The tick is out, so the pump now holds off one interval on the fake clock.
    assert_eq!(
        read(cx, &hub, |h| h.progress(StoreKind::Sounds)),
        Some(SyncTick {
            fetched: 10,
            total: 30
        })
    );
    let summary = SyncSummary {
        fresh: false,
        fetched: 30,
        rows: 30,
    };
    run.finish(Ok(summary));
    // The clock never moves here, so only a pump that stops holding off can finish.
    support::wait_real(cx, "the run to finish inside the interval", |cx| {
        !read(cx, &hub, |h| h.is_running(StoreKind::Sounds))
    });
    assert_eq!(read(cx, &hub, |h| h.last(StoreKind::Sounds)), Some(summary));
    assert_eq!(
        *seen.borrow(),
        vec![SyncEvent::Finished {
            kind: StoreKind::Sounds
        }]
    );
}

#[gpui_kit::test]
fn a_failure_stays_until_a_later_run_succeeds(cx: &mut TestAppContext) {
    let (hub, runs) = support::fake_hub(cx);
    let seen = events(cx, &hub);
    start(cx, &hub, StoreKind::Crosshairs, false);
    runs.try_recv()
        .unwrap()
        .finish(Err(support::offline_failure()));
    let failure = |cx: &mut TestAppContext| {
        read(cx, &hub, |h| {
            h.failure(StoreKind::Crosshairs).map(str::to_owned)
        })
    };
    support::wait_hub(cx, "the failure", |cx| failure(cx).is_some());
    assert_eq!(
        failure(cx).as_deref(),
        Some("Couldn't reach GameBanana. Check your internet connection and try again.")
    );
    assert_eq!(read(cx, &hub, |h| h.last(StoreKind::Crosshairs)), None);
    assert_eq!(
        *seen.borrow(),
        vec![SyncEvent::Failed {
            kind: StoreKind::Crosshairs,
            failure: support::offline_failure()
        }]
    );

    assert!(
        start(cx, &hub, StoreKind::Crosshairs, true),
        "a failed kind can start again"
    );
    let retry = runs.try_recv().unwrap();
    assert!(retry.force);
    cx.run_until_parked();
    assert_eq!(
        failure(cx).as_deref(),
        Some("Couldn't reach GameBanana. Check your internet connection and try again."),
        "kept while the retry runs"
    );
    retry.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the retry", |cx| {
        !read(cx, &hub, |h| h.is_running(StoreKind::Crosshairs))
    });
    assert_eq!(failure(cx), None);
    assert_eq!(
        read(cx, &hub, |h| h.last(StoreKind::Crosshairs)),
        Some(SyncSummary::default())
    );
}

#[gpui_kit::test]
fn cancel_aborts_the_running_future_and_frees_the_kind(cx: &mut TestAppContext) {
    let (hub, runs) = support::fake_hub(cx);
    let seen = events(cx, &hub);
    start(cx, &hub, StoreKind::Sounds, false);
    let first = runs.try_recv().unwrap();
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.cancel(StoreKind::Sounds, cx))));
    assert!(!read(cx, &hub, |h| h.is_running(StoreKind::Sounds)));
    support::wait_real(cx, "the abort to drop the sync future", |_| first.aborted());
    assert!(!cx.update(|cx| hub.update(cx, |h, cx| h.cancel(StoreKind::Sounds, cx))));
    assert!(
        seen.borrow().is_empty(),
        "a cancelled run neither finishes nor fails"
    );
    assert!(start(cx, &hub, StoreKind::Sounds, true));
    assert!(runs.try_recv().unwrap().force);
}

#[gpui_kit::test]
fn start_all_starts_each_kind_once_without_force(cx: &mut TestAppContext) {
    let (hub, runs) = support::fake_hub(cx);
    cx.update(|cx| hub.update(cx, |h, cx| h.start_all(cx)));
    let got: Vec<_> = runs.try_iter().collect();
    let kinds: Vec<_> = got.iter().map(|r| (r.kind, r.force)).collect();
    assert_eq!(kinds.len(), 5);
    for kind in StoreKind::ALL {
        assert!(kinds.contains(&(kind, false)), "{kind:?}");
    }
    cx.update(|cx| hub.update(cx, |h, cx| h.start_all(cx)));
    assert_eq!(
        runs.try_iter().count(),
        0,
        "running kinds are not started twice"
    );
    drop(got);
}

#[gpui_kit::test]
fn install_makes_the_hub_the_app_hub(cx: &mut TestAppContext) {
    assert!(cx.update(|cx| sync::sync_hub(cx)).is_none());
    let (hub, _runs) = support::fake_hub(cx);
    assert_eq!(
        cx.update(|cx| sync::sync_hub(cx)).map(|h| h.entity_id()),
        Some(hub.entity_id())
    );
}
