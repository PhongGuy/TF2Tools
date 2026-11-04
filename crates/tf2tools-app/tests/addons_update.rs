mod support;

use std::time::Duration;

use gpui_kit::{ElementId, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::addons::AddonsPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_download::StoreDownloads;
use tf2tools_core::{AddonName, StoreItemId, StoreKind, Tf2Install};

const FOOT: &str = "mastercomfig-addon-no-footsteps.vpk";
const PYRO: &str = "mastercomfig-addon-no-pyroland.vpk";
const TUT: &str = "mastercomfig-addon-no-tutorial.vpk";

fn name(asset: &str) -> AddonName {
    AddonName::new(asset).unwrap()
}

fn old() -> Vec<u8> {
    support::vpk_bytes(&[("cfg/addons/x.cfg", b"old".as_slice())])
}

fn new() -> Vec<u8> {
    support::vpk_bytes(&[("cfg/addons/x.cfg", b"new release".as_slice())])
}

/// A shell over a store that lists FOOT at 9.100.1, with FOOT 9.0.0 in the library
/// (installed when `installed`), then the Addons page, loaded. The store page is never opened.
fn setup(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
    installed: bool,
) -> (support::ShellWindow, Entity<AddonsPage>) {
    let s = support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, server);
    // The download rows come from this watcher, which only `app::start` installs.
    cx.update(tf2tools_app::activity::watch_stores);
    support::seed_addons(env, &server.uri(), &[(FOOT, "No Footsteps", "9.100.1")]);
    support::seed_library_addon(&s.services, env, FOOT, "9.0.0", &old(), installed);
    support::click_id(cx, s.win, ids::nav(Page::Addons));
    let page = cx
        .update(|cx| s.shell.read(cx).addons_page())
        .expect("an Addons page");
    support::wait_real(cx, "Addons to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (s, page)
}

#[gpui_kit::test]
fn update_on_an_installed_addon_reinstalls_the_new_release(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::from_millis(50));
    let (s, _page) = setup(cx, &env, &server, true);
    assert!(!support::present(cx, s.win, ids::addon_check(&name(FOOT))));
    support::click_id(cx, s.win, ids::addon_update(&name(FOOT)));
    let custom = env.game.join("tf/custom").join(FOOT);
    support::wait_real(cx, "the reinstall", |_| {
        std::fs::read(&custom).unwrap() == new()
    });
    support::wait_real(cx, "the card to settle", |cx| {
        support::label_of(cx, s.win, ids::addon_tag(&name(FOOT))).as_deref() == Some("9.100.1")
            && support::present(cx, s.win, ids::addon_check(&name(FOOT)))
    });
    assert!(!support::present(cx, s.win, ids::addon_update(&name(FOOT))));
    // The rows: the download (appended in the background) and the reinstall, in either order.
    support::wait_real(cx, "both rows", |_| {
        support::log_rows(&s.services).len() == 2
    });
    let mut actions: Vec<_> = support::log_rows(&s.services)
        .iter()
        .map(|r| (r.category().to_owned(), r.action().to_owned()))
        .collect();
    actions.sort();
    assert_eq!(
        actions,
        [
            ("addon_store".to_owned(), "download".to_owned()),
            ("addon_store".to_owned(), "install".to_owned()),
        ]
    );
}

#[gpui_kit::test]
fn update_on_a_library_addon_only_downloads(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::from_millis(50));
    let (s, _page) = setup(cx, &env, &server, false);
    support::click_id(cx, s.win, ids::addon_update(&name(FOOT)));
    support::wait_real(cx, "the new tag", |cx| {
        support::label_of(cx, s.win, ids::addon_tag(&name(FOOT))).as_deref() == Some("9.100.1")
    });
    assert!(!env.game.join("tf/custom").join(FOOT).exists());
    assert!(support::present(cx, s.win, ids::addon_install(&name(FOOT))));
}

