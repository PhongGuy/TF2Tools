use crate::support;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Subscription, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::remote_images::queue::MAX_FETCHES;
use tf2tools_app::remote_images::{FetchFailure, ImageFetch, ImageKey, RemoteImage, RemoteImages};
use tf2tools_app::store_grid::{SEARCH_DEBOUNCE, StoreGrid, StoreGridConfig};
use tf2tools_app::sync::SyncSummary;
use tf2tools_core::{GbId, StoreItemId, StoreKind, StoreSort};

fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
    seed: impl FnOnce(&support::Env),
) -> (AnyWindowHandle, Entity<StoreGrid>) {
    support::store_grid_window(
        cx,
        env,
        StoreGridConfig::new(StoreKind::Crosshairs, "crosshairs"),
        seed,
    )
}

fn card_id(id: u64) -> gpui_kit::ElementId {
    ids::store_card(
        StoreKind::Crosshairs,
        &StoreItemId::GameBanana(GbId::new(id)),
    )
}

fn first_id(cx: &mut TestAppContext, grid: &Entity<StoreGrid>) -> u64 {
    cx.update(|cx| match &grid.read(cx).cards()[0].id {
        StoreItemId::GameBanana(id) => id.get(),
        other => panic!("{other:?}"),
    })
}

fn loaded(cx: &mut TestAppContext, win: AnyWindowHandle, grid: &Entity<StoreGrid>) {
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded() && !grid.read(cx).is_loading())
    });
    support::settle(cx, win);
}

fn page_to_end(cx: &mut TestAppContext, win: AnyWindowHandle, grid: &Entity<StoreGrid>) {
    for _ in 0..20 {
        if cx.update(|cx| grid.read(cx).is_end()) {
            return;
        }
        cx.update(|cx| grid.read(cx).card_grid().scroll_handle().scroll_to_bottom());
        support::settle(cx, win);
        loaded(cx, win, grid);
    }
    panic!("paging never reached the end");
}

#[gpui_kit::test]
fn the_first_page_and_count_show_and_scrolling_pages_to_the_end(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = open(cx, &env, |env| {
        support::seed_crosshairs(env, 1..=150, 0, false)
    });
    loaded(cx, win, &grid);
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), 60);
    assert_eq!(first_id(cx, &grid), 150);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("150 of 150 crosshairs")
    );
    for id in [
        card_id(150),
        ids::STORE_SEARCH.into(),
        ids::STORE_SORT.into(),
    ] {
        assert!(support::present(cx, win, id));
    }
    page_to_end(cx, win, &grid);
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), 150);
}

#[gpui_kit::test]
fn a_short_page_is_not_taken_as_the_end(cx: &mut TestAppContext) {
    let env = support::env();
    // Five rows with negative ids top the likes; core drops them, so the first page holds 55.
    let (win, grid) = open(cx, &env, |env| {
        support::seed_crosshairs(env, -5..=-1, 10_000, false);
        support::seed_crosshairs(env, 1..=100, 0, false);
    });
    loaded(cx, win, &grid);
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), 55);
    assert!(!cx.update(|cx| grid.read(cx).is_end()));
    page_to_end(cx, win, &grid);
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), 100);
}

