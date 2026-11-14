//! The Maps page's GameBanana tab while the maps sync runs, fails and commits pages.

use crate::support;
use crate::support::maps::{
    fake_maps_sync, maps_shell, open_gamebanana, open_maps, seed_maps, shown,
};

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::maps_sync::MapsSync;
use tf2tools_app::store_grid::SEARCH_DEBOUNCE;
use tf2tools_app::sync::SyncSummary;

const DONE: SyncSummary = SyncSummary {
    fresh: false,
    fetched: 3,
    rows: 3,
};

fn start(cx: &mut TestAppContext, sync: &gpui_kit::Entity<MapsSync>) {
    assert!(
        cx.update(|cx| sync.update(cx, |s, cx| s.start(false, cx))),
        "no maps sync was running"
    );
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_banner_shows_only_while_the_maps_sync_runs(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    let (sync, runs) = fake_maps_sync(cx);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    assert!(!support::present(cx, s.win, ids::SYNC_BANNER));
    start(cx, &sync);
    let run = runs.try_recv().unwrap();
    run.report(50, 6802);
    support::next_repaint(cx);
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::SYNC_BANNER).as_deref(),
        Some("Syncing maps from GameBanana... Not all items loaded yet.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SYNC_BANNER_COUNT).as_deref(),
        Some("50 / 6802")
    );
    run.finish(Ok(DONE));
    support::wait_hub(cx, "the banner to go", |cx| {
        support::settle(cx, s.win);
        !support::present(cx, s.win, ids::SYNC_BANNER)
    });
}

#[gpui_kit::test]
fn with_no_rows_a_running_sync_shows_the_banner_alone(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    let (sync, _runs) = fake_maps_sync(cx);
    start(cx, &sync);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::SYNC_BANNER).as_deref(),
        Some("Syncing maps from GameBanana...")
    );
    assert!(!support::present(cx, s.win, ids::MAPS_GB_EMPTY));
    assert!(!support::present(cx, s.win, ids::MAPS_GB_ERROR));
}

#[gpui_kit::test]
fn with_no_rows_a_failed_sync_shows_its_message_and_retry_starts_another(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    let (sync, runs) = fake_maps_sync(cx);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    start(cx, &sync);
    runs.try_recv()
        .unwrap()
        .finish(Err(support::offline_failure()));
    support::wait_hub(cx, "the failure line", |cx| {
        support::settle(cx, s.win);
        support::present(cx, s.win, ids::MAPS_GB_ERROR)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::MAPS_GB_ERROR),
        Some(support::offline_failure().message)
    );
    support::click_id(cx, s.win, ids::MAPS_GB_RETRY);
    cx.run_until_parked();
    assert!(!runs.try_recv().expect("Retry starts a run").force);
}

#[gpui_kit::test]
fn a_committed_page_reloads_the_grid(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    let (sync, runs) = fake_maps_sync(cx);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    start(cx, &sync);
    let run = runs.try_recv().unwrap();
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute(
            "INSERT INTO store_gb_maps (gb_id, name, download_rank, fetched_at) VALUES (20, 'tr_walkway', 0, 'x')",
            [],
        )
        .unwrap();
    run.report(50, 6802);
    support::wait_hub(cx, "the new top map", |cx| {
        shown(cx, &tab).first() == Some(&20)
    });
    assert!(cx.update(|cx| tab.read(cx).requeries()) >= 1);
}

// A first launch builds the tab before the first sync stores the category tree.
#[gpui_kit::test]
fn a_finished_sync_fills_the_gamemodes_and_the_grid(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    let (sync, runs) = fake_maps_sync(cx);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    assert!(cx.update(|cx| tab.read(cx).gamemodes().is_empty()));
    start(cx, &sync);
    let run = runs.try_recv().unwrap();
    seed_maps(&env);
    run.finish(Ok(DONE));
    support::wait_hub(cx, "the gamemodes and maps", |cx| {
        cx.update(|cx| tab.read(cx).gamemodes().len() == 3) && shown(cx, &tab) == [10, 11, 12]
    });
    support::click_id(cx, s.win, ids::MAPS_GB_GAMEMODE);
    assert!(support::present(
        cx,
        s.win,
        ids::select_option(ids::MAPS_GB_GAMEMODE, "5385")
    ));
}

#[gpui_kit::test]
fn a_search_or_gamemode_matching_nothing_says_no_maps_found_despite_a_failed_sync(
    cx: &mut TestAppContext,
) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    let (sync, runs) = fake_maps_sync(cx);
    seed_maps(&env);
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute(
            "INSERT INTO store_gb_map_categories (category_id, name, gamemode_id) VALUES (5400, 'Zombie', 5400)",
            [],
        )
        .unwrap();
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    start(cx, &sync);
    runs.try_recv()
        .unwrap()
        .finish(Err(support::offline_failure()));
    support::wait_hub(cx, "the run to fail", |cx| {
        cx.update(|cx| sync.read(cx).error().is_some())
    });
    cx.update(|cx| tab.update(cx, |t, cx| t.search_typed("zzz".to_owned(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "no search results", |cx| shown(cx, &tab).is_empty());
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::MAPS_GB_EMPTY), "search");
    assert!(!support::present(cx, s.win, ids::MAPS_GB_ERROR), "search");

    cx.update(|cx| tab.update(cx, |t, cx| t.search_typed(String::new(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "every map", |cx| shown(cx, &tab).len() == 3);
    support::wait_real(cx, "the gamemodes", |cx| {
        cx.update(|cx| tab.read(cx).gamemodes().len() == 4)
    });
    support::click_id(cx, s.win, ids::MAPS_GB_GAMEMODE);
    support::click_id(cx, s.win, ids::select_option(ids::MAPS_GB_GAMEMODE, "5400"));
    support::wait_real(cx, "no Zombie maps", |cx| shown(cx, &tab).is_empty());
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::MAPS_GB_EMPTY), "gamemode");
    assert!(!support::present(cx, s.win, ids::MAPS_GB_ERROR), "gamemode");
}

#[gpui_kit::test]
fn a_gamemode_gone_after_a_sync_falls_back_to_every_map(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    let (sync, runs) = fake_maps_sync(cx);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    support::wait_real(cx, "the gamemodes", |cx| {
        cx.update(|cx| tab.read(cx).gamemodes().len() == 3)
    });
    support::click_id(cx, s.win, ids::MAPS_GB_GAMEMODE);
    support::click_id(cx, s.win, ids::select_option(ids::MAPS_GB_GAMEMODE, "5385"));
    support::wait_real(cx, "King of the Hill", |cx| shown(cx, &tab) == [12]);
    start(cx, &sync);
    let run = runs.try_recv().unwrap();
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute(
            "DELETE FROM store_gb_map_categories WHERE category_id = 5385",
            [],
        )
        .unwrap();
    run.finish(Ok(DONE));
    support::wait_hub(cx, "every map again", |cx| {
        cx.update(|cx| tab.read(cx).gamemodes().len() == 2) && shown(cx, &tab) == [10, 11, 12]
    });
    assert_eq!(cx.update(|cx| tab.read(cx).gamemode()), None);
}
