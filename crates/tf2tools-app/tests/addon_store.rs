use crate::support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ScrollDelta, TestAppContext, point, px, size};
use tf2tools_app::ids;
use tf2tools_app::pages::addon_store::InfoLink;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::sync::SyncSummary;
use tf2tools_core::{AddonName, StoreItemId, StoreKind, Tf2Install};

const A: StoreKind = StoreKind::Addons;
const FLAT: &str = "mastercomfig-addon-flat-mouse.vpk";
const LOW: &str = "mastercomfig-addon-low-memory.vpk";
const TUT: &str = "mastercomfig-addon-no-tutorial.vpk";
const PYRO: &str = "mastercomfig-addon-no-pyroland.vpk";

fn addon(asset: &str) -> StoreItemId {
    StoreItemId::Addon(AddonName::new(asset).unwrap())
}

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

/// Three store addons; Low Memory in the library, No Tutorial installed.
fn seed(uri: String) -> impl FnOnce(&support::Env, &tf2tools_app::services::Services) {
    move |env, s| {
        support::seed_addons(
            env,
            &uri,
            &[
                (FLAT, "Flat Mouse", "9.100.1"),
                (LOW, "Low Memory", "9.100.1"),
                (TUT, "No Tutorial", "9.100.1"),
            ],
        );
        let vpk = support::vpk_bytes(&[("cfg/addons/x.cfg", b"v1".as_slice())]);
        support::seed_library_addon(s, env, LOW, "9.100.1", &vpk, false);
        support::seed_library_addon(s, env, TUT, "9.100.1", &vpk, true);
    }
}

#[gpui_kit::test]
fn the_page_shows_v5_s_header_and_card_states(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::addon_store_window(cx, &env, &server, found(&env), seed(server.uri()));
    for asset in [FLAT, LOW, TUT] {
        assert!(
            support::present(cx, w.win, ids::store_card(A, &addon(asset))),
            "card {asset}"
        );
    }
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_STORE_TAG).as_deref(),
        Some("9.100.1")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_REFRESH).as_deref(),
        Some("Refresh")
    );
    assert!(support::present(cx, w.win, ids::ADDON_STORE_INFO));
    support::assert_starts_under_top_bar(
        cx,
        w.win,
        ids::page(Page::AddonStore),
        ids::ADDON_STORE_TAG,
    );
    assert!(!support::present(cx, w.win, ids::ADDON_STORE_NO_TF2));
    assert_eq!(
        support::label_of(cx, w.win, ids::store_download(A, &addon(FLAT))).as_deref(),
        Some("Download")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::store_state(A, &addon(LOW))).as_deref(),
        Some("In Library")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::store_state(A, &addon(TUT))).as_deref(),
        Some("Installed")
    );
    assert!(
        !support::present(cx, w.win, ids::store_download(A, &addon(TUT))),
        "a badge, not a button"
    );
}

#[gpui_kit::test]
fn the_installed_check_is_v5_s_16_px(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::addon_store_window(cx, &env, &server, found(&env), seed(server.uri()));
    let check = support::bounds_of(cx, w.win, ids::store_check(A, &addon(TUT))).expect("installed");
    assert_eq!((check.size.width, check.size.height), (px(16.), px(16.)));
    assert!(
        !support::present(cx, w.win, ids::store_check(A, &addon(LOW))),
        "in the library, not installed"
    );
}

/// The three addons, none in the library, so every card has the same Download footer.
fn seed_store_only(uri: String) -> impl FnOnce(&support::Env, &tf2tools_app::services::Services) {
    move |env, _| {
        support::seed_addons(
            env,
            &uri,
            &[
                (FLAT, "Flat Mouse", "9.100.1"),
                (LOW, "Low Memory", "9.100.1"),
                (TUT, "No Tutorial", "9.100.1"),
            ],
        )
    }
}