#[gpui_kit::test]
fn search_waits_out_the_debounce_and_a_new_sort_starts_from_the_top(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = open(cx, &env, |env| {
        support::seed_crosshairs(env, 1..=150, 0, false)
    });
    loaded(cx, win, &grid);
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("Cross 7".into(), cx)));
    cx.executor().advance_clock(Duration::from_millis(399));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| grid.read(cx).shown()), Some(150));
    cx.executor().advance_clock(Duration::from_millis(1));
    support::wait_real(cx, "the search", |cx| {
        cx.update(|cx| grid.read(cx).shown() == Some(11) && !grid.read(cx).is_loading())
    });
    // "Cross 7" and "Cross 70" to "Cross 79"; the most liked first.
    assert_eq!(first_id(cx, &grid), 79);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("11 of 150 crosshairs")
    );
    // A new sort starts from the top.
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed(String::new(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    loaded(cx, win, &grid);
    cx.update(|cx| {
        grid.read(cx)
            .card_grid()
            .scroll_handle()
            .scroll_to_item(10, gpui_kit::ScrollStrategy::Top)
    });
    support::settle(cx, win);
    cx.update(|cx| grid.update(cx, |g, cx| g.set_sort(StoreSort::Newest, cx)));
    loaded(cx, win, &grid);
    assert_eq!(first_id(cx, &grid), 1);
    let offset = cx.update(|cx| {
        grid.read(cx)
            .card_grid()
            .scroll_handle()
            .0
            .borrow()
            .base_handle
            .offset()
    });
    assert_eq!(offset.y, gpui_kit::px(0.));
}

#[gpui_kit::test]
fn a_search_stamps_its_query_start_when_the_debounce_ends(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = open(cx, &env, |env| {
        support::seed_crosshairs(env, 1..=20, 0, false)
    });
    loaded(cx, win, &grid);
    let opened = cx.update(|cx| grid.read(cx).query_started());
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("Cross 7".into(), cx)));
    let typed = Instant::now();
    std::thread::sleep(Duration::from_millis(5));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| grid.read(cx).query_started()), opened);
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    cx.run_until_parked();
    let started = cx.update(|cx| grid.read(cx).query_started());
    assert!(
        started >= typed + Duration::from_millis(5),
        "{started:?} {typed:?}"
    );
}

fn thumb(id: u64) -> String {
    format!("https://images.gamebanana.com/img/ss/mods/100-90_{id}.jpg")
}

/// Installs `RemoteImages` over a fetcher that logs each URL and holds every fetch until
/// the returned semaphore gets permits; the fetch then fails, which is enough to log it.
fn gated_images(cx: &mut TestAppContext) -> (Arc<Mutex<Vec<String>>>, Arc<tokio::sync::Semaphore>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (log, held) = (Arc::clone(&asked), Arc::clone(&gate));
    let fetch: ImageFetch = Arc::new(move |url: String| {
        log.lock().unwrap().push(url);
        let held = Arc::clone(&held);
        Box::pin(async move {
            let _permit = held.acquire().await;
            Err(FetchFailure::Permanent)
        })
    });
    // Fetches cross the tokio runtime, so the test executor must be allowed to park.
    cx.executor().allow_parking();
    cx.update(|cx| RemoteImages::install(cx, fetch));
    (asked, gate)
}

fn image_grid() -> StoreGridConfig {
    StoreGridConfig::new(StoreKind::Crosshairs, "crosshairs").image_card(gpui_kit::px(40.))
}

fn wait_asked(
    cx: &mut TestAppContext,
    asked: &Arc<Mutex<Vec<String>>>,
    ids: impl IntoIterator<Item = u64>,
) {
    let urls: Vec<String> = ids.into_iter().map(thumb).collect();
    support::wait_real(cx, "the thumbnails", |cx| {
        cx.run_until_parked();
        let asked = asked.lock().unwrap();
        urls.iter().all(|url| asked.contains(url))
    });
}

#[gpui_kit::test]
fn two_grids_keep_their_own_thumbnail_wants(cx: &mut TestAppContext) {
    let env = support::env();
    let (asked, gate) = gated_images(cx);
    // Grid A's first three rows (ids 150 to 139) outnumber the six fetch slots, so most wait queued.
    let (win_a, a) = support::store_grid_window(cx, &env, image_grid(), |env| {
        support::seed_crosshairs(env, 1..=150, 0, true)
    });
    loaded(cx, win_a, &a);
    // A one-column grid wants only a few of the same thumbnails; its want and its drop
    // must leave grid A's queued thumbnails wanted.
    let (win_b, b) = support::open_store_grid(cx, image_grid().max_columns(1));
    loaded(cx, win_b, &b);
    cx.update_window(win_b, |_, window, _| window.remove_window())
        .unwrap();
    drop(b);
    cx.run_until_parked();
    gate.add_permits(1_000);
    wait_asked(cx, &asked, (139..=150).rev());
}

