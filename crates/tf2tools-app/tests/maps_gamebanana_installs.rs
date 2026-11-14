//! The Maps page's GameBanana tab with a library: badges, the update check and the Installed view.

use gpui_kit::{AnyWindowHandle, TestAppContext};
use tf2tools_app::ids;
use tf2tools_core::GbId;

use crate::support;
use crate::support::maps::{
    maps_shell_found, open_gamebanana, open_maps, seed_map_library, seed_maps,
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
