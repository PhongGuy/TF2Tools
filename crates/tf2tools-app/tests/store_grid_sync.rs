mod support;

use std::time::Duration;

use gpui_kit::{ScrollStrategy, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::store_grid::StoreGridConfig;
use tf2tools_app::sync::SyncSummary;
use tf2tools_core::{GbId, StoreItemId, StoreKind, StoreSort};

fn card(id: u64) -> gpui_kit::ElementId {
    ids::store_card(
        StoreKind::Crosshairs,
        &StoreItemId::GameBanana(GbId::new(id)),
    )
}

#[gpui_kit::test]
fn sync_progress_requeries_once_a_second_and_keeps_the_first_visible_card(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let config = StoreGridConfig::new(StoreKind::Crosshairs, "crosshairs");
    let (win, grid) = support::store_grid_window(cx, &env, config, |env| {
        support::seed_crosshairs(env, 1..=150, 0, false)
    });
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded())
    });
    // Row 5 at the top: its first card is index 20, crosshair 130.
    cx.update(|cx| {
        grid.read(cx)
            .card_grid()
            .scroll_handle()
            .scroll_to_item(5, ScrollStrategy::Top)
    });
    support::settle(cx, win);
    let before = support::bounds_of(cx, win, card(130)).unwrap().origin.y;

    cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Crosshairs, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    support::seed_crosshairs(&env, 1001..=1008, 5_000, false);
    run.report(8, 158);
    support::wait_hub(cx, "the first re-query", |cx| {
        cx.update(|cx| grid.read(cx).requeries() == 1)
    });
    support::settle(cx, win);
    assert_eq!(
        support::bounds_of(cx, win, card(130)).unwrap().origin.y,
        before
    );
    // The reload re-reads the 60 loaded from the top; the new cards lead it.
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), 60);
    assert_eq!(
        cx.update(|cx| grid.read(cx).cards()[0].name.clone()),
        "Cross 1008"
    );

    // A report within the second waits for the trailing run.
    support::seed_crosshairs(&env, 1009..=1010, 5_000, false);
    run.report(10, 160);
    support::next_repaint(cx);
    assert_eq!(cx.update(|cx| grid.read(cx).requeries()), 1);
    support::wait_hub(cx, "the trailing re-query", |cx| {
        cx.update(|cx| grid.read(cx).requeries() == 2)
    });
    assert_eq!(
        cx.update(|cx| grid.read(cx).cards()[0].name.clone()),
        "Cross 1010"
    );

    // The closing re-query fails: the cards, their place and the error line stay as they were.
    let db = rusqlite::Connection::open(env.data.db_path()).unwrap();
    db.execute_batch("ALTER TABLE store_crosshairs RENAME TO hidden")
        .unwrap();
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the closing re-query", |cx| {
        cx.update(|cx| grid.read(cx).requeries() == 3)
    });
    support::settle(cx, win);
    assert_eq!(
        support::bounds_of(cx, win, card(130)).unwrap().origin.y,
        before
    );
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), 60);
    assert!(!support::present(cx, win, ids::STORE_ERROR));
    db.execute_batch("ALTER TABLE hidden RENAME TO store_crosshairs")
        .unwrap();
}

#[gpui_kit::test]
fn a_new_query_drops_the_trailing_requery_without_stalling_the_next(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let config = StoreGridConfig::new(StoreKind::Crosshairs, "crosshairs");
    let (_win, grid) = support::store_grid_window(cx, &env, config, |env| {
        support::seed_crosshairs(env, 1..=10, 0, false)
    });
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded())
    });
    cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Crosshairs, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    run.report(1, 10);
    support::wait_hub(cx, "the first re-query", |cx| {
        cx.update(|cx| grid.read(cx).requeries() == 1)
    });

    // A report within the second schedules a trailing re-query; a new sort drops it.
    run.report(2, 10);
    support::next_repaint(cx);
    cx.update(|cx| grid.update(cx, |g, cx| g.set_sort(StoreSort::Newest, cx)));
    for _ in 0..15 {
        support::next_repaint(cx);
    }
    assert_eq!(cx.update(|cx| grid.read(cx).requeries()), 1);

    // The throttle is free again: the next report re-queries at once.
    run.report(3, 10);
    support::wait_hub(cx, "the next re-query", |cx| {
        cx.update(|cx| grid.read(cx).requeries() == 2)
    });
}
