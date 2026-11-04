mod support;

use std::time::Duration;

use gpui_kit::TestAppContext;
use tf2tools_app::addon_updates::{AddonEvent, AddonUpdates};
use tf2tools_app::services::Tf2State;
use tf2tools_app::store_download::{DownloadEvent, StoreDownloads};
use tf2tools_core::{AddonName, Tf2Install};

const FOOT: &str = "mastercomfig-addon-no-footsteps.vpk";
const FLAT: &str = "mastercomfig-addon-flat-mouse.vpk";

fn foot() -> AddonName {
    AddonName::new(FOOT).unwrap()
}

fn old() -> Vec<u8> {
    support::vpk_bytes(&[("cfg/addons/x.cfg", b"old".as_slice())])
}

fn new() -> Vec<u8> {
    support::vpk_bytes(&[("cfg/addons/x.cfg", b"new release".as_slice())])
}

#[gpui_kit::test]
fn an_update_reinstalls_on_its_own_finish(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::from_millis(200));
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_addons(&env, &server.uri(), &[(FOOT, "No Footsteps", "9.100.1")]);
    support::seed_library_addon(&s.services, &env, FOOT, "9.0.0", &old(), true);
    let updates = cx.update(AddonUpdates::global);
    let settled = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = settled.clone();
    let _sub = cx.update(|cx| {
        cx.subscribe(&updates, move |_, event: &AddonEvent, _| {
            seen.borrow_mut().push(event.clone())
        })
    });
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), true, s.win, cx)
        })
    });
    assert!(cx.update(|cx| updates.read(cx).is_pending(&foot())));
    let custom = env.game.join("tf/custom").join(FOOT);
    support::wait_real(cx, "the reinstall", |_| {
        std::fs::read(&custom).unwrap() == new()
    });
    support::wait_real(cx, "settled", |_| !settled.borrow().is_empty());
    assert_eq!(*settled.borrow(), [AddonEvent::Settled { name: foot() }]);
    assert!(cx.update(
        |cx| !updates.read(cx).is_pending(&foot()) && !updates.read(cx).is_installing(&foot())
    ));
}

#[gpui_kit::test]
fn a_refused_start_leaves_nothing_pending(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FLAT, new(), Duration::from_millis(800));
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_addons(
        &env,
        &server.uri(),
        &[
            (FOOT, "No Footsteps", "9.100.1"),
            (FLAT, "Flat Mouse", "9.100.1"),
        ],
    );
    support::seed_library_addon(&s.services, &env, FOOT, "9.0.0", &old(), true);
    let updates = cx.update(AddonUpdates::global);
    let flat = AddonName::new(FLAT).unwrap();
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(flat, "Flat Mouse".into(), false, s.win, cx)
        })
    });
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    support::wait_real(cx, "the first download", |cx| {
        cx.update(|cx| dl.read(cx).is_busy())
    });
    // A second start while one runs is refused by StoreDownloads; its reinstall must not linger.
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), true, s.win, cx)
        })
    });
    support::wait_real(cx, "the refusal", |cx| {
        cx.update(|cx| !updates.read(cx).is_pending(&foot()))
    });
    support::wait_real(cx, "idle", |cx| cx.update(|cx| !dl.read(cx).is_busy()));
    assert_eq!(
        std::fs::read(env.game.join("tf/custom").join(FOOT)).unwrap(),
        old()
    );
}

#[gpui_kit::test]
fn a_reinstall_writes_the_install_row(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::ZERO);
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_addons(&env, &server.uri(), &[(FOOT, "No Footsteps", "9.100.1")]);
    support::seed_library_addon(&s.services, &env, FOOT, "9.0.0", &old(), true);
    let updates = cx.update(AddonUpdates::global);
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), true, s.win, cx)
        })
    });
    support::wait_real(cx, "the row", |_| {
        support::log_rows(&s.services)
            .iter()
            .any(|e| e.category() == "addon_store" && e.action() == "install")
    });
    let rows = support::log_rows(&s.services);
    assert_eq!(rows[0].message(), format!("Installed addon: {FOOT}"));
}

#[gpui_kit::test]
fn is_busy_spans_a_reinstalling_update(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::from_millis(200));
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_addons(&env, &server.uri(), &[(FOOT, "No Footsteps", "9.100.1")]);
    support::seed_library_addon(&s.services, &env, FOOT, "9.0.0", &old(), true);
    let updates = cx.update(AddonUpdates::global);
    assert!(!cx.update(|cx| updates.read(cx).is_busy()));
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), true, s.win, cx)
        })
    });
    assert!(cx.update(|cx| updates.read(cx).is_busy()));
    let custom = env.game.join("tf/custom").join(FOOT);
    support::wait_real(cx, "the reinstall", |_| {
        std::fs::read(&custom).unwrap() == new()
    });
    support::wait_real(cx, "idle", |cx| cx.update(|cx| !updates.read(cx).is_busy()));
}

