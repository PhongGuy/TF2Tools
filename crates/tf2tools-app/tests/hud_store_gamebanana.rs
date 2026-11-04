mod support;

use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_store::gamebanana_tab::GameBananaTab;
use tf2tools_app::pages::hud_store::{HudStorePage, HudStoreTab};
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_download::StoreDownloads;
use tf2tools_app::store_grid::SEARCH_DEBOUNCE;
use tf2tools_app::sync::SyncSummary;
use tf2tools_core::{GbId, Provenance, StoreItemId, StoreKind, Tf2Install};

const GB: StoreKind = StoreKind::GameBananaHuds;

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

fn gamebanana(cx: &mut TestAppContext, page: &Entity<HudStorePage>) -> Entity<GameBananaTab> {
    let tab = cx
        .update(|cx| page.read(cx).gamebanana())
        .expect("the GameBanana tab");
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| tab.read(cx).grid().read(cx).is_loaded())
    });
    tab
}

/// FlawHUD (7) and ToonHUD (8) in the store, ToonHUD imported from GameBanana; on the GameBanana tab.
fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> (AnyWindowHandle, Entity<HudStorePage>) {
    let (win, page) = support::store_window(
        cx,
        env,
        server,
        |env| {
            support::seed_gb_huds(env, &[(7, "FlawHUD"), (8, "ToonHUD")]);
            let archive = env.tmp.path().join("toonhud.zip");
            std::fs::write(&archive, support::hud_zip_bytes("toonhud")).unwrap();
        },
        HudStorePage::new,
    );
    cx.update(|cx| {
        let s = cx.global::<Services>().clone();
        let gb8 = Provenance::GameBanana {
            gb_id: GbId::new(8),
            author: "vex".into(),
        };
        s.library
            .import_hud(&env.tmp.path().join("toonhud.zip"), gb8)
            .unwrap();
    });
    support::click_id(cx, win, ids::HUD_STORE_TAB_GAMEBANANA);
    (win, page)
}

const OVERLONG: &str = "Casual Compatibility: Casual Compatible in every gamemode, for every class and every weapon slot";

/// FlawHUD (7) with one overlong tag, ToonHUD (8) with `support::LONG_TAGS`, NoTags (9) with
/// none; on the GameBanana tab.
fn open_tagged(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> (AnyWindowHandle, Entity<HudStorePage>) {
    let (win, page) = support::store_window(
        cx,
        env,
        server,
        |env| {
            support::seed_gb_huds(env, &[(7, "FlawHUD"), (8, "ToonHUD"), (9, "NoTags")]);
            support::set_gb_tags(env, "store_huds_gb", 7, &[OVERLONG]);
            support::set_gb_tags(env, "store_huds_gb", 8, &support::LONG_TAGS);
            support::set_gb_tags(env, "store_huds_gb", 9, &[]);
        },
        HudStorePage::new,
    );
    support::click_id(cx, win, ids::HUD_STORE_TAB_GAMEBANANA);
    (win, page)
}

#[gpui_kit::test]
fn the_tab_lists_the_store_with_library_state(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    assert_eq!(cx.update(|cx| page.read(cx).tab()), HudStoreTab::GameBanana);
    let _tab = gamebanana(cx, &page);
    assert!(support::present(cx, win, ids::store_card(GB, &gb(7))));
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("2 of 2 HUDs")
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_download(GB, &gb(7))).as_deref(),
        Some("Download")
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_download(GB, &gb(8))).as_deref(),
        Some("In Library")
    );
}

#[gpui_kit::test]
fn the_in_library_button_opens_the_detail_with_its_badge(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    support::click_id(cx, win, ids::store_download(GB, &gb(8)));
    assert!(support::present(cx, win, ids::STORE_DETAIL));
    assert!(support::present(cx, win, ids::STORE_DETAIL_BADGE));
}

#[gpui_kit::test]
fn a_click_on_the_card_opens_the_detail(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    support::click_id(cx, win, ids::store_card(GB, &gb(7)));
    assert!(support::present(cx, win, ids::STORE_DETAIL));
    assert!(!support::present(cx, win, ids::STORE_DETAIL_BADGE));
}

