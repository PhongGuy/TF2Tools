use crate::support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_store::curated::{CuratedHud, Facet, Os};
use tf2tools_app::pages::hud_store::curated_tab::CuratedTab;
use tf2tools_app::pages::hud_store::{HudStorePage, HudStoreTab};
use tf2tools_app::remote_images::RemoteImages;
use tf2tools_app::remote_images::decode::decode_image;
use tf2tools_app::remote_images::queue::IMAGE_BUDGET;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_download::StoreDownloads;
use tf2tools_core::{Provenance, StoreItemId, StoreKind};

const CURATED: StoreKind = StoreKind::CuratedHuds;

fn cid(id: &str) -> StoreItemId {
    StoreItemId::Curated(id.into())
}

/// rayshud (all OSes), budhud (Windows only, repo served), prehud (prerelease); rayshud is in the library.
fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> (AnyWindowHandle, Entity<HudStorePage>, Entity<CuratedTab>) {
    let bud_repo = server.mount_curated("budhud", support::hud_zip_bytes("budhud"), Duration::ZERO);
    let (win, page) = support::store_window(
        cx,
        env,
        server,
        move |env| {
            support::seed_curated(
                env,
                "budhud",
                "budhud",
                "Whisker",
                Some(&bud_repo),
                r#"{"flags":["menus"],"traits":{"os":["windows"]},"resources":["banner"]}"#,
            );
            support::seed_curated(
                env,
                "prehud",
                "prehud",
                "Pre",
                None,
                r#"{"prerelease":true,"traits":{"os":["windows","linux"]}}"#,
            );
            support::seed_curated(
                env,
                "rayshud",
                "rayshud",
                "raysfire",
                None,
                r#"{"flags":["menus","minmode","crosshairs","materials","scoreboards"],"traits":{"os":["windows","mac","linux"]},"resources":["banner"]}"#,
            );
            let archive = env.tmp.path().join("rayshud.zip");
            std::fs::write(&archive, support::hud_zip_bytes("rayshud")).unwrap();
            // The page loads as it opens, so the library HUD goes in first, through a second Services.
            let library = support::services(env, support::no_steam()).library;
            library
                .import_hud(
                    &archive,
                    Provenance::Curated {
                        hud_id: "rayshud".into(),
                    },
                )
                .unwrap();
        },
        HudStorePage::new,
    );
    let tab = cx
        .update(|cx| page.read(cx).curated())
        .expect("the curated tab");
    support::wait_real(cx, "the curated list", |cx| {
        cx.update(|cx| tab.read(cx).is_loaded())
    });
    (win, page, tab)
}

fn shown(cx: &mut TestAppContext, tab: &Entity<CuratedTab>) -> Vec<String> {
    cx.update(|cx| tab.read(cx).shown_ids())
}

