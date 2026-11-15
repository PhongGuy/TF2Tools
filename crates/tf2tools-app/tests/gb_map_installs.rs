//! Installing, updating and removing GameBanana maps through the shared install runner.

use std::path::PathBuf;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
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
fn a_pack_update_offers_every_map_ticked_including_the_ones_it_already_holds(
    cx: &mut TestAppContext,
) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let old = support::zip_of(&[
        ("maps/cp_a.bsp", &bsp(b"a1")),
        ("maps/cp_b.bsp", &bsp(b"b1")),
    ]);
    server.mount_gb_addon(38, "Pack", &[("pack.zip", old, 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 38);
    support::wait_real(cx, "the install checklist", |cx| {
        support::present(cx, w.win, ids::PAYLOAD_CHECKLIST)
    });
    support::click_id(cx, w.win, ids::PAYLOAD_INSTALL);
    toast_is(cx, w.win, "Map installed. Pack");
    idle(cx);
    server.reset();
    let new = support::zip_of(&[
        ("maps/cp_a.bsp", &bsp(b"a2")),
        ("maps/cp_b.bsp", &bsp(b"b2")),
        ("maps/cp_c.bsp", &bsp(b"c2")),
    ]);
    server.mount_gb_addon(38, "Pack", &[("pack_v2.zip", new, 300)]);
    cx.update(|cx| Tf2Probe::install(cx, || false));
    run(cx, &w, |i, win, cx| i.update(GbId::new(38), win, cx));
    support::wait_real(cx, "the update checklist", |cx| {
        support::present(cx, w.win, ids::PAYLOAD_CHECKLIST)
    });
    // The item's own installed maps are not "already in the game", so none is disabled.
    for (n, name) in ["cp_a", "cp_b", "cp_c"].iter().enumerate() {
        assert_eq!(
            support::label_of(cx, w.win, ids::payload_check(n)).as_deref(),
            Some(*name)
        );
    }
    support::click_id(cx, w.win, ids::PAYLOAD_INSTALL);
    toast_is(cx, w.win, "Map updated. Pack");
    idle(cx);
    let maps = folder(&env, 38, "pack").join("maps");
    for (name, tag) in [("cp_a", b"a2"), ("cp_b", b"b2"), ("cp_c", b"c2")] {
        assert_eq!(
            std::fs::read(maps.join(format!("{name}.bsp"))).unwrap(),
            bsp(tag),
            "{name} is the updated map"
        );
    }
}

// The refusal comes before the download, so a big map isn't fetched only to be thrown away.
#[gpui_kit::test]
fn an_update_while_tf2_runs_refuses_without_downloading(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(43, "Alpha", &[("cp_alpha.bsp", bsp(b"old"), 100)]);
    let w = window(cx, &env, &server);
    install(cx, &w, 43);
    idle(cx);
    server.reset();
    server.mount_gb_addon(43, "Alpha", &[("cp_alpha_v2.bsp", bsp(b"new"), 300)]);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    run(cx, &w, |i, win, cx| i.update(GbId::new(43), win, cx));
    toast_is(
        cx,
        w.win,
        "Close TF2 first. TF2 is running. Close the game, then try again.",
    );
    idle(cx);
    assert_eq!(server.requests_where(|p| p.starts_with("/dl/addon/43/")), 0);
    let path = folder(&env, 43, "alpha").join("maps/cp_alpha.bsp");
    assert_eq!(std::fs::read(path).unwrap(), bsp(b"old"));
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
    toast_is(cx, w.win, "Map installed. Alpha");
    idle(cx);
    // Clear the first install's toast so only Install again can show the map wording.
    cx.update_window(w.win, |_, window, cx| {
        tf2tools_app::shell::toast::clear_toasts(window, cx)
    })
    .unwrap();
    // The close animates on the fake clock; a second of ticks stays inside the 5 s autohide.
    for _ in 0..20 {
        if support::toast_labels(cx, w.win).is_empty() {
            break;
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
    }
    assert_eq!(support::toast_labels(cx, w.win), Vec::<String>::new());
    std::fs::remove_dir_all(folder(&env, 39, "alpha")).unwrap();
    run(cx, &w, |i, win, cx| i.reinstall(GbId::new(39), win, cx));
    toast_is(cx, w.win, "Map installed. Alpha");
    idle(cx);
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

#[gpui_kit::test]
fn a_folder_deleted_outside_the_app_reads_not_installed_when_the_tab_opens_again(
    cx: &mut TestAppContext,
) {
    use crate::support::maps::{maps_shell_found, open_gamebanana, open_maps, seed_maps};
    use tf2tools_app::shell::route::Page;
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(10, "cp_alpha", &[("cp_alpha.bsp", bsp(b"a"), 100)]);
    let s = maps_shell_found(cx, &env, &server);
    seed_maps(&env);
    let page = open_maps(cx, &s);
    open_gamebanana(cx, &s, &page);
    let p = profile(cx, 10);
    let (i, win) = (installs(cx), s.win);
    assert!(cx.update(|cx| i.update(cx, |i, cx| i.install(AddonSection::MAPS, p, win, cx))));
    idle(cx);
    let badge = ids::addon_gb_card_badge("maps", GbId::new(10));
    support::wait_real(cx, "the Installed badge", |cx| {
        support::label_of(cx, s.win, badge.clone()).as_deref() == Some("Installed")
    });
    std::fs::remove_dir_all(folder(&env, 10, "cp_alpha")).unwrap();
    // Leaving Maps and coming back is the tab opening again.
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::nav(Page::Maps));
    support::wait_real(cx, "the Not installed badge", |cx| {
        support::label_of(cx, s.win, badge.clone()).as_deref() == Some("Not installed")
    });
    support::click_id(cx, s.win, ids::addon_gb_card("maps", GbId::new(10)));
    support::wait_real(cx, "Install again", |cx| {
        support::label_of(cx, s.win, ids::ADDON_DETAIL_INSTALL).as_deref() == Some("Install again")
    });
}