// The six fetch slots hold six thumbnails and the rest of the visible cards wait queued; once
// the grid is gone, freeing the slots must start none of them.
#[gpui_kit::test]
fn a_dropped_grid_hands_back_its_thumbnail_wants(cx: &mut TestAppContext) {
    let env = support::env();
    let (asked, gate) = gated_images(cx);
    let (win, grid) = support::store_grid_window(cx, &env, image_grid(), |env| {
        support::seed_crosshairs(env, 1..=150, 0, true)
    });
    loaded(cx, win, &grid);
    assert_eq!(asked.lock().unwrap().len(), MAX_FETCHES);
    cx.update_window(win, |_, window, _| window.remove_window())
        .unwrap();
    // Release listeners run when an update flushes, not on a bare drop.
    cx.update(move |_| drop(grid));
    let images = cx.update(|cx| RemoteImages::global(cx)).unwrap();
    let held: Vec<ImageKey> = asked
        .lock()
        .unwrap()
        .iter()
        .cloned()
        .map(ImageKey::card)
        .collect();
    gate.add_permits(1_000);
    support::wait_real(cx, "the held fetches to fail", |cx| {
        cx.update(|cx| {
            held.iter()
                .all(|key| matches!(images.read(cx).image(key), RemoteImage::Failed))
        })
    });
    cx.run_until_parked();
    let asked = asked.lock().unwrap().clone();
    assert_eq!(
        asked.len(),
        MAX_FETCHES,
        "a queued thumbnail was fetched: {asked:?}"
    );
}

#[gpui_kit::test]
fn a_requery_landing_on_the_same_count_wants_its_thumbnails(cx: &mut TestAppContext) {
    let env = support::env();
    let (asked, gate) = gated_images(cx);
    gate.add_permits(1_000_000);
    let (win, grid) = support::store_grid_window(cx, &env, image_grid(), |env| {
        support::seed_crosshairs(env, 1..=150, 0, true)
    });
    loaded(cx, win, &grid);
    wait_asked(cx, &asked, [150]);
    // Newest first is ids 1 up: another page of 60 in the same visible range.
    cx.update(|cx| grid.update(cx, |g, cx| g.set_sort(StoreSort::Newest, cx)));
    loaded(cx, win, &grid);
    assert_eq!(first_id(cx, &grid), 1);
    wait_asked(cx, &asked, 1..=4);
}

#[gpui_kit::test]
fn a_failed_query_shows_an_inline_error_that_retry_clears(cx: &mut TestAppContext) {
    let env = support::env();
    let db = env.data.db_path();
    let (win, grid) = open(cx, &env, |env| {
        support::seed_crosshairs(env, 1..=10, 0, false);
        rusqlite::Connection::open(env.data.db_path())
            .unwrap()
            .execute_batch("ALTER TABLE store_crosshairs RENAME TO store_crosshairs_hidden")
            .unwrap();
    });
    support::wait_real(cx, "the error", |cx| {
        cx.update(|cx| grid.read(cx).error().is_some())
    });
    assert!(support::present(cx, win, ids::STORE_ERROR));
    assert_eq!(
        support::label_of(cx, win, ids::STORE_ERROR).as_deref(),
        Some("TF2Tools couldn't read the store. Restart TF2Tools and try again.")
    );
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("ALTER TABLE store_crosshairs_hidden RENAME TO store_crosshairs")
        .unwrap();
    support::click_id(cx, win, ids::STORE_RETRY);
    loaded(cx, win, &grid);
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), 10);
    assert!(!support::present(cx, win, ids::STORE_ERROR));
}