// `text` shaped at text-sm in the theme font and `weight`, as the window lays it out.
fn text_width(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    text: &str,
    weight: gpui_kit::FontWeight,
) -> gpui_kit::Pixels {
    use gpui_kit::component::ActiveTheme as _;
    cx.update_window(win, |_, w, cx| {
        let font = gpui_kit::Font {
            weight,
            ..gpui_kit::font(cx.theme().font_family.clone())
        };
        let run = gpui_kit::TextRun {
            len: text.len(),
            font,
            ..Default::default()
        };
        let size = gpui_kit::rems(0.875).to_pixels(w.rem_size());
        w.text_system()
            .shape_line(text.to_owned().into(), size, &[run], None)
            .width
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_hud_store_is_a_real_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::click_id(cx, s.win, ids::nav(Page::HudStore));
    assert!(support::present(cx, s.win, ids::page(Page::HudStore)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
    assert!(cx.update(|cx| s.shell.read(cx).hud_store_page()).is_some());
}

#[gpui_kit::test]
fn curated_opens_with_v5_defaults_and_counts(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page, tab) = open(cx, &env, &server);
    assert_eq!(cx.update(|cx| page.read(cx).tab()), HudStoreTab::Curated);
    cx.update(|cx| {
        let f = tab.read(cx).filter();
        assert!(f.hide_prerelease);
        assert_eq!(f.os.iter().copied().collect::<Vec<_>>(), [Os::current()]);
    });
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    assert_eq!(shown(cx, &tab), ["budhud", "rayshud"]);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_COUNT).as_deref(),
        Some("2 of 3 HUDs")
    );
    assert!(support::present(
        cx,
        win,
        ids::store_card(CURATED, &cid("rayshud"))
    ));
    support::click_id(cx, win, ids::STORE_FILTER_PRERELEASE);
    assert_eq!(shown(cx, &tab), ["budhud", "prehud", "rayshud"]);
    support::click_id(cx, win, ids::store_filter_os(Os::Mac.key()));
    assert_eq!(shown(cx, &tab), ["rayshud"]);
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    support::click_id(cx, win, ids::STORE_FILTER_MORE);
    support::click_id(cx, win, ids::store_filter(Facet::Flag.key(), "minmode"));
    assert_eq!(shown(cx, &tab), ["rayshud"]);
    support::click_id(cx, win, ids::store_filter(Facet::Flag.key(), "minmode"));
    cx.update(|cx| tab.update(cx, |t, cx| t.set_search("WHISK".into(), cx)));
    assert_eq!(shown(cx, &tab), ["budhud"]);
    cx.update(|cx| tab.update(cx, |t, cx| t.set_search("nobody".into(), cx)));
    assert!(shown(cx, &tab).is_empty());
    assert!(support::present(cx, win, ids::STORE_EMPTY));
}

#[gpui_kit::test]
fn cards_know_what_is_downloaded_and_flip_after_a_download(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, tab) = open(cx, &env, &server);
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    assert_eq!(
        support::label_of(cx, win, ids::store_download(CURATED, &cid("rayshud"))).as_deref(),
        Some("Re-download")
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_download(CURATED, &cid("budhud"))).as_deref(),
        Some("Download")
    );
    support::click_id(cx, win, ids::store_download(CURATED, &cid("budhud")));
    assert!(support::present(cx, win, ids::STORE_DETAIL));
    support::wait_real(cx, "the archive row", |cx| {
        support::present(cx, win, ids::store_file(0))
    });
    support::click_id(cx, win, ids::store_file(0));
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    support::wait_real(cx, "the download", |cx| {
        cx.update(|cx| !dl.read(cx).is_busy())
    });
    support::wait_real(cx, "the flip", |cx| {
        cx.update(|cx| tab.read(cx).is_downloaded("budhud"))
    });
    cx.update_window(win, |_, window, cx| window.close_dialog(cx))
        .unwrap();
    assert_eq!(
        support::label_of(cx, win, ids::store_download(CURATED, &cid("budhud"))).as_deref(),
        Some("Re-download")
    );
}

#[gpui_kit::test]
fn tabs_switch_and_keep_their_state(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page, tab) = open(cx, &env, &server);
    cx.update(|cx| tab.update(cx, |t, cx| t.set_search("ray".into(), cx)));
    support::click_id(cx, win, ids::HUD_STORE_TAB_GAMEBANANA);
    assert_eq!(cx.update(|cx| page.read(cx).tab()), HudStoreTab::GameBanana);
    support::click_id(cx, win, ids::HUD_STORE_TAB_CURATED);
    assert_eq!(
        cx.update(|cx| page.read(cx).curated()),
        Some(tab.clone()),
        "the same tab"
    );
    assert_eq!(cx.update(|cx| tab.read(cx).filter().search.clone()), "ray");
}

#[gpui_kit::test]
fn an_empty_store_says_it_could_not_load(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(cx, &env, &server, |_| {}, HudStorePage::new);
    let tab = cx.update(|cx| page.read(cx).curated()).unwrap();
    support::wait_real(cx, "the curated list", |cx| {
        cx.update(|cx| tab.read(cx).is_loaded())
    });
    assert_eq!(
        support::label_of(cx, win, ids::STORE_UNLOADED).as_deref(),
        Some("Could not load store")
    );
    assert!(!support::present(cx, win, ids::STORE_EMPTY));
    assert!(!support::present(cx, win, ids::STORE_SYNCING));
}

