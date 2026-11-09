use crate::support;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::addon_store::AddonStoreTab;
use tf2tools_app::pages::addon_store::gamebanana_tab::AddonTab;
use tf2tools_app::services::Tf2State;
use tf2tools_app::store_grid::SEARCH_DEBOUNCE;
use tf2tools_core::{AddonSection, AddonSort, GbId, Tf2Install};

const MD: &str = "Generic_MostDownloaded";

fn window(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> support::AddonStoreWindow {
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    support::addon_store_window(cx, env, server, state, |_, _| {})
}

fn open_tab(cx: &mut TestAppContext, w: &support::AddonStoreWindow, key: &str) -> Entity<AddonTab> {
    support::click_id(cx, w.win, ids::addon_store_tab(key));
    let section = AddonSection::by_key(key).unwrap();
    cx.update(|cx| w.page.read(cx).gamebanana(section))
        .expect("the tab is built on first show")
}

fn settled(cx: &mut TestAppContext, tab: &Entity<AddonTab>, what: &str) {
    support::wait_real(cx, what, |cx| {
        cx.update(|cx| {
            let t = tab.read(cx);
            !t.is_loading()
                && (t.wants_load_more()
                    || t.is_no_results()
                    || !t.cards().is_empty()
                    || t.error().is_some())
        })
    });
}

#[gpui_kit::test]
fn the_store_opens_on_mastercomfig_with_a_tab_per_section(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let w = window(cx, &env, &server);
    assert_eq!(
        cx.update(|cx| w.page.read(cx).tab()),
        AddonStoreTab::Mastercomfig
    );
    assert!(support::present(
        cx,
        w.win,
        ids::addon_store_tab("mastercomfig")
    ));
    for section in AddonSection::ALL {
        assert!(
            support::present(cx, w.win, ids::addon_store_tab(section.key())),
            "{}",
            section.key()
        );
    }
    assert!(support::present(cx, w.win, ids::ADDON_STORE_INFO));
    assert!(
        server.gb_index_requests().is_empty(),
        "no tab loads until it is shown"
    );
}

#[gpui_kit::test]
fn a_section_tab_streams_its_cards_and_leaves_out_excluded_ones(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let mut old = support::gb_addon(3, 1090, Some(1094));
    old["_bIsObsolete"] = true.into();
    let records = vec![
        support::gb_addon(1, 1090, Some(1094)),
        support::gb_addon(2, 1090, Some(1105)),
        old,
    ];
    server.mount_gb_index(1090, MD, 1, support::gb_listing(records, true));
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "effects");
    settled(cx, &tab, "the first step");
    let shown: Vec<u64> = cx.update(|cx| tab.read(cx).cards().iter().map(|c| c.id.get()).collect());
    assert_eq!(shown, [1, 3]);
    assert!(support::present(
        cx,
        w.win,
        ids::addon_gb_card("effects", GbId::new(1))
    ));
    assert_eq!(
        support::label_of(cx, w.win, ids::addon_gb_card_name("effects", GbId::new(1))).as_deref(),
        Some("Addon 1")
    );
    assert!(support::present(
        cx,
        w.win,
        ids::addon_gb_card_outdated("effects", GbId::new(3))
    ));
    assert!(!support::present(
        cx,
        w.win,
        ids::addon_gb_card_outdated("effects", GbId::new(1))
    ));
    assert!(!support::present(cx, w.win, ids::ADDON_LOAD_MORE));
    assert!(!support::present(cx, w.win, ids::ADDON_EMPTY));
}