#[gpui_kit::test]
fn retry_after_a_failed_page_alone_asks_only_for_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let db = env.data.db_path();
    // Only the page reads the library table, so hiding it fails the page while the counts land.
    let (_win, grid) = open(cx, &env, |env| {
        support::seed_crosshairs(env, 1..=10, 0, false);
        rusqlite::Connection::open(env.data.db_path())
            .unwrap()
            .execute_batch("ALTER TABLE library_crosshairs RENAME TO library_crosshairs_hidden")
            .unwrap();
    });
    support::wait_real(cx, "the page error with the counts", |cx| {
        cx.update(|cx| grid.read(cx).error().is_some() && grid.read(cx).shown() == Some(10))
    });
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("ALTER TABLE library_crosshairs_hidden RENAME TO library_crosshairs")
        .unwrap();
    // Rows added now show in a re-read page but would only reach the counts if Retry re-counted.
    support::seed_crosshairs(&env, 11..=15, 0, false);
    cx.update(|cx| grid.update(cx, |g, cx| g.retry(cx)));
    support::wait_real(cx, "the retried page", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded() && !grid.read(cx).is_loading())
    });
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), 15);
    assert_eq!(cx.update(|cx| grid.read(cx).shown()), Some(10));
    assert!(cx.update(|cx| grid.read(cx).error().is_none()));
}

fn sound_grid(cx: &mut TestAppContext, env: &support::Env) -> (AnyWindowHandle, Entity<StoreGrid>) {
    support::store_grid_window(
        cx,
        env,
        StoreGridConfig::new(StoreKind::Sounds, "sounds"),
        |env| {
            support::seed_sounds(
                env,
                &[
                    (1, "Ding", "hitsound", None),
                    (2, "Boom", "killsound", None),
                    (3, "Blip", "hitsound", None),
                    (4, "Hum", "sound", None),
                ],
            )
        },
    )
}

fn ids_of(cx: &mut TestAppContext, grid: &Entity<StoreGrid>) -> Vec<u64> {
    cx.update(|cx| {
        grid.read(cx)
            .cards()
            .iter()
            .map(|c| match &c.id {
                StoreItemId::GameBanana(id) => id.get(),
                other => panic!("{other:?}"),
            })
            .collect()
    })
}

/// Every notify's shown count and card ids, in order.
type Seen = Rc<RefCell<Vec<(Option<u64>, Vec<u64>)>>>;

/// Logs `grid` after every notify, so a stale landing shows even when a newer one follows it.
fn watch(cx: &mut TestAppContext, grid: &Entity<StoreGrid>) -> (Seen, Subscription) {
    let seen = Seen::default();
    let log = Rc::clone(&seen);
    let sub = cx.update(|cx| {
        cx.observe(grid, move |grid, cx| {
            let g = grid.read(cx);
            let ids = g
                .cards()
                .iter()
                .filter_map(|c| match &c.id {
                    StoreItemId::GameBanana(id) => Some(id.get()),
                    _ => None,
                })
                .collect();
            log.borrow_mut().push((g.shown(), ids));
        })
    });
    (seen, sub)
}

#[gpui_kit::test]
fn a_category_filters_the_cards_and_the_shown_count(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = sound_grid(cx, &env);
    loaded(cx, win, &grid);
    assert_eq!(
        cx.update(|cx| grid.read(cx).category().map(str::to_owned)),
        None
    );
    cx.update(|cx| grid.update(cx, |g, cx| g.set_category(Some("hitsound".into()), cx)));
    support::wait_real(cx, "the filter", |cx| {
        cx.update(|cx| grid.read(cx).shown() == Some(2) && !grid.read(cx).is_loading())
    });
    assert_eq!(ids_of(cx, &grid), [3, 1]);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("2 of 4 sounds")
    );
    cx.update(|cx| grid.update(cx, |g, cx| g.set_category(None, cx)));
    support::wait_real(cx, "the unfiltered grid", |cx| {
        cx.update(|cx| grid.read(cx).shown() == Some(4) && !grid.read(cx).is_loading())
    });
    assert_eq!(ids_of(cx, &grid), [4, 3, 2, 1]);
}