#[gpui_kit::test]
fn a_download_from_the_store_lands_on_the_huds_page(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        7,
        &[("flawhud.zip", support::hud_zip_bytes("flawhud"))],
        Duration::from_millis(800),
    );
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_gb_huds(&env, &[(7, "FlawHUD")]);
    support::click_id(cx, s.win, ids::nav(Page::HudStore));
    let page = cx
        .update(|cx| s.shell.read(cx).hud_store_page())
        .expect("the page");
    support::click_id(cx, s.win, ids::HUD_STORE_TAB_GAMEBANANA);
    let tab = gamebanana(cx, &page);
    support::click_id(cx, s.win, ids::store_download(GB, &gb(7)));
    support::wait_real(cx, "the file row", |cx| {
        support::present(cx, s.win, ids::store_file(0))
    });
    // The fake server's delay is real time and a settle can outlast it, so click without
    // settling past the click and check the download is running straight away.
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, win, cx| win.click(ids::store_file(0), cx))
        .unwrap();
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    assert!(cx.update(|cx| dl.read(cx).is_downloading(GB, &gb(7))));
    // One frame draws the busy card; `label_of` would settle and could outlast the download.
    let label = cx
        .update_window(s.win, |_, win, cx| {
            win.render_frame(cx);
            win.try_find(ids::store_download(GB, &gb(7)))
                .and_then(|e| e.label().map(str::to_owned))
        })
        .unwrap()
        .unwrap();
    assert!(
        label.starts_with("Downloading..."),
        "the card shows progress: {label}"
    );
    support::wait_real(cx, "the download", |cx| {
        cx.update(|cx| !dl.read(cx).is_busy())
    });
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("HUD downloaded. FlawHUD added to your library")
    );
    cx.update_window(s.win, |_, window, cx| window.close_dialog(cx))
        .unwrap();
    let card = cx.update(|cx| tab.read(cx).grid().read(cx).cards()[0].clone());
    assert!(cx.update(|cx| tab.read(cx).is_in_library(&card)));
    assert_eq!(
        support::label_of(cx, s.win, ids::store_download(GB, &gb(7))).as_deref(),
        Some("In Library")
    );
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::wait_real(cx, "the HUD card", |cx| {
        support::present(cx, s.win, ids::hud_card("flawhud"))
    });
}

#[gpui_kit::test]
fn the_card_download_button_spans_the_card_as_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    support::assert_full_width_download(
        cx,
        win,
        ids::store_card(GB, &gb(7)),
        ids::store_download(GB, &gb(7)),
    );
}

#[gpui_kit::test]
fn cards_and_the_detail_show_compact_counts(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(
        cx,
        &env,
        &server,
        |env| {
            support::seed_gb_huds(env, &[(7, "FlawHUD")]);
            let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
            conn.execute(
                "UPDATE store_huds_gb SET like_count = 1250, view_count = 1250000 WHERE gb_id = 7",
                [],
            )
            .unwrap();
        },
        HudStorePage::new,
    );
    support::click_id(cx, win, ids::HUD_STORE_TAB_GAMEBANANA);
    let _tab = gamebanana(cx, &page);
    assert_eq!(
        support::label_of(cx, win, ids::store_likes(GB, &gb(7))).as_deref(),
        Some("1.3K")
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_views(GB, &gb(7))).as_deref(),
        Some("1.3M")
    );
    support::click_id(cx, win, ids::store_card(GB, &gb(7)));
    support::wait_real(cx, "the detail counts", |cx| {
        support::present(cx, win, ids::STORE_DETAIL_LIKES)
    });
    assert_eq!(
        support::label_of(cx, win, ids::STORE_DETAIL_LIKES).as_deref(),
        Some("1.3K")
    );
    assert_eq!(
        support::label_of(cx, win, ids::STORE_DETAIL_VIEWS).as_deref(),
        Some("1.3M")
    );
}

// v5's GameBanana toolbar: w-72 search left; count, then "Sort:" and a w-36 trigger, right.
#[gpui_kit::test]
fn the_toolbar_reads_search_left_then_count_and_sort_right_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    let search = support::bounds_of(cx, win, ids::STORE_SEARCH).unwrap();
    let count = support::bounds_of(cx, win, ids::STORE_COUNT).unwrap();
    let label = support::bounds_of(cx, win, ids::STORE_SORT_LABEL).unwrap();
    let sort = support::bounds_of(cx, win, ids::STORE_SORT).unwrap();
    assert_eq!(
        support::label_of(cx, win, ids::STORE_SORT_LABEL).as_deref(),
        Some("Sort:")
    );
    assert_eq!(
        search.size,
        gpui_kit::size(gpui_kit::px(288.), gpui_kit::px(36.))
    );
    assert_eq!(
        sort.size,
        gpui_kit::size(gpui_kit::px(144.), gpui_kit::px(36.))
    );
    let one = gpui_kit::px(1.);
    for b in [count, label, sort] {
        assert!(
            (support::mid_y(b) - support::mid_y(search)).abs() <= one,
            "{b:?} {search:?}"
        );
    }
    // Search alone on the left, the rest grouped on the right.
    assert!(
        count.left() - search.right() > gpui_kit::px(100.),
        "{search:?} {count:?}"
    );
    assert!(
        (label.left() - count.right() - gpui_kit::px(16.)).abs() <= one,
        "{count:?} {label:?}"
    );
    assert!(
        (sort.left() - label.right() - gpui_kit::px(8.)).abs() <= one,
        "{label:?} {sort:?}"
    );
    let card = support::bounds_of(cx, win, ids::store_card(GB, &gb(8))).unwrap();
    let first = support::bounds_of(cx, win, ids::store_card(GB, &gb(7))).unwrap();
    let left = if card.left() < first.left() {
        card.left()
    } else {
        first.left()
    };
    assert!(
        (search.left() - left).abs() <= one,
        "{search:?} cards from {left:?}"
    );
}