#[gpui_kit::test]
fn a_dry_step_continues_once_then_shows_load_more(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    for page in 1..=12u32 {
        let huds = (0..50)
            .map(|i| support::gb_addon(u64::from(page) * 100 + i, 1644, Some(1649)))
            .collect();
        server.mount_gb_index(1644, MD, page, support::gb_listing(huds, false));
    }
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "guis");
    settled(cx, &tab, "two dry steps");
    assert!(cx.update(|cx| tab.read(cx).wants_load_more()));
    assert_eq!(
        server.gb_index_requests().len(),
        8,
        "two steps of four calls"
    );
    assert!(support::present(cx, w.win, ids::ADDON_LOAD_MORE));
    assert!(
        !support::present(cx, w.win, ids::ADDON_EMPTY),
        "no results only shows at the end"
    );
    support::click_id(cx, w.win, ids::ADDON_LOAD_MORE);
    support::wait_real(cx, "the clicked step", |cx| {
        cx.update(|cx| !tab.read(cx).is_loading()) && server.gb_index_requests().len() == 12
    });
    assert!(
        support::present(cx, w.win, ids::ADDON_LOAD_MORE),
        "a dry clicked step offers the button again"
    );
}

#[gpui_kit::test]
fn scrolling_to_the_end_runs_the_next_step(cx: &mut TestAppContext) {
    // Pages 1 to 4 are open-ended with two cards each, so the first step ends at its 4-call cap
    // with cards (Idle). Only the grid's near_end can start the step that reads page 5.
    let (env, server) = (support::env(), support::StoreServer::start());
    for page in 1..=5u32 {
        let first = u64::from(page) * 10;
        let records = (first..first + 2)
            .map(|i| support::gb_addon(i, 587, None))
            .collect();
        server.mount_gb_index(587, MD, page, support::gb_listing(records, page == 5));
    }
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "textures");
    support::wait_real(cx, "the step near_end asked for", |cx| {
        cx.update(|cx| tab.read(cx).cards().len() == 10 && !tab.read(cx).is_loading())
    });
    let pages: Vec<String> = server
        .gb_index_requests()
        .into_iter()
        .map(|r| r.2)
        .collect();
    assert_eq!(pages, ["1", "2", "3", "4", "5"]);
    assert!(support::present(
        cx,
        w.win,
        ids::addon_gb_card("textures", GbId::new(51))
    ));
    assert!(!support::present(cx, w.win, ids::ADDON_LOAD_MORE));
}

#[gpui_kit::test]
fn the_sort_select_reloads_newest_first(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        2774,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(1, 2774, None)], true),
    );
    server.mount_gb_index(
        2774,
        "Generic_Newest",
        1,
        support::gb_listing(vec![support::gb_addon(2, 2774, None)], true),
    );
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "gamefiles");
    settled(cx, &tab, "the first step");
    support::click_id(cx, w.win, ids::ADDON_SORT);
    support::click_id(cx, w.win, ids::select_option(ids::ADDON_SORT, "Newest"));
    support::wait_real(cx, "the newest step", |cx| {
        cx.update(|cx| tab.read(cx).cards().first().map(|c| c.id.get()) == Some(2))
    });
    assert_eq!(cx.update(|cx| tab.read(cx).sort()), AddonSort::Newest);
}

#[gpui_kit::test]
fn the_subcategory_select_lists_children_without_the_excluded_one(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_categories(1644, &[(1649, "HUDs"), (1648, "Icons")]);
    server.mount_gb_index(
        1644,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(1, 1644, Some(1648))], true),
    );
    server.mount_gb_index(
        1648,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(2, 1644, Some(1648))], true),
    );
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "guis");
    settled(cx, &tab, "the first step");
    support::wait_real(cx, "the subcategories", |cx| {
        cx.update(|cx| tab.read(cx).categories().len() == 1)
    });
    support::click_id(cx, w.win, ids::ADDON_CATEGORY);
    assert!(support::present(
        cx,
        w.win,
        ids::select_option(ids::ADDON_CATEGORY, "1648")
    ));
    assert!(!support::present(
        cx,
        w.win,
        ids::select_option(ids::ADDON_CATEGORY, "1649")
    ));
    support::click_id(cx, w.win, ids::select_option(ids::ADDON_CATEGORY, "1648"));
    support::wait_real(cx, "the icons step", |cx| {
        cx.update(|cx| tab.read(cx).cards().first().map(|c| c.id.get()) == Some(2))
    });
    assert_eq!(cx.update(|cx| tab.read(cx).category()), Some(1648));
    assert!(
        !support::present(cx, w.win, ids::ADDON_WEAPON),
        "only Skins classes get a weapon Select"
    );
}

