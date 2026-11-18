//! The Maps page's GameBanana source: the synced grid, its Selects and search, and details.

use crate::support;
use crate::support::maps::{
    maps_shell, maps_shell_found, open_gamebanana, open_maps, seed_maps, shown, wheel_into_view,
};

use gpui_kit::{TestAppContext, px};
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

fn bsp(tag: &[u8]) -> Vec<u8> {
    let mut body = b"VBSP".to_vec();
    body.extend_from_slice(tag);
    body
}
fn install_label(cx: &mut TestAppContext, win: gpui_kit::AnyWindowHandle) -> Option<String> {
    support::label_of(cx, win, ids::ADDON_DETAIL_INSTALL)
}

#[gpui_kit::test]
fn a_map_detail_installs_lists_the_maps_and_removes(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(10, "cp_alpha", &[("cp_alpha.bsp", bsp(b"a"), 100)]);
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(10)));
    support::wait_real(cx, "Install", |cx| {
        support::present(cx, s.win, ids::ADDON_DETAIL_DOWNLOADS)
            && install_label(cx, s.win).as_deref() == Some("Install")
    });
    assert!(!support::present(cx, s.win, ids::ADDON_DETAIL_REMOVE));
    assert!(!support::present(cx, s.win, ids::ADDON_DETAIL_MAPS));
    support::click_id(cx, s.win, ids::ADDON_DETAIL_INSTALL);
    support::wait_real(cx, "installed", |cx| {
        install_label(cx, s.win).as_deref() == Some("Installed")
    });
    let folder = env.game.join("tf/custom/tf2tools-gb-10-cp_alpha");
    assert!(folder.join("maps/cp_alpha.bsp").is_file());
    assert_eq!(
        support::label_of(cx, s.win, ids::addon_detail_map(0)).as_deref(),
        Some("cp_alpha")
    );
    support::click_id(cx, s.win, ids::ADDON_DETAIL_REMOVE);
    support::wait_real(cx, "the confirm", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DIALOG)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Remove Map")
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "removed", |cx| {
        install_label(cx, s.win).as_deref() == Some("Install")
    });
    assert!(!folder.exists());
    assert!(!support::present(cx, s.win, ids::ADDON_DETAIL_MAPS));
}

// Installs map 10 under `name` in a window `width` wide and returns the bounds of the header's name,
// Remove and GameBanana link, and of the whole dialog.
fn installed_header(
    cx: &mut TestAppContext,
    name: &str,
    width: f32,
) -> [gpui_kit::Bounds<gpui_kit::Pixels>; 4] {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(10, name, &[("cp_alpha.bsp", bsp(b"a"), 100)]);
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(10)));
    support::wait_real(cx, "Install", |cx| {
        support::present(cx, s.win, ids::ADDON_DETAIL_DOWNLOADS)
            && install_label(cx, s.win).as_deref() == Some("Install")
    });
    support::click_id(cx, s.win, ids::ADDON_DETAIL_INSTALL);
    support::wait_real(cx, "installed", |cx| {
        install_label(cx, s.win).as_deref() == Some("Installed")
            && support::present(cx, s.win, ids::ADDON_DETAIL_REMOVE)
    });
    // Sized once installed, so the grid and the dialog open at the usual size first.
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(width), gpui_kit::px(600.)),
    );
    support::settle(cx, s.win);
    [
        ids::ADDON_DETAIL_NAME.into(),
        ids::ADDON_DETAIL_REMOVE.into(),
        ids::store_detail_link("gamebanana"),
        ids::ADDON_DETAIL.into(),
    ]
    .map(|id: gpui_kit::ElementId| support::bounds_of(cx, s.win, id).unwrap())
}

// A name with no spaces never breaks mid-word; the buttons move under it instead.
#[gpui_kit::test]
fn a_long_name_without_spaces_stays_whole_and_the_buttons_go_under_it(cx: &mut TestAppContext) {
    let [name, remove, link, _] = installed_header(cx, "cp_orange_orange_rush_v3.1", 900.);
    assert_eq!(
        name.size.height,
        gpui_kit::px(28.),
        "the name broke mid-word: {name:?}"
    );
    assert!(
        remove.top() >= name.bottom() && link.top() >= name.bottom(),
        "the buttons {remove:?} {link:?} should sit under the name {name:?}"
    );
}

// A short name keeps the buttons beside it on the same row.
#[gpui_kit::test]
fn a_short_name_keeps_the_buttons_beside_it(cx: &mut TestAppContext) {
    let [name, remove, link, _] = installed_header(cx, "cp_alpha", 900.);
    assert!(
        remove.top() < name.bottom() && remove.left() > name.right() && link.top() < name.bottom(),
        "the buttons {remove:?} {link:?} should sit beside the name {name:?}"
    );
}

