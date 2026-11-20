use crate::support;

use std::time::Duration;

// `update_window` is a trait method on the test context.
use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext, px};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshair_store::CrosshairStorePage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_grid::SEARCH_DEBOUNCE;
use tf2tools_app::sync::SyncSummary;
use tf2tools_core::{GbId, Provenance, StoreItemId, StoreKind, StoreSort, Tf2Install};

const C: StoreKind = StoreKind::Crosshairs;

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

/// Store crosshairs 1-3 (likes = id); crosshair 2 already in the library from GameBanana.
fn seed(env: &support::Env) {
    support::seed_crosshairs(env, 1..=3, 0, false);
    let path = env.tmp.path().join("two.png");
    std::fs::write(&path, support::crosshair_png([255, 0, 0, 255])).unwrap();
    let from_gb = Provenance::GameBanana {
        gb_id: GbId::new(2),
        author: "vex".into(),
    };
    support::services(env, support::no_steam())
        .library
        .import_crosshair(&path, "Cross 2", from_gb)
        .unwrap();
}

#[gpui_kit::test]
fn the_page_lists_crosshairs_most_liked_first_with_library_state(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("3 of 3 crosshairs")
    );
    let order: Vec<StoreItemId> = cx.update(|cx| {
        page.read(cx)
            .grid()
            .read(cx)
            .cards()
            .iter()
            .map(|c| c.id.clone())
            .collect()
    });
    assert_eq!(order, [gb(3), gb(2), gb(1)]);
    for id in 1..=3 {
        assert!(
            support::present(cx, win, ids::store_card(C, &gb(id))),
            "card {id}"
        );
    }
    assert!(support::present(cx, win, ids::STORE_SEARCH));
    assert!(support::present(cx, win, ids::STORE_SORT));
    assert_eq!(
        support::label_of(cx, win, ids::store_download(C, &gb(1))).as_deref(),
        Some("Download")
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_download(C, &gb(2))).as_deref(),
        Some("In Library")
    );
    let two = cx.update(|cx| page.read(cx).grid().read(cx).cards()[1].clone());
    assert!(cx.update(|cx| page.read(cx).is_in_library(&two)));
}

#[gpui_kit::test]
fn the_crosshair_store_is_a_real_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    assert!(!Page::CrosshairStore.is_placeholder());
    support::click_id(cx, s.win, ids::nav(Page::CrosshairStore));
    assert!(
        cx.update(|cx| s.shell.read(cx).crosshair_store_page())
            .is_some()
    );
    assert!(support::present(cx, s.win, ids::page(Page::CrosshairStore)));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(
        cx.update(|cx| s.shell.read(cx).crosshair_store_page())
            .is_none()
    );
}

#[gpui_kit::test]
fn the_toolbar_starts_under_the_top_bar(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    support::assert_starts_under_top_bar(cx, win, ids::page(Page::CrosshairStore), ids::STORE_SORT);
}

// v5's Card is py-6 with a px-6 body: the image starts 25 px below the card's outer top, the name
// 24 px below the image, and the button sits 25 px from the sides and bottom.
#[gpui_kit::test]
fn the_crosshair_card_pads_24_px_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    let card = support::bounds_of(cx, win, ids::store_card(C, &gb(1))).unwrap();
    let image = support::bounds_of(cx, win, ids::store_card_image(C, &gb(1))).unwrap();
    let name = support::bounds_of(cx, win, ids::store_card_name(C, &gb(1))).expect("the name");
    let button = support::bounds_of(cx, win, ids::store_download(C, &gb(1))).unwrap();
    assert_eq!(image.top() - card.top(), px(25.), "{image:?} in {card:?}");
    assert_eq!(
        name.top() - image.bottom(),
        px(24.),
        "{name:?} under {image:?}"
    );
    assert_eq!(
        button.left() - card.left(),
        px(25.),
        "{button:?} in {card:?}"
    );
    assert_eq!(
        card.right() - button.right(),
        px(25.),
        "{button:?} in {card:?}"
    );
    assert_eq!(
        card.bottom() - button.bottom(),
        px(25.),
        "{button:?} in {card:?}"
    );
}