// v5's store AddonCard is the library card plus its stats line: py-6 and gap-6, a pb-2 header,
// a pb-2 content ending at the 16 px stats line, so a card is as tall as what it holds.
#[gpui_kit::test]
fn addon_store_cards_size_to_their_content(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::addon_store_window(
        cx,
        &env,
        &server,
        found(&env),
        seed_store_only(server.uri()),
    );
    let id = addon(FLAT);
    support::assert_full_width_download(
        cx,
        w.win,
        ids::store_card(A, &id),
        ids::store_download(A, &id),
    );
    let win = w.win;
    let at = |cx: &mut TestAppContext, el: gpui_kit::ElementId| {
        support::bounds_of(cx, win, el).expect("drawn")
    };
    let card = at(cx, ids::store_card(A, &id));
    let tag = at(cx, ids::store_release_tag(A, &id));
    let desc = at(cx, ids::store_description(A, &id));
    let button = at(cx, ids::store_download(A, &id));
    assert_eq!(tag.top() - card.top(), px(25.), "the 1 px border plus py-6");
    assert_eq!(desc.top() - tag.bottom(), px(32.), "header pb-2 then gap-6");
    // mt-2, the 16 px stats line, content pb-2, gap-6: nothing else above Download.
    assert_eq!(button.top() - desc.bottom(), px(56.));
    for asset in [LOW, TUT] {
        let other = at(cx, ids::store_card(A, &addon(asset)));
        assert_eq!(
            (other.top(), other.size.height),
            (card.top(), card.size.height),
            "{asset} shares the row"
        );
    }
}

// The store renders every card unvirtualized, so its grid div must scroll a long list itself.
#[gpui_kit::test]
fn a_long_addon_store_scrolls(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let assets: Vec<String> = (0..30)
        .map(|i| format!("mastercomfig-addon-a{i:02}.vpk"))
        .collect();
    let uri = server.uri();
    let rows = assets.clone();
    let w = support::addon_store_window(cx, &env, &server, found(&env), move |env, _| {
        let rows: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|a| (a.as_str(), "Addon", "9.100.1"))
            .collect();
        support::seed_addons(env, &uri, &rows);
    });
    let first = ids::store_card(A, &addon(&assets[0]));
    let before = support::bounds_of(cx, w.win, first.clone()).expect("drawn");
    cx.update_window(w.win, |_, win, cx| {
        let down = ScrollDelta::Pixels(point(px(0.), px(-500.)));
        win.scroll(ids::STORE_GRID, down, cx)
    })
    .unwrap();
    let after = support::bounds_of(cx, w.win, first).expect("drawn");
    assert_eq!(before.top() - after.top(), px(500.));
}

// v5's store skeletons sit in `grid-cols-1 sm:grid-cols-2 lg:grid-cols-3`.
#[gpui_kit::test]
fn the_addon_skeletons_follow_v5_s_responsive_columns(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, _runs) = support::fake_hub(cx);
    let w = support::addon_store_window(cx, &env, &server, found(&env), |_, _| {});
    cx.update(|cx| hub.update(cx, |h, cx| h.start(A, false, cx)));
    support::next_repaint(cx);
    assert!(support::present(cx, w.win, ids::STORE_SYNCING));
    let win = w.win;
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
fn without_tf2_the_page_warns_and_nothing_reads_installed(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::addon_store_window(
        cx,
        &env,
        &server,
        Tf2State::Missing { saved: None },
        seed(server.uri()),
    );
    assert!(support::present(cx, w.win, ids::ADDON_STORE_NO_TF2));
    assert_eq!(
        support::label_of(cx, w.win, ids::store_state(A, &addon(TUT))).as_deref(),
        Some("In Library")
    );
}

#[gpui_kit::test]
fn an_empty_store_says_it_could_not_load(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::addon_store_window(cx, &env, &server, found(&env), |_, _| {});
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_EMPTY).as_deref(),
        Some("Could not load addons")
    );
    assert!(!support::present(cx, w.win, ids::ADDON_STORE_TAG));
}

#[gpui_kit::test]
fn a_failed_load_shows_the_inline_error_with_retry(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::addon_store_window(cx, &env, &server, found(&env), |env, _| {
        rusqlite::Connection::open(env.data.db_path())
            .unwrap()
            .execute("DROP TABLE store_addons", [])
            .unwrap();
    });
    assert!(support::present(cx, w.win, ids::STORE_ERROR));
    assert!(support::present(cx, w.win, ids::STORE_RETRY));
    assert!(!support::present(cx, w.win, ids::STORE_EMPTY));
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_ERROR).as_deref(),
        Some("TF2Tools couldn't read the store. Restart TF2Tools and try again.")
    );
}

