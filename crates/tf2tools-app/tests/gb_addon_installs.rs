use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::gb_addon_installs::{GbAddonInstalls, install_blocker};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::tokio_bridge;
use tf2tools_core::{AddonProfile, AddonSection, GbId, Tf2Install};

use crate::support;

pub fn window(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> support::AddonStoreWindow {
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let w = support::addon_store_window(cx, env, server, state, |_, _| {});
    cx.update(|cx| Tf2Probe::install(cx, || false));
    w
}

pub fn vpk(tag: &[u8]) -> Vec<u8> {
    support::vpk_bytes(&[("materials/a.vmt", tag)])
}

// The installed `Services` global points at the mock server; `w.services` predates that.
pub fn profile(cx: &mut TestAppContext, id: u64) -> AddonProfile {
    let (rt, store) = cx.update(|cx| {
        (
            tokio_bridge::handle(cx),
            cx.global::<Services>().store.clone(),
        )
    });
    rt.block_on(store.addon_catalog().profile(GbId::new(id)))
        .unwrap()
}

fn installs(cx: &mut TestAppContext) -> Entity<GbAddonInstalls> {
    cx.update(GbAddonInstalls::global)
}

fn start_install(cx: &mut TestAppContext, w: &support::AddonStoreWindow, id: u64) -> bool {
    let p = profile(cx, id);
    let i = installs(cx);
    let win = w.win;
    let section = AddonSection::by_key("effects").unwrap();
    cx.update(|cx| i.update(cx, |i, cx| i.install(section, p, win, cx)))
}

// A second operation's toast stacks on the first one's, so any visible toast may match.
pub fn toast_is(cx: &mut TestAppContext, win: gpui_kit::AnyWindowHandle, text: &str) {
    support::wait_real(cx, text, |cx| {
        support::toast_labels(cx, win).iter().any(|l| l == text)
    });
}

fn idle(cx: &mut TestAppContext) {
    let i = installs(cx);
    support::wait_real(cx, "the operation to end", |cx| {
        cx.update(|cx| !i.read(cx).is_busy())
    });
}

#[gpui_kit::test]
fn a_one_payload_addon_installs_without_asking(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(8, "Toon FX", &[("toon.vpk", vpk(b"v"), 100)]);
    let w = window(cx, &env, &server);
    let (changes, _sub) = support::library_changes(cx);
    assert!(start_install(cx, &w, 8));
    toast_is(cx, w.win, "Add-on installed. Toon FX");
    assert!(
        env.game
            .join("tf/custom/tf2tools-gb-8-toon-fx.vpk")
            .is_file()
    );
    idle(cx);
    let i = installs(cx);
    assert!(cx.update(|cx| {
        i.read(cx)
            .entry(GbId::new(8))
            .is_some_and(|e| e.is_installed())
    }));
    assert!(changes.get() >= 1);
}

#[gpui_kit::test]
fn a_vpk_while_tf2_runs_says_close_tf2_first(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(11, "Crit", &[("crit.vpk", vpk(b"v"), 100)]);
    let w = window(cx, &env, &server);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    assert!(start_install(cx, &w, 11));
    toast_is(
        cx,
        w.win,
        "Close TF2 first. TF2 is running. Close the game, then try again.",
    );
    assert!(!env.game.join("tf/custom/tf2tools-gb-11-crit.vpk").exists());
}

#[gpui_kit::test]
fn a_refused_download_names_its_reason(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let body = support::zip_of(&[("readme.txt", b"r")]);
    server.mount_gb_addon(12, "Loose", &[("loose.zip", body, 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 12));
    toast_is(
        cx,
        w.win,
        "Couldn't install the add-on. TF2Tools found nothing in this download it knows how to install, so it wasn't installed.",
    );
}

#[gpui_kit::test]
fn blockers_are_read_from_the_newest_file(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(15, "Fine", &[("a.vpk", vpk(b"v"), 100)]);
    server.mount_gb_profile(
        16,
        serde_json::json!({"_idRow": 16, "_sName": "Bad", "_aFiles": [
            {"_idRow": 1, "_sFile": "a.zip", "_nFilesize": 10, "_tsDateAdded": 100,
             "_sAvResult": "infected", "_sDownloadUrl": format!("{}/dl/x", server.uri())}]}),
    );
    server.mount_gb_profile(
        17,
        serde_json::json!({"_idRow": 17, "_sName": "Huge", "_aFiles": [
            {"_idRow": 1, "_sFile": "a.zip", "_nFilesize": 400u64 * 1024 * 1024,
             "_tsDateAdded": 100, "_sAvResult": "clean",
             "_sDownloadUrl": format!("{}/dl/x", server.uri())}]}),
    );
    server.mount_gb_profile(
        18,
        serde_json::json!({"_idRow": 18, "_sName": "Empty", "_aFiles": []}),
    );
    let _w = window(cx, &env, &server);
    assert_eq!(install_blocker(&profile(cx, 15)), None);
    assert_eq!(
        install_blocker(&profile(cx, 16)).as_deref(),
        Some("GameBanana's virus scan didn't pass this file, so it wasn't installed.")
    );
    assert_eq!(
        install_blocker(&profile(cx, 17)).as_deref(),
        Some("This download is too big for TF2Tools to install.")
    );
    assert_eq!(
        install_blocker(&profile(cx, 18)).as_deref(),
        Some("This add-on has no files to install. Pick another add-on.")
    );
}

#[gpui_kit::test]
fn several_payloads_ask_and_install_the_chosen_ones(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let body = support::zip_of(&[("A.vpk", &vpk(&[1u8; 64])), ("B.vpk", &vpk(b"b"))]);
    server.mount_gb_addon(9, "Pack", &[("pack.zip", body, 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 9));
    support::wait_real(cx, "the checklist", |cx| {
        support::present(cx, w.win, ids::PAYLOAD_CHECKLIST)
    });
    support::click_id(cx, w.win, ids::payload_check(1));
    support::click_id(cx, w.win, ids::PAYLOAD_INSTALL);
    toast_is(cx, w.win, "Add-on installed. Pack");
    let custom = env.game.join("tf/custom");
    assert!(custom.join("tf2tools-gb-9-pack.vpk").is_file());
    assert!(custom.join("tf2tools-gb-9-pack-2.vpk").is_file());
}

#[gpui_kit::test]
fn a_cancelled_checklist_installs_nothing_and_says_nothing(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let body = support::zip_of(&[("A.vpk", &vpk(&[1u8; 64])), ("B.vpk", &vpk(b"b"))]);
    server.mount_gb_addon(10, "Pack", &[("pack.zip", body, 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 10));
    support::wait_real(cx, "the checklist", |cx| {
        support::present(cx, w.win, ids::PAYLOAD_CHECKLIST)
    });
    support::click_id(cx, w.win, ids::PAYLOAD_CANCEL);
    idle(cx);
    assert!(
        std::fs::read_dir(env.game.join("tf/custom"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!support::present(cx, w.win, ids::TOAST));
}

#[gpui_kit::test]
fn escape_on_the_checklist_installs_nothing(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let body = support::zip_of(&[("A.vpk", &vpk(&[1u8; 64])), ("B.vpk", &vpk(b"b"))]);
    server.mount_gb_addon(19, "Pack", &[("pack.zip", body, 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 19));
    support::wait_real(cx, "the checklist", |cx| {
        support::present(cx, w.win, ids::PAYLOAD_CHECKLIST)
    });
    support::press(cx, w.win, "escape");
    idle(cx);
    assert!(!support::present(cx, w.win, ids::PAYLOAD_CHECKLIST));
    assert!(
        std::fs::read_dir(env.game.join("tf/custom"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!support::present(cx, w.win, ids::TOAST));
}

#[gpui_kit::test]
fn a_second_operation_waits_for_the_first(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let body = support::zip_of(&[("A.vpk", &vpk(&[1u8; 64])), ("B.vpk", &vpk(b"b"))]);
    server.mount_gb_addon(13, "Pack", &[("pack.zip", body, 100)]);
    server.mount_gb_addon(14, "Crit", &[("crit.vpk", vpk(b"v"), 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 13));
    support::wait_real(cx, "the checklist", |cx| {
        support::present(cx, w.win, ids::PAYLOAD_CHECKLIST)
    });
    assert!(!start_install(cx, &w, 14), "busy while the checklist waits");
    support::click_id(cx, w.win, ids::PAYLOAD_CANCEL);
    idle(cx);
    assert!(start_install(cx, &w, 14));
}

fn act(
    cx: &mut TestAppContext,
    w: &support::AddonStoreWindow,
    f: impl FnOnce(
        &mut GbAddonInstalls,
        gpui_kit::AnyWindowHandle,
        &mut gpui_kit::Context<GbAddonInstalls>,
    ) -> bool,
) {
    let i = installs(cx);
    let win = w.win;
    assert!(
        cx.update(|cx| i.update(cx, |i, cx| f(i, win, cx))),
        "the operation starts"
    );
}

fn entry(cx: &mut TestAppContext, id: u64) -> Option<tf2tools_core::GbAddonEntry> {
    let i = installs(cx);
    cx.update(|cx| i.read(cx).entry(GbId::new(id)).cloned())
}

/// Installs a two-part pack with both parts chosen.
fn install_both(
    cx: &mut TestAppContext,
    w: &support::AddonStoreWindow,
    server: &support::StoreServer,
    id: u64,
) {
    let body = support::zip_of(&[("A.vpk", &vpk(&[1u8; 64])), ("B.vpk", &vpk(b"b"))]);
    server.mount_gb_addon(id, "Pack", &[("pack.zip", body, 100)]);
    assert!(start_install(cx, w, id), "the install starts");
    support::wait_real(cx, "the checklist", |cx| {
        support::present(cx, w.win, ids::PAYLOAD_CHECKLIST)
    });
    support::click_id(cx, w.win, ids::payload_check(1));
    support::click_id(cx, w.win, ids::PAYLOAD_INSTALL);
    toast_is(cx, w.win, "Add-on installed. Pack");
    idle(cx);
}

#[gpui_kit::test]
fn the_entry_is_recorded_before_busy_ends(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(27, "Toon FX", &[("toon.vpk", vpk(b"v"), 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 27));
    // The first notification after busy ends must already carry the entry; the library
    // listing that follows it would hide a finish that left the entry to it.
    let seen = std::rc::Rc::new(std::cell::Cell::new(None));
    let i = installs(cx);
    let _sub = cx.update(|cx| {
        let seen = std::rc::Rc::clone(&seen);
        cx.observe(&i, move |i, cx| {
            let me = i.read(cx);
            if !me.is_busy() && seen.get().is_none() {
                seen.set(Some(me.entry(GbId::new(27)).is_some()));
            }
        })
    });
    idle(cx);
    assert_eq!(seen.get(), Some(true));
}

#[gpui_kit::test]
fn install_without_tf2_says_tf2_not_found(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(28, "Toon FX", &[("toon.vpk", vpk(b"v"), 100)]);
    let state = Tf2State::Missing { saved: None };
    let w = support::addon_store_window(cx, &env, &server, state, |_, _| {});
    assert!(!start_install(cx, &w, 28));
    toast_is(
        cx,
        w.win,
        "TF2 not found. Set the TF2 folder in Settings, then try again.",
    );
    let i = installs(cx);
    assert!(!cx.update(|cx| i.read(cx).is_busy()));
}

#[gpui_kit::test]
fn an_update_keeps_the_chosen_parts_without_asking(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let w = window(cx, &env, &server);
    install_both(cx, &w, &server, 20);
    server.reset();
    let body = support::zip_of(&[("A.vpk", &vpk(&[2u8; 64])), ("B.vpk", &vpk(b"b2"))]);
    server.mount_gb_addon(20, "Pack", &[("pack2.zip", body, 300)]);
    act(cx, &w, |i, win, cx| i.update(GbId::new(20), win, cx));
    toast_is(cx, w.win, "Add-on updated. Pack");
    idle(cx);
    assert!(!support::present(cx, w.win, ids::PAYLOAD_CHECKLIST));
    assert_eq!(entry(cx, 20).unwrap().file_date(), 300);
    assert!(
        env.game
            .join("tf/custom/tf2tools-gb-20-pack-2.vpk")
            .is_file()
    );
}

#[gpui_kit::test]
fn an_update_whose_parts_changed_asks_again(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let w = window(cx, &env, &server);
    install_both(cx, &w, &server, 21);
    server.reset();
    let body = support::zip_of(&[("C.vpk", &vpk(&[3u8; 64])), ("D.vpk", &vpk(b"d"))]);
    server.mount_gb_addon(21, "Pack", &[("pack2.zip", body, 300)]);
    act(cx, &w, |i, win, cx| i.update(GbId::new(21), win, cx));
    support::wait_real(cx, "the checklist", |cx| {
        support::present(cx, w.win, ids::PAYLOAD_CHECKLIST)
    });
    support::click_id(cx, w.win, ids::PAYLOAD_CANCEL);
    idle(cx);
    assert_eq!(entry(cx, 21).unwrap().file_date(), 100);
    assert!(env.game.join("tf/custom/tf2tools-gb-21-pack.vpk").is_file());
}

#[gpui_kit::test]
fn updating_an_item_gone_from_gamebanana_says_so_and_marks_it(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(22, "Trashed", &[("a.vpk", vpk(b"v"), 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 22));
    idle(cx);
    server.reset();
    server.mount_gb_profile(
        22,
        serde_json::json!({"_idRow": 22, "_sName": "Trashed", "_bIsTrashed": true}),
    );
    act(cx, &w, |i, win, cx| i.update(GbId::new(22), win, cx));
    toast_is(
        cx,
        w.win,
        "Couldn't update the add-on. This item is no longer on GameBanana.",
    );
    idle(cx);
    assert!(entry(cx, 22).unwrap().is_gone());
    assert!(
        env.game
            .join("tf/custom/tf2tools-gb-22-trashed.vpk")
            .is_file()
    );
}

#[gpui_kit::test]
fn updating_an_item_gamebanana_cannot_find_says_try_later(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(23, "Lost", &[("a.vpk", vpk(b"v"), 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 23));
    idle(cx);
    server.reset();
    act(cx, &w, |i, win, cx| i.update(GbId::new(23), win, cx));
    toast_is(
        cx,
        w.win,
        "Couldn't update the add-on. GameBanana couldn't find this. Try again later.",
    );
    idle(cx);
    assert!(!entry(cx, 23).unwrap().is_gone());
}

#[gpui_kit::test]
fn remove_deletes_and_drops_the_entry(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(24, "Crit", &[("crit.vpk", vpk(b"v"), 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 24));
    idle(cx);
    act(cx, &w, |i, win, cx| i.remove(GbId::new(24), win, cx));
    toast_is(cx, w.win, "Add-on removed. Crit");
    idle(cx);
    assert!(entry(cx, 24).is_none());
    assert!(!env.game.join("tf/custom/tf2tools-gb-24-crit.vpk").exists());
    assert!(!env.data.gb_addon_dir().join("24").exists());
}

#[gpui_kit::test]
fn remove_while_tf2_runs_says_close_tf2_first(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(25, "Crit", &[("crit.vpk", vpk(b"v"), 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 25));
    idle(cx);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    act(cx, &w, |i, win, cx| i.remove(GbId::new(25), win, cx));
    toast_is(
        cx,
        w.win,
        "Close TF2 first. TF2 is running. Close the game, then try again.",
    );
    idle(cx);
    assert!(entry(cx, 25).is_some());
    assert!(env.game.join("tf/custom/tf2tools-gb-25-crit.vpk").is_file());
}

#[gpui_kit::test]
fn install_again_puts_the_library_copy_back(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(29, "Back", &[("a.vpk", vpk(b"v"), 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 29));
    idle(cx);
    let file = env.game.join("tf/custom/tf2tools-gb-29-back.vpk");
    std::fs::remove_file(&file).unwrap();
    server.reset();
    // Its toast reads as the first install's did, so the outcome is checked on disk.
    act(cx, &w, |i, win, cx| i.reinstall(GbId::new(29), win, cx));
    idle(cx);
    assert!(file.is_file());
    assert!(entry(cx, 29).unwrap().is_installed());
}

#[gpui_kit::test]
fn install_again_without_the_library_copy_says_install_from_the_store(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(26, "Back", &[("a.vpk", vpk(b"v"), 100)]);
    let w = window(cx, &env, &server);
    assert!(start_install(cx, &w, 26));
    idle(cx);
    std::fs::remove_file(env.game.join("tf/custom/tf2tools-gb-26-back.vpk")).unwrap();
    std::fs::remove_dir_all(env.data.gb_addon_dir().join("26")).unwrap();
    act(cx, &w, |i, win, cx| i.reinstall(GbId::new(26), win, cx));
    toast_is(
        cx,
        w.win,
        "Couldn't install the add-on. This add-on's files are no longer in your library. Install it again from the Addon Store.",
    );
}
