//! The Maps page's GameBanana source: the switch, the map grid, gamemodes and details.

use crate::support;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::addon_store::gamebanana_tab::{AddonTab, category_all_label};
use tf2tools_app::pages::maps::{MapListSource, MapsPage, MapsSource};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{AddonSection, GbId};

const MD: &str = "Generic_MostDownloaded";

const LIST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tf2tools-core/tests/fixtures/store/serveme-maps.html"
));

/// A shell whose store talks to `server` for serveme.tf and GameBanana alike.
fn shell(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> support::ShellWindow {
    cx.executor().allow_parking();
    server.mount_bytes("/serveme/maps", LIST.as_bytes().to_vec());
    let uri = server.uri();
    let endpoints = server
        .endpoints()
        .with_serveme(format!("{uri}/serveme"))
        .with_map_downloads(format!("{uri}/fastdl/maps"));
    let s = support::shell_window_tweaked(cx, env, Tf2State::Missing { saved: None }, |services| {
        *services = services.clone().with_endpoints(endpoints);
    });
    let store = s.services.store.clone();
    cx.update(|cx| MapListSource::install(cx, store));
    s
}

/// Opens Maps through the sidebar and waits for serveme.tf's list.
fn open_maps(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<MapsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Maps));
    let page = cx
        .update(|cx| s.shell.read(cx).maps_page())
        .expect("a Maps page");
    support::wait_real(cx, "the serveme list", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.is_loaded() && !p.is_syncing() && !p.shown_names().is_empty()
        })
    });
    support::settle(cx, s.win);
    page
}

/// Clicks the GameBanana source and returns its tab once its first step has settled.
fn open_gamebanana(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<MapsPage>,
) -> Entity<AddonTab> {
    support::click_id(cx, s.win, ids::maps_source("gamebanana"));
    let tab = cx
        .update(|cx| page.read(cx).gamebanana())
        .expect("the tab is built on first show");
    support::wait_real(cx, "the first map step", |cx| {
        cx.update(|cx| {
            let t = tab.read(cx);
            !t.is_loading()
                && (t.wants_load_more()
                    || t.is_no_results()
                    || !t.cards().is_empty()
                    || t.error().is_some())
        })
    });
    support::settle(cx, s.win);
    tab
}

fn one_map_listing(server: &support::StoreServer) {
    let records = vec![
        support::gb_addon(1, 5371, Some(5385)),
        support::gb_addon(2, 5371, None),
    ];
    server.mount_gb_index(5371, MD, 1, support::gb_listing(records, true));
}

#[gpui_kit::test]
fn the_page_opens_on_serveme_and_asks_gamebanana_nothing(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = shell(cx, &env, &server);
    let page = open_maps(cx, &s);
    assert_eq!(cx.update(|cx| page.read(cx).source()), MapsSource::Serveme);
    assert!(support::present(cx, s.win, ids::MAPS_SOURCE_TABS));
    for source in MapsSource::ALL {
        assert_eq!(
            support::label_of(cx, s.win, ids::maps_source(source.key())).as_deref(),
            Some(source.title())
        );
    }
    assert!(support::present(cx, s.win, ids::MAPS_SEARCH));
    assert!(!support::present(cx, s.win, ids::MAPS_GB_INFO));
    assert!(cx.update(|cx| page.read(cx).gamebanana()).is_none());
    assert!(server.gb_index_requests().is_empty());
}

#[gpui_kit::test]
fn the_gamebanana_source_streams_maps_from_their_category(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    one_map_listing(&server);
    let s = shell(cx, &env, &server);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    assert_eq!(
        cx.update(|cx| page.read(cx).source()),
        MapsSource::GameBanana
    );
    let shown: Vec<u64> = cx.update(|cx| tab.read(cx).cards().iter().map(|c| c.id.get()).collect());
    assert_eq!(shown, [1, 2]);
    assert!(support::present(
        cx,
        s.win,
        ids::addon_gb_card("maps", GbId::new(1))
    ));
    assert_eq!(
        server.gb_index_requests()[0],
        ("5371".to_owned(), MD.to_owned(), "1".to_owned())
    );
    assert!(support::present(cx, s.win, ids::ADDON_SEARCH));
    assert!(support::present(cx, s.win, ids::MAPS_GB_INFO));
    assert!(!support::present(cx, s.win, ids::MAPS_SEARCH));
    assert!(
        !support::present(cx, s.win, ids::MAPS_NO_TF2),
        "browsing needs no game"
    );
}