#[gpui_kit::test]
fn refresh_forces_an_addon_sync_and_reloads_when_it_ends(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (_hub, runs) = support::fake_hub(cx);
    let w = support::addon_store_window(cx, &env, &server, found(&env), seed(server.uri()));
    support::click_id(cx, w.win, ids::STORE_REFRESH);
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!((run.kind, run.force), (A, true));
    support::next_repaint(cx);
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_REFRESH).as_deref(),
        Some("Syncing...")
    );
    let before = cx.update(|cx| w.page.read(cx).reloads());
    support::seed_addons(&env, &server.uri(), &[(PYRO, "No Pyroland", "9.100.1")]);
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the reload", |cx| {
        support::present(cx, w.win, ids::store_card(A, &addon(PYRO)))
    });
    assert_eq!(
        cx.update(|cx| w.page.read(cx).reloads()),
        before + 1,
        "one reload per run"
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_REFRESH).as_deref(),
        Some("Refresh")
    );
}

#[gpui_kit::test]
fn the_addon_store_is_a_real_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    assert!(!Page::AddonStore.is_placeholder());
    support::click_id(cx, s.win, ids::nav(Page::AddonStore));
    assert!(
        cx.update(|cx| s.shell.read(cx).addon_store_page())
            .is_some()
    );
    assert!(support::present(cx, s.win, ids::page(Page::AddonStore)));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(
        cx.update(|cx| s.shell.read(cx).addon_store_page())
            .is_none()
    );
}

// v5's info banner wraps its sentence onto a second line at 1200 px instead of running past the box.
#[gpui_kit::test]
fn the_info_banner_wraps_inside_its_box(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::addon_store_window(cx, &env, &server, found(&env), seed(server.uri()));
    let info = support::bounds_of(cx, w.win, ids::ADDON_STORE_INFO).unwrap();
    assert!(info.size.height > px(70.), "one-line banner: {info:?}");
}

#[gpui_kit::test]
fn refresh_is_v5_s_32_px(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::addon_store_window(cx, &env, &server, found(&env), seed(server.uri()));
    let b = support::bounds_of(cx, w.win, ids::STORE_REFRESH).unwrap();
    assert_eq!(b.size.height, px(32.));
}

#[test]
fn the_info_links_are_v5_s() {
    assert_eq!(
        InfoLink::Mastercomfig.url(),
        Some("https://mastercomfig.com")
    );
    assert_eq!(InfoLink::Addons.url(), None);
}

#[gpui_kit::test]
fn an_empty_store_shows_v5_s_wifi_off_card(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = support::addon_store_window(cx, &env, &server, found(&env), |_, _| {});
    let icon = support::bounds_of(cx, w.win, ids::STORE_EMPTY_ICON).expect("the icon");
    assert_eq!(icon.size.height, px(48.));
    let card = support::bounds_of(cx, w.win, ids::STORE_EMPTY_CARD).expect("the card");
    let title = support::bounds_of(cx, w.win, ids::STORE_EMPTY).unwrap();
    assert!(card.contains(&title.origin));
    assert!(!support::present(cx, w.win, ids::skeleton_card(0)));
    // v5 pads three layers (Card py-6, CardContent py-12, EmptyState py-12) inside a 1 px border.
    assert_eq!(icon.origin.y - card.origin.y, px(121.));
    assert_eq!(card.size.height, px(361.5));
}

#[gpui_kit::test]
fn a_sync_into_an_empty_store_shows_six_skeleton_cards(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, _runs) = support::fake_hub(cx);
    let w = support::addon_store_window(cx, &env, &server, found(&env), |_, _| {});
    cx.update(|cx| hub.update(cx, |h, cx| h.start(A, false, cx)));
    support::next_repaint(cx);
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_SYNCING).as_deref(),
        Some("Syncing addons...")
    );
    for ix in 0..6 {
        assert!(support::present(cx, w.win, ids::skeleton_card(ix)), "{ix}");
    }
    assert!(!support::present(cx, w.win, ids::skeleton_card(6)));
    assert!(!support::present(cx, w.win, ids::STORE_EMPTY));
}