#[gpui_kit::test]
fn a_sync_requery_keeps_the_category(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let (win, grid) = sound_grid(cx, &env);
    loaded(cx, win, &grid);
    cx.update(|cx| grid.update(cx, |g, cx| g.set_category(Some("killsound".into()), cx)));
    support::wait_real(cx, "the filter", |cx| {
        cx.update(|cx| grid.read(cx).shown() == Some(1) && !grid.read(cx).is_loading())
    });
    cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Sounds, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    support::seed_sounds(
        &env,
        &[
            (5, "Crash", "killsound", None),
            (6, "Tick", "hitsound", None),
        ],
    );
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the closing re-query", |cx| {
        cx.update(|cx| grid.read(cx).requeries() == 1)
    });
    assert_eq!(ids_of(cx, &grid), [5, 2]);
}

// The open's unfiltered page and counts (4 shown, ids 4 to 1) are still pending when the
// category changes, so they must never show.
#[gpui_kit::test]
fn set_category_drops_the_page_and_count_in_flight(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = sound_grid(cx, &env);
    assert!(
        cx.update(|cx| {
            let g = grid.read(cx);
            g.is_loading() && g.cards().is_empty() && g.shown().is_none()
        }),
        "the open's reads have not run yet"
    );
    let (seen, _sub) = watch(cx, &grid);
    cx.update(|cx| grid.update(cx, |g, cx| g.set_category(Some("killsound".into()), cx)));
    loaded(cx, win, &grid);
    assert_eq!(ids_of(cx, &grid), [2]);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("1 of 4 sounds")
    );
    let seen = seen.borrow();
    assert!(
        seen.iter()
            .all(|(shown, ids)| *shown != Some(4) && ids.iter().all(|id| *id == 2)),
        "a stale page or count showed: {seen:?}"
    );
}

// From the killsound filter (1 shown), three queries start before any read runs: no filter
// (4 shown, ids 4 to 1), hitsound most liked (ids 3, 1), then hitsound newest (ids 1, 3).
// Only the last, a sort, may land.
#[gpui_kit::test]
fn a_page_and_count_of_an_older_query_are_ignored(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = sound_grid(cx, &env);
    loaded(cx, win, &grid);
    cx.update(|cx| grid.update(cx, |g, cx| g.set_category(Some("killsound".into()), cx)));
    loaded(cx, win, &grid);
    assert_eq!(ids_of(cx, &grid), [2]);
    let (seen, _sub) = watch(cx, &grid);
    cx.update(|cx| {
        grid.update(cx, |g, cx| {
            g.set_category(None, cx);
            g.set_category(Some("hitsound".into()), cx);
            g.set_sort(StoreSort::Newest, cx);
        })
    });
    loaded(cx, win, &grid);
    assert_eq!(ids_of(cx, &grid), [1, 3]);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("2 of 4 sounds")
    );
    let seen = seen.borrow();
    assert!(
        seen.iter().all(|(shown, ids)| *shown != Some(4)
            && ids.len() <= 2
            && ids.iter().all(|id| [1, 3].contains(id))),
        "a stale page or count showed: {seen:?}"
    );
}

#[gpui_kit::test]
fn an_external_toolbar_grid_draws_no_toolbar(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = support::store_grid_window(
        cx,
        &env,
        StoreGridConfig::new(StoreKind::Crosshairs, "crosshairs").external_toolbar(),
        |env| support::seed_crosshairs(env, 1..=3, 0, false),
    );
    loaded(cx, win, &grid);
    assert!(support::present(cx, win, card_id(3)));
    for id in [
        ids::STORE_SEARCH,
        ids::STORE_COUNT,
        ids::STORE_SORT,
        ids::STORE_SORT_LABEL,
    ] {
        assert!(!support::present(cx, win, id), "{id}");
    }
}

fn offset_y(cx: &mut TestAppContext, grid: &Entity<StoreGrid>) -> gpui_kit::Pixels {
    cx.update(|cx| {
        grid.read(cx)
            .card_grid()
            .scroll_handle()
            .0
            .borrow()
            .base_handle
            .offset()
            .y
    })
}

