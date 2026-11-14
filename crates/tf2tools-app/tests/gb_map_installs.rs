//! Installing, updating and removing GameBanana maps through the shared install runner.

use std::path::PathBuf;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::gb_addon_installs::GbAddonInstalls;
use tf2tools_app::ids;
use tf2tools_core::{AddonSection, GbAddonEntry, GbId};

use crate::gb_addon_installs::{profile, toast_is, window};
use crate::support;

fn bsp(tag: &[u8]) -> Vec<u8> {
    let mut body = b"VBSP".to_vec();
    body.extend_from_slice(tag);
    body
}
fn installs(cx: &mut TestAppContext) -> Entity<GbAddonInstalls> {
    cx.update(GbAddonInstalls::global)
}
fn idle(cx: &mut TestAppContext) {
    let i = installs(cx);
    support::wait_real(cx, "the operation to end", |cx| {
        cx.update(|cx| !i.read(cx).is_busy())
    });
}
fn entry(cx: &mut TestAppContext, id: u64) -> Option<GbAddonEntry> {
    let i = installs(cx);
    cx.update(|cx| i.read(cx).entry(GbId::new(id)).cloned())
}
fn install(cx: &mut TestAppContext, w: &support::AddonStoreWindow, id: u64) {
    let p = profile(cx, id);
    let (i, win) = (installs(cx), w.win);
    let started = cx.update(|cx| i.update(cx, |i, cx| i.install(AddonSection::MAPS, p, win, cx)));
    assert!(started, "the install should start");
}
fn folder(env: &support::Env, id: u64, slug: &str) -> PathBuf {
    env.game
        .join("tf/custom")
        .join(format!("tf2tools-gb-{id}-{slug}"))
}
fn stock(env: &support::Env, name: &str) {
    std::fs::create_dir_all(env.game.join("tf/maps")).unwrap();
    std::fs::write(
        env.game.join("tf/maps").join(format!("{name}.bsp")),
        bsp(b"stock"),
    )
    .unwrap();
}

#[gpui_kit::test]
fn a_one_map_item_installs_without_asking(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(30, "Alpha", &[("cp_alpha.bsp", bsp(b"a"), 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 30);
    toast_is(cx, w.win, "Map installed. Alpha");
    idle(cx);
    assert!(!support::present(cx, w.win, ids::PAYLOAD_CHECKLIST));
    assert!(
        folder(&env, 30, "alpha")
            .join("maps/cp_alpha.bsp")
            .is_file()
    );
    let e = entry(cx, 30).unwrap();
    assert_eq!((e.section(), e.is_installed()), ("maps", true));
    assert_eq!(e.payloads(), ["cp_alpha"]);
}

#[gpui_kit::test]
fn a_pack_asks_with_the_maps_the_game_has_disabled(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    stock(&env, "cp_b");
    let body = support::zip_of(&[
        ("maps/cp_a.bsp", &bsp(b"a")),
        ("maps/cp_b.bsp", &bsp(b"b")),
        ("maps/cp_c.bsp", &bsp(b"c")),
    ]);
    server.mount_gb_addon(31, "Pack", &[("pack.zip", body, 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 31);
    support::wait_real(cx, "the checklist", |cx| {
        support::present(cx, w.win, ids::PAYLOAD_CHECKLIST)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::payload_check(1)).as_deref(),
        Some("cp_b · Already in the game")
    );
    support::click_id(cx, w.win, ids::PAYLOAD_INSTALL);
    toast_is(cx, w.win, "Map installed. Pack");
    let maps = folder(&env, 31, "pack").join("maps");
    assert!(maps.join("cp_a.bsp").is_file() && maps.join("cp_c.bsp").is_file());
    assert!(!maps.join("cp_b.bsp").exists());
}

#[gpui_kit::test]
fn a_pack_the_game_has_whole_skips_the_checklist_and_names_serveme(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    stock(&env, "cp_a");
    stock(&env, "cp_b");
    let body = support::zip_of(&[("maps/cp_a.bsp", &bsp(b"a")), ("maps/cp_b.bsp", &bsp(b"b"))]);
    server.mount_gb_addon(32, "Pack", &[("pack.zip", body, 100)]);
    let w = window(cx, &env, &server);
    support::sql(&env)
        .execute(
            "INSERT INTO downloaded_maps (name, size, downloaded_at) VALUES ('cp_a', 4, 'x')",
            [],
        )
        .unwrap();
    install(cx, &w, 32);
    toast_is(
        cx,
        w.win,
        "Couldn't install the map. You already have cp_a and cp_b. Remove it on the serveme.tf tab to use GameBanana's version.",
    );
    idle(cx);
    assert!(!support::present(cx, w.win, ids::PAYLOAD_CHECKLIST));
    assert!(entry(cx, 32).is_none());
}

#[gpui_kit::test]
fn a_download_without_a_map_says_so(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(
        37,
        "Notes",
        &[("notes.zip", support::zip_of(&[("readme.txt", b"r")]), 100)],
    );
    let w = window(cx, &env, &server);
    install(cx, &w, 37);
    toast_is(
        cx,
        w.win,
        "Couldn't install the map. No playable map was found in this download.",
    );
}