#[gpui_kit::test]
fn the_gamebanana_card_tags_are_v5_s_21_px_badges(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    let card = support::bounds_of(cx, win, ids::store_card(GB, &gb(7))).unwrap();
    let tag = support::bounds_of(cx, win, ids::store_card_tag(GB, &gb(7), 0)).unwrap();
    let button = support::bounds_of(cx, win, ids::store_download(GB, &gb(7))).unwrap();
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
        support::label_of(cx, win, ids::store_card_tag(GB, &gb(7), 0)).as_deref(),
        Some("Minimal")
    );
}

#[gpui_kit::test]
fn the_gamebanana_card_pads_24_px_above_its_image_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    let card = support::bounds_of(cx, win, ids::store_card(GB, &gb(7))).unwrap();
    let image = support::bounds_of(cx, win, ids::store_card_image(GB, &gb(7))).unwrap();
    assert_eq!(
        image.top() - card.top(),
        gpui_kit::px(25.),
        "{image:?} in {card:?}"
    );
}

// v5's card body: px-6 under a gap-6 and over a py-6 inside the 1 px border, so the button sits
// 25 px from the card's sides and bottom.
#[gpui_kit::test]
fn the_gamebanana_card_pads_24_px_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    let id = gb(7);
    let card = support::bounds_of(cx, win, ids::store_card(GB, &id)).unwrap();
    let image = support::bounds_of(cx, win, ids::store_card_image(GB, &id)).unwrap();
    let button = support::bounds_of(cx, win, ids::store_download(GB, &id)).unwrap();
    assert_eq!(
        button.left() - card.left(),
        gpui_kit::px(25.),
        "{button:?} in {card:?}"
    );
    assert_eq!(
        card.right() - button.right(),
        gpui_kit::px(25.),
        "{button:?} in {card:?}"
    );
    assert_eq!(
        card.bottom() - button.bottom(),
        gpui_kit::px(25.),
        "{button:?} in {card:?}"
    );
    let name = support::bounds_of(cx, win, ids::store_card_name(GB, &id)).expect("the name");
    assert_eq!(
        name.top() - image.bottom(),
        gpui_kit::px(24.),
        "{name:?} under {image:?}"
    );
    assert_eq!(name.left(), button.left());
    assert_eq!(
        support::label_of(cx, win, ids::store_card_name(GB, &id)).as_deref(),
        Some("FlawHUD")
    );
    let likes = support::bounds_of(cx, win, ids::store_likes(GB, &id)).unwrap();
    assert!(
        likes.bottom() + gpui_kit::px(8.) <= button.top(),
        "{likes:?} crowds {button:?}"
    );
    assert_eq!(card.size.height, gpui_kit::px(465.5), "{card:?}");
}

#[gpui_kit::test]
// v5's Card body: the 24 px name and 20 px author, 14 px under them (the header's 6 px row gap
// and pb-2), gap-6 to the tags, then mt-2 to the 16 px stats.
fn the_gamebanana_card_spaces_its_sections_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    let id = gb(7);
    let name = support::bounds_of(cx, win, ids::store_card_name(GB, &id)).unwrap();
    let tag = support::bounds_of(cx, win, ids::store_card_tag(GB, &id, 0)).unwrap();
    let likes = support::bounds_of(cx, win, ids::store_likes(GB, &id)).unwrap();
    assert_eq!(name.size.height, gpui_kit::px(24.), "{name:?}");
    assert_eq!(
        tag.top() - name.top(),
        gpui_kit::px(82.),
        "{tag:?} under {name:?}"
    );
    assert_eq!(
        likes.top() - tag.bottom(),
        gpui_kit::px(8.),
        "{likes:?} under {tag:?}"
    );
    assert_eq!(likes.size.height, gpui_kit::px(16.), "{likes:?}");
}