#[gpui_kit::test]
fn a_finished_curated_sync_fills_an_empty_store(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, runs) = support::fake_hub(cx);
    let (win, page) = support::store_window(cx, &env, &server, |_| {}, HudStorePage::new);
    let tab = cx.update(|cx| page.read(cx).curated()).unwrap();
    support::wait_real(cx, "the curated list", |cx| {
        cx.update(|cx| tab.read(cx).is_loaded())
    });

    cx.update(|cx| hub.update(cx, |h, cx| h.start(CURATED, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    support::settle(cx, win);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_SYNCING).as_deref(),
        Some("Syncing HUDs...")
    );
    assert!(!support::present(cx, win, ids::STORE_UNLOADED));
    for ix in 0..8 {
        assert!(support::present(cx, win, ids::skeleton_card(ix)), "{ix}");
    }

    support::seed_curated(
        &env,
        "rayshud",
        "rayshud",
        "raysfire",
        None,
        r#"{"traits":{"os":["windows","mac","linux"]}}"#,
    );
    run.finish(Ok(tf2tools_app::sync::SyncSummary::default()));
    support::wait_hub(cx, "the re-read", |cx| {
        cx.update(|cx| tab.read(cx).shown_ids() == ["rayshud"])
    });
    support::settle(cx, win);
    assert!(support::present(
        cx,
        win,
        ids::store_card(CURATED, &cid("rayshud"))
    ));
    assert!(!support::present(cx, win, ids::STORE_SYNCING));
}

// v5's curated skeletons sit in `grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4`.
#[gpui_kit::test]
fn the_curated_skeletons_follow_v5_s_responsive_columns(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, _runs) = support::fake_hub(cx);
    let (win, page) = support::store_window(cx, &env, &server, |_| {}, HudStorePage::new);
    let tab = cx.update(|cx| page.read(cx).curated()).unwrap();
    support::wait_real(cx, "the curated list", |cx| {
        cx.update(|cx| tab.read(cx).is_loaded())
    });
    cx.update(|cx| hub.update(cx, |h, cx| h.start(CURATED, false, cx)));
    support::next_repaint(cx);
    assert!(support::present(cx, win, ids::STORE_SYNCING));
    for (width, columns) in [(1200., 3), (800., 2), (500., 1)] {
        cx.simulate_window_resize(win, size(px(width), px(900.)));
        let card = |cx: &mut TestAppContext, ix: usize| {
            support::bounds_of(cx, win, ids::skeleton_card(ix)).expect("a skeleton")
        };
        let first = card(cx, 0);
        let row_end = card(cx, columns - 1);
        let next = card(cx, columns);
        assert_eq!(row_end.top(), first.top(), "{width} px: {columns} to a row");
        assert!(
            next.top() > first.top(),
            "{width} px: skeleton {columns} starts row two"
        );
        assert_eq!(next.left(), first.left(), "{width} px");
    }
}

#[gpui_kit::test]
fn the_card_download_button_spans_the_card_as_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    support::assert_full_width_download(
        cx,
        win,
        ids::store_card(CURATED, &cid("rayshud")),
        ids::store_download(CURATED, &cid("rayshud")),
    );
}

#[gpui_kit::test]
fn the_filter_buttons_are_v5_s_32_px(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    for id in [ids::STORE_FILTER_CLEAR, ids::STORE_FILTER_MORE] {
        let b = support::bounds_of(cx, win, id).expect(id);
        assert_eq!(b.size.height, gpui_kit::px(32.), "{id}");
    }
}

#[gpui_kit::test]
fn a_filter_that_hides_everything_says_v5_s_hint(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, tab) = open(cx, &env, &server);
    cx.update(|cx| tab.update(cx, |t, cx| t.set_search("nobody".into(), cx)));
    assert_eq!(
        support::label_of(cx, win, ids::STORE_EMPTY).as_deref(),
        Some("No HUDs match your filters")
    );
    assert_eq!(
        support::label_of(cx, win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Try adjusting your search or filter criteria.")
    );
    assert!(support::present(cx, win, ids::STORE_EMPTY_ICON));
}

