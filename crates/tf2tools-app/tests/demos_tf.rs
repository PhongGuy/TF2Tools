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
