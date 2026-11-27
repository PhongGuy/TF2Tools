//! View demo from a logs.tf log: the Demos.tf viewer it opens and the way back to the log.

use std::path::PathBuf;

use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::shell::route::Page;

use crate::support;
use crate::support::demo_view_fixtures::demo_header;
use crate::support::demo_viewer::{demo_file, install_map};
use crate::support::demostf::{DemosTf, demos_tf_shell, mount_first};

fn file() -> Vec<u8> {
    demo_header("cp_synthetic", 60.0)
}

// The Demos.tf page over an empty demos.tf list, and a cp_synthetic demo on disk whose map is
// installed.
fn demos_page(cx: &mut TestAppContext) -> (support::Env, support::StoreServer, DemosTf, PathBuf) {
    let (env, server) = (support::env(), support::StoreServer::start());
    install_map(&env, "cp_synthetic");
    mount_first(&server, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    let path = demo_file(&env, "logs.dem", &file());
    (env, server, d, path)
}

// Opens `path` the way logs.tf's View demo does, then waits for the viewport.
fn open_from_logs(cx: &mut TestAppContext, d: &DemosTf, path: PathBuf) {
    let page = d.page.clone();
    cx.update_window(d.s.win, |_, window, cx| {
        page.update(cx, |page, cx| {
            page.open_from(path, Page::LogsTf, window, cx)
        });
    })
    .unwrap();
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, d.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
}

#[gpui_kit::test]
fn a_demo_opened_from_logs_tf_offers_the_way_back_to_the_log(cx: &mut TestAppContext) {
    let (_env, _server, d, path) = demos_page(cx);
    open_from_logs(cx, &d, path);
    assert_eq!(
        support::label_of(cx, d.s.win, ids::DEMOS_TF_CRUMB).as_deref(),
        Some("Back to the log")
    );
    assert_eq!(
        support::label_of(cx, d.s.win, ids::DEMOS_TF_CRUMB_NAME).as_deref(),
        Some("cp_synthetic")
    );
    assert_eq!(cx.update(|cx| d.page.read(cx).origin()), Some(Page::LogsTf));
}

#[gpui_kit::test]
fn back_from_a_logs_tf_demo_returns_to_logs_tf_and_frees_the_viewer(cx: &mut TestAppContext) {
    let (_env, _server, d, path) = demos_page(cx);
    open_from_logs(cx, &d, path);
    support::click_id(cx, d.s.win, ids::DEMOS_TF_CRUMB);
    support::settle(cx, d.s.win);
    assert_eq!(cx.update(|cx| d.s.shell.read(cx).page()), Page::LogsTf);
    assert!(cx.update(|cx| d.page.read(cx).viewer().is_none()));
    assert_eq!(cx.update(|cx| d.page.read(cx).origin()), None);
    let stubs = d.stubs.lock().unwrap();
    assert_eq!(stubs.len(), 1);
    assert!(!stubs[0].is_alive(), "the viewer's renderer is freed");
}

#[gpui_kit::test]
fn leaving_a_logs_tf_demo_by_the_sidebar_goes_where_the_player_clicked(cx: &mut TestAppContext) {
    let (_env, _server, d, path) = demos_page(cx);
    open_from_logs(cx, &d, path);
    support::click_nav(cx, d.s.win, Page::Settings);
    support::settle(cx, d.s.win);
    assert_eq!(cx.update(|cx| d.s.shell.read(cx).page()), Page::Settings);
    assert!(cx.update(|cx| d.page.read(cx).viewer().is_none()));
    assert_eq!(cx.update(|cx| d.page.read(cx).origin()), None);
    support::click_nav(cx, d.s.win, Page::DemosTf);
    support::settle(cx, d.s.win);
    assert!(support::present(cx, d.s.win, ids::DEMOS_TF_LIST));
    assert!(!support::present(cx, d.s.win, ids::DEMOS_TF_CRUMB));
}