// v5 holds `syncing` until the post-sync load returns, so the WifiOff card cannot flash between.
#[gpui_kit::test]
fn the_curated_store_stays_syncing_until_the_post_sync_reload_lands(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, runs) = support::fake_hub(cx);
    let (_win, page) = support::store_window(cx, &env, &server, |_| {}, HudStorePage::new);
    let tab = cx.update(|cx| page.read(cx).curated()).unwrap();
    support::wait_real(cx, "the curated list", |cx| {
        cx.update(|cx| tab.read(cx).is_loaded())
    });
    cx.update(|cx| hub.update(cx, |h, cx| h.start(CURATED, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(cx.update(|cx| tab.read(cx).is_syncing(cx)));
    let idle_and_empty = Rc::new(Cell::new(false));
    let seen = Rc::clone(&idle_and_empty);
    let _watch = cx.update(|cx| {
        cx.observe(&tab, move |tab, cx| {
            if !tab.read(cx).is_syncing(cx) && tab.read(cx).shown_ids().is_empty() {
                seen.set(true);
            }
        })
    });
    support::seed_curated(
        &env,
        "rayshud",
        "rayshud",
        "raysfire",
        None,
        r#"{"traits":{"os":["windows","mac","linux"]}}"#,
    );
    run.finish(Ok(tf2tools_app::sync::SyncSummary::default()));
    support::wait_hub(cx, "the re-read", |cx| {
        cx.update(|cx| tab.read(cx).shown_ids() == ["rayshud"])
    });
    assert!(
        !idle_and_empty.get(),
        "idle and empty before the reload landed"
    );
    assert!(!cx.update(|cx| tab.read(cx).is_syncing(cx)));
}

// v5 shows "Could not load store" when the load fails, not an endless skeleton.
#[gpui_kit::test]
fn a_failed_first_load_shows_could_not_load(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(
        cx,
        &env,
        &server,
        |env| {
            // Without its table the library read fails as a damaged library would.
            rusqlite::Connection::open(env.data.db_path())
                .unwrap()
                .execute("DROP TABLE library_huds", [])
                .unwrap();
        },
        HudStorePage::new,
    );
    let tab = cx.update(|cx| page.read(cx).curated()).unwrap();
    support::wait_real(cx, "the failed load", |cx| {
        cx.update(|cx| tab.read(cx).is_loaded())
    });
    support::settle(cx, win);
    assert!(!support::present(cx, win, ids::STORE_LOADING));
    assert_eq!(
        support::label_of(cx, win, ids::STORE_UNLOADED).as_deref(),
        Some("Could not load store")
    );
    assert!(support::present(cx, win, ids::STORE_ERROR));
    assert_eq!(
        support::label_of(cx, win, ids::STORE_ERROR).as_deref(),
        Some("TF2Tools couldn't read the store. Restart TF2Tools and try again.")
    );
}

// v5's StoreFilters: a w-72 search box left and "X of Y HUDs" right, on one row.
#[gpui_kit::test]
fn the_curated_search_is_left_and_the_count_right_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    let search = support::bounds_of(cx, win, ids::STORE_SEARCH).unwrap();
    let count = support::bounds_of(cx, win, ids::STORE_COUNT).unwrap();
    assert_eq!(search.size.width, gpui_kit::px(288.));
    assert_eq!(search.size.height, gpui_kit::px(36.));
    assert!(search.right() < count.left(), "{search:?} {count:?}");
    assert!(
        (support::mid_y(search) - support::mid_y(count)).abs() <= gpui_kit::px(1.),
        "{search:?} {count:?}"
    );
    // v5's gap-6 column puts the filter row 24 px below the search row.
    let os = support::bounds_of(cx, win, ids::STORE_FILTER_OS_GROUP).unwrap();
    assert!(
        (os.top() - search.bottom() - gpui_kit::px(24.)).abs() <= gpui_kit::px(1.),
        "{search:?} {os:?}"
    );
}

// "OS:", then Windows, Mac and Linux joined as one outline group of v5's 32 px toggles; the
// prerelease checkbox 16 px on, and Clear and More filters at the row's right edge.
#[gpui_kit::test]
fn the_os_filter_is_a_labelled_joined_toggle_group_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_FILTER_OS_LABEL).as_deref(),
        Some("OS:")
    );
    assert!(support::present(cx, win, ids::STORE_FILTER_OS_GROUP));
    let label = support::bounds_of(cx, win, ids::STORE_FILTER_OS_LABEL).unwrap();
    let [windows, mac, linux] = Os::ALL
        .map(|os| support::bounds_of(cx, win, ids::store_filter_os(os.key())).expect("a toggle"));
    let one = gpui_kit::px(1.);
    assert!(
        (windows.left() - label.right() - gpui_kit::px(8.)).abs() <= one,
        "{label:?} {windows:?}"
    );
    for b in [windows, mac, linux] {
        assert_eq!(b.size.height, gpui_kit::px(32.), "{b:?}");
        assert!(
            (support::mid_y(b) - support::mid_y(label)).abs() <= one,
            "{b:?}"
        );
    }
    assert!(
        (mac.left() - windows.right()).abs() <= one,
        "joined: {windows:?} {mac:?}"
    );
    assert!(
        (linux.left() - mac.right()).abs() <= one,
        "joined: {mac:?} {linux:?}"
    );
    // A middle item is px-3 each side of its text-sm medium label plus its 1 px right border
    // (the segmented group drops its left border); the kit's own px_2 would be 8 px narrower.
    let text = text_width(cx, win, "Mac", gpui_kit::FontWeight::MEDIUM);
    assert!(
        (mac.size.width - (text + gpui_kit::px(25.))).abs() <= one,
        "{mac:?} for text {text:?}"
    );
    let prerelease = support::bounds_of(cx, win, ids::STORE_FILTER_PRERELEASE).unwrap();
    assert!(
        (prerelease.left() - linux.right() - gpui_kit::px(16.)).abs() <= one,
        "{linux:?} {prerelease:?}"
    );
    let clear = support::bounds_of(cx, win, ids::STORE_FILTER_CLEAR).unwrap();
    let more = support::bounds_of(cx, win, ids::STORE_FILTER_MORE).unwrap();
    let count = support::bounds_of(cx, win, ids::STORE_COUNT).unwrap();
    assert!(
        (more.left() - clear.right() - gpui_kit::px(8.)).abs() <= one,
        "{clear:?} {more:?}"
    );
    assert!(
        (more.right() - count.right()).abs() <= one,
        "{more:?} {count:?}"
    );
    // The facet chips open 12 px (the column's gap_3) below the OS row.
    support::click_id(cx, win, ids::STORE_FILTER_MORE);
    let os = support::bounds_of(cx, win, ids::STORE_FILTER_OS_GROUP).unwrap();
    let chip = support::bounds_of(cx, win, ids::store_filter(Facet::Flag.key(), "menus")).unwrap();
    assert_eq!(
        chip.top() - os.bottom(),
        gpui_kit::px(12.),
        "{os:?} {chip:?}"
    );
}

