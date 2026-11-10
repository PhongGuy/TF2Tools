use crate::support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::pages::sound_store::model::Category;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_grid::SEARCH_DEBOUNCE;
use tf2tools_app::sync::SyncSummary;
use tf2tools_core::{GbId, StoreItemId, StoreKind, StoreSort};

const S: StoreKind = StoreKind::Sounds;

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

fn seed(env: &support::Env) {
    support::seed_sounds(
        env,
        &[
            (1, "Ding", "hitsound", None),
            (2, "Boom", "killsound", None),
            (3, "Blip", "hitsound", None),
        ],
    );
}

#[gpui_kit::test]
fn the_page_lists_sound_cards_under_v5_s_pills(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    for id in 1..=3 {
        assert!(
            support::present(cx, w.win, ids::store_card(S, &gb(id))),
            "card {id}"
        );
    }
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_COUNT).as_deref(),
        Some("3 of 3 sounds")
    );
    for slug in ["all", "hitsound", "killsound"] {
        assert!(
            support::present(cx, w.win, ids::store_category(slug)),
            "pill {slug}"
        );
    }
    assert!(
        !support::present(cx, w.win, ids::SYNC_BANNER),
        "no banner without a sync"
    );
    assert_eq!(cx.update(|cx| w.page.read(cx).category()), Category::All);
}

// v5: search left and count right; 24 px below, the pills left and Sort at the right edge.
#[gpui_kit::test]
fn the_search_row_sits_above_the_pills_and_sort_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    let search = support::bounds_of(cx, w.win, ids::STORE_SEARCH).unwrap();
    let count = support::bounds_of(cx, w.win, ids::STORE_COUNT).unwrap();
    let all = support::bounds_of(cx, w.win, ids::store_category("all")).unwrap();
    let hit = support::bounds_of(cx, w.win, ids::store_category("hitsound")).unwrap();
    let kill = support::bounds_of(cx, w.win, ids::store_category("killsound")).unwrap();
    let label = support::bounds_of(cx, w.win, ids::STORE_SORT_LABEL).unwrap();
    let sort = support::bounds_of(cx, w.win, ids::STORE_SORT).unwrap();
    let one = gpui_kit::px(1.);
    assert_eq!(search.size.width, gpui_kit::px(288.));
    assert!(search.right() < count.left(), "{search:?} {count:?}");
    assert!((support::mid_y(search) - support::mid_y(count)).abs() <= one);
    // gap-6 between the rows; the 36 px (h_9) trigger is the second row's tallest child.
    assert!(
        (sort.top() - search.bottom() - gpui_kit::px(24.)).abs() <= one,
        "{search:?} {sort:?}"
    );
    for b in [all, hit, kill, label] {
        assert!(
            (support::mid_y(b) - support::mid_y(sort)).abs() <= one,
            "{b:?} {sort:?}"
        );
    }
    assert!(
        (hit.left() - all.right() - gpui_kit::px(4.)).abs() <= one,
        "gap-1: {all:?} {hit:?}"
    );
    assert!(
        (kill.left() - hit.right() - gpui_kit::px(4.)).abs() <= one,
        "gap-1: {hit:?} {kill:?}"
    );
    assert!(kill.right() < label.left(), "{kill:?} {label:?}");
    assert!(
        (sort.left() - label.right() - gpui_kit::px(8.)).abs() <= one,
        "{label:?} {sort:?}"
    );
    assert_eq!(sort.size.width, gpui_kit::px(144.));
    assert!(
        (sort.right() - count.right()).abs() <= one,
        "{sort:?} {count:?}"
    );
}

#[gpui_kit::test]
fn a_category_pill_filters_the_grid(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    support::click_id(cx, w.win, ids::store_category("killsound"));
    assert_eq!(
        cx.update(|cx| w.page.read(cx).category()),
        Category::Killsounds
    );
    support::wait_real(cx, "the filter", |cx| {
        cx.update(|cx| w.page.read(cx).grid().read(cx).shown() == Some(1))
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_COUNT).as_deref(),
        Some("1 of 3 sounds")
    );
    assert!(!support::present(cx, w.win, ids::store_card(S, &gb(1))));
    support::click_id(cx, w.win, ids::store_category("all"));
    support::wait_real(cx, "all again", |cx| {
        cx.update(|cx| w.page.read(cx).grid().read(cx).shown() == Some(3))
    });
}

