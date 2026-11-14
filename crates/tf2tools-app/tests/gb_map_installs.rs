//! Installing, updating and removing GameBanana maps through the shared install runner.

use std::path::PathBuf;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::gb_addon_installs::GbAddonInstalls;
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
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

fn run(
    cx: &mut TestAppContext,
    w: &support::AddonStoreWindow,
    f: impl FnOnce(
        &mut GbAddonInstalls,
        gpui_kit::AnyWindowHandle,
        &mut gpui_kit::Context<GbAddonInstalls>,
    ) -> bool,
) {
    let (i, win) = (installs(cx), w.win);
    assert!(
        cx.update(|cx| i.update(cx, |i, cx| f(i, win, cx))),
        "the operation starts"
    );
}

#[gpui_kit::test]
fn installing_while_tf2_runs_goes_ahead_and_says_restart(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(40, "Alpha", &[("cp_alpha.bsp", bsp(b"a"), 100)]);
    let w = window(cx, &env, &server);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    install(cx, &w, 40);
    toast_is(cx, w.win, "Installed. Restart TF2 to play it. Alpha");
    assert!(
        folder(&env, 40, "alpha")
            .join("maps/cp_alpha.bsp")
            .is_file()
    );
}

#[gpui_kit::test]
fn skipped_extra_files_are_named(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let body = support::zip_of(&[
        ("maps/cp_alpha.bsp", &bsp(b"a")),
        ("resource/ui/x.res", b"res"),
    ]);
    server.mount_gb_addon(41, "Alpha", &[("alpha.zip", body, 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 41);
    toast_is(
        cx,
        w.win,
        "Map installed. Alpha. Some extra files in this download were skipped.",
    );
    assert!(!folder(&env, 41, "alpha").join("resource").exists());
}

#[gpui_kit::test]
fn an_update_waits_for_tf2_then_replaces_the_maps_in_place(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(34, "Alpha", &[("cp_alpha.bsp", bsp(b"old"), 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 34);
    idle(cx);
    server.reset();
    let body = support::zip_of(&[("cp_alpha.bsp", &bsp(b"new"))]);
    server.mount_gb_addon(34, "Alpha", &[("cp_alpha_v2.zip", body, 300)]);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    run(cx, &w, |i, win, cx| i.update(GbId::new(34), win, cx));
    toast_is(
        cx,
        w.win,
        "Close TF2 first. TF2 is running. Close the game, then try again.",
    );
    idle(cx);
    cx.update(|cx| Tf2Probe::install(cx, || false));
    run(cx, &w, |i, win, cx| i.update(GbId::new(34), win, cx));
    toast_is(cx, w.win, "Map updated. Alpha");
    idle(cx);
    let path = folder(&env, 34, "alpha").join("maps/cp_alpha.bsp");
    assert_eq!(std::fs::read(path).unwrap(), bsp(b"new"));
    assert_eq!(entry(cx, 34).unwrap().file_date(), 300);
}

#[gpui_kit::test]
fn remove_waits_for_tf2_then_deletes_the_folder(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(35, "Alpha", &[("cp_alpha.bsp", bsp(b"a"), 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 35);
    idle(cx);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    run(cx, &w, |i, win, cx| i.remove(GbId::new(35), win, cx));
    toast_is(
        cx,
        w.win,
        "Close TF2 first. TF2 is running. Close the game, then try again.",
    );
    idle(cx);
    assert!(folder(&env, 35, "alpha").is_dir());
    cx.update(|cx| Tf2Probe::install(cx, || false));
    run(cx, &w, |i, win, cx| i.remove(GbId::new(35), win, cx));
    toast_is(cx, w.win, "Map removed. Alpha");
    idle(cx);
    assert!(!folder(&env, 35, "alpha").exists());
    assert!(entry(cx, 35).is_none());
}

#[gpui_kit::test]
fn install_again_downloads_even_while_tf2_runs_and_says_restart(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(36, "Alpha", &[("cp_alpha.bsp", bsp(b"a"), 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 36);
    idle(cx);
    std::fs::remove_dir_all(folder(&env, 36, "alpha")).unwrap();
    let i = installs(cx);
    cx.update(|cx| i.update(cx, |i, cx| i.reload(cx)));
    support::wait_real(cx, "not installed", |cx| {
        cx.update(|cx| {
            i.read(cx)
                .entry(GbId::new(36))
                .is_some_and(|e| !e.is_installed())
        })
    });
    cx.update(|cx| Tf2Probe::install(cx, || true));
    run(cx, &w, |i, win, cx| i.reinstall(GbId::new(36), win, cx));
    toast_is(cx, w.win, "Installed. Restart TF2 to play it. Alpha");
    assert!(
        folder(&env, 36, "alpha")
            .join("maps/cp_alpha.bsp")
            .is_file()
    );
}

// Remove reads only the library row, so a map GameBanana no longer serves still goes.
#[gpui_kit::test]
fn remove_works_when_gamebanana_no_longer_has_the_map(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(38, "Alpha", &[("cp_alpha.bsp", bsp(b"a"), 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 38);
    idle(cx);
    server.reset();
    run(cx, &w, |i, win, cx| i.remove(GbId::new(38), win, cx));
    toast_is(cx, w.win, "Map removed. Alpha");
    idle(cx);
    assert!(!folder(&env, 38, "alpha").exists());
    assert!(entry(cx, 38).is_none());
}

#[gpui_kit::test]
fn install_again_with_tf2_closed_reads_as_a_map(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(39, "Alpha", &[("cp_alpha.bsp", bsp(b"a"), 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 39);
    idle(cx);
    std::fs::remove_dir_all(folder(&env, 39, "alpha")).unwrap();
    run(cx, &w, |i, win, cx| i.reinstall(GbId::new(39), win, cx));
    idle(cx);
    // The first install's toast is still up, so Install again's is the second of its kind.
    let labels = support::toast_labels(cx, w.win);
    let maps = labels
        .iter()
        .filter(|l| *l == "Map installed. Alpha")
        .count();
    assert_eq!(maps, 2, "{labels:?}");
    assert!(
        folder(&env, 39, "alpha")
            .join("maps/cp_alpha.bsp")
            .is_file()
    );
}

#[gpui_kit::test]
fn install_again_of_a_gone_map_says_remove_it(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(42, "Alpha", &[("cp_alpha.bsp", bsp(b"a"), 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 42);
    idle(cx);
    std::fs::remove_dir_all(folder(&env, 42, "alpha")).unwrap();
    server.reset();
    server.mount_gb_profile(
        42,
        serde_json::json!({"_idRow": 42, "_sName": "Alpha", "_bIsTrashed": true}),
    );
    run(cx, &w, |i, win, cx| i.reinstall(GbId::new(42), win, cx));
    toast_is(
        cx,
        w.win,
        "Couldn't install the map. This map is no longer on GameBanana, so it can't be installed again. Remove it.",
    );
    idle(cx);
    assert!(entry(cx, 42).unwrap().is_gone());
}
