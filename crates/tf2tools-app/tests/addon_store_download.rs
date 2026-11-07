use crate::support;

use std::time::Duration;

use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;
use tf2tools_app::addon_updates::AddonUpdates;
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_download::StoreDownloads;
use tf2tools_core::{AddonName, StoreItemId, StoreKind, Tf2Install};

const A: StoreKind = StoreKind::Addons;
const FLAT: &str = "mastercomfig-addon-flat-mouse.vpk";
const FOOT: &str = "mastercomfig-addon-no-footsteps.vpk";

fn addon(asset: &str) -> StoreItemId {
    StoreItemId::Addon(AddonName::new(asset).unwrap())
}

fn foot() -> AddonName {
    AddonName::new(FOOT).unwrap()
}

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn old() -> Vec<u8> {
    support::vpk_bytes(&[("cfg/addons/x.cfg", b"old".as_slice())])
}

fn new() -> Vec<u8> {
    support::vpk_bytes(&[("cfg/addons/x.cfg", b"new release".as_slice())])
}

/// Flat Mouse only in the store; No Footsteps installed at 9.0.0 with the store at 9.100.1.
fn seed(uri: String) -> impl FnOnce(&support::Env, &tf2tools_app::services::Services) {
    move |env, s| {
        support::seed_addons(
            env,
            &uri,
            &[
                (FLAT, "Flat Mouse", "9.100.1"),
                (FOOT, "No Footsteps", "9.100.1"),
            ],
        );
        support::seed_library_addon(s, env, FOOT, "9.0.0", &old(), true);
    }
}

#[gpui_kit::test]
fn a_download_lands_in_the_library_only(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FLAT, new(), Duration::from_millis(800));
    let w = support::addon_store_window(cx, &env, &server, found(&env), seed(server.uri()));
    support::click_id(cx, w.win, ids::store_download(A, &addon(FLAT)));
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    support::wait_real(cx, "the download to start", |cx| {
        cx.update(|cx| dl.read(cx).is_downloading(A, &addon(FLAT)))
    });
    let label = support::label_of(cx, w.win, ids::store_download(A, &addon(FLAT))).unwrap();
    assert!(
        label.starts_with("Downloading..."),
        "the card shows progress: {label}"
    );
    support::wait_real(cx, "the download", |cx| {
        cx.update(|cx| !dl.read(cx).is_busy())
    });
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, w.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::TOAST).as_deref(),
        Some("Addon downloaded. Flat Mouse added to your library")
    );
    support::wait_real(cx, "the badge", |cx| {
        support::label_of(cx, w.win, ids::store_state(A, &addon(FLAT))).as_deref()
            == Some("In Library")
    });
    assert!(
        !env.game.join("tf/custom").join(FLAT).exists(),
        "a download does not install"
    );
    let lib = w.services.library.list_addons(None).unwrap();
    let flat = lib
        .iter()
        .find(|e| e.name().as_str() == FLAT)
        .expect("in the library");
    assert_eq!(flat.installed_tag(), "9.100.1");
}

#[gpui_kit::test]
fn update_downloads_the_release_and_reinstalls_it(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::from_millis(200));
    let w = support::addon_store_window(cx, &env, &server, found(&env), seed(server.uri()));
    assert_eq!(
        support::label_of(cx, w.win, ids::store_download(A, &addon(FOOT))).as_deref(),
        Some("Update")
    );
    support::click_id(cx, w.win, ids::store_download(A, &addon(FOOT)));
    support::wait_real(cx, "the reinstall", |cx| {
        support::label_of(cx, w.win, ids::store_state(A, &addon(FOOT))).as_deref()
            == Some("Installed")
    });
    let name = AddonName::new(FOOT).unwrap();
    let updates = cx.update(AddonUpdates::global);
    assert!(
        cx.update(
            |cx| !updates.read(cx).is_installing(&name) && !updates.read(cx).is_pending(&name)
        )
    );
    assert_eq!(
        std::fs::read(env.game.join("tf/custom").join(FOOT)).unwrap(),
        new()
    );
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    let lib = w.services.library.list_addons(Some(&tf2)).unwrap();
    let foot = lib.iter().find(|e| e.name().as_str() == FOOT).unwrap();
    assert_eq!(foot.installed_tag(), "9.100.1");
    assert!(foot.is_installed() && !foot.update_available());
    // Downloading Flat Mouse afterwards must not install it: the pending reinstall was FOOT's alone.
    server.mount_addon(FLAT, new(), Duration::ZERO);
    support::click_id(cx, w.win, ids::store_download(A, &addon(FLAT)));
    support::wait_real(cx, "the second download", |cx| {
        support::label_of(cx, w.win, ids::store_state(A, &addon(FLAT))).as_deref()
            == Some("In Library")
    });
    assert!(!env.game.join("tf/custom").join(FLAT).exists());
}

