//! The Maps page's GameBanana source: the synced grid, its Selects and search, and details.

use crate::support;
use crate::support::maps::{maps_shell, open_gamebanana, open_maps, seed_maps, shown};

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::pages::maps::MapsSource;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_grid::SEARCH_DEBOUNCE;
use tf2tools_core::{GbId, GbMapSort};

#[gpui_kit::test]
fn the_page_opens_on_serveme_and_builds_no_gamebanana_tab(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    let page = open_maps(cx, &s);
    assert_eq!(cx.update(|cx| page.read(cx).source()), MapsSource::Serveme);
    for source in MapsSource::ALL {
        assert_eq!(
            support::label_of(cx, s.win, ids::maps_source(source.key())).as_deref(),
            Some(source.title())
        );
    }
    assert!(support::present(cx, s.win, ids::MAPS_SEARCH));
    assert!(!support::present(cx, s.win, ids::MAPS_GB_INFO));
    assert!(cx.update(|cx| page.read(cx).gamebanana()).is_none());
    assert_eq!(server.requests_where(|p| p.starts_with("/apiv11")), 0);
}

#[gpui_kit::test]
fn the_gamebanana_source_shows_the_synced_maps_without_the_network(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    assert_eq!(shown(cx, &tab), [10, 11, 12]);
    assert!(support::present(
        cx,
        s.win,
        ids::addon_gb_card("maps", GbId::new(10))
    ));
    for id in [
        ids::MAPS_GB_SEARCH,
        ids::MAPS_GB_SORT,
        ids::MAPS_GB_GAMEMODE,
        ids::MAPS_GB_INFO,
    ] {
        assert!(support::present(cx, s.win, id), "{id}");
    }
    assert!(!support::present(cx, s.win, ids::MAPS_SEARCH));
    assert!(
        !support::present(cx, s.win, ids::MAPS_NO_TF2),
        "browsing needs no game"
    );
    assert_eq!(
        server.requests_where(|p| p.starts_with("/apiv11")),
        0,
        "browsing is local"
    );
}

#[gpui_kit::test]
fn an_empty_catalogue_says_no_maps_found(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    assert!(shown(cx, &tab).is_empty());
    assert_eq!(
        support::label_of(cx, s.win, ids::MAPS_GB_EMPTY).as_deref(),
        Some("No maps found")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Maps appear here after a sync with GameBanana.")
    );
    assert!(!support::present(cx, s.win, ids::MAPS_GB_LOADING));
}

#[gpui_kit::test]
fn switching_and_leaving_hide_the_tab_and_keep_it(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    support::click_id(cx, s.win, ids::maps_source("serveme"));
    support::settle(cx, s.win);
    assert!(
        cx.update(|cx| tab.read(cx).is_hidden()),
        "serveme.tf hides the tab"
    );
    assert!(!support::present(cx, s.win, ids::MAPS_GB_SEARCH));
    assert!(!support::present(cx, s.win, ids::MAPS_GB_INFO));
    assert!(support::present(cx, s.win, ids::map_row("1cp_cage")));
    support::click_id(cx, s.win, ids::maps_source("gamebanana"));
    support::settle(cx, s.win);
    let again = cx.update(|cx| page.read(cx).gamebanana()).unwrap();
    assert_eq!(
        again.entity_id(),
        tab.entity_id(),
        "the tab is kept, not rebuilt"
    );
    assert_eq!(shown(cx, &tab), [10, 11, 12], "its cards are kept too");
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::settle(cx, s.win);
    assert!(
        cx.update(|cx| tab.read(cx).is_hidden()),
        "a hidden page hides the tab"
    );
    support::click_id(cx, s.win, ids::nav(Page::Maps));
    support::settle(cx, s.win);
    assert_eq!(
        cx.update(|cx| page.read(cx).source()),
        MapsSource::GameBanana
    );
    assert!(!cx.update(|cx| tab.read(cx).is_hidden()));
}