#[gpui_kit::test]
// v5's tags flex-wrap: three tags too long to share a row stack 4 px apart, each whole inside the
// body, and the stats follow the last row (as on v5's Helltaker card).
fn three_long_tags_wrap_onto_three_rows_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open_tagged(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    let id = gb(8);
    let card = support::bounds_of(cx, win, ids::store_card(GB, &id)).unwrap();
    let button = support::bounds_of(cx, win, ids::store_download(GB, &id)).unwrap();
    let rows = support::bounds_of(cx, win, ids::store_card_tags(GB, &id)).expect("the tag rows");
    let tags: Vec<_> = (0..3)
        .map(|ix| support::bounds_of(cx, win, ids::store_card_tag(GB, &id, ix)).expect("a tag"))
        .collect();
    for (ix, tag) in tags.iter().enumerate() {
        assert_eq!(tag.size.height, gpui_kit::px(21.), "tag {ix}");
        assert!(
            tag.left() >= button.left() && tag.right() <= button.right(),
            "tag {ix} {tag:?} past {button:?}"
        );
    }
    for pair in tags.windows(2) {
        assert_eq!(
            pair[1].top() - pair[0].bottom(),
            gpui_kit::px(4.),
            "{pair:?}"
        );
    }
    assert_eq!(rows.bottom(), tags[2].bottom(), "{rows:?}");
    let likes = support::bounds_of(cx, win, ids::store_likes(GB, &id)).unwrap();
    assert_eq!(
        likes.top() - tags[2].bottom(),
        gpui_kit::px(8.),
        "{likes:?}"
    );
    assert!(
        likes.bottom() + gpui_kit::px(32.) <= button.top(),
        "{likes:?} crowds {button:?}"
    );
    assert_eq!(card.size.height, gpui_kit::px(465.5), "{card:?}");
}

#[gpui_kit::test]
// A tag wider than the body stops at the body's edge with an ellipsis; v5 ran it under the card.
fn an_overlong_tag_stays_inside_the_card_body(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open_tagged(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    let id = gb(7);
    let tag = support::bounds_of(cx, win, ids::store_card_tag(GB, &id, 0)).unwrap();
    let button = support::bounds_of(cx, win, ids::store_download(GB, &id)).unwrap();
    assert_eq!(tag.size.height, gpui_kit::px(21.), "{tag:?}");
    assert!(
        tag.left() >= button.left() && tag.right() <= button.right(),
        "{tag:?} past {button:?}"
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_card_tag(GB, &id, 0)).as_deref(),
        Some(OVERLONG)
    );
}

#[gpui_kit::test]
// v5 leaves out the tag row of a card without tags, so its stats sit mt-2 under the header block.
fn a_card_without_tags_puts_its_stats_under_the_header(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open_tagged(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    let id = gb(9);
    let name = support::bounds_of(cx, win, ids::store_card_name(GB, &id)).unwrap();
    let likes = support::bounds_of(cx, win, ids::store_likes(GB, &id)).unwrap();
    assert_eq!(
        likes.top() - name.top(),
        gpui_kit::px(90.),
        "{likes:?} under {name:?}"
    );
    assert!(!support::present(cx, win, ids::store_card_tags(GB, &id)));
}

#[gpui_kit::test]
// v5's gap-6 column puts the grid 24 px under the search row.
fn the_grid_starts_24_px_under_the_search_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    let search = support::bounds_of(cx, win, ids::STORE_SEARCH).unwrap();
    let card = support::bounds_of(cx, win, ids::store_card(GB, &gb(7))).unwrap();
    assert_eq!(
        card.top() - search.bottom(),
        gpui_kit::px(24.),
        "{card:?} under {search:?}"
    );
}

#[gpui_kit::test]
fn the_banner_shows_while_gamebanana_huds_sync(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, runs) = support::fake_hub(cx);
    let (win, page) = open(cx, &env, &server);
    let _tab = gamebanana(cx, &page);
    assert!(!support::present(cx, win, ids::SYNC_BANNER));
    cx.update(|cx| hub.update(cx, |h, cx| h.start(GB, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(run.kind, GB);
    run.report(12, 90);
    support::next_repaint(cx);
    assert_eq!(
        support::label_of(cx, win, ids::SYNC_BANNER).as_deref(),
        Some("Syncing HUDs from GameBanana... Not all items loaded yet.")
    );
    assert_eq!(
        support::label_of(cx, win, ids::SYNC_BANNER_COUNT).as_deref(),
        Some("12 / 90")
    );
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the banner to go", |cx| {
        !support::present(cx, win, ids::SYNC_BANNER)
    });
}

#[gpui_kit::test]
fn a_search_with_no_match_says_v5_s_hint(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = open(cx, &env, &server);
    let tab = gamebanana(cx, &page);
    let grid = cx.update(|cx| tab.read(cx).grid().clone());
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("zzzq".into(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the search", |cx| {
        cx.update(|cx| grid.read(cx).shown() == Some(0) && !grid.read(cx).is_loading())
    });
    assert_eq!(
        support::label_of(cx, win, ids::STORE_EMPTY).as_deref(),
        Some("No HUDs match your search")
    );
    assert_eq!(
        support::label_of(cx, win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Try adjusting your search criteria.")
    );
}
