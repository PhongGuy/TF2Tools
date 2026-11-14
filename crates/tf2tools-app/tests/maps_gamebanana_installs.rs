//! The Maps page's GameBanana tab with a library: badges, the update check and the Installed view.

use gpui_kit::{AnyWindowHandle, TestAppContext};
use tf2tools_app::gb_addon_installs::GbAddonInstalls;
use tf2tools_app::ids;
use tf2tools_core::{AddonSection, GbId, GbMapShow};

use crate::support;
use crate::support::maps::{
    maps_shell_found, open_gamebanana, open_maps, seed_map_library, seed_maps, shown,
};

fn bsp(tag: &[u8]) -> Vec<u8> {
    let mut body = b"VBSP".to_vec();
    body.extend_from_slice(tag);
    body
}
fn badge(cx: &mut TestAppContext, win: AnyWindowHandle, id: u64) -> Option<String> {
    support::label_of(cx, win, ids::addon_gb_card_badge("maps", GbId::new(id)))
}

#[gpui_kit::test]
fn cards_carry_the_librarys_state(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    seed_map_library(&env);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    support::wait_real(cx, "the badges", |cx| {
        badge(cx, s.win, 10).as_deref() == Some("Installed")
    });
    assert_eq!(badge(cx, s.win, 11).as_deref(), Some("Update"));
    assert_eq!(
        badge(cx, s.win, 12).as_deref(),
        Some("Not installed"),
        "a missing folder beats an update"
    );
    assert_eq!(
        server.requests_where(|p| p.starts_with("/apiv11")),
        0,
        "nothing was due"
    );
}

#[gpui_kit::test]
fn opening_the_tab_checks_for_updates(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(10, "cp_alpha", &[("cp_alpha2.bsp", bsp(b"b"), 300)]);
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    seed_map_library(&env);
    support::sql(&env)
        .execute(
            "UPDATE library_gb_addons SET checked_at = 0 WHERE gb_id = 10",
            [],
        )
        .unwrap();
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    support::wait_real(cx, "the update", |cx| {
        badge(cx, s.win, 10).as_deref() == Some("Update")
    });
}

#[gpui_kit::test]
fn an_install_badges_its_card(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(12, "koth_beta", &[("koth_beta.bsp", bsp(b"k"), 100)]);
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    assert_eq!(badge(cx, s.win, 12), None);
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(12)));
    support::wait_real(cx, "Install", |cx| {
        support::present(cx, s.win, ids::ADDON_DETAIL_DOWNLOADS)
            && support::label_of(cx, s.win, ids::ADDON_DETAIL_INSTALL).as_deref() == Some("Install")
    });
    support::click_id(cx, s.win, ids::ADDON_DETAIL_INSTALL);
    support::wait_real(cx, "the badge", |cx| {
        badge(cx, s.win, 12).as_deref() == Some("Installed")
    });
}

fn show_installed(cx: &mut TestAppContext, win: AnyWindowHandle) {
    support::click_id(cx, win, ids::MAPS_GB_SHOW);
    support::click_id(cx, win, ids::select_option(ids::MAPS_GB_SHOW, "Installed"));
}

#[gpui_kit::test]
fn installed_lists_the_library_with_uncatalogued_maps_last(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    seed_map_library(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    show_installed(cx, s.win);
    support::wait_real(cx, "the library", |cx| shown(cx, &tab) == [10, 11, 12, 99]);
    assert_eq!(cx.update(|cx| tab.read(cx).show()), GbMapShow::Installed);
    assert_eq!(
        support::label_of(cx, s.win, ids::addon_gb_card_name("maps", GbId::new(99))).as_deref(),
        Some("gone_map")
    );
    support::wait_real(cx, "the gone badge", |cx| {
        badge(cx, s.win, 99).as_deref() == Some("No longer on GameBanana")
    });
}

#[gpui_kit::test]
fn installed_needs_no_catalogue(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell_found(cx, &env, &server);
    seed_map_library(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    assert!(shown(cx, &tab).is_empty());
    show_installed(cx, s.win);
    support::wait_real(cx, "the library", |cx| shown(cx, &tab) == [99, 12, 11, 10]);
}

#[gpui_kit::test]
fn an_empty_library_says_no_maps_installed(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    show_installed(cx, s.win);
    support::wait_real(cx, "the empty state", |cx| {
        support::label_of(cx, s.win, ids::MAPS_GB_EMPTY).as_deref() == Some("No maps installed")
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Install a map from All maps.")
    );
    assert!(!support::present(cx, s.win, ids::MAPS_GB_SYNC));
}

#[gpui_kit::test]
fn removing_a_gone_map_drops_its_installed_card(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    seed_map_library(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    show_installed(cx, s.win);
    support::wait_real(cx, "the library", |cx| shown(cx, &tab).contains(&99));
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(99)));
    support::wait_real(cx, "Remove", |cx| {
        support::present(cx, s.win, ids::ADDON_DETAIL_REMOVE)
    });
    support::click_id(cx, s.win, ids::ADDON_DETAIL_REMOVE);
    support::wait_real(cx, "the confirm", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DIALOG)
    });
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the card gone", |cx| shown(cx, &tab) == [10, 11, 12]);
    assert!(!env.game.join("tf/custom/tf2tools-gb-99-gone_map").exists());
}

#[gpui_kit::test]
fn install_progress_leaves_the_installed_list_alone(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(
        10,
        "cp_alpha",
        &[("cp_alpha.bsp", bsp(&[7; 256 * 1024]), 100)],
    );
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    let tab = open_gamebanana(cx, &s, &page);
    show_installed(cx, s.win);
    support::wait_real(cx, "the empty library", |cx| {
        support::label_of(cx, s.win, ids::MAPS_GB_EMPTY).as_deref() == Some("No maps installed")
    });
    let installs = cx.update(GbAddonInstalls::global);
    let before = cx.update(|cx| tab.read(cx).requeries());
    let profile = crate::gb_addon_installs::profile(cx, 10);
    let started = cx.update(|cx| {
        installs.update(cx, |i, cx| {
            i.install(AddonSection::MAPS, profile, s.win, cx)
        })
    });
    assert!(started);
    // Every stage and byte count notifies; none of them changes an entry's state.
    let mut stages = 0;
    support::wait_real(cx, "the install", |cx| {
        let busy = cx.update(|cx| installs.read(cx).is_busy());
        if busy {
            stages += 1;
            assert_eq!(cx.update(|cx| tab.read(cx).requeries()), before);
        }
        !busy
    });
    assert!(stages > 0, "the install ran while watched");
    support::wait_real(cx, "the installed card", |cx| shown(cx, &tab) == [10]);
    assert_eq!(cx.update(|cx| tab.read(cx).requeries()), before + 1);
}