#[gpui_kit::test]
fn an_os_toggle_turns_off_on_a_second_click(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, tab) = open(cx, &env, &server);
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    support::click_id(cx, win, ids::store_filter_os(Os::Mac.key()));
    assert_eq!(shown(cx, &tab), ["rayshud"]);
    cx.update(|cx| assert!(tab.read(cx).filter().os.contains(&Os::Mac)));
    support::click_id(cx, win, ids::store_filter_os(Os::Mac.key()));
    assert_eq!(shown(cx, &tab), ["budhud", "rayshud"]);
    cx.update(|cx| assert!(tab.read(cx).filter().os.is_empty()));
}

// The grid's re-armed hook asks for the drawn cards' thumbnails; nothing else may ask too.
// An emptied list draws no grid, so it asks once directly.
#[gpui_kit::test]
fn a_filter_change_sends_one_image_want(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let images = cx.update(|cx| {
        RemoteImages::install_with(cx, support::thumbnail_fetch(), IMAGE_BUDGET, decode_image)
    });
    let (win, _page, tab) = open(cx, &env, &server);
    support::settle(cx, win);
    let wants = |cx: &mut TestAppContext| cx.update(|cx| images.read(cx).stats().wants);
    // "ray" keeps the same one card, so the grid's range and count do not change.
    for search in ["ray", "nobody", ""] {
        let before = wants(cx);
        cx.update(|cx| tab.update(cx, |t, cx| t.set_search(search.into(), cx)));
        support::settle(cx, win);
        assert_eq!(wants(cx) - before, 1, "search {search:?}");
    }
}

#[gpui_kit::test]
fn a_click_on_a_curated_card_opens_its_detail(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    support::click_id(cx, win, ids::store_card(CURATED, &cid("budhud")));
    assert!(support::present(cx, win, ids::STORE_DETAIL));
}

#[gpui_kit::test]
fn the_curated_download_button_opens_one_detail(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    support::click_id(cx, win, ids::store_download(CURATED, &cid("budhud")));
    // No lookup by STORE_DETAIL here: two stacked details make that id ambiguous and panic.
    let opened = cx
        .update_window(win, |_, w, cx| w.has_active_dialog(cx))
        .unwrap();
    assert!(opened, "the button opened no detail");
    cx.update_window(win, |_, window, cx| window.close_dialog(cx))
        .unwrap();
    support::settle(cx, win);
    let still_open = cx
        .update_window(win, |_, w, cx| w.has_active_dialog(cx))
        .unwrap();
    assert!(
        !still_open,
        "the card under the button opened a second detail"
    );
}

