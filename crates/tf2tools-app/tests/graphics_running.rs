//! The TF2-running banner: the shell's remembered answer, pushed into the page.

use crate::support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::TestAppContext;
use support::graphics::{
    HEADER, found, idle, modules_cfg, open, reveal, saved, value, wait_for_list,
};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::probe::Tf2Probe;

#[gpui_kit::test]
fn the_banner_follows_the_header_and_only_warns(cx: &mut TestAppContext) {
    let env = support::env();
    let running = Arc::new(AtomicBool::new(true));
    let probe = Arc::clone(&running);
    cx.update(|cx| Tf2Probe::install(cx, move || probe.load(Ordering::SeqCst)));
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    support::wait_real(cx, "the running answer", |cx| {
        cx.update(|cx| page.read(cx).tf2_running())
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_RUNNING).as_deref(),
        Some("TF2 is running. Your changes apply the next time it starts.")
    );
    let status = support::bounds_of(cx, s.win, ids::GRAPHICS_STATUS).unwrap();
    let banner = support::bounds_of(cx, s.win, ids::GRAPHICS_RUNNING).unwrap();
    assert!(banner.top() >= status.bottom(), "{banner:?} {status:?}");
    assert!(
        support::disabled_of(cx, s.win, ids::PLAY_TF2),
        "the header agrees"
    );
    reveal(cx, &s, ids::graphics_switch("vsync"));
    support::click_id(cx, s.win, ids::graphics_switch("vsync"));
    support::wait_real(cx, "vsync on", |cx| {
        saved(cx, &page, "vsync") == Some(value("on"))
    });
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}vsync=on\n").as_str())
    );
    running.store(false, Ordering::SeqCst);
    cx.update(|cx| s.tf2.update(cx, |_, cx| cx.notify()));
    support::wait_real(cx, "TF2 closed", |cx| {
        cx.update(|cx| !page.read(cx).tf2_running())
    });
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::GRAPHICS_RUNNING));
    assert!(!support::disabled_of(cx, s.win, ids::PLAY_TF2));
}

#[gpui_kit::test]
fn no_banner_while_tf2_is_closed(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let page = open(cx, &s);
    assert!(!cx.update(|cx| page.read(cx).tf2_running()));
    assert!(!support::present(cx, s.win, ids::GRAPHICS_RUNNING));
}
