use crate::support;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::mature::MatureAddons;
use tf2tools_app::pages::addon_store::AddonStoreTab;
use tf2tools_app::pages::addon_store::gamebanana_tab::{AddonTab, search_placeholder};
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
    assert!(cx.update(|cx| tab.read(cx).cards().iter().any(|c| c.id == GbId::new(51))));
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
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Try another category.")
    );
}

// Search mode disables the category, so its empty hint speaks only of the search.
#[gpui_kit::test]
fn a_search_with_nothing_found_says_try_another_search(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        1644,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(1, 1644, None)], true),
    );
    server.mount_gb_search(1, support::gb_listing(Vec::new(), true));
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "guis");
    settled(cx, &tab, "the first step");
    cx.update(|cx| tab.update(cx, |t, cx| t.search_typed("zzz".into(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the empty search", |cx| {
        cx.update(|cx| tab.read(cx).is_no_results())
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Try another search.")
    );
}

#[gpui_kit::test]
fn mature_items_hide_until_shown_then_blur_until_clicked(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let mut warned = support::gb_addon(2, 587, None);
    warned["_sInitialVisibility"] = "warn".into();
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(1, 587, None), warned], true),
    );
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    let ids_now = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            tab.read(cx)
                .cards()
                .iter()
                .map(|c| c.id.get())
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(ids_now(cx), [1]);
    cx.update(|cx| {
        let m = MatureAddons::global(cx);
        m.update(cx, |m, cx| m.set(true, cx))
    });
    support::wait_real(cx, "the mature card", |cx| ids_now(cx) == [1, 2]);
    assert!(support::present(
        cx,
        w.win,
        ids::addon_gb_card_reveal("textures", GbId::new(2))
    ));
    assert!(!support::present(
        cx,
        w.win,
        ids::addon_gb_card_reveal("textures", GbId::new(1))
    ));
    support::click_id(
        cx,
        w.win,
        ids::addon_gb_card_reveal("textures", GbId::new(2)),
    );
    assert!(cx.update(|cx| tab.read(cx).is_revealed(GbId::new(2))));
    assert!(!support::present(
        cx,
        w.win,
        ids::addon_gb_card_reveal("textures", GbId::new(2))
    ));
    assert!(
        !support::present(cx, w.win, ids::ADDON_DETAIL),
        "revealing does not open the detail"
    );
    assert_eq!(
        server.gb_index_requests().len(),
        1,
        "the toggle reread the cached page"
    );
    support::wait_real(cx, "the save", |_| {
        w.services.db.settings().unwrap().show_mature_addons()
    });
    let set_mature = |cx: &mut TestAppContext, show: bool| {
        cx.update(|cx| {
            let m = MatureAddons::global(cx);
            m.update(cx, |m, cx| m.set(show, cx))
        })
    };
    set_mature(cx, false);
    support::wait_real(cx, "the mature card gone", |cx| ids_now(cx) == [1]);
    set_mature(cx, true);
    support::wait_real(cx, "the mature card back", |cx| ids_now(cx) == [1, 2]);
    assert!(
        !cx.update(|cx| tab.read(cx).is_revealed(GbId::new(2))),
        "a flip of the setting blurs revealed cards again"
    );
    assert!(support::present(
        cx,
        w.win,
        ids::addon_gb_card_reveal("textures", GbId::new(2))
    ));
}

#[gpui_kit::test]
fn a_card_opens_its_detail_with_downloads_credits_files_and_ratings(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(8, 587, None)], true),
    );
    server.mount_gb_profile(8, serde_json::json!({
        "_idRow": 8, "_sName": "Addon 8", "_sText": "<p>Hello</p>", "_nDownloadCount": 1234, "_bIsObsolete": true,
        "_aSubmitter": {"_sName": "vex"}, "_sProfileUrl": "https://gamebanana.com/mods/8",
        "_aContentRatings": {"sa": "Skimpy Attire"},
        "_aCredits": [{"_sGroupName": "Key Authors", "_aAuthors": [{"_sName": "vex", "_sRole": "Modeller"}]}],
        "_aFiles": [{"_idRow": 2, "_sFile": "new.zip", "_nFilesize": 2048, "_tsDateAdded": 200, "_sAvResult": "clean", "_sDownloadUrl": "https://gamebanana.com/dl/2"}]
    }));
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    support::click_id(cx, w.win, ids::addon_gb_card("textures", GbId::new(8)));
    support::wait_real(cx, "the profile", |cx| {
        support::present(cx, w.win, ids::ADDON_DETAIL_DOWNLOADS)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_NAME).as_deref(),
        Some("Addon 8")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_DOWNLOADS).as_deref(),
        Some("1,234 downloads")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_RATINGS).as_deref(),
        Some("Mature content: Skimpy Attire")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_DESCRIPTION).as_deref(),
        Some("Hello")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::addon_detail_credit(0, 0)).as_deref(),
        Some("vex (Modeller)")
    );
    assert!(
        support::label_of(cx, w.win, ids::addon_detail_file(0))
            .unwrap()
            .starts_with("new.zip")
    );
    assert!(support::present(cx, w.win, ids::ADDON_DETAIL_OUTDATED));
    assert!(support::present(
        cx,
        w.win,
        ids::store_detail_link("gamebanana")
    ));
}