#[gpui_kit::test]
fn retry_after_a_count_only_failure_reads_no_extra_page(cx: &mut TestAppContext) {
    let env = support::env();
    let db = env.data.db_path();
    let (win, grid) = open(cx, &env, |env| {
        support::seed_crosshairs(env, 1..=150, 0, false)
    });
    loaded(cx, win, &grid);
    // abs() of i64::MIN is an SQLite error, and only crosshair 1 reaches it: COUNT(*) scans
    // every row, while the views-sorted first page walks its index down from 150 and stops at 91.
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch(
            "ALTER TABLE store_crosshairs RENAME TO store_crosshairs_hidden;
             CREATE VIEW store_crosshairs AS SELECT * FROM store_crosshairs_hidden
                 WHERE abs(gb_id - 9223372036854775807 - 2) >= 0;",
        )
        .unwrap();
    cx.update(|cx| grid.update(cx, |g, cx| g.set_sort(StoreSort::MostViewed, cx)));
    support::wait_real(cx, "the count error", |cx| {
        cx.update(|cx| {
            let g = grid.read(cx);
            g.error().is_some() && g.is_loaded() && !g.is_loading()
        })
    });
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), 60);
    assert_eq!(first_id(cx, &grid), 150);
    support::settle(cx, win);
    support::click_id(cx, win, ids::STORE_RETRY);
    support::wait_real(cx, "the retried count", |cx| {
        cx.update(|cx| {
            let g = grid.read(cx);
            g.error().is_some() && !g.is_loading()
        })
    });
    support::settle(cx, win);
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), 60);
}

#[gpui_kit::test]
fn a_search_that_ends_on_the_current_text_keeps_the_feed_and_scroll(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = open(cx, &env, |env| {
        support::seed_crosshairs(env, 1..=150, 0, false)
    });
    loaded(cx, win, &grid);
    cx.update(|cx| {
        grid.read(cx)
            .card_grid()
            .scroll_handle()
            .scroll_to_item(10, gpui_kit::ScrollStrategy::Top)
    });
    // Row 10 is near the end of the first page, so let the next page land first.
    support::settle(cx, win);
    loaded(cx, win, &grid);
    let before = offset_y(cx, &grid);
    assert!(before < gpui_kit::px(0.), "{before:?}");
    let cards = cx.update(|cx| grid.read(cx).cards().len());
    assert!(cards > 60, "{cards}");
    let started = cx.update(|cx| grid.read(cx).query_started());
    // Typed and taken back within the debounce: the last text equals the current search.
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("Cross 7".into(), cx)));
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed(String::new(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| grid.read(cx).query_started()), started);
    assert_eq!(offset_y(cx, &grid), before);
    // Blank text is the same query as no search.
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("   ".into(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| grid.read(cx).query_started()), started);
    assert_eq!(offset_y(cx, &grid), before);
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), cards);
}

// Types `text`, waits out the debounce and lands a search that matches nothing.
fn search_nothing(cx: &mut TestAppContext, grid: &Entity<StoreGrid>, text: &str) {
    let text = text.to_owned();
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed(text, cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the search", |cx| {
        cx.update(|cx| {
            let g = grid.read(cx);
            g.shown() == Some(0) && g.is_loaded() && !g.is_loading()
        })
    });
}

#[gpui_kit::test]
fn a_crosshair_search_with_no_match_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = open(cx, &env, |env| {
        support::seed_crosshairs(env, 1..=3, 0, false)
    });
    loaded(cx, win, &grid);
    search_nothing(cx, &grid, "zzzq");
    assert_eq!(
        support::label_of(cx, win, ids::STORE_EMPTY).as_deref(),
        Some("No crosshairs match your search")
    );
    assert_eq!(
        support::label_of(cx, win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Try adjusting your search criteria.")
    );
    assert!(support::present(cx, win, ids::STORE_EMPTY_ICON));
}

