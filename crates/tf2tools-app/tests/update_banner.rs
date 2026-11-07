use std::time::Duration;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::leave;
use tf2tools_app::shell::route::Page;
use tf2tools_app::updater::backend::describe_download;
use tf2tools_app::updater::{AfterInstall, REOPEN, UpdateState, Updater};
use tf2tools_core::UpdateDownloadError;

use crate::support;
use support::ShellWindow;
use support::update_fake::{
    APPIMAGE, FakeUpdates, failure, fake_updater, offer, reach_available, reach_ready, state,
    wait_state,
};

struct Banner {
    _env: support::Env,
    s: ShellWindow,
    u: Entity<Updater>,
    fake: FakeUpdates,
}

fn banner(cx: &mut TestAppContext) -> Banner {
    let env = support::env();
    let (u, fake) = fake_updater(cx, true);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    Banner {
        _env: env,
        s,
        u,
        fake,
    }
}

fn label(cx: &mut TestAppContext, b: &Banner, id: &'static str) -> Option<String> {
    support::label_of(cx, b.s.win, id)
}

#[gpui_kit::test]
fn no_banner_while_idle_up_to_date_or_failed(cx: &mut TestAppContext) {
    let b = banner(cx);
    assert!(!support::present(cx, b.s.win, ids::UPDATE_BANNER));
    cx.update(|cx| b.u.update(cx, |u, cx| u.check(cx)));
    assert!(
        !support::present(cx, b.s.win, ids::UPDATE_BANNER),
        "not while checking"
    );
    b.fake.next_check(cx).send(Ok(None)).unwrap();
    wait_state(cx, &b.u, "UpToDate", |s| *s == UpdateState::UpToDate);
    assert!(!support::present(cx, b.s.win, ids::UPDATE_BANNER));
    cx.update(|cx| b.u.update(cx, |u, cx| u.check(cx)));
    b.fake
        .next_check(cx)
        .send(Err(failure(
            "Couldn't reach the update server. Check your internet connection and try again.",
            false,
        )))
        .unwrap();
    wait_state(cx, &b.u, "Failed", |s| matches!(s, UpdateState::Failed(_)));
    assert!(!support::present(cx, b.s.win, ids::UPDATE_BANNER));
}

#[gpui_kit::test]
fn no_banner_when_updates_are_unavailable(cx: &mut TestAppContext) {
    let env = support::env();
    let (_u, _fake) = fake_updater(cx, false);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    assert!(!support::present(cx, s.win, ids::UPDATE_BANNER));
}

#[gpui_kit::test]
fn an_available_update_sits_under_the_header_and_downloads(cx: &mut TestAppContext) {
    let b = banner(cx);
    reach_available(cx, &b.u, &b.fake, offer("6.0.1", true));
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_TEXT).as_deref(),
        Some("Update v6.0.1 available")
    );
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_ACTION).as_deref(),
        Some("Update")
    );
    assert!(support::present(cx, b.s.win, ids::UPDATE_BANNER_DISMISS));
    let header = support::bounds_of(cx, b.s.win, ids::HEADER).unwrap();
    let bar = support::bounds_of(cx, b.s.win, ids::UPDATE_BANNER).unwrap();
    assert_eq!(
        bar.top(),
        header.bottom(),
        "directly under the 56 px header"
    );
    assert_eq!((bar.left(), bar.right()), (header.left(), header.right()));
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    let _download = b.fake.next_download(cx);
}

#[gpui_kit::test]
fn an_unmanaged_install_gets_a_one_click_release_page(cx: &mut TestAppContext) {
    let b = banner(cx);
    reach_available(cx, &b.u, &b.fake, offer("6.0.1", false));
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_TEXT).as_deref(),
        Some("Update v6.0.1 available")
    );
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_ACTION).as_deref(),
        Some("Open release page")
    );
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    assert_eq!(cx.opened_url(), Some(offer("6.0.1", false).release_page));
    assert!(!b.fake.download_pending());
}

