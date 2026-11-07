//! Where a player finds help or reports a problem: the end of Settings, and the crash banner.
mod support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ScrollDelta, TestAppContext, point, px};
use tf2tools_app::crash::notice::CrashNotice;
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;

const REPORT_PAGE: &str = "https://github.com/PhongGuy/TF2Tools/issues/new/choose";
const HELP_PAGE: &str = "https://github.com/PhongGuy/TF2Tools#readme";

// Opens Settings and wheels to its end, where the Help and feedback card sits.
fn settings_end(cx: &mut TestAppContext) -> (support::Env, support::ShellWindow) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    let page = cx
        .update(|cx| s.shell.read(cx).settings_page())
        .expect("a settings page");
    support::wait_real(cx, "settings to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    cx.update_window(s.win, |_, w, cx| {
        let down = ScrollDelta::Pixels(point(px(0.), px(-4000.)));
        w.scroll(ids::page(Page::Settings), down, cx)
    })
    .unwrap();
    support::settle(cx, s.win);
    (env, s)
}

#[gpui_kit::test]
fn settings_ends_with_help_and_feedback(cx: &mut TestAppContext) {
    let (_env, s) = settings_end(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_REPORT_PROBLEM).as_deref(),
        Some("Report a problem")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_HELP).as_deref(),
        Some("Help")
    );
    let notices = support::bounds_of(cx, s.win, ids::SETTINGS_NOTICES).unwrap();
    let report = support::bounds_of(cx, s.win, ids::SETTINGS_REPORT_PROBLEM).unwrap();
    assert!(report.top() > notices.bottom(), "{report:?} {notices:?}");
}

#[gpui_kit::test]
fn report_a_problem_opens_the_new_issue_page(cx: &mut TestAppContext) {
    let (_env, s) = settings_end(cx);
    support::click_id(cx, s.win, ids::SETTINGS_REPORT_PROBLEM);
    assert_eq!(cx.opened_url().as_deref(), Some(REPORT_PAGE));
}

#[gpui_kit::test]
fn help_opens_the_readme(cx: &mut TestAppContext) {
    let (_env, s) = settings_end(cx);
    support::click_id(cx, s.win, ids::SETTINGS_HELP);
    assert_eq!(cx.opened_url().as_deref(), Some(HELP_PAGE));
}

#[gpui_kit::test]
fn the_crash_banner_offers_the_report_page(cx: &mut TestAppContext) {
    let env = support::env();
    cx.update(|cx| {
        CrashNotice::install(cx, Some("TF2Tools closed unexpectedly.\n".to_owned()));
    });
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    assert_eq!(
        support::label_of(cx, s.win, ids::CRASH_BANNER_REPORT).as_deref(),
        Some("Report a problem")
    );
    support::click_id(cx, s.win, ids::CRASH_BANNER_REPORT);
    assert_eq!(cx.opened_url().as_deref(), Some(REPORT_PAGE));
    assert!(
        support::present(cx, s.win, ids::CRASH_BANNER),
        "reporting keeps the banner"
    );
}