#[gpui_kit::test]
fn the_banner_shows_while_sounds_sync(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, runs) = support::fake_hub(cx);
    let w = support::sound_store_window(cx, &env, &server, seed);
    cx.update(|cx| hub.update(cx, |h, cx| h.start(S, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    run.report(120, 1000);
    support::next_repaint(cx);
    assert_eq!(
        support::label_of(cx, w.win, ids::SYNC_BANNER).as_deref(),
        Some("Syncing sounds from GameBanana... Not all items loaded yet.")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::SYNC_BANNER_COUNT).as_deref(),
        Some("120 / 1000")
    );
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the banner to go", |cx| {
        !support::present(cx, w.win, ids::SYNC_BANNER)
    });
}

#[gpui_kit::test]
fn the_sound_store_is_a_real_page_that_shows_the_player(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    assert!(!Page::SoundStore.is_placeholder());
    assert!(Page::SoundStore.shows_player());
    support::click_id(cx, s.win, ids::nav(Page::SoundStore));
    assert!(
        cx.update(|cx| s.shell.read(cx).sound_store_page())
            .is_some()
    );
    assert!(support::present(cx, s.win, ids::page(Page::SoundStore)));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(
        cx.update(|cx| s.shell.read(cx).sound_store_page())
            .is_none()
    );
}

#[gpui_kit::test]
fn the_card_download_button_spans_the_card_as_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    support::assert_full_width_download(
        cx,
        w.win,
        ids::store_card(S, &gb(1)),
        ids::store_download(S, &gb(1)),
    );
}

// v5's card and detail show likes and views compacted; seed_sounds gives id 1250 1250 likes
// (1.3K) and 1,250,000 views (1.3M), so the two labels cannot be mistaken for each other.
#[gpui_kit::test]
fn the_sound_card_and_detail_show_likes_and_views(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, |env| {
        support::seed_sounds(env, &[(1250, "Ding", "hitsound", None)]);
    });
    let id = gb(1250);
    assert_eq!(
        support::label_of(cx, w.win, ids::store_likes(S, &id)).as_deref(),
        Some("1.3K")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::store_views(S, &id)).as_deref(),
        Some("1.3M")
    );
    support::click_id(cx, w.win, ids::store_card(S, &id));
    support::wait_real(cx, "the detail counts", |cx| {
        support::present(cx, w.win, ids::STORE_DETAIL_LIKES)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_DETAIL_LIKES).as_deref(),
        Some("1.3K")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_DETAIL_VIEWS).as_deref(),
        Some("1.3M")
    );
}

#[gpui_kit::test]
fn the_sound_card_badges_are_v5_s_21_px(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    let card = support::bounds_of(cx, w.win, ids::store_card(S, &gb(1))).unwrap();
    let category = support::bounds_of(cx, w.win, ids::store_card_category(S, &gb(1))).unwrap();
    let tag = support::bounds_of(cx, w.win, ids::store_card_tag(S, &gb(1), 0)).unwrap();
    let button = support::bounds_of(cx, w.win, ids::store_download(S, &gb(1))).unwrap();
    assert_eq!(category.size.height, gpui_kit::px(21.));
    assert_eq!(tag.size.height, gpui_kit::px(21.));
    assert!(
        tag.bottom() + gpui_kit::px(8.) <= button.top(),
        "{tag:?} crowds {button:?}"
    );
    assert_eq!(
        card.bottom() - button.bottom(),
        gpui_kit::px(25.),
        "{button:?} in {card:?}"
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::store_card_tag(S, &gb(1), 0)).as_deref(),
        Some("Purity: Fun")
    );
}