#[gpui_kit::test]
fn choosing_a_class_adds_the_weapon_select(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_categories(7951, &[(7962, "The Scout"), (27900, "Packs")]);
    server.mount_gb_categories(7962, &[(7972, "Scattergun")]);
    for (category, id) in [(7951, 1), (7962, 2), (7972, 3), (27900, 4)] {
        server.mount_gb_index(
            category,
            MD,
            1,
            support::gb_listing(vec![support::gb_addon(id, 7951, None)], true),
        );
    }
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "skins");
    settled(cx, &tab, "the first step");
    support::wait_real(cx, "the classes", |cx| {
        cx.update(|cx| tab.read(cx).categories().len() == 2)
    });
    assert!(!support::present(cx, w.win, ids::ADDON_WEAPON));
    support::click_id(cx, w.win, ids::ADDON_CATEGORY);
    support::click_id(cx, w.win, ids::select_option(ids::ADDON_CATEGORY, "7962"));
    support::wait_real(cx, "the weapon list", |cx| {
        cx.update(|cx| tab.read(cx).weapons().len() == 1)
            && support::present(cx, w.win, ids::ADDON_WEAPON)
    });
    support::click_id(cx, w.win, ids::ADDON_WEAPON);
    support::click_id(cx, w.win, ids::select_option(ids::ADDON_WEAPON, "7972"));
    support::wait_real(cx, "the scattergun step", |cx| {
        cx.update(|cx| tab.read(cx).cards().first().map(|c| c.id.get()) == Some(3))
    });
    assert_eq!(
        server
            .gb_index_requests()
            .last()
            .map(|r| r.0.clone())
            .as_deref(),
        Some("7972")
    );
    support::click_id(cx, w.win, ids::ADDON_CATEGORY);
    support::click_id(cx, w.win, ids::select_option(ids::ADDON_CATEGORY, "27900"));
    support::wait_real(cx, "the packs step", |cx| {
        cx.update(|cx| {
            tab.read(cx).weapon().is_none()
                && tab.read(cx).cards().first().map(|c| c.id.get()) == Some(4)
        })
    });
    assert!(
        !support::present(cx, w.win, ids::ADDON_WEAPON),
        "Packs is not a class"
    );
}

#[gpui_kit::test]
fn search_mode_filters_to_the_tab_and_disables_the_selects(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_categories(7951, &[(7962, "The Scout")]);
    server.mount_gb_index(
        7951,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(9, 7951, None)], true),
    );
    let found = vec![
        support::gb_addon(1, 7951, Some(7962)),
        support::gb_addon(2, 1090, Some(1094)),
        support::gb_addon(3, 7951, None),
    ];
    server.mount_gb_search(1, support::gb_listing(found, true));
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "skins");
    settled(cx, &tab, "the first step");
    assert!(cx.update(|cx| tab.read(cx).selects_enabled()));
    cx.update(|cx| tab.update(cx, |t, cx| t.search_typed("pink".into(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the search", |cx| {
        cx.update(|cx| {
            tab.read(cx)
                .cards()
                .iter()
                .map(|c| c.id.get())
                .collect::<Vec<_>>()
                == [1, 3]
        })
    });
    assert!(!cx.update(|cx| tab.read(cx).selects_enabled()));
    assert_eq!(server.gb_search_count(), 1);
    cx.update(|cx| tab.update(cx, |t, cx| t.search_typed(String::new(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the listing again", |cx| {
        cx.update(|cx| tab.read(cx).cards().first().map(|c| c.id.get()) == Some(9))
    });
    assert!(cx.update(|cx| tab.read(cx).selects_enabled()));
}

#[gpui_kit::test]
fn a_listing_that_ends_with_nothing_says_no_results(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        1644,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(1, 1644, Some(1649))], true),
    );
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "guis");
    settled(cx, &tab, "the end");
    assert!(support::present(cx, w.win, ids::ADDON_EMPTY));
    assert!(!support::present(cx, w.win, ids::ADDON_LOAD_MORE));
}