#[gpui_kit::test]
fn a_trashed_item_says_it_is_gone_and_keeps_its_link(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(5, 587, None)], true),
    );
    server.mount_gb_profile(
        5,
        serde_json::json!({"_idRow": 5, "_sName": "x", "_bIsTrashed": true}),
    );
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    support::click_id(cx, w.win, ids::addon_gb_card("textures", GbId::new(5)));
    support::wait_real(cx, "the failure", |cx| {
        support::present(cx, w.win, ids::ADDON_DETAIL_ERROR)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_ERROR).as_deref(),
        Some("This item is no longer on GameBanana.")
    );
    assert!(!support::present(cx, w.win, ids::ADDON_DETAIL_RETRY));
    assert!(support::present(
        cx,
        w.win,
        ids::store_detail_link("gamebanana")
    ));
}

#[gpui_kit::test]
fn a_failed_step_shows_retry_and_retry_loads(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_status("/apiv11/Mod/Index", 503);
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "skins");
    settled(cx, &tab, "the failure");
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_ERROR).as_deref(),
        Some("GameBanana is having problems right now. Try again later.")
    );
    server.reset();
    server.mount_gb_index(
        7951,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(1, 7951, None)], true),
    );
    support::click_id(cx, w.win, ids::STORE_RETRY);
    support::wait_real(cx, "the retry", |cx| {
        cx.update(|cx| tab.read(cx).cards().len() == 1)
    });
    assert!(!support::present(cx, w.win, ids::STORE_ERROR));
}

#[gpui_kit::test]
fn the_sort_select_is_labelled_and_shows_its_longest_label_whole(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(1, 587, None)], true),
    );
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_SORT_LABEL).as_deref(),
        Some("Sort:")
    );
    let sort = support::bounds_of(cx, w.win, ids::ADDON_SORT).unwrap();
    let longest = label_width(cx, w.win, "Most downloaded");
    assert!(
        longest + SELECT_CHROME <= sort.size.width,
        "{longest:?} of label in a {:?} select",
        sort.size.width
    );
}

// The kit's medium Select trigger around its title: two 1 px borders, 10 px padding a side,
// a 4 px gap and the 16 px caret.
const SELECT_CHROME: gpui_kit::Pixels = gpui_kit::px(42.);

// `text` shaped at the Select's text-sm in the theme font, as the window lays it out.
fn label_width(
    cx: &mut TestAppContext,
    win: gpui_kit::AnyWindowHandle,
    text: &str,
) -> gpui_kit::Pixels {
    use gpui_kit::AppContext as _;
    use gpui_kit::component::ActiveTheme as _;
    cx.update_window(win, |_, w, cx| {
        let run = gpui_kit::TextRun {
            len: text.len(),
            font: gpui_kit::font(cx.theme().font_family.clone()),
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
fn a_class_left_before_its_weapons_arrive_never_shows_them(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_categories(7951, &[(7962, "The Scout"), (7960, "The Soldier")]);
    let slow = std::time::Duration::from_millis(400);
    server.mount_gb_categories_after(7962, &[(7972, "Scattergun")], slow);
    server.mount_gb_categories(7960, &[(7980, "Rocket Launcher")]);
    for category in [7951, 7962, 7960] {
        server.mount_gb_index(
            category,
            MD,
            1,
            support::gb_listing(vec![support::gb_addon(1, 7951, None)], true),
        );
    }
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "skins");
    settled(cx, &tab, "the first step");
    support::wait_real(cx, "the classes", |cx| {
        cx.update(|cx| tab.read(cx).categories().len() == 2)
    });
    let weapons = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            tab.read(cx)
                .weapons()
                .iter()
                .map(|c| c.id())
                .collect::<Vec<_>>()
        })
    };
    support::click_id(cx, w.win, ids::ADDON_CATEGORY);
    support::click_id(cx, w.win, ids::select_option(ids::ADDON_CATEGORY, "7962"));
    support::click_id(cx, w.win, ids::ADDON_CATEGORY);
    support::click_id(cx, w.win, ids::select_option(ids::ADDON_CATEGORY, "7960"));
    support::wait_real(cx, "the soldier's weapons", |cx| weapons(cx) == [7980]);
    let started = std::time::Instant::now();
    support::wait_real(cx, "the scout's late reply", |_| {
        started.elapsed() > slow * 2
    });
    assert_eq!(weapons(cx), [7980], "the scout's weapons never show");
    assert_eq!(cx.update(|cx| tab.read(cx).category()), Some(7960));
}