// v5's sound card: py-6 puts a 24 px band above the 88 px header, then the body is px-6 under a
// gap-6 and over the py-6.
#[gpui_kit::test]
fn the_sound_card_pads_24_px_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    let id = gb(1);
    let card = support::bounds_of(cx, w.win, ids::store_card(S, &id)).unwrap();
    let header = support::bounds_of(cx, w.win, ids::store_card_image(S, &id)).expect("the header");
    let name = support::bounds_of(cx, w.win, ids::store_card_name(S, &id)).expect("the name");
    let button = support::bounds_of(cx, w.win, ids::store_download(S, &id)).unwrap();
    assert_eq!(
        header.top() - card.top(),
        gpui_kit::px(25.),
        "{header:?} in {card:?}"
    );
    assert_eq!(header.size.height, gpui_kit::px(88.));
    assert_eq!(header.size.width, card.size.width - gpui_kit::px(2.));
    assert_eq!(
        name.top() - header.bottom(),
        gpui_kit::px(24.),
        "{name:?} under {header:?}"
    );
    assert_eq!(
        name.left() - card.left(),
        gpui_kit::px(25.),
        "{name:?} in {card:?}"
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::store_card_name(S, &id)).as_deref(),
        Some("Ding")
    );
    assert_eq!(
        card.bottom() - button.bottom(),
        gpui_kit::px(25.),
        "{button:?} in {card:?}"
    );
    assert_eq!(card.size.height, gpui_kit::px(382.), "{card:?}");
}

#[gpui_kit::test]
// v5's SoundStoreCard body: the header block and 14 px under it, gap-6 to the badges, which sit
// gap-1.5 apart, then mt-2 to the 16 px stats and pb-2 plus gap-6 to the button.
fn the_sound_card_spaces_its_sections_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    let id = gb(1);
    let name = support::bounds_of(cx, w.win, ids::store_card_name(S, &id)).unwrap();
    let category = support::bounds_of(cx, w.win, ids::store_card_category(S, &id)).unwrap();
    let tag = support::bounds_of(cx, w.win, ids::store_card_tag(S, &id, 0)).unwrap();
    let rows = support::bounds_of(cx, w.win, ids::store_card_tags(S, &id)).unwrap();
    let likes = support::bounds_of(cx, w.win, ids::store_likes(S, &id)).unwrap();
    let button = support::bounds_of(cx, w.win, ids::store_download(S, &id)).unwrap();
    assert_eq!(name.size.height, gpui_kit::px(24.), "{name:?}");
    assert_eq!(
        category.top() - name.top(),
        gpui_kit::px(82.),
        "{category:?} under {name:?}"
    );
    assert_eq!(
        tag.left() - category.right(),
        gpui_kit::px(6.),
        "{tag:?} beside {category:?}"
    );
    assert_eq!(
        likes.top() - category.bottom(),
        gpui_kit::px(8.),
        "{likes:?}"
    );
    assert_eq!(likes.size.height, gpui_kit::px(16.), "{likes:?}");
    assert!(rows.size.height <= gpui_kit::px(48.), "{rows:?}");
    assert!(
        likes.bottom() + gpui_kit::px(32.) <= button.top(),
        "{likes:?} crowds {button:?}"
    );
}

// v5's items-start: the play button's top meets the name's, not centred on name and author.
#[gpui_kit::test]
fn the_play_button_sits_at_the_top_of_the_name_row_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let url = server.mount_preview(1, vec![1, 2, 3]);
    let w = support::sound_store_window(cx, &env, &server, |env| {
        support::seed_sounds(env, &[(1, "Ding", "hitsound", Some(url.as_str()))]);
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::store_card_name(S, &gb(1))).as_deref(),
        Some("Ding")
    );
    let name = support::bounds_of(cx, w.win, ids::store_card_name(S, &gb(1))).unwrap();
    let play = support::bounds_of(cx, w.win, ids::store_play(S, &gb(1))).unwrap();
    assert_eq!(play.size.height, gpui_kit::px(36.));
    assert_eq!(play.top(), name.top(), "{play:?} {name:?}");
}

fn pill_selected(
    cx: &mut TestAppContext,
    win: gpui_kit::AnyWindowHandle,
    slug: &str,
) -> Option<bool> {
    support::selected_of(cx, win, ids::store_category(slug))
}