#[gpui_kit::test]
fn a_plain_download_is_busy_from_its_lookup(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FLAT, new(), Duration::from_millis(200));
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_addons(&env, &server.uri(), &[(FLAT, "Flat Mouse", "9.100.1")]);
    let updates = cx.update(AddonUpdates::global);
    let flat = AddonName::new(FLAT).unwrap();
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(flat, "Flat Mouse".into(), false, s.win, cx)
        })
    });
    assert!(
        cx.update(|cx| updates.read(cx).is_busy()),
        "busy during the lookup"
    );
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    support::wait_real(cx, "idle", |cx| {
        cx.update(|cx| !updates.read(cx).is_busy() && !dl.read(cx).is_busy())
    });
}

#[gpui_kit::test]
fn a_same_item_refusal_settles_once(cx: &mut TestAppContext) {
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
    let updates = cx.update(AddonUpdates::global);
    let settled = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = settled.clone();
    let _sub = cx.update(|cx| {
        cx.subscribe(&updates, move |_, event: &AddonEvent, _| {
            seen.borrow_mut().push(event.clone())
        })
    });
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), false, s.win, cx)
        })
    });
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    support::wait_real(cx, "the first download", |cx| {
        cx.update(|cx| dl.read(cx).is_busy())
    });
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), true, s.win, cx)
        })
    });
    let custom = env.game.join("tf/custom").join(FOOT);
    support::wait_real(cx, "the kept reinstall", |_| {
        std::fs::read(&custom).unwrap() == new()
    });
    support::wait_real(cx, "idle", |cx| cx.update(|cx| !updates.read(cx).is_busy()));
    assert_eq!(*settled.borrow(), [AddonEvent::Settled { name: foot() }]);
    assert!(
        !support::toast_labels(cx, s.win).iter().any(|l| l == BUSY),
        "the addon's own run is not another download"
    );
}

#[gpui_kit::test]
fn a_failed_update_keeps_the_old_copy_and_settles(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // Nothing mounted for the release file: the download answers 404 and fails.
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_addons(&env, &server.uri(), &[(FOOT, "No Footsteps", "9.100.1")]);
    support::seed_library_addon(&s.services, &env, FOOT, "9.0.0", &old(), true);
    let updates = cx.update(AddonUpdates::global);
    let dl = cx.update(StoreDownloads::install);
    let failed = std::rc::Rc::new(std::cell::Cell::new(false));
    let saw = failed.clone();
    let _dl_sub = cx.update(|cx| {
        cx.subscribe(&dl, move |_, event: &DownloadEvent, _| {
            if matches!(event, DownloadEvent::Failed { .. }) {
                saw.set(true);
            }
        })
    });
    let settled = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = settled.clone();
    let _sub = cx.update(|cx| {
        cx.subscribe(&updates, move |_, event: &AddonEvent, _| {
            seen.borrow_mut().push(event.clone())
        })
    });
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), true, s.win, cx)
        })
    });
    support::wait_real(cx, "the failure", |_| failed.get());
    support::wait_real(cx, "settled", |_| !settled.borrow().is_empty());
    assert_eq!(*settled.borrow(), [AddonEvent::Settled { name: foot() }]);
    assert!(cx.update(|cx| !updates.read(cx).is_pending(&foot()) && !updates.read(cx).is_busy()));
    assert_eq!(
        std::fs::read(env.game.join("tf/custom").join(FOOT)).unwrap(),
        old()
    );
}

#[gpui_kit::test]
fn overlapping_lookups_hold_busy_until_the_last_ends(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    // No store row for FOOT, so each lookup fails on its own and settles.
    let updates = cx.update(AddonUpdates::global);
    let busy = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = busy.clone();
    let _sub = cx.update(|cx| {
        cx.subscribe(&updates, move |u, _: &AddonEvent, cx| {
            seen.borrow_mut().push(u.read(cx).is_busy())
        })
    });
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), false, s.win, cx);
            u.start(foot(), "No Footsteps".into(), false, s.win, cx);
        })
    });
    support::wait_real(cx, "both lookups", |_| busy.borrow().len() == 2);
    assert_eq!(*busy.borrow(), [true, false]);
}

const BUSY: &str = "Another download is running. Wait for it to finish, then try again.";

#[gpui_kit::test]
fn a_refusal_for_another_download_says_to_wait(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FLAT, new(), Duration::from_millis(800));
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_addons(
        &env,
        &server.uri(),
        &[
            (FOOT, "No Footsteps", "9.100.1"),
            (FLAT, "Flat Mouse", "9.100.1"),
        ],
    );
    support::seed_library_addon(&s.services, &env, FOOT, "9.0.0", &old(), true);
    let updates = cx.update(AddonUpdates::global);
    let flat = AddonName::new(FLAT).unwrap();
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(flat, "Flat Mouse".into(), false, s.win, cx)
        })
    });
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    support::wait_real(cx, "the first download", |cx| {
        cx.update(|cx| dl.read(cx).is_busy())
    });
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), true, s.win, cx)
        })
    });
    support::wait_real(cx, "the info toast", |cx| {
        support::toast_labels(cx, s.win).iter().any(|l| l == BUSY)
    });
    support::assert_plain(BUSY);
    assert!(cx.update(|cx| !updates.read(cx).is_pending(&foot())));
    support::wait_real(cx, "idle", |cx| cx.update(|cx| !dl.read(cx).is_busy()));
}