#[gpui_kit::test]
fn a_step_that_succeeds_after_a_failure_clears_the_error(cx: &mut TestAppContext) {
    // Pages 1 to 4 fill the first step; page 5 is missing, so the step the grid asks for next
    // fails. Shown again, the tab's grid asks once more without the player pressing Retry.
    let (env, server) = (support::env(), support::StoreServer::start());
    for page in 1..=4u32 {
        let first = u64::from(page) * 10;
        let records = (first..first + 2)
            .map(|i| support::gb_addon(i, 587, None))
            .collect();
        server.mount_gb_index(587, MD, page, support::gb_listing(records, false));
    }
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "textures");
    support::wait_real(cx, "the failed second step", |cx| {
        cx.update(|cx| tab.read(cx).error().is_some() && !tab.read(cx).is_loading())
    });
    server.mount_gb_index(
        587,
        MD,
        5,
        support::gb_listing(vec![support::gb_addon(50, 587, None)], true),
    );
    support::click_id(cx, w.win, ids::addon_store_tab("mastercomfig"));
    support::click_id(cx, w.win, ids::addon_store_tab("textures"));
    support::wait_real(cx, "the second step", |cx| {
        cx.update(|cx| tab.read(cx).cards().len() == 9 && !tab.read(cx).is_loading())
    });
    assert!(cx.update(|cx| tab.read(cx).error().is_none()));
    assert!(!support::present(cx, w.win, ids::STORE_ERROR));
}

#[gpui_kit::test]
fn the_detail_retries_a_failed_profile(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(8, 587, None)], true),
    );
    server.mount_status("/apiv11/Mod/8/ProfilePage", 503);
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    support::click_id(cx, w.win, ids::addon_gb_card("textures", GbId::new(8)));
    support::wait_real(cx, "the failure", |cx| {
        support::present(cx, w.win, ids::ADDON_DETAIL_ERROR)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_ERROR).as_deref(),
        Some("GameBanana is having problems right now. Try again later.")
    );
    server.reset();
    server.mount_gb_profile(
        8,
        serde_json::json!({"_idRow": 8, "_sName": "Addon 8", "_nDownloadCount": 3}),
    );
    support::click_id(cx, w.win, ids::ADDON_DETAIL_RETRY);
    support::wait_real(cx, "the profile", |cx| {
        support::present(cx, w.win, ids::ADDON_DETAIL_DOWNLOADS)
    });
    assert!(!support::present(cx, w.win, ids::ADDON_DETAIL_ERROR));
}

#[gpui_kit::test]
fn an_off_site_profile_link_opens_the_items_gamebanana_page(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let mut item = support::gb_addon(8, 587, None);
    item["_sProfileUrl"] = "https://gamebanana.com.evil.example/mods/8".into();
    server.mount_gb_index(587, MD, 1, support::gb_listing(vec![item], true));
    server.mount_gb_profile(8, serde_json::json!({"_idRow": 8, "_sName": "Addon 8"}));
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    support::click_id(cx, w.win, ids::addon_gb_card("textures", GbId::new(8)));
    support::wait_real(cx, "the profile", |cx| {
        support::present(cx, w.win, ids::ADDON_DETAIL_DOWNLOADS)
    });
    support::click_id(cx, w.win, ids::store_detail_link("gamebanana"));
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://gamebanana.com/mods/8")
    );
}

#[test]
fn the_search_placeholder_names_the_tab_as_its_title_does() {
    let placeholder = |key| search_placeholder(AddonSection::by_key(key).unwrap());
    assert_eq!(placeholder("guis"), "Search GUIs...");
    assert_eq!(placeholder("skins"), "Search Skins...");
    assert_eq!(placeholder("gamefiles"), "Search Game files...");
}

