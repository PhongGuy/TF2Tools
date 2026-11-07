use std::time::Duration;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::settings::SettingsPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::updater::{AfterInstall, REOPEN, UpdateState, Updater};

use crate::support;
use support::ShellWindow;
use support::update_fake::{
    APPIMAGE, FakeUpdates, failure, fake_updater, offer, reach_available, reach_ready, wait_state,
};

struct Card {
    _env: support::Env,
    s: ShellWindow,
    u: Entity<Updater>,
    fake: FakeUpdates,
}

fn open_settings(cx: &mut TestAppContext, s: &ShellWindow) -> Entity<SettingsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    let page = cx
        .update(|cx| s.shell.read(cx).settings_page())
        .expect("a settings page");
    support::wait_real(cx, "settings to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn card(cx: &mut TestAppContext, available: bool) -> Card {
    let env = support::env();
    let (u, fake) = fake_updater(cx, available);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    open_settings(cx, &s);
    Card {
        _env: env,
        s,
        u,
        fake,
    }
}

fn status(cx: &mut TestAppContext, c: &Card) -> Option<String> {
    support::label_of(cx, c.s.win, ids::SETTINGS_UPDATE_STATUS)
}

fn buttons(cx: &mut TestAppContext, c: &Card) -> [Option<String>; 3] {
    [
        ids::SETTINGS_UPDATE_CHECK,
        ids::SETTINGS_UPDATE_INSTALL,
        ids::SETTINGS_UPDATE_RESTART,
    ]
    .map(|id| support::label_of(cx, c.s.win, id))
}

fn some(s: &str) -> Option<String> {
    Some(s.to_owned())
}

#[gpui_kit::test]
fn the_card_keeps_the_version_and_starts_up_to_date(cx: &mut TestAppContext) {
    let c = card(cx, true);
    assert_eq!(
        support::label_of(cx, c.s.win, ids::SETTINGS_VERSION),
        Some(format!("Current version: v{}", env!("CARGO_PKG_VERSION")))
    );
    assert_eq!(status(cx, &c), some("Up to date"));
    assert_eq!(buttons(cx, &c), [some("Check for Updates"), None, None]);
}

#[gpui_kit::test]
fn an_unavailable_build_says_so_calmly_and_links_the_releases(cx: &mut TestAppContext) {
    let c = card(cx, false);
    assert_eq!(
        status(cx, &c),
        some(
            "Automatic updates aren't available in this version. Get new versions from the release page."
        )
    );
    assert_eq!(buttons(cx, &c), [None, some("Open release page"), None]);
    support::click_id(cx, c.s.win, ids::SETTINGS_UPDATE_INSTALL);
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(support::update_fake::RELEASES)
    );
    assert!(
        support::toast_labels(cx, c.s.win)
            .iter()
            .all(|t| !t.contains("Update"))
    );
}

#[gpui_kit::test]
fn check_for_updates_runs_a_check_and_shows_progress(cx: &mut TestAppContext) {
    let c = card(cx, true);
    support::click_id(cx, c.s.win, ids::SETTINGS_UPDATE_CHECK);
    let reply = c.fake.next_check(cx);
    assert_eq!(status(cx, &c), some("Checking for updates..."));
    assert_eq!(buttons(cx, &c), [None, None, None]);
    reply.send(Ok(Some(offer("6.0.1", true)))).unwrap();
    wait_state(cx, &c.u, "Available", |s| {
        matches!(s, UpdateState::Available(_))
    });
    assert_eq!(status(cx, &c), some("v6.0.1 available"));
    assert_eq!(
        buttons(cx, &c),
        [some("Check for Updates"), some("Download & Install"), None]
    );
}

#[gpui_kit::test]
fn an_unmanaged_install_is_pointed_at_the_release_page(cx: &mut TestAppContext) {
    let c = card(cx, true);
    reach_available(cx, &c.u, &c.fake, offer("6.0.1", false));
    assert_eq!(
        status(cx, &c),
        some("v6.0.1 available. Download it from the release page.")
    );
    assert_eq!(
        buttons(cx, &c),
        [some("Check for Updates"), some("Open release page"), None]
    );
    support::click_id(cx, c.s.win, ids::SETTINGS_UPDATE_INSTALL);
    assert_eq!(cx.opened_url(), Some(offer("6.0.1", false).release_page));
}