// v5's FilterBadge is a <button>: Tab reaches each pill, and Enter or Space picks it.
#[gpui_kit::test]
fn the_category_pills_take_tab_focus_and_enter_or_space(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    support::tab_to(cx, w.win, ids::store_category("killsound"), 64);
    support::press(cx, w.win, "enter");
    assert_eq!(
        cx.update(|cx| w.page.read(cx).category()),
        Category::Killsounds
    );
    support::tab_to(cx, w.win, ids::store_category("all"), 64);
    support::press(cx, w.win, "space");
    assert_eq!(cx.update(|cx| w.page.read(cx).category()), Category::All);
}

// v5's FilterBadge: 21 px badges, the chosen category filled, the others outline.
#[gpui_kit::test]
fn the_category_pills_are_v5_s_21_px_badges_and_keep_their_selection(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    for slug in ["all", "hitsound", "killsound"] {
        let pill = support::bounds_of(cx, w.win, ids::store_category(slug)).expect(slug);
        assert_eq!(pill.size.height, gpui_kit::px(21.), "{slug}");
    }
    assert_eq!(
        support::label_of(cx, w.win, ids::store_category("killsound")).as_deref(),
        Some("Killsounds")
    );
    assert_eq!(pill_selected(cx, w.win, "all"), Some(true));
    assert_eq!(pill_selected(cx, w.win, "killsound"), Some(false));
    support::click_id(cx, w.win, ids::store_category("killsound"));
    assert_eq!(pill_selected(cx, w.win, "killsound"), Some(true));
    assert_eq!(pill_selected(cx, w.win, "all"), Some(false));
    assert_eq!(
        cx.update(|cx| w.page.read(cx).category()),
        Category::Killsounds
    );
}

#[gpui_kit::test]
// v5's Sound Store starts its grid 24 px under the category and Sort row.
fn the_sound_grid_starts_24_px_under_the_filter_row_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    let sort = support::bounds_of(cx, w.win, ids::STORE_SORT).unwrap();
    let card = support::bounds_of(cx, w.win, ids::store_card(S, &gb(1))).unwrap();
    assert_eq!(
        card.top() - sort.bottom(),
        gpui_kit::px(24.),
        "{card:?} under {sort:?}"
    );
}

#[gpui_kit::test]
// The category and three long tags overflow v5's 2-row cap: the row box stops at two rows, the
// stats sit 8 px under it and nothing past the cap starts inside it.
fn the_sound_card_caps_its_badges_at_two_rows(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, |env| {
        seed(env);
        support::set_gb_tags(env, "store_sounds", 1, &support::LONG_TAGS);
    });
    let id = gb(1);
    let card = support::bounds_of(cx, w.win, ids::store_card(S, &id)).unwrap();
    let rows = support::bounds_of(cx, w.win, ids::store_card_tags(S, &id)).unwrap();
    let likes = support::bounds_of(cx, w.win, ids::store_likes(S, &id)).unwrap();
    let button = support::bounds_of(cx, w.win, ids::store_download(S, &id)).unwrap();
    assert_eq!(rows.size.height, gpui_kit::px(48.), "{rows:?}");
    for ix in 0..3 {
        let tag = support::bounds_of(cx, w.win, ids::store_card_tag(S, &id, ix)).unwrap();
        assert!(
            tag.bottom() <= rows.bottom() || tag.top() >= rows.bottom(),
            "{tag:?} half shown in {rows:?}"
        );
    }
    let last = support::bounds_of(cx, w.win, ids::store_card_tag(S, &id, 2)).unwrap();
    assert!(last.top() >= rows.bottom(), "{last:?} shows in {rows:?}");
    assert_eq!(
        likes.top() - rows.bottom(),
        gpui_kit::px(8.),
        "{likes:?} under {rows:?}"
    );
    assert_eq!(
        button.left() - card.left(),
        gpui_kit::px(25.),
        "{button:?} in {card:?}"
    );
    assert_eq!(card.size.height, gpui_kit::px(382.), "{card:?}");
}