// v5's footer: the download button spans the card less px-6, 25 px from its sides and bottom.
#[gpui_kit::test]
fn the_card_download_button_spans_the_card_as_v5(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    support::assert_full_width_download(
        cx,
        win,
        ids::store_card(C, &gb(1)),
        ids::store_download(C, &gb(1)),
    );
}

// v5's crosshair card measures 418 px tall at 1200x900.
#[gpui_kit::test]
fn the_crosshair_card_is_v5s_height(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    let card = support::bounds_of(cx, win, ids::store_card(C, &gb(1))).unwrap();
    let height = f32::from(card.size.height);
    assert!((416.0..=421.0).contains(&height), "{card:?}");
}

// The HUD GameBanana tab's toolbar: the 288x36 search alone at the left over the cards' edge,
// the count, "Sort:" 16 px on and the 144 px select 8 px after at the right, the grid 24 px under.
#[gpui_kit::test]
fn the_toolbar_reads_search_left_then_count_and_sort_right_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    let search = support::bounds_of(cx, win, ids::STORE_SEARCH).expect("the search box");
    let count = support::bounds_of(cx, win, ids::STORE_COUNT).unwrap();
    let label = support::bounds_of(cx, win, ids::STORE_SORT_LABEL).unwrap();
    let sort = support::bounds_of(cx, win, ids::STORE_SORT).unwrap();
    let card = support::bounds_of(cx, win, ids::store_card(C, &gb(3))).unwrap();
    assert_eq!(search.size, gpui_kit::size(px(288.), px(36.)));
    assert_eq!(sort.size, gpui_kit::size(px(144.), px(36.)));
    let one = px(1.);
    for b in [count, label, sort] {
        assert!(
            (support::mid_y(b) - support::mid_y(search)).abs() <= one,
            "{b:?} {search:?}"
        );
    }
    assert!(
        count.left() - search.right() > px(100.),
        "{search:?} {count:?}"
    );
    assert!(
        (label.left() - count.right() - px(16.)).abs() <= one,
        "{count:?} {label:?}"
    );
    assert!(
        (sort.left() - label.right() - px(8.)).abs() <= one,
        "{label:?} {sort:?}"
    );
    assert!(
        (search.left() - card.left()).abs() <= one,
        "{search:?} {card:?}"
    );
    assert_eq!(
        card.top() - search.bottom(),
        px(24.),
        "{card:?} under {search:?}"
    );
}

#[gpui_kit::test]
// The Crosshair card keeps v5's one-tag height, so its tags take one row: a tag that would start
// a second row is hidden whole, never cut, and the stats sit mt-2 under the row.
fn the_crosshair_card_shows_one_row_of_tags(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(
        cx,
        &env,
        &server,
        |env| {
            seed(env);
            support::set_gb_tags(env, "store_crosshairs", 1, &support::LONG_TAGS);
        },
        CrosshairStorePage::new,
    );
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    let id = gb(1);
    let card = support::bounds_of(cx, win, ids::store_card(C, &id)).unwrap();
    let button = support::bounds_of(cx, win, ids::store_download(C, &id)).unwrap();
    let rows = support::bounds_of(cx, win, ids::store_card_tags(C, &id)).expect("the tag row");
    let first = support::bounds_of(cx, win, ids::store_card_tag(C, &id, 0)).unwrap();
    assert_eq!(rows.size.height, px(21.), "{rows:?}");
    assert_eq!(first.top(), rows.top(), "{first:?} in {rows:?}");
    if let Some(second) = support::bounds_of(cx, win, ids::store_card_tag(C, &id, 1)) {
        assert!(
            second.top() >= rows.bottom(),
            "{second:?} shows in {rows:?}"
        );
    }
    let likes = support::bounds_of(cx, win, ids::store_likes(C, &id)).unwrap();
    assert_eq!(
        likes.top() - rows.bottom(),
        px(8.),
        "{likes:?} under {rows:?}"
    );
    assert!(
        likes.bottom() + px(32.) <= button.top(),
        "{likes:?} crowds {button:?}"
    );
    assert_eq!(card.size.height, px(417.), "{card:?}");
}

