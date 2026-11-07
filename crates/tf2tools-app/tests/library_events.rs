use crate::support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{Subscription, TestAppContext};
use tf2tools_app::addon_updates::{AddonEvent, AddonUpdates};
use tf2tools_app::library_events::{LibraryChanged, LibraryEvents};
use tf2tools_app::services::Tf2State;
use tf2tools_core::{AddonName, Tf2Install};

const FOOT: &str = "mastercomfig-addon-no-footsteps.vpk";

fn foot() -> AddonName {
    AddonName::new(FOOT).unwrap()
}

fn old() -> Vec<u8> {
    support::vpk_bytes(&[("cfg/addons/x.cfg", b"old".as_slice())])
}

fn new() -> Vec<u8> {
    support::vpk_bytes(&[("cfg/addons/x.cfg", b"new release".as_slice())])
}

/// Counts `LibraryChanged` emissions from now on.
fn counter(cx: &mut TestAppContext) -> (Rc<Cell<usize>>, Subscription) {
    let count = Rc::new(Cell::new(0));
    let seen = Rc::clone(&count);
    let sub = cx.update(|cx| {
        let events = LibraryEvents::global(cx);
        cx.subscribe(&events, move |_, _: &LibraryChanged, _| {
            seen.set(seen.get() + 1)
        })
    });
    (count, sub)
}

#[gpui_kit::test]
fn changed_reaches_every_subscriber_and_global_is_one_instance(cx: &mut TestAppContext) {
    support::init(cx);
    let (count, _sub) = counter(cx);
    let (a, b) = cx.update(|cx| (LibraryEvents::global(cx), LibraryEvents::global(cx)));
    assert_eq!(a, b);
    cx.update(LibraryEvents::changed);
    cx.update(LibraryEvents::changed);
    cx.run_until_parked();
    assert_eq!(count.get(), 2);
}

// An Update download with reinstall: the import emits on Finished, the copy into tf/custom
// emits again once it succeeds.
#[gpui_kit::test]
fn downloads_emit_on_finish_and_reinstalls_on_success(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_addon(FOOT, new(), Duration::from_millis(50));
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_addons(&env, &server.uri(), &[(FOOT, "No Footsteps", "9.100.1")]);
    support::seed_library_addon(&s.services, &env, FOOT, "9.0.0", &old(), true);
    let updates = cx.update(AddonUpdates::global);
    let settled = Rc::new(Cell::new(false));
    let done = Rc::clone(&settled);
    let _settled =
        cx.update(|cx| cx.subscribe(&updates, move |_, _: &AddonEvent, _| done.set(true)));
    let (count, _sub) = counter(cx);
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), true, s.win, cx)
        })
    });
    support::wait_real(cx, "settled", |_| settled.get());
    assert_eq!(count.get(), 2);
}

#[gpui_kit::test]
fn a_failed_download_emits_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // Nothing mounted: the release URL answers 404.
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_addons(&env, &server.uri(), &[(FOOT, "No Footsteps", "9.100.1")]);
    support::seed_library_addon(&s.services, &env, FOOT, "9.0.0", &old(), true);
    let updates = cx.update(AddonUpdates::global);
    let settled = Rc::new(Cell::new(false));
    let done = Rc::clone(&settled);
    let _settled =
        cx.update(|cx| cx.subscribe(&updates, move |_, _: &AddonEvent, _| done.set(true)));
    let (count, _sub) = counter(cx);
    cx.update(|cx| {
        updates.update(cx, |u, cx| {
            u.start(foot(), "No Footsteps".into(), true, s.win, cx)
        })
    });
    support::wait_real(cx, "settled", |_| settled.get());
    assert_eq!(count.get(), 0);
    assert_eq!(
        std::fs::read(env.game.join("tf/custom").join(FOOT)).unwrap(),
        old()
    );
}