#[gpui_kit::test]
fn the_addons_link_opens_the_addons_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::click_id(cx, s.win, ids::nav(Page::AddonStore));
    let page = cx.update(|cx| s.shell.read(cx).addon_store_page()).unwrap();
    cx.update(|cx| page.update(cx, |p, cx| p.follow_info_link(InfoLink::Addons, cx)));
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Addons);
}

// The kit's Button exposes no a11y disabled flag, so the test reads what disabling does: a
// click while syncing starts nothing, and once the sync and its reload end a click starts a run.
#[gpui_kit::test]
fn refresh_is_disabled_while_syncing_and_enabled_after(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (_hub, runs) = support::fake_hub(cx);
    let w = support::addon_store_window(cx, &env, &server, found(&env), seed(server.uri()));
    support::click_id(cx, w.win, ids::STORE_REFRESH);
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    support::next_repaint(cx);
    assert!(cx.update(|cx| w.page.read(cx).is_syncing()));
    support::click_id(cx, w.win, ids::STORE_REFRESH);
    assert!(runs.try_recv().is_err(), "a syncing Refresh starts nothing");
    let before = cx.update(|cx| w.page.read(cx).reloads());
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the reload", |cx| {
        cx.update(|cx| w.page.read(cx).reloads()) > before
    });
    assert!(!cx.update(|cx| w.page.read(cx).is_syncing()));
    support::click_id(cx, w.win, ids::STORE_REFRESH);
    let again = runs
        .recv_timeout(Duration::from_secs(5))
        .expect("Refresh is enabled again");
    assert_eq!((again.kind, again.force), (A, true));
}

// v5 holds `syncing` until the post-sync load returns, so the empty card cannot flash between.
#[gpui_kit::test]
fn the_store_stays_syncing_until_the_post_sync_reload_lands(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (_hub, runs) = support::fake_hub(cx);
    let w = support::addon_store_window(cx, &env, &server, found(&env), |_, _| {});
    support::click_id(cx, w.win, ids::STORE_REFRESH);
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    support::next_repaint(cx);
    assert!(cx.update(|cx| w.page.read(cx).is_syncing()));
    let before = cx.update(|cx| w.page.read(cx).reloads());
    let idle_before_reload = Rc::new(Cell::new(false));
    let seen = Rc::clone(&idle_before_reload);
    let _watch = cx.update(|cx| {
        cx.observe(&w.page, move |page, cx| {
            let page = page.read(cx);
            if !page.is_syncing() && page.reloads() == before {
                seen.set(true);
            }
        })
    });
    support::seed_addons(&env, &server.uri(), &[(PYRO, "No Pyroland", "9.100.1")]);
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the reload", |cx| {
        cx.update(|cx| w.page.read(cx).reloads()) > before
    });
    assert!(!idle_before_reload.get(), "idle before the reload landed");
    assert!(!cx.update(|cx| w.page.read(cx).is_syncing()));
}

// The bar sits in a gutter beside the cards, so the last column keeps its full width.
#[gpui_kit::test]
fn a_long_addon_store_shows_a_bar_beside_its_cards(cx: &mut TestAppContext) {
    use support::scroll::{assert_bar_beside, assert_clear_of_bar, assert_track_scrolls};
    let env = support::env();
    let server = support::StoreServer::start();
    let assets: Vec<String> = (0..30)
        .map(|i| format!("mastercomfig-addon-a{i:02}.vpk"))
        .collect();
    let uri = server.uri();
    let rows = assets.clone();
    let w = support::addon_store_window(cx, &env, &server, found(&env), move |env, _| {
        let rows: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|a| (a.as_str(), "Addon", "9.100.1"))
            .collect();
        support::seed_addons(env, &uri, &rows);
    });
    let bar = ids::grid_scrollbar(ids::STORE_GRID);
    assert_bar_beside(cx, w.win, ids::STORE_GRID, bar.clone());
    // Three columns at 1200 px: the third card ends the first row at the grid's right edge.
    let third = ids::store_card(A, &addon(&assets[2]));
    assert_clear_of_bar(cx, w.win, third, bar.clone());
    assert_track_scrolls(cx, w.win, bar, ids::store_card(A, &addon(&assets[0])));
}