// At the shell's content width in an 800 px window (the app's minimum), every tab shows whole
// inside the page's padding instead of running past the grid.
#[gpui_kit::test]
fn every_tab_fits_a_narrow_page(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let w = window(cx, &env, &server);
    let width = gpui_kit::px(544.);
    cx.simulate_window_resize(w.win, gpui_kit::size(width, gpui_kit::px(600.)));
    let padding = tf2tools_app::shell::CONTENT_PADDING;
    let keys = std::iter::once("mastercomfig").chain(AddonSection::ALL.iter().map(|s| s.key()));
    for key in keys {
        let tab = support::bounds_of(cx, w.win, ids::addon_store_tab(key)).unwrap();
        assert!(
            tab.left() >= padding && tab.right() <= width - padding,
            "tab {key} spans {:?}..{:?}",
            tab.left(),
            tab.right()
        );
    }
}

// A long add-on name wraps beside "Open on GameBanana" instead of running under it.
#[gpui_kit::test]
fn a_long_addon_name_wraps_clear_of_the_header_button(cx: &mut TestAppContext) {
    let name = "Paysus' Engineer First Person Animation Overhaul Deluxe Edition";
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(8, 587, None)], true),
    );
    server.mount_gb_profile(
        8,
        serde_json::json!({
            "_idRow": 8, "_sName": name, "_sText": "", "_nDownloadCount": 1,
            "_aSubmitter": {"_sName": "vex"}, "_sProfileUrl": "https://gamebanana.com/mods/8"
        }),
    );
    let w = window(cx, &env, &server);
    cx.simulate_window_resize(
        w.win,
        gpui_kit::size(gpui_kit::px(544.), gpui_kit::px(600.)),
    );
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    support::click_id(cx, w.win, ids::addon_gb_card("textures", GbId::new(8)));
    support::wait_real(cx, "the profile", |cx| {
        support::label_of(cx, w.win, ids::ADDON_DETAIL_NAME).as_deref() == Some(name)
    });
    let title = support::bounds_of(cx, w.win, ids::ADDON_DETAIL_NAME).unwrap();
    let link = support::bounds_of(cx, w.win, ids::store_detail_link("gamebanana")).unwrap();
    assert!(
        title.right() <= link.left(),
        "title {title:?} runs under {link:?}"
    );
    assert!(
        title.size.height > link.size.height,
        "the title should wrap: {title:?}"
    );
}

fn pin_tf2_idle(cx: &mut TestAppContext) {
    cx.update(|cx| tf2tools_app::pages::profiles::probe::Tf2Probe::install(cx, || false));
}

// The control is disabled until the profile is read, so a click waits for both.
fn detail_ready(cx: &mut TestAppContext, w: &support::AddonStoreWindow, label: &str) {
    support::wait_real(cx, "the read profile", |cx| {
        support::present(cx, w.win, ids::ADDON_DETAIL_DOWNLOADS)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_INSTALL).as_deref(),
        Some(label),
        "the control once the profile is read"
    );
}

fn addon_vpk() -> Vec<u8> {
    support::vpk_bytes(&[("materials/a.vmt", b"v")])
}

#[gpui_kit::test]
fn install_from_the_detail_then_the_card_says_installed(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(8, 587, None)], true),
    );
    server.mount_gb_addon(8, "Addon 8", &[("a.vpk", addon_vpk(), 100)]);
    let w = window(cx, &env, &server);
    pin_tf2_idle(cx);
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    assert!(!support::present(
        cx,
        w.win,
        ids::addon_gb_card_badge("textures", GbId::new(8))
    ));
    support::click_id(cx, w.win, ids::addon_gb_card("textures", GbId::new(8)));
    detail_ready(cx, &w, "Install");
    support::click_id(cx, w.win, ids::ADDON_DETAIL_INSTALL);
    support::wait_real(cx, "installed", |cx| {
        support::label_of(cx, w.win, ids::ADDON_DETAIL_INSTALL).as_deref() == Some("Installed")
    });
    assert!(
        env.game
            .join("tf/custom/tf2tools-gb-8-addon-8.vpk")
            .is_file()
    );
    assert_eq!(
        support::label_of(
            cx,
            w.win,
            ids::addon_gb_card_badge("textures", GbId::new(8))
        )
        .as_deref(),
        Some("Installed")
    );
}