// v5's outline Badge: 21 px, at least the column's 8 px gap above the button, and the button
// 25 px (py-6 and the border) from the bottom.
#[gpui_kit::test]
fn the_curated_card_tags_are_v5_s_21_px_badges(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    let rays = cid("rayshud");
    let card = support::bounds_of(cx, win, ids::store_card(CURATED, &rays)).unwrap();
    let button = support::bounds_of(cx, win, ids::store_download(CURATED, &rays)).unwrap();
    for ix in 0..4 {
        let tag =
            support::bounds_of(cx, win, ids::store_card_tag(CURATED, &rays, ix)).expect("a tag");
        assert_eq!(tag.size.height, gpui_kit::px(21.), "tag {ix}");
        assert!(
            tag.bottom() + gpui_kit::px(8.) <= button.top(),
            "tag {ix} {tag:?} crowds {button:?}"
        );
    }
    assert_eq!(
        card.bottom() - button.bottom(),
        gpui_kit::px(25.),
        "{button:?} in {card:?}"
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_card_tag(CURATED, &rays, 0)).as_deref(),
        Some("menus")
    );
}

// v5's Card is py-6 inside a 1 px border with a px-6 body under a gap-6: the screenshot starts
// 25 px below the card's top and spans it edge to edge; the name starts 24 px below it and 25 px
// in from the card's left.
#[gpui_kit::test]
fn the_curated_card_pads_24_px_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    let rays = cid("rayshud");
    let card = support::bounds_of(cx, win, ids::store_card(CURATED, &rays)).unwrap();
    let image = support::bounds_of(cx, win, ids::store_card_image(CURATED, &rays)).unwrap();
    let name = support::bounds_of(cx, win, ids::store_card_name(CURATED, &rays)).unwrap();
    let button = support::bounds_of(cx, win, ids::store_download(CURATED, &rays)).unwrap();
    assert_eq!(
        image.top() - card.top(),
        gpui_kit::px(25.),
        "{image:?} in {card:?}"
    );
    assert_eq!(
        image.left() - card.left(),
        gpui_kit::px(1.),
        "{image:?} in {card:?}"
    );
    assert_eq!(image.size.width, card.size.width - gpui_kit::px(2.));
    assert_eq!(
        name.top() - image.bottom(),
        gpui_kit::px(24.),
        "{name:?} under {image:?}"
    );
    assert_eq!(
        name.left() - card.left(),
        gpui_kit::px(25.),
        "{name:?} in {card:?}"
    );
    assert_eq!(
        button.left() - card.left(),
        gpui_kit::px(25.),
        "{button:?} in {card:?}"
    );
    assert_eq!(card.size.height, gpui_kit::px(415.5), "{card:?}");
}

#[gpui_kit::test]
// v5's HudStoreCard body: the 44 px name block and 14 px under it, gap-6 to the flags, which wrap
// onto at most two rows 4 px apart, then pb-2 and gap-6 to the button.
fn the_curated_card_spaces_its_sections_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    let rays = cid("rayshud");
    let name = support::bounds_of(cx, win, ids::store_card_name(CURATED, &rays)).unwrap();
    let first = support::bounds_of(cx, win, ids::store_card_tag(CURATED, &rays, 0)).unwrap();
    let last = support::bounds_of(cx, win, ids::store_card_tag(CURATED, &rays, 3)).unwrap();
    let rows = support::bounds_of(cx, win, ids::store_card_tags(CURATED, &rays)).unwrap();
    let button = support::bounds_of(cx, win, ids::store_download(CURATED, &rays)).unwrap();
    assert_eq!(name.size.height, gpui_kit::px(24.), "{name:?}");
    assert_eq!(
        first.top() - name.top(),
        gpui_kit::px(82.),
        "{first:?} under {name:?}"
    );
    assert_eq!(
        last.top() - first.bottom(),
        gpui_kit::px(4.),
        "{last:?} not on the row under {first:?}"
    );
    assert!(rows.size.height <= gpui_kit::px(46.), "{rows:?}");
    assert!(
        last.bottom() + gpui_kit::px(32.) <= button.top(),
        "{last:?} crowds {button:?}"
    );
}

