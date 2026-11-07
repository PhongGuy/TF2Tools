use gpui_kit::TestAppContext;
use tf2tools_app::crash::notice::{CrashNotice, NOTICE, crash_notice};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;

use crate::support;

const REPORT: &str = "TF2Tools 6.0.0 closed unexpectedly.\nMessage: boom\n";

fn shell_with(
    cx: &mut TestAppContext,
    report: Option<&str>,
) -> (support::Env, support::ShellWindow) {
    let env = support::env();
    cx.update(|cx| {
        CrashNotice::install(cx, report.map(str::to_owned));
    });
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    (env, s)
}

#[gpui_kit::test]
fn no_banner_without_a_crash(cx: &mut TestAppContext) {
    let (_env, s) = shell_with(cx, None);
    assert!(!support::present(cx, s.win, ids::CRASH_BANNER));
}

#[gpui_kit::test]
fn the_banner_says_so_in_plain_words(cx: &mut TestAppContext) {
    let (_env, s) = shell_with(cx, Some(REPORT));
    assert!(support::present(cx, s.win, ids::CRASH_BANNER));
    let text = support::label_of(cx, s.win, ids::CRASH_BANNER_TEXT).unwrap();
    assert_eq!(text, NOTICE);
    assert_eq!(text, "TF2Tools closed unexpectedly last time.");
    support::assert_plain(&text);
    assert_eq!(
        support::label_of(cx, s.win, ids::CRASH_BANNER_COPY).as_deref(),
        Some("Copy details")
    );
    assert!(support::present(cx, s.win, ids::CRASH_BANNER_DISMISS));
}

#[gpui_kit::test]
fn copy_puts_the_details_on_the_clipboard_and_says_so(cx: &mut TestAppContext) {
    let (_env, s) = shell_with(cx, Some(REPORT));
    support::click_id(cx, s.win, ids::CRASH_BANNER_COPY);
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some(REPORT)
    );
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts.contains(&"Crash details copied. Paste them into your bug report.".to_owned()),
        "{toasts:?}"
    );
    assert!(
        support::present(cx, s.win, ids::CRASH_BANNER),
        "copy keeps the banner"
    );
}

#[gpui_kit::test]
fn dismiss_hides_the_banner(cx: &mut TestAppContext) {
    let (_env, s) = shell_with(cx, Some(REPORT));
    support::click_id(cx, s.win, ids::CRASH_BANNER_DISMISS);
    assert!(!support::present(cx, s.win, ids::CRASH_BANNER));
    let notice = cx.update(|cx| crash_notice(cx)).unwrap();
    assert_eq!(
        cx.update(|cx| notice.read(cx).report().map(str::to_owned)),
        None
    );
}