#[gpui_kit::test]
fn the_banner_suffix_appears_when_rows_land_without_a_hub_tick(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, runs) = support::fake_hub(cx);
    let w = support::sound_store_window(cx, &env, &server, |_| {});
    cx.update(|cx| hub.update(cx, |h, cx| h.start(S, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    run.report(1, 1000);
    support::next_repaint(cx);
    assert_eq!(
        support::label_of(cx, w.win, ids::SYNC_BANNER).as_deref(),
        Some("Syncing sounds from GameBanana...")
    );
    // Rows land through the grid's own re-query; the hub reports nothing new.
    seed(&env);
    let grid = cx.update(|cx| w.page.read(cx).grid().clone());
    cx.update(|cx| grid.update(cx, |g, cx| g.set_sort(StoreSort::Newest, cx)));
    support::wait_real(cx, "the rows", |cx| {
        cx.update(|cx| grid.read(cx).total() == Some(3))
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::SYNC_BANNER).as_deref(),
        Some("Syncing sounds from GameBanana... Not all items loaded yet.")
    );
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the banner to go", |cx| {
        !support::present(cx, w.win, ids::SYNC_BANNER)
    });
}

#[gpui_kit::test]
fn the_page_redraws_when_its_grid_changes(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    let grid = cx.update(|cx| w.page.read(cx).grid().clone());
    let redraws = Rc::new(Cell::new(0));
    let seen = Rc::clone(&redraws);
    let _sub = cx.update(|cx| cx.observe(&w.page, move |_, _| seen.set(seen.get() + 1)));
    cx.update(|cx| grid.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
    assert!(
        redraws.get() > 0,
        "the banner reads the grid's rows, so a grid change must redraw the page"
    );
}

#[gpui_kit::test]
fn a_search_with_no_match_says_v5_s_hint_until_it_is_cleared(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, seed);
    let grid = cx.update(|cx| w.page.read(cx).grid().clone());
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("zzzq".into(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the search", |cx| {
        cx.update(|cx| grid.read(cx).shown() == Some(0) && !grid.read(cx).is_loading())
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_COUNT).as_deref(),
        Some("0 of 3 sounds")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_EMPTY).as_deref(),
        Some("No sounds match your filters")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Try adjusting your search or filter criteria.")
    );
    assert!(support::present(cx, w.win, ids::STORE_EMPTY_ICON));

    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed(String::new(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the cleared search", |cx| {
        cx.update(|cx| grid.read(cx).shown() == Some(3) && !grid.read(cx).is_loading())
    });
    assert!(support::present(cx, w.win, ids::store_card(S, &gb(1))));
    assert!(!support::present(cx, w.win, ids::STORE_EMPTY));
}

#[gpui_kit::test]
fn a_category_with_no_match_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::sound_store_window(cx, &env, &server, |env| {
        support::seed_sounds(env, &[(1, "Ding", "hitsound", None)])
    });
    support::click_id(cx, w.win, ids::store_category("killsound"));
    support::wait_real(cx, "the filter", |cx| {
        cx.update(|cx| {
            let grid = w.page.read(cx).grid().read(cx);
            grid.shown() == Some(0) && !grid.is_loading()
        })
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_EMPTY).as_deref(),
        Some("No sounds match your filters")
    );
}

// At the shell's content width in an 800 px window, the banner's count stays inside the banner,
// beside its text, instead of running past the page's right padding.
#[gpui_kit::test]
fn the_banner_count_fits_a_narrow_page(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, runs) = support::fake_hub(cx);
    let w = support::sound_store_window(cx, &env, &server, seed);
    let width = gpui_kit::px(544.);
    cx.simulate_window_resize(w.win, gpui_kit::size(width, gpui_kit::px(600.)));
    cx.update(|cx| hub.update(cx, |h, cx| h.start(S, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    run.report(3450, 3450);
    support::next_repaint(cx);
    let text = support::bounds_of(cx, w.win, ids::SYNC_BANNER).unwrap();
    let count = support::bounds_of(cx, w.win, ids::SYNC_BANNER_COUNT).unwrap();
    let edge = width - tf2tools_app::shell::CONTENT_PADDING;
    assert!(count.right() <= edge, "{count:?} past {edge:?}");
    assert!(text.right() <= count.left(), "{text:?} {count:?}");
    run.finish(Ok(SyncSummary::default()));
}