// The no-match query leaves its zero count behind, so the next query must not read it as a
// result while its own reads are pending; the frame is drawn before any of them run.
#[gpui_kit::test]
fn the_no_match_card_stays_away_while_the_next_query_loads(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = sound_grid(cx, &env);
    loaded(cx, win, &grid);
    search_nothing(cx, &grid, "zzzq");
    assert!(support::present(cx, win, ids::STORE_EMPTY));

    cx.update(|cx| grid.update(cx, |g, cx| g.set_category(Some("hitsound".into()), cx)));
    let card = cx
        .update_window(win, |_, w, cx| {
            w.render_frame(cx);
            w.simulate_next_frame(cx);
            w.try_find(ids::STORE_EMPTY).is_some()
        })
        .unwrap();
    assert!(!card, "the card showed while the query was loading");
}

// A stale launch syncs for minutes, so a filled store answers a search while its sync runs.
#[gpui_kit::test]
fn a_sync_keeps_the_no_match_card_until_a_match_lands(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let (win, grid) = sound_grid(cx, &env);
    loaded(cx, win, &grid);
    search_nothing(cx, &grid, "zzzq");
    assert!(support::present(cx, win, ids::STORE_EMPTY));

    cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Sounds, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    support::settle(cx, win);
    assert!(support::present(cx, win, ids::STORE_EMPTY));
    assert!(!support::present(cx, win, ids::STORE_SYNCING));

    run.report(1, 5);
    support::wait_hub(cx, "the empty re-query", |cx| {
        cx.update(|cx| grid.read(cx).requeries() == 1 && !grid.read(cx).is_loading())
    });
    assert!(support::present(cx, win, ids::STORE_EMPTY));

    support::seed_sounds(&env, &[(5, "Zzzq Ding", "hitsound", None)]);
    run.report(2, 5);
    support::wait_hub(cx, "the live match", |cx| {
        cx.update(|cx| grid.read(cx).shown() == Some(1) && !grid.read(cx).is_loading())
    });
    assert!(!support::present(cx, win, ids::STORE_EMPTY));
    let card = ids::store_card(StoreKind::Sounds, &StoreItemId::GameBanana(GbId::new(5)));
    assert!(support::present(cx, win, card));
    run.finish(Ok(SyncSummary::default()));
}

#[gpui_kit::test]
fn an_empty_store_says_it_could_not_load_even_under_a_search(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = open(cx, &env, |_| {});
    loaded(cx, win, &grid);
    search_nothing(cx, &grid, "zzzq");
    assert_eq!(
        support::label_of(cx, win, ids::STORE_UNLOADED).as_deref(),
        Some("Could not load crosshairs")
    );
    assert!(!support::present(cx, win, ids::STORE_EMPTY));
}

// The empty page lands, so only the error guard keeps the card away. abs() of i64::MIN fails on
// crosshair 1 alone, and SQLite runs the correlated term after the search's LIKE, so the search
// never reaches it while the unfiltered total does.
#[gpui_kit::test]
fn a_failed_count_keeps_its_error_line_and_no_no_match_card(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid) = open(cx, &env, |env| {
        support::seed_crosshairs(env, 1..=3, 0, false)
    });
    loaded(cx, win, &grid);
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute_batch(
            "ALTER TABLE store_crosshairs RENAME TO store_crosshairs_hidden;
             CREATE VIEW store_crosshairs AS SELECT * FROM store_crosshairs_hidden h
                 WHERE (SELECT abs(h.gb_id - 9223372036854775807 - 2)) >= 0;",
        )
        .unwrap();
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("zzzq".into(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the count error", |cx| {
        cx.update(|cx| {
            let g = grid.read(cx);
            g.error().is_some() && g.is_loaded() && !g.is_loading()
        })
    });
    assert!(cx.update(|cx| grid.read(cx).cards().is_empty()));
    assert!(support::present(cx, win, ids::STORE_ERROR));
    assert!(!support::present(cx, win, ids::STORE_EMPTY));
}