#[gpui_kit::test]
fn a_failed_update_toasts_and_keeps_the_installed_copy(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start(); // nothing mounted: the release URL is a 404
    let w = support::addon_store_window(cx, &env, &server, found(&env), seed(server.uri()));
    support::click_id(cx, w.win, ids::store_download(A, &addon(FOOT)));
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, w.win, ids::TOAST)
    });
    let toast = support::label_of(cx, w.win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Failed to download addon"), "{toast}");
    support::assert_plain(&toast);
    assert!(toast.contains("GitHub"), "{toast}");
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    support::wait_real(cx, "idle", |cx| cx.update(|cx| !dl.read(cx).is_busy()));
    assert_eq!(
        support::label_of(cx, w.win, ids::store_download(A, &addon(FOOT))).as_deref(),
        Some("Update")
    );
    assert_eq!(
        std::fs::read(env.game.join("tf/custom").join(FOOT)).unwrap(),
        old()
    );
    let updates = cx.update(AddonUpdates::global);
    assert!(!cx.update(|cx| updates.read(cx).is_pending(&AddonName::new(FOOT).unwrap())));
}

#[gpui_kit::test]
fn an_update_reinstalls_after_the_page_is_left(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::from_millis(800));
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_addons(&env, &server.uri(), &[(FOOT, "No Footsteps", "9.100.1")]);
    support::seed_library_addon(&s.services, &env, FOOT, "9.0.0", &old(), true);
    support::click_id(cx, s.win, ids::nav(Page::AddonStore));
    let page = cx
        .update(|cx| s.shell.read(cx).addon_store_page())
        .expect("the page");
    support::wait_real(cx, "the page", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::click_id(
        cx,
        s.win,
        ids::store_download(A, &StoreItemId::Addon(foot())),
    );
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    support::wait_real(cx, "the download to start", |cx| {
        cx.update(|cx| dl.read(cx).is_busy())
    });
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(
        cx.update(|cx| s.shell.read(cx).addon_store_page())
            .is_none(),
        "the page is gone"
    );
    let custom = env.game.join("tf/custom").join(FOOT);
    support::wait_real(cx, "the reinstall", |_| {
        std::fs::read(&custom).unwrap() == new()
    });
    let updates = cx.update(AddonUpdates::global);
    support::wait_real(cx, "settled", |cx| {
        cx.update(|cx| !updates.read(cx).is_installing(&foot()))
    });
    support::click_id(cx, s.win, ids::nav(Page::AddonStore));
    let page = cx
        .update(|cx| s.shell.read(cx).addon_store_page())
        .expect("the page again");
    support::wait_real(cx, "the page", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::store_state(A, &StoreItemId::Addon(foot()))).as_deref(),
        Some("Installed")
    );
}

#[gpui_kit::test]
fn no_addon_starts_while_another_updates(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::from_millis(200));
    server.mount_addon(FLAT, new(), Duration::ZERO);
    let w = support::addon_store_window(cx, &env, &server, found(&env), seed(server.uri()));
    let updates = cx.update(AddonUpdates::global);
    let flat = AddonName::new(FLAT).unwrap();
    cx.update_window(w.win, |_, window, cx| {
        w.page.update(cx, |p, cx| {
            p.download(&foot(), window, cx);
            p.download(&flat, window, cx);
        })
    })
    .unwrap();
    let custom = env.game.join("tf/custom").join(FOOT);
    support::wait_real(cx, "the update", |_| {
        std::fs::read(&custom).unwrap() == new()
    });
    support::wait_real(cx, "idle", |cx| cx.update(|cx| !updates.read(cx).is_busy()));
    let library = w.services.library.list_addons(None).unwrap();
    assert!(
        !library.iter().any(|a| a.name() == &flat),
        "Flat Mouse must not download while No Footsteps updates"
    );
}