// v5's HudStoreCard: a 36 px ghost ExternalLink button at the top right of the name block,
// only for a HUD with a repository; a click opens the repository and not the detail.
#[gpui_kit::test]
fn a_curated_card_links_its_repository_beside_the_name_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    let bud = cid("budhud");
    assert!(
        !support::present(cx, win, ids::store_card_repo(CURATED, &cid("rayshud"))),
        "no repo, no icon"
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_card_name(CURATED, &bud)).as_deref(),
        Some("budhud")
    );
    let card = support::bounds_of(cx, win, ids::store_card(CURATED, &bud)).unwrap();
    let name = support::bounds_of(cx, win, ids::store_card_name(CURATED, &bud)).unwrap();
    let repo = support::bounds_of(cx, win, ids::store_card_repo(CURATED, &bud)).unwrap();
    assert_eq!(repo.size.width, gpui_kit::px(36.));
    assert_eq!(repo.size.height, gpui_kit::px(36.));
    assert_eq!(repo.top(), name.top(), "items-start: {name:?} {repo:?}");
    assert!(name.right() <= repo.left(), "{name:?} {repo:?}");
    // The card body's 24 px side padding inside the 1 px border.
    assert_eq!(
        card.right() - repo.right(),
        gpui_kit::px(25.),
        "{repo:?} in {card:?}"
    );
    support::click_id(cx, win, ids::store_card_repo(CURATED, &bud));
    let opened = cx
        .update_window(win, |_, w, cx| w.has_active_dialog(cx))
        .unwrap();
    assert!(!opened, "the card under the icon opened its detail");
    assert_eq!(
        cx.opened_url(),
        Some(format!("{}/repo/budhud", server.uri()))
    );
}

// v5 shows Clear only under hasActiveFilters; More stays at the row's right edge without it.
#[gpui_kit::test]
fn clear_filters_shows_only_while_a_filter_is_on(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, tab) = open(cx, &env, &server);
    assert!(
        support::present(cx, win, ids::STORE_FILTER_CLEAR),
        "the OS preset"
    );
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    assert!(
        !support::present(cx, win, ids::STORE_FILTER_CLEAR),
        "cleared"
    );
    let more = support::bounds_of(cx, win, ids::STORE_FILTER_MORE).unwrap();
    let count = support::bounds_of(cx, win, ids::STORE_COUNT).unwrap();
    assert!(
        (more.right() - count.right()).abs() <= gpui_kit::px(1.),
        "{more:?} {count:?}"
    );
    support::click_id(cx, win, ids::STORE_FILTER_PRERELEASE);
    assert!(
        support::present(cx, win, ids::STORE_FILTER_CLEAR),
        "prereleases shown"
    );
    support::click_id(cx, win, ids::STORE_FILTER_PRERELEASE);
    assert!(!support::present(cx, win, ids::STORE_FILTER_CLEAR));
    cx.update(|cx| tab.update(cx, |t, cx| t.set_search("bud".into(), cx)));
    assert!(
        support::present(cx, win, ids::STORE_FILTER_CLEAR),
        "a search"
    );
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    assert!(!support::present(cx, win, ids::STORE_FILTER_CLEAR));
    support::click_id(cx, win, ids::STORE_FILTER_MORE);
    support::click_id(cx, win, ids::store_filter(Facet::Flag.key(), "menus"));
    assert!(
        support::present(cx, win, ids::STORE_FILTER_CLEAR),
        "a facet"
    );
}

#[gpui_kit::test]
fn more_filters_reads_less_filters_while_the_rows_are_open(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    let chip = ids::store_filter(Facet::Flag.key(), "menus");
    assert_eq!(
        support::label_of(cx, win, ids::STORE_FILTER_MORE).as_deref(),
        Some("More filters")
    );
    support::click_id(cx, win, ids::STORE_FILTER_MORE);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_FILTER_MORE).as_deref(),
        Some("Less filters")
    );
    assert!(support::present(cx, win, chip.clone()));
    support::click_id(cx, win, ids::STORE_FILTER_MORE);
    assert_eq!(
        support::label_of(cx, win, ids::STORE_FILTER_MORE).as_deref(),
        Some("More filters")
    );
    assert!(!support::present(cx, win, chip));
}

#[gpui_kit::test]
// v5's curated tab starts its grid 24 px under the OS filter row.
fn the_curated_grid_starts_24_px_under_the_filters_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    support::click_id(cx, win, ids::STORE_FILTER_CLEAR);
    let os = support::bounds_of(cx, win, ids::STORE_FILTER_OS_GROUP).unwrap();
    let card = support::bounds_of(cx, win, ids::store_card(CURATED, &cid("rayshud"))).unwrap();
    assert_eq!(
        card.top() - os.bottom(),
        gpui_kit::px(24.),
        "{card:?} under {os:?}"
    );
}

