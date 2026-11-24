//! The Demos.tf page: its list placeholder, opening a demo into the viewer and the way back.

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;

use crate::support;
use crate::support::demo_view_fixtures::demo_header;
use crate::support::demo_viewer::{StubAnswer, demo_file, demo_viewer_shell, install_map};
use crate::support::graphics::found;

#[gpui_kit::test]
fn the_page_starts_on_the_list_with_open_demo(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::click_nav(cx, s.win, Page::DemosTf);
    assert!(support::present(cx, s.win, ids::DEMOS_TF_LIST));
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMOS_TF_OPEN).as_deref(),
        Some("Open demo")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMOS_TF_COMING).as_deref(),
        Some("The demos.tf list is coming. For now, open a demo from your computer.")
    );
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER));
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_BAR));
}

#[gpui_kit::test]
fn open_demo_goes_to_the_viewer(cx: &mut TestAppContext) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    assert!(!support::present(cx, w.s.win, ids::DEMOS_TF_LIST));
    assert!(support::present(cx, w.s.win, ids::DEMO_VIEWER_BAR));
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMO_VIEWER_CAPTION).as_deref(),
        Some("cp_synthetic")
    );
    let title = cx.update(|cx| {
        w.s.shell
            .read(cx)
            .demo_viewer_page(cx)
            .map(|v| v.read(cx).title())
    });
    assert_eq!(title.as_deref(), Some("cp_synthetic"));
}

#[gpui_kit::test]
fn a_cancelled_pick_stays_on_the_list(cx: &mut TestAppContext) {
    let env = support::env();
    let w = demo_viewer_shell(cx, &env, found(&env), vec![], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::settle(cx, w.s.win);
    assert_eq!(w.picker.asked().len(), 1);
    assert!(support::present(cx, w.s.win, ids::DEMOS_TF_LIST));
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER));
}

// Opens a synthetic demo from the list and waits for its viewport.
fn open_synthetic(
    cx: &mut TestAppContext,
) -> (support::Env, crate::support::demo_viewer::DemoViewerShell) {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    (env, w)
}

#[gpui_kit::test]
fn the_crumb_names_the_page_and_the_map(cx: &mut TestAppContext) {
    let (_env, w) = open_synthetic(cx);
    assert!(!support::present(cx, w.s.win, ids::HEADER_TITLE));
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMOS_TF_CRUMB).as_deref(),
        Some("Back to the demo list")
    );
    assert_eq!(
        support::label_of(cx, w.s.win, ids::DEMOS_TF_CRUMB_NAME).as_deref(),
        Some("cp_synthetic")
    );
}

#[gpui_kit::test]
fn back_returns_to_the_list_and_drops_the_viewer(cx: &mut TestAppContext) {
    let (_env, w) = open_synthetic(cx);
    support::click_id(cx, w.s.win, ids::DEMOS_TF_CRUMB);
    support::settle(cx, w.s.win);
    assert!(support::present(cx, w.s.win, ids::DEMOS_TF_LIST));
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER));
    assert!(!support::present(cx, w.s.win, ids::DEMOS_TF_CRUMB));
    assert_eq!(
        support::label_of(cx, w.s.win, ids::HEADER_TITLE).as_deref(),
        Some("Demos.tf")
    );
    assert!(cx.update(|cx| w.s.shell.read(cx).demo_viewer_page(cx).is_none()));
    // The viewer's renderer went with it, so its GPU mesh is freed.
    let stubs = w.stubs.lock().unwrap();
    assert_eq!(stubs.len(), 1);
    assert!(!stubs[0].is_alive());
}

#[gpui_kit::test]
fn leaving_the_page_drops_the_viewer_and_comes_back_on_the_list(cx: &mut TestAppContext) {
    let (_env, w) = open_synthetic(cx);
    support::click_nav(cx, w.s.win, Page::Settings);
    assert!(!support::present(cx, w.s.win, ids::DEMOS_TF_CRUMB));
    assert!(!w.stubs.lock().unwrap()[0].is_alive());
    support::click_nav(cx, w.s.win, Page::DemosTf);
    assert!(support::present(cx, w.s.win, ids::DEMOS_TF_LIST));
    assert!(!support::present(cx, w.s.win, ids::DEMO_VIEWER));
    assert!(!support::present(cx, w.s.win, ids::DEMOS_TF_CRUMB));
}