// A server error is usually brief: the banner keeps Update, so the player retries
// without knowing to check for updates again.
#[gpui_kit::test]
fn a_server_error_keeps_the_update_button_for_a_retry(cx: &mut TestAppContext) {
    let b = banner(cx);
    reach_available(cx, &b.u, &b.fake, offer("6.0.1", true));
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    let (sink, reply) = b.fake.next_download(cx);
    drop(sink);
    let busy = describe_download(&UpdateDownloadError::Status {
        url: "u".into(),
        status: 503,
    });
    reply.send(Err(busy)).unwrap();
    wait_state(cx, &b.u, "Available again", |s| {
        matches!(s, UpdateState::Available(_))
    });
    assert_eq!(
        state(cx, &b.u),
        UpdateState::Available(offer("6.0.1", true))
    );
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_ACTION).as_deref(),
        Some("Update")
    );
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    let _retry = b.fake.next_download(cx);
}

#[gpui_kit::test]
fn downloading_shows_progress_and_cannot_be_dismissed(cx: &mut TestAppContext) {
    let b = banner(cx);
    reach_available(cx, &b.u, &b.fake, offer("6.0.1", true));
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    let (sink, _reply) = b.fake.next_download(cx);
    sink.report(42, 100);
    support::next_repaint(cx);
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_TEXT).as_deref(),
        Some("Downloading...")
    );
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_PERCENT).as_deref(),
        Some("42%")
    );
    assert!(!support::present(cx, b.s.win, ids::UPDATE_BANNER_DISMISS));
    assert!(!support::present(cx, b.s.win, ids::UPDATE_BANNER_ACTION));
}

#[gpui_kit::test]
fn a_ready_update_restarts_from_the_banner(cx: &mut TestAppContext) {
    let b = banner(cx);
    reach_ready(cx, &b.u, &b.fake, offer("6.0.1", true));
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_TEXT).as_deref(),
        Some("Update ready — restart to apply")
    );
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_ACTION).as_deref(),
        Some("Restart")
    );
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    support::wait_real(cx, "the relaunch", |_| !b.fake.restarts.borrow().is_empty());
    assert_eq!(
        *b.fake.restarts.borrow(),
        [AfterInstall::Relaunch(std::path::PathBuf::from(APPIMAGE))]
    );
}

#[gpui_kit::test]
fn the_x_hides_the_banner_for_the_session(cx: &mut TestAppContext) {
    let b = banner(cx);
    reach_available(cx, &b.u, &b.fake, offer("6.0.1", true));
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_DISMISS);
    assert!(!support::present(cx, b.s.win, ids::UPDATE_BANNER));
    assert!(
        matches!(state(cx, &b.u), UpdateState::Available(_)),
        "dismiss keeps the offer"
    );
}

#[gpui_kit::test]
fn a_failed_download_toasts_in_plain_words(cx: &mut TestAppContext) {
    let b = banner(cx);
    reach_available(cx, &b.u, &b.fake, offer("6.0.1", true));
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    let (sink, reply) = b.fake.next_download(cx);
    drop(sink);
    reply
        .send(Err(failure("The download was damaged. Try again.", false)))
        .unwrap();
    wait_state(cx, &b.u, "Available again", |s| {
        matches!(s, UpdateState::Available(_))
    });
    let toasts = support::toast_labels(cx, b.s.win);
    assert!(
        toasts
            .iter()
            .any(|t| t == "Update failed. The download was damaged. Try again."),
        "{toasts:?}"
    );
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_ACTION).as_deref(),
        Some("Update")
    );
}

// An install whose relaunch never quit asks the player to reopen, with
// nothing left to click but the X.
#[gpui_kit::test]
fn an_update_that_did_not_relaunch_asks_to_reopen(cx: &mut TestAppContext) {
    let b = banner(cx);
    cx.update(|cx| {
        b.u.update(cx, |u, _| u.set_restart_grace(Duration::from_secs(1)))
    });
    reach_ready(cx, &b.u, &b.fake, offer("6.0.1", true));
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    support::wait_real(cx, "the relaunch", |_| !b.fake.restarts.borrow().is_empty());
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_TEXT).as_deref(),
        Some("Installing update...")
    );
    assert!(!support::present(cx, b.s.win, ids::UPDATE_BANNER_ACTION));
    assert!(!support::present(cx, b.s.win, ids::UPDATE_BANNER_DISMISS));
    wait_state(cx, &b.u, "Updated", |s| {
        matches!(s, UpdateState::Updated(_))
    });
    assert_eq!(
        label(cx, &b, ids::UPDATE_BANNER_TEXT).as_deref(),
        Some(REOPEN)
    );
    assert!(!support::present(cx, b.s.win, ids::UPDATE_BANNER_ACTION));
}