#[gpui_kit::test]
// v5's Crosshair Store starts its grid 24 px under the title row's Sort.
fn the_grid_starts_24_px_under_the_toolbar_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    let sort = support::bounds_of(cx, win, ids::STORE_SORT).unwrap();
    let card = support::bounds_of(cx, win, ids::store_card(C, &gb(1))).unwrap();
    assert_eq!(
        card.top() - sort.bottom(),
        px(24.),
        "{card:?} under {sort:?}"
    );
}

#[gpui_kit::test]
fn the_banner_shows_while_crosshairs_sync(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, runs) = support::fake_hub(cx);
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    assert!(!support::present(cx, win, ids::SYNC_BANNER));
    cx.update(|cx| hub.update(cx, |h, cx| h.start(C, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(run.kind, C);
    run.report(40, 300);
    support::next_repaint(cx);
    assert_eq!(
        support::label_of(cx, win, ids::SYNC_BANNER).as_deref(),
        Some("Syncing crosshairs from GameBanana... Not all items loaded yet.")
    );
    assert_eq!(
        support::label_of(cx, win, ids::SYNC_BANNER_COUNT).as_deref(),
        Some("40 / 300")
    );
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the banner to go", |cx| {
        !support::present(cx, win, ids::SYNC_BANNER)
    });
}

#[gpui_kit::test]
fn another_stores_sync_leaves_the_banner_hidden(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, runs) = support::fake_hub(cx);
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Sounds, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(run.kind, StoreKind::Sounds);
    run.report(40, 300);
    support::next_repaint(cx);
    let sounds = cx.update(|cx| hub.read(cx).progress(StoreKind::Sounds).is_some());
    assert!(sounds, "the sounds sync is under way");
    assert!(!support::present(cx, win, ids::SYNC_BANNER));
    run.finish(Ok(SyncSummary::default()));
}

#[gpui_kit::test]
fn a_long_store_grid_shows_a_bar_beside_its_cards(cx: &mut TestAppContext) {
    use support::scroll::{assert_bar_beside, assert_clear_of_bar, assert_track_scrolls_virtual};
    let env = support::env();
    let server = support::StoreServer::start();
    let seed = |env: &support::Env| support::seed_crosshairs(env, 1..=200, 0, false);
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    support::settle(cx, win);
    let bar = ids::grid_scrollbar(ids::STORE_GRID);
    assert_bar_beside(cx, win, ids::STORE_GRID, bar.clone());
    // Most liked first: 200 heads the first row and 197 ends it at the grid's right edge.
    let columns = 4;
    let last = ids::store_card(C, &gb(201 - columns));
    assert_clear_of_bar(cx, win, last, bar.clone());
    assert_track_scrolls_virtual(cx, win, bar, ids::store_card(C, &gb(200)));
}

// Types `text` into the page's search box, waits out the debounce and lands `shown` cards.
fn search(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    page: &Entity<CrosshairStorePage>,
    text: &str,
    shown: u64,
) {
    assert!(
        support::present(cx, win, ids::STORE_SEARCH),
        "the search box"
    );
    let grid = cx.update(|cx| page.read(cx).grid().clone());
    let text = text.to_owned();
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed(text, cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the search", |cx| {
        cx.update(|cx| {
            let g = grid.read(cx);
            g.shown() == Some(shown) && g.is_loaded() && !g.is_loading()
        })
    });
}

fn shown_ids(cx: &mut TestAppContext, page: &Entity<CrosshairStorePage>) -> Vec<StoreItemId> {
    cx.update(|cx| {
        page.read(cx)
            .grid()
            .read(cx)
            .cards()
            .iter()
            .map(|c| c.id.clone())
            .collect()
    })
}

