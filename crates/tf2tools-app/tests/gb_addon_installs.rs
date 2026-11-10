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
fn profile(cx: &mut TestAppContext, id: u64) -> AddonProfile {
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

pub fn toast_is(cx: &mut TestAppContext, win: gpui_kit::AnyWindowHandle, text: &str) {
    support::wait_real(cx, text, |cx| {
        support::label_of(cx, win, ids::TOAST).as_deref() == Some(text)
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