#[gpui_kit::test]
fn download_and_install_shows_the_percent_then_restart(cx: &mut TestAppContext) {
    let c = card(cx, true);
    reach_available(cx, &c.u, &c.fake, offer("6.0.1", true));
    support::click_id(cx, c.s.win, ids::SETTINGS_UPDATE_INSTALL);
    let (sink, reply) = c.fake.next_download(cx);
    sink.report(7, 10);
    support::next_repaint(cx);
    assert_eq!(status(cx, &c), some("Downloading... 70%"));
    assert_eq!(buttons(cx, &c), [None, None, None]);
    drop(sink);
    reply.send(Ok(())).unwrap();
    wait_state(cx, &c.u, "Ready", |s| matches!(s, UpdateState::Ready(_)));
    assert_eq!(status(cx, &c), some("Update ready — restart to apply"));
    assert_eq!(buttons(cx, &c), [None, None, some("Restart Now")]);
    support::click_id(cx, c.s.win, ids::SETTINGS_UPDATE_RESTART);
    support::wait_real(cx, "the relaunch", |_| !c.fake.restarts.borrow().is_empty());
    assert_eq!(
        *c.fake.restarts.borrow(),
        [AfterInstall::Relaunch(std::path::PathBuf::from(APPIMAGE))]
    );
}

#[gpui_kit::test]
fn a_failed_check_shows_its_advice_and_can_be_retried(cx: &mut TestAppContext) {
    let c = card(cx, true);
    support::click_id(cx, c.s.win, ids::SETTINGS_UPDATE_CHECK);
    let message = "Couldn't reach the update server. Check your internet connection and try again.";
    c.fake
        .next_check(cx)
        .send(Err(failure(message, false)))
        .unwrap();
    wait_state(cx, &c.u, "Failed", |s| matches!(s, UpdateState::Failed(_)));
    assert_eq!(status(cx, &c), some(message));
    assert_eq!(buttons(cx, &c), [some("Check for Updates"), None, None]);
    support::click_id(cx, c.s.win, ids::SETTINGS_UPDATE_CHECK);
    let _again = c.fake.next_check(cx);
}

#[gpui_kit::test]
fn installing_shows_no_buttons(cx: &mut TestAppContext) {
    let c = card(cx, true);
    reach_ready(cx, &c.u, &c.fake, offer("6.0.1", true));
    // Holding the fake's apply keeps the state at Installing while the card is read.
    let held = c.fake.apply_hold.lock().unwrap();
    cx.update(|cx| c.u.update(cx, |u, cx| u.restart(cx)));
    assert_eq!(status(cx, &c), some("Installing update..."));
    assert_eq!(buttons(cx, &c), [None, None, None]);
    drop(held);
    support::wait_real(cx, "the relaunch", |_| !c.fake.restarts.borrow().is_empty());
}

// An install whose relaunch never quit asks the player to reopen by hand,
// with nothing to click.
#[gpui_kit::test]
fn an_update_that_did_not_relaunch_asks_to_reopen(cx: &mut TestAppContext) {
    let c = card(cx, true);
    cx.update(|cx| {
        c.u.update(cx, |u, _| u.set_restart_grace(Duration::from_secs(1)))
    });
    reach_ready(cx, &c.u, &c.fake, offer("6.0.1", true));
    support::click_id(cx, c.s.win, ids::SETTINGS_UPDATE_RESTART);
    wait_state(cx, &c.u, "Updated", |s| {
        matches!(s, UpdateState::Updated(_))
    });
    assert_eq!(status(cx, &c), some(REOPEN));
    assert_eq!(buttons(cx, &c), [None, None, None]);
}

// A line too long to sit beside the buttons puts them under it, so it never
// squeezes into a narrow column; a short line keeps v5's single row.
#[gpui_kit::test]
fn a_long_line_puts_the_buttons_under_it(cx: &mut TestAppContext) {
    let c = card(cx, true);
    reach_available(cx, &c.u, &c.fake, offer("6.0.1", true));
    let line = support::bounds_of(cx, c.s.win, ids::SETTINGS_UPDATE_STATUS).unwrap();
    let button = support::bounds_of(cx, c.s.win, ids::SETTINGS_UPDATE_INSTALL).unwrap();
    assert!(button.top() < line.bottom(), "{button:?} beside {line:?}");
    reach_available(cx, &c.u, &c.fake, offer("6.0.1", false));
    let line = support::bounds_of(cx, c.s.win, ids::SETTINGS_UPDATE_STATUS).unwrap();
    let button = support::bounds_of(cx, c.s.win, ids::SETTINGS_UPDATE_INSTALL).unwrap();
    assert!(button.top() >= line.bottom(), "{button:?} under {line:?}");
}