#[gpui_kit::test]
fn the_search_box_asks_for_crosshairs(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    support::settle(cx, win);
    let placeholder = cx
        .update_window(win, |_, w, _| {
            w.within(ids::STORE_SEARCH)
                .elements()
                .iter()
                .find_map(|e| e.placeholder().map(str::to_owned))
        })
        .unwrap();
    assert_eq!(placeholder.as_deref(), Some("Search crosshairs..."));
}

#[gpui_kit::test]
fn a_search_matches_part_of_a_name_in_any_case(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    for text in ["cross 2", "ROSS 2"] {
        search(cx, win, &page, text, 1);
        assert_eq!(shown_ids(cx, &page), [gb(2)], "{text}");
        assert_eq!(
            support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
            Some("1 of 3 crosshairs"),
            "{text}"
        );
        assert!(support::present(cx, win, ids::store_card(C, &gb(2))));
        assert!(!support::present(cx, win, ids::store_card(C, &gb(1))));
    }
}

#[gpui_kit::test]
fn a_search_matches_the_author(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let seed = |env: &support::Env| {
        seed(env);
        rusqlite::Connection::open(env.data.db_path())
            .unwrap()
            .execute(
                "UPDATE store_crosshairs SET author = 'Kestrel' WHERE gb_id = 1",
                (),
            )
            .unwrap();
    };
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    search(cx, win, &page, "kestrel", 1);
    assert_eq!(shown_ids(cx, &page), [gb(1)]);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("1 of 3 crosshairs")
    );
}

#[gpui_kit::test]
fn a_new_sort_keeps_the_search_and_reorders_its_matches(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let seed = |env: &support::Env| support::seed_crosshairs(env, 1..=12, 0, false);
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    // "Cross 1" and "Cross 10" to "Cross 12": the most liked first, then the newest first.
    search(cx, win, &page, "Cross 1", 4);
    assert_eq!(shown_ids(cx, &page), [gb(12), gb(11), gb(10), gb(1)]);
    let grid = cx.update(|cx| page.read(cx).grid().clone());
    cx.update(|cx| grid.update(cx, |g, cx| g.set_sort(StoreSort::Newest, cx)));
    support::wait_real(cx, "the sort", |cx| {
        cx.update(|cx| {
            let g = grid.read(cx);
            g.is_loaded() && !g.is_loading() && g.cards().first().map(|c| &c.id) == Some(&gb(1))
        })
    });
    assert_eq!(cx.update(|cx| grid.read(cx).search().to_owned()), "Cross 1");
    assert_eq!(shown_ids(cx, &page), [gb(1), gb(10), gb(11), gb(12)]);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("4 of 12 crosshairs")
    );
}

#[gpui_kit::test]
fn clearing_the_search_brings_every_crosshair_back(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    search(cx, win, &page, "Cross 2", 1);
    search(cx, win, &page, "", 3);
    assert_eq!(shown_ids(cx, &page), [gb(3), gb(2), gb(1)]);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("3 of 3 crosshairs")
    );
}

#[gpui_kit::test]
fn a_search_with_no_match_says_so_on_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    search(cx, win, &page, "zzzq", 0);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_EMPTY).as_deref(),
        Some("No crosshairs match your search")
    );
    assert_eq!(
        support::label_of(cx, win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Try adjusting your search criteria.")
    );
}

#[gpui_kit::test]
fn the_search_waits_out_the_debounce(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    assert!(
        support::present(cx, win, ids::STORE_SEARCH),
        "the search box"
    );
    let grid = cx.update(|cx| page.read(cx).grid().clone());
    let opened = cx.update(|cx| grid.read(cx).query_started());
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("Cross 2".into(), cx)));
    cx.executor()
        .advance_clock(SEARCH_DEBOUNCE - Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| grid.read(cx).query_started()), opened);
    assert_eq!(cx.update(|cx| grid.read(cx).shown()), Some(3));
    cx.executor().advance_clock(Duration::from_millis(1));
    support::wait_real(cx, "the search", |cx| {
        cx.update(|cx| grid.read(cx).shown() == Some(1) && !grid.read(cx).is_loading())
    });
    assert_eq!(shown_ids(cx, &page), [gb(2)]);
}