#[gpui_kit::test]
fn a_map_card_opens_its_details_with_the_gamebanana_link(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_profile(10, serde_json::json!({
        "_idRow": 10, "_sName": "cp_alpha", "_sText": "<p>A control point map</p>", "_nDownloadCount": 10,
        "_aSubmitter": {"_sName": "vex"}, "_sProfileUrl": "https://gamebanana.com/mods/10",
        "_aCredits": [],
        "_aFiles": [{"_idRow": 5, "_sFile": "koth_x_b1.zip", "_nFilesize": 4096, "_tsDateAdded": 100,
            "_sAvResult": "clean", "_sDownloadUrl": "https://gamebanana.com/dl/5"}]
    }));
    let s = maps_shell(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(10)));
    support::wait_real(cx, "the map's profile", |cx| {
        support::present(cx, s.win, ids::ADDON_DETAIL_DOWNLOADS)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDON_DETAIL_NAME).as_deref(),
        Some("cp_alpha")
    );
    assert!(
        support::label_of(cx, s.win, ids::addon_detail_file(0))
            .unwrap()
            .starts_with("koth_x_b1.zip")
    );
    assert!(support::present(
        cx,
        s.win,
        ids::store_detail_link("gamebanana")
    ));
}

#[gpui_kit::test]
fn a_map_detail_offers_no_install(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(10, "cp_alpha", &[("cp_alpha.bsp", b"map".to_vec(), 100)]);
    let s = maps_shell(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(10)));
    support::wait_real(cx, "the map's profile", |cx| {
        support::present(cx, s.win, ids::ADDON_DETAIL_DOWNLOADS)
    });
    // Until maps install, a map's detail neither offers Install nor explains why it can't.
    assert!(!support::present(cx, s.win, ids::ADDON_DETAIL_INSTALL));
    assert!(!support::present(cx, s.win, ids::ADDON_DETAIL_BLOCKED));
}

#[gpui_kit::test]
fn the_gamemode_select_lists_first_level_gamemodes_and_filters(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    support::wait_real(cx, "the gamemodes", |cx| {
        cx.update(|cx| tab.read(cx).gamemodes().len() == 3)
    });
    support::click_id(cx, s.win, ids::MAPS_GB_GAMEMODE);
    let option = |key: &str| ids::select_option(ids::MAPS_GB_GAMEMODE, key);
    assert_eq!(
        support::label_of(cx, s.win, option("all")).as_deref(),
        Some("All gamemodes")
    );
    for key in ["5383", "5373", "5385"] {
        assert!(support::present(cx, s.win, option(key)), "{key}");
    }
    assert!(
        !support::present(cx, s.win, option("5388")),
        "a nested category is no gamemode"
    );
    support::click_id(cx, s.win, option("5383"));
    support::wait_real(cx, "Achievement/Idle", |cx| shown(cx, &tab) == [11]);
    assert_eq!(cx.update(|cx| tab.read(cx).gamemode()), Some(5383));
}

#[gpui_kit::test]
fn the_sort_select_orders_newest_first(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    support::click_id(cx, s.win, ids::MAPS_GB_SORT);
    support::click_id(cx, s.win, ids::select_option(ids::MAPS_GB_SORT, "Newest"));
    support::wait_real(cx, "newest first", |cx| shown(cx, &tab) == [11, 10, 12]);
    assert_eq!(cx.update(|cx| tab.read(cx).sort()), GbMapSort::Newest);
}

#[gpui_kit::test]
fn search_matches_name_or_author_locally_and_says_when_nothing_does(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    cx.update(|cx| tab.update(cx, |t, cx| t.search_typed("VEX".to_owned(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the author search", |cx| shown(cx, &tab) == [10, 12]);
    cx.update(|cx| tab.update(cx, |t, cx| t.search_typed("zzz".to_owned(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "no results", |cx| shown(cx, &tab).is_empty());
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::MAPS_GB_EMPTY));
    assert_eq!(
        support::label_of(cx, s.win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Try another search.")
    );
    assert_eq!(server.requests_where(|p| p.starts_with("/apiv11")), 0);
}