// A name wider than the whole dialog still stays inside it.
#[gpui_kit::test]
fn a_name_wider_than_the_dialog_stays_inside_it(cx: &mut TestAppContext) {
    let long = "cp_orange_orange_rush_v3.1_".repeat(4);
    let [name, _, _, dialog] = installed_header(cx, &long, 544.);
    assert!(
        name.right() <= dialog.right(),
        "the name {name:?} overflows the dialog {dialog:?}"
    );
}

#[gpui_kit::test]
fn a_missing_folder_offers_install_again_over_update(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    support::sql(&env).execute_batch(
        "INSERT INTO library_gb_addons (gb_id, section, name, author, file_id, file_date, latest_file_date,
            checked_at, payloads, files, installed, gone) VALUES (12, 'maps', 'koth_beta', 'vex', 1, 100, 200,
            CAST(strftime('%s','now') AS INTEGER), '[\"koth_beta\"]', '[\"custom/tf2tools-gb-12-koth_beta\"]', 1, 0);",
    ).unwrap();
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(12)));
    support::wait_real(cx, "the control", |cx| {
        install_label(cx, s.win).as_deref() == Some("Install again")
    });
}

/// Map 10's detail with one file listed at `size` bytes; env and server outlive the body.
fn detail_with_size(
    cx: &mut TestAppContext,
    size: u64,
) -> (support::Env, support::StoreServer, support::ShellWindow) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_profile(
        10,
        serde_json::json!({"_idRow": 10, "_sName": "cp_alpha", "_aFiles": [
        {"_idRow": 1, "_sFile": "a.zip", "_nFilesize": size, "_tsDateAdded": 100,
         "_sAvResult": "clean", "_sDownloadUrl": format!("{}/dl/x", server.uri())}]}),
    );
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(10)));
    support::wait_real(cx, "the profile", |cx| {
        support::present(cx, s.win, ids::ADDON_DETAIL_DOWNLOADS)
    });
    (env, server, s)
}

#[gpui_kit::test]
fn a_map_past_the_add_on_cap_can_still_install(cx: &mut TestAppContext) {
    let (_env, _server, s) = detail_with_size(cx, 400 * 1024 * 1024);
    assert!(!support::present(cx, s.win, ids::ADDON_DETAIL_BLOCKED));
    assert_eq!(install_label(cx, s.win).as_deref(), Some("Install"));
}

#[gpui_kit::test]
fn a_map_past_the_map_cap_says_it_is_too_big(cx: &mut TestAppContext) {
    let (_env, _server, s) = detail_with_size(cx, 600 * 1024 * 1024);
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDON_DETAIL_BLOCKED).as_deref(),
        Some("This map is too big for TF2Tools to install.")
    );
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

#[gpui_kit::test]
fn at_800_by_600_every_map_card_scrolls_wholly_into_view(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell(cx, &env, &server);
    let rows: Vec<String> = (100..124)
        .map(|id| {
            format!(
                "({id}, 'map_{id}', 'vex', {id}, 5373, 'Control Points', 5373, {id}, \
                 'https://gamebanana.com/mods/{id}', 'x')"
            )
        })
        .collect();
    support::sql(&env)
        .execute_batch(&format!(
            "INSERT INTO store_gb_map_categories (category_id, name, gamemode_id) VALUES \
                 (5373, 'Control Points', 5373);
             INSERT INTO store_gb_maps (gb_id, name, author, date_added, category_id, \
                 category_name, gamemode_id, download_rank, profile_url, fetched_at) VALUES {};",
            rows.join(", ")
        ))
        .unwrap();
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    let ids_shown = shown(cx, &tab);
    assert_eq!(ids_shown.len(), 24);
    // The page scrolls the grid's one-row floor into view, then the grid scrolls its rows.
    let maps = ids::page(Page::Maps);
    wheel_into_view(cx, &s, maps.clone(), ids::MAPS_GB_GRID);
    let view = support::bounds_of(cx, s.win, maps).expect("the page");
    let grid = support::bounds_of(cx, s.win, ids::MAPS_GB_GRID).expect("the grid");
    assert!(
        grid.size.height < view.size.height,
        "the grid stays a scrolling window: {grid:?}"
    );
    let bar = support::bounds_of(cx, s.win, ids::grid_scrollbar(ids::MAPS_GB_GRID)).expect("bar");
    assert!(
        bar.right() <= view.right(),
        "the grid's bar {bar:?} lies inside {view:?}"
    );
    let last = *ids_shown.last().unwrap();
    wheel_into_view(
        cx,
        &s,
        ids::MAPS_GB_GRID,
        ids::addon_gb_card("maps", GbId::new(last)),
    );
}