#[gpui_kit::test]
fn a_file_that_failed_the_scan_shows_why_instead_of_install(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(9, 587, None)], true),
    );
    server.mount_gb_profile(9, serde_json::json!({"_idRow": 9, "_sName": "Addon 9", "_aFiles": [
        {"_idRow": 1, "_sFile": "a.zip", "_nFilesize": 10, "_tsDateAdded": 100, "_sAvResult": "infected",
         "_sDownloadUrl": "https://gamebanana.com/dl/1"}]}));
    let w = window(cx, &env, &server);
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    support::click_id(cx, w.win, ids::addon_gb_card("textures", GbId::new(9)));
    support::wait_real(cx, "the reason", |cx| {
        support::present(cx, w.win, ids::ADDON_DETAIL_BLOCKED)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_BLOCKED).as_deref(),
        Some("GameBanana's virus scan didn't pass this file, so it wasn't installed.")
    );
    assert!(!support::present(cx, w.win, ids::ADDON_DETAIL_INSTALL));
    assert!(support::present(
        cx,
        w.win,
        ids::store_detail_link("gamebanana")
    ));
}

#[gpui_kit::test]
fn a_newer_file_shows_update_on_the_card_and_in_the_detail(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(10, 587, None)], true),
    );
    server.mount_gb_addon(10, "Addon 10", &[("a.vpk", addon_vpk(), 100)]);
    let w = window(cx, &env, &server);
    pin_tf2_idle(cx);
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    support::click_id(cx, w.win, ids::addon_gb_card("textures", GbId::new(10)));
    detail_ready(cx, &w, "Install");
    support::click_id(cx, w.win, ids::ADDON_DETAIL_INSTALL);
    support::wait_real(cx, "installed", |cx| {
        support::label_of(cx, w.win, ids::ADDON_DETAIL_INSTALL).as_deref() == Some("Installed")
    });
    // A newer file recorded by the daily check.
    w.services
        .library
        .record_gb_addon_check(
            GbId::new(10),
            tf2tools_core::GbAddonCheck::Latest { file_date: 300 },
            1,
        )
        .unwrap();
    let installs = cx.update(tf2tools_app::gb_addon_installs::GbAddonInstalls::global);
    cx.update(|cx| installs.update(cx, |i, cx| i.reload(cx)));
    support::wait_real(cx, "the update badge", |cx| {
        support::label_of(
            cx,
            w.win,
            ids::addon_gb_card_badge("textures", GbId::new(10)),
        )
        .as_deref()
            == Some("Update")
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_INSTALL).as_deref(),
        Some("Update")
    );
}

#[gpui_kit::test]
fn an_unreadable_profile_still_offers_the_installed_addon(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(11, "Addon 11", &[("a.vpk", addon_vpk(), 100)]);
    let w = window(cx, &env, &server);
    pin_tf2_idle(cx);
    let installs = cx.update(tf2tools_app::gb_addon_installs::GbAddonInstalls::global);
    let profile = crate::gb_addon_installs::profile(cx, 11);
    let (win, section) = (w.win, AddonSection::by_key("textures").unwrap());
    assert!(cx.update(|cx| installs.update(cx, |i, cx| i.install(section, profile, win, cx))));
    support::wait_real(cx, "the install", |cx| {
        cx.update(|cx| {
            installs
                .read(cx)
                .entry(GbId::new(11))
                .is_some_and(|e| e.is_installed())
        })
    });
    // GameBanana is down now; the library still knows the add-on.
    server.reset();
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(11, 587, None)], true),
    );
    server.mount_status("/apiv11/Mod/11/ProfilePage", 503);
    let tab = open_tab(cx, &w, "textures");
    settled(cx, &tab, "the first step");
    support::click_id(cx, w.win, ids::addon_gb_card("textures", GbId::new(11)));
    support::wait_real(cx, "the profile failure", |cx| {
        support::present(cx, w.win, ids::ADDON_DETAIL_ERROR)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_INSTALL).as_deref(),
        Some("Installed")
    );

    let vpk = env.game.join("tf/custom/tf2tools-gb-11-addon-11.vpk");
    std::fs::remove_file(&vpk).unwrap();
    cx.update(|cx| installs.update(cx, |i, cx| i.reload(cx)));
    support::wait_real(cx, "install again", |cx| {
        support::label_of(cx, w.win, ids::ADDON_DETAIL_INSTALL).as_deref() == Some("Install again")
    });
    support::click_id(cx, w.win, ids::ADDON_DETAIL_INSTALL);
    support::wait_real(cx, "installed again", |cx| {
        support::label_of(cx, w.win, ids::ADDON_DETAIL_INSTALL).as_deref() == Some("Installed")
    });
    assert!(vpk.is_file());
}