#[gpui_kit::test]
fn while_an_update_runs_every_addon_button_rests(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::from_millis(800));
    let (s, page) = setup(cx, &env, &server, true);
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &old(), false);
    cx.update(tf2tools_app::library_events::LibraryEvents::changed);
    support::wait_real(cx, "PYRO's card", |cx| {
        support::present(cx, s.win, ids::addon_install(&name(PYRO)))
    });
    support::click_id(cx, s.win, ids::addon_update(&name(FOOT)));
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    let foot = StoreItemId::Addon(name(FOOT));
    support::wait_real(cx, "the download to start", |cx| {
        cx.update(|cx| dl.read(cx).is_downloading(StoreKind::Addons, &foot))
    });
    let label = support::label_of(cx, s.win, ids::addon_busy(&name(FOOT))).unwrap();
    assert!(label.starts_with("Downloading..."), "{label}");
    assert!(cx.update(|cx| page.read(cx).is_any_busy(cx)));
    support::click_id(cx, s.win, ids::addon_install(&name(PYRO)));
    support::settle(cx, s.win);
    assert!(!env.game.join("tf/custom").join(PYRO).exists());
    support::wait_real(cx, "idle", |cx| {
        cx.update(|cx| !page.read(cx).is_any_busy(cx))
    });
    assert!(!env.game.join("tf/custom").join(PYRO).exists());
}

// FOOT's update is the busy job; PYRO (library, update available) shows Update, Install and
// Delete, and TUT (installed, not in the store) shows Uninstall and Delete.
#[gpui_kit::test]
fn every_addon_button_renders_disabled_while_an_update_runs(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::from_millis(1500));
    let (s, page) = setup(cx, &env, &server, true);
    support::seed_addons(&env, &server.uri(), &[(PYRO, "No Pyroland", "9.100.1")]);
    support::seed_library_addon(&s.services, &env, PYRO, "9.0.0", &old(), false);
    support::seed_library_addon(&s.services, &env, TUT, "9.100.1", &old(), true);
    cx.update(tf2tools_app::library_events::LibraryEvents::changed);
    let (foot, pyro, tut) = (name(FOOT), name(PYRO), name(TUT));
    support::wait_real(cx, "the new cards", |cx| {
        support::present(cx, s.win, ids::addon_update(&pyro))
            && support::present(cx, s.win, ids::addon_uninstall(&tut))
    });
    let rest: Vec<ElementId> = vec![
        ids::addon_update(&pyro),
        ids::addon_install(&pyro),
        ids::addon_delete(&pyro),
        ids::addon_uninstall(&tut),
        ids::addon_delete(&tut),
    ];
    assert_eq!(
        support::disabled_ids(cx, s.win, &rest),
        Vec::<ElementId>::new()
    );
    assert!(!support::disabled_of(cx, s.win, ids::addon_update(&foot)));

    support::click_id(cx, s.win, ids::addon_update(&foot));
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    let item = StoreItemId::Addon(foot.clone());
    support::wait_real(cx, "the download to start", |cx| {
        cx.update(|cx| dl.read(cx).is_downloading(StoreKind::Addons, &item))
    });
    assert_eq!(support::disabled_ids(cx, s.win, &rest), rest);
    assert!(support::disabled_of(cx, s.win, ids::addon_busy(&foot)));

    support::wait_real(cx, "idle", |cx| {
        cx.update(|cx| !page.read(cx).is_any_busy(cx))
    });
    support::wait_real(cx, "FOOT's card to settle", |cx| {
        support::present(cx, s.win, ids::addon_check(&foot))
    });
    assert_eq!(
        support::disabled_ids(cx, s.win, &rest),
        Vec::<ElementId>::new()
    );
}

#[gpui_kit::test]
fn a_failed_update_toasts_and_keeps_the_old_copy(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // Nothing mounted: the release URL answers 404.
    let (s, page) = setup(cx, &env, &server, true);
    support::click_id(cx, s.win, ids::addon_update(&name(FOOT)));
    support::wait_real(cx, "the failure toast", |cx| {
        support::label_of(cx, s.win, ids::TOAST)
            .is_some_and(|t| t.starts_with("Failed to download addon"))
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    support::assert_plain(&toast);
    assert!(toast.contains("GitHub"), "{toast}");
    support::wait_real(cx, "idle", |cx| {
        cx.update(|cx| !page.read(cx).is_any_busy(cx))
    });
    assert_eq!(
        std::fs::read(env.game.join("tf/custom").join(FOOT)).unwrap(),
        old()
    );
    assert!(support::present(cx, s.win, ids::addon_update(&name(FOOT))));
}