// The kit's default bundle lacks some lucide icons, and a missing one draws nothing.
#[test]
fn the_banner_icons_ship_with_the_app() {
    use gpui_kit::AssetSource as _;
    use gpui_kit::assets::IconName;
    use tf2tools_app::assets::AppAssets;
    for icon in [IconName::Download, IconName::RotateCw, IconName::X] {
        let path = icon.path();
        assert!(
            AppAssets.load(&path).unwrap().is_some(),
            "{path} is missing"
        );
    }
}

// The banner keeps one height in every state, so the page never shifts when
// a download starts or ends.
#[gpui_kit::test]
fn the_banner_keeps_one_height(cx: &mut TestAppContext) {
    let b = banner(cx);
    let height = |cx: &mut TestAppContext| {
        support::bounds_of(cx, b.s.win, ids::UPDATE_BANNER)
            .expect("the banner")
            .size
            .height
    };
    reach_available(cx, &b.u, &b.fake, offer("6.0.1", true));
    let available = height(cx);
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    let (sink, reply) = b.fake.next_download(cx);
    sink.report(5, 10);
    support::next_repaint(cx);
    assert_eq!(height(cx), available, "downloading");
    drop(sink);
    reply.send(Ok(())).unwrap();
    wait_state(cx, &b.u, "Ready", |s| matches!(s, UpdateState::Ready(_)));
    assert_eq!(height(cx), available, "ready");
}

// A line too long for the banner is cut short on one line, keeping its
// buttons in view.
#[gpui_kit::test]
fn a_long_banner_line_stays_on_one_line(cx: &mut TestAppContext) {
    let b = banner(cx);
    reach_available(cx, &b.u, &b.fake, offer("6.0.1", true));
    let short = support::bounds_of(cx, b.s.win, ids::UPDATE_BANNER_TEXT).unwrap();
    let long = format!("6.0.1-{}", "prerelease".repeat(30));
    reach_available(cx, &b.u, &b.fake, offer(&long, true));
    let text = support::bounds_of(cx, b.s.win, ids::UPDATE_BANNER_TEXT).unwrap();
    let action = support::bounds_of(cx, b.s.win, ids::UPDATE_BANNER_ACTION).unwrap();
    let whole = support::bounds_of(cx, b.s.win, ids::UPDATE_BANNER).unwrap();
    assert_eq!(text.size.height, short.size.height, "{text:?}");
    assert!(
        text.right() <= action.left(),
        "{text:?} overlaps {action:?}"
    );
    assert!(
        action.right() <= whole.right(),
        "{action:?} outside {whole:?}"
    );
}

#[gpui_kit::test]
fn restart_waits_while_the_maker_has_unsaved_changes(cx: &mut TestAppContext) {
    let b = banner(cx);
    reach_ready(cx, &b.u, &b.fake, offer("6.0.1", true));
    support::click_id(cx, b.s.win, ids::nav(Page::CrosshairMaker));
    // A click on the grid paints its centre cell.
    support::click_id(cx, b.s.win, ids::CROSSHAIR_MAKER_GRID);
    assert!(!cx.update(|cx| leave::can_leave(cx)));
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    assert!(support::present(cx, b.s.win, ids::CONFIRM_DIALOG));
    support::click_id(cx, b.s.win, ids::CONFIRM_CANCEL);
    assert!(b.fake.restarts.borrow().is_empty());
    assert!(matches!(state(cx, &b.u), UpdateState::Ready(_)));
    support::click_id(cx, b.s.win, ids::UPDATE_BANNER_ACTION);
    support::click_id(cx, b.s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the relaunch", |_| !b.fake.restarts.borrow().is_empty());
}