#[gpui_kit::test]
// Four long flags take a row each; the two past the 2-row cap start under the row box, hidden
// whole. A hidden flag keeps its layout bounds, so the check is against the box, not presence.
fn the_curated_card_caps_its_flags_at_two_rows(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(
        cx,
        &env,
        &server,
        |env| {
            support::seed_curated(
                env,
                "longhud",
                "longhud",
                "Long",
                None,
                r#"{"flags":["Software Used: VTFEdit","Software Used: GIMP","Software Used: Paint.NET","Software Used: Photoshop"],"traits":{"os":["windows","mac","linux"]}}"#,
            );
        },
        HudStorePage::new,
    );
    let tab = cx
        .update(|cx| page.read(cx).curated())
        .expect("the curated tab");
    support::wait_real(cx, "the curated list", |cx| {
        cx.update(|cx| tab.read(cx).is_loaded())
    });
    let id = cid("longhud");
    let card = support::bounds_of(cx, win, ids::store_card(CURATED, &id)).unwrap();
    let rows = support::bounds_of(cx, win, ids::store_card_tags(CURATED, &id)).unwrap();
    let button = support::bounds_of(cx, win, ids::store_download(CURATED, &id)).unwrap();
    assert_eq!(rows.size.height, gpui_kit::px(46.), "{rows:?}");
    for ix in 0..2 {
        let flag = support::bounds_of(cx, win, ids::store_card_tag(CURATED, &id, ix)).unwrap();
        assert!(flag.bottom() <= rows.bottom(), "{flag:?} cut in {rows:?}");
    }
    for ix in 2..4 {
        if let Some(flag) = support::bounds_of(cx, win, ids::store_card_tag(CURATED, &id, ix)) {
            assert!(flag.top() >= rows.bottom(), "{flag:?} shows in {rows:?}");
        }
    }
    assert_eq!(
        button.left() - card.left(),
        gpui_kit::px(25.),
        "{button:?} in {card:?}"
    );
    assert_eq!(card.size.height, gpui_kit::px(415.5), "{card:?}");
}

#[gpui_kit::test]
// v5's TabsList is h-9: the Curated and GameBanana tabs are 36 px tall.
fn the_tab_strip_is_v5_s_36_px(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _page, _tab) = open(cx, &env, &server);
    for id in [ids::HUD_STORE_TAB_CURATED, ids::HUD_STORE_TAB_GAMEBANANA] {
        let tab = support::bounds_of(cx, win, id).unwrap();
        assert_eq!(tab.size.height, gpui_kit::px(36.), "{id}: {tab:?}");
    }
}

// The curated reader takes curated rows only; a GameBanana HUD row from the store is not one.
#[test]
fn only_a_curated_row_reads_as_a_curated_hud() {
    let env = support::env();
    let s = support::services(&env, support::no_steam());
    support::seed_curated(&env, "rayshud", "rayshud", "raysfire", None, "{}");
    support::seed_gb_huds(&env, &[(7, "FlawHUD")]);
    let gamebanana = s.store.store_items(StoreKind::GameBananaHuds).unwrap();
    assert_eq!(gamebanana.len(), 1);
    assert!(CuratedHud::from_item(&gamebanana[0]).is_none());
    let curated = s.store.store_items(CURATED).unwrap();
    let hud = CuratedHud::from_item(&curated[0]).expect("a curated HUD");
    assert_eq!(hud.hud_id, "rayshud");
}

// A reload that fails after a good one shows its error line and keeps the HUDs it last read.
#[gpui_kit::test]
fn a_failed_reload_shows_its_error_over_the_last_huds(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, runs) = support::fake_hub(cx);
    let (win, _page, tab) = open(cx, &env, &server);
    let before = shown(cx, &tab);
    assert!(!before.is_empty());
    assert!(!support::present(cx, win, ids::STORE_ERROR));
    // Without its table the library read fails, as a damaged library would.
    support::rename_table(&env, "library_huds", "library_huds_gone");
    cx.update(|cx| hub.update(cx, |h, cx| h.start(CURATED, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    run.finish(Ok(tf2tools_app::sync::SyncSummary::default()));
    // The hub's pump sleeps on the virtual clock, so each poll moves it on.
    support::wait_real(cx, "the failed reload", |cx| {
        support::next_repaint(cx);
        support::present(cx, win, ids::STORE_ERROR)
    });
    assert_eq!(
        support::label_of(cx, win, ids::STORE_ERROR).as_deref(),
        Some("TF2Tools couldn't read the store. Restart TF2Tools and try again.")
    );
    assert_eq!(shown(cx, &tab), before);
    for id in &before {
        assert!(
            support::present(cx, win, ids::store_card(CURATED, &cid(id))),
            "{id} still drawn"
        );
    }
    assert!(!support::present(cx, win, ids::STORE_UNLOADED));
}