#[gpui_kit::test]
fn switching_back_shows_the_serveme_list_and_hides_the_tab(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    one_map_listing(&server);
    let s = shell(cx, &env, &server);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    assert!(!cx.update(|cx| tab.read(cx).is_hidden()));
    support::click_id(cx, s.win, ids::maps_source("serveme"));
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| page.read(cx).source()), MapsSource::Serveme);
    assert!(
        cx.update(|cx| tab.read(cx).is_hidden()),
        "serveme.tf hides the tab"
    );
    assert!(support::present(cx, s.win, ids::map_row("1cp_cage")));
    assert!(!support::present(cx, s.win, ids::ADDON_SEARCH));
    assert!(!support::present(cx, s.win, ids::MAPS_GB_INFO));

    support::click_id(cx, s.win, ids::maps_source("gamebanana"));
    support::settle(cx, s.win);
    let again = cx.update(|cx| page.read(cx).gamebanana()).unwrap();
    assert_eq!(
        again.entity_id(),
        tab.entity_id(),
        "the tab is kept, not rebuilt"
    );
    assert!(!cx.update(|cx| tab.read(cx).is_hidden()));
    assert_eq!(
        server.gb_index_requests().len(),
        1,
        "its cards are kept too"
    );
}

#[gpui_kit::test]
fn leaving_the_page_hides_the_tab_and_returning_shows_it(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    one_map_listing(&server);
    let s = shell(cx, &env, &server);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
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

#[test]
fn only_maps_names_its_first_category_row_by_gamemode() {
    assert_eq!(category_all_label(AddonSection::MAPS), "All gamemodes");
    for section in AddonSection::ALL {
        assert_eq!(category_all_label(section), "All", "{}", section.key());
    }
}

#[gpui_kit::test]
fn the_gamemode_select_narrows_the_maps(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_categories(5371, &[(5385, "King of the Hill"), (5386, "Jump")]);
    one_map_listing(&server);
    server.mount_gb_index(
        5385,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(3, 5371, Some(5385))], true),
    );
    let s = shell(cx, &env, &server);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    support::wait_real(cx, "the gamemodes", |cx| {
        cx.update(|cx| tab.read(cx).categories().len() == 2)
    });
    support::click_id(cx, s.win, ids::ADDON_CATEGORY);
    assert!(support::present(
        cx,
        s.win,
        ids::select_option(ids::ADDON_CATEGORY, "all")
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::select_option(ids::ADDON_CATEGORY, "5386")
    ));
    support::click_id(cx, s.win, ids::select_option(ids::ADDON_CATEGORY, "5385"));
    support::wait_real(cx, "the King of the Hill step", |cx| {
        cx.update(|cx| tab.read(cx).cards().first().map(|c| c.id.get()) == Some(3))
    });
    assert_eq!(cx.update(|cx| tab.read(cx).category()), Some(5385));
    assert!(!support::present(cx, s.win, ids::ADDON_WEAPON));
}

#[gpui_kit::test]
fn a_map_card_opens_its_details_with_the_gamebanana_link(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    one_map_listing(&server);
    server.mount_gb_profile(1, serde_json::json!({
        "_idRow": 1, "_sName": "Addon 1", "_sText": "<p>A koth map</p>", "_nDownloadCount": 10,
        "_aSubmitter": {"_sName": "vex"}, "_sProfileUrl": "https://gamebanana.com/mods/1",
        "_aCredits": [],
        "_aFiles": [{"_idRow": 5, "_sFile": "koth_x_b1.zip", "_nFilesize": 4096, "_tsDateAdded": 100,
            "_sAvResult": "clean", "_sDownloadUrl": "https://gamebanana.com/dl/5"}]
    }));
    let s = shell(cx, &env, &server);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(1)));
    support::wait_real(cx, "the map's profile", |cx| {
        support::present(cx, s.win, ids::ADDON_DETAIL_DOWNLOADS)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDON_DETAIL_NAME).as_deref(),
        Some("Addon 1")
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
    one_map_listing(&server);
    server.mount_gb_addon(1, "koth_x", &[("koth_x_b1.bsp", b"map".to_vec(), 100)]);
    let s = shell(cx, &env, &server);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(1)));
    support::wait_real(cx, "the map's profile", |cx| {
        support::present(cx, s.win, ids::ADDON_DETAIL_DOWNLOADS)
    });
    // Until maps install, a map's detail neither offers Install nor explains why it can't.
    assert!(!support::present(cx, s.win, ids::ADDON_DETAIL_INSTALL));
    assert!(!support::present(cx, s.win, ids::ADDON_DETAIL_BLOCKED));
}
