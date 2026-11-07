use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::TestAppContext;
use tf2tools_app::updater::{AfterInstall, UpdateEvent, UpdateState};
use tf2tools_core::LogLevel;

use crate::support;
use support::update_fake::{
    APPIMAGE, act, events, failure, fake_updater, offer, reach_available, reach_ready, state,
    update_rows, visible, wait_state, with_services,
};

const USE_PAGE: &str = "TF2Tools can't update itself where it's installed. Download the new version from the release page instead.";

#[gpui_kit::test]
fn download_progress_reaches_the_ui_at_most_once_per_repaint_interval(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, true);
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    assert!(act(cx, &u, |u, cx| u.download(cx)));
    let (sink, _reply) = fake.next_download(cx);
    cx.run_until_parked();
    let notes = Rc::new(Cell::new(0));
    let n = notes.clone();
    cx.update(|cx| cx.observe(&u, move |_, _| n.set(n.get() + 1)).detach());
    let percent = |cx: &mut TestAppContext| match state(cx, &u) {
        UpdateState::Downloading { percent, .. } => percent,
        other => panic!("not downloading: {other:?}"),
    };

    for received in 1..=50 {
        sink.report(received, 100);
    }
    cx.run_until_parked();
    assert_eq!(percent(cx), 50);
    assert_eq!(notes.get(), 1, "a burst of reports is one repaint");

    sink.report(60, 100);
    cx.run_until_parked();
    assert_eq!(percent(cx), 50, "held inside the interval");
    support::next_repaint(cx);
    assert_eq!(percent(cx), 60);
    assert_eq!(notes.get(), 2);
}

#[gpui_kit::test]
fn downloading_ignores_checks_downloads_and_dismiss(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, true);
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    act(cx, &u, |u, cx| u.download(cx));
    let _download = fake.next_download(cx);
    assert!(!act(cx, &u, |u, cx| u.check(cx)));
    assert!(!act(cx, &u, |u, cx| u.download(cx)));
    act(cx, &u, |u, cx| u.dismiss(cx));
    assert!(visible(cx, &u), "the banner stays while downloading");
    cx.run_until_parked();
    assert!(!fake.check_pending() && !fake.download_pending());
}

#[gpui_kit::test]
fn a_finished_download_is_ready_logged_and_keeps_its_staged_file(cx: &mut TestAppContext) {
    let env = support::env();
    let services = with_services(cx, &env);
    let (u, fake) = fake_updater(cx, true);
    reach_ready(cx, &u, &fake, offer("6.0.1", true));
    assert_eq!(state(cx, &u), UpdateState::Ready(offer("6.0.1", true)));
    assert!(
        !act(cx, &u, |u, cx| u.check(cx)),
        "a check would forget the staged update"
    );
    support::wait_real(cx, "the download row", |_| {
        !update_rows(&services).is_empty()
    });
    assert_eq!(
        update_rows(&services),
        [(
            LogLevel::Info,
            "update-download".to_owned(),
            "Downloaded TF2Tools v6.0.1".to_owned()
        )]
    );
}

#[gpui_kit::test]
fn a_failed_download_returns_to_available_raises_and_logs(cx: &mut TestAppContext) {
    let env = support::env();
    let services = with_services(cx, &env);
    let (u, fake) = fake_updater(cx, true);
    let seen = events(cx, &u);
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    act(cx, &u, |u, cx| u.download(cx));
    let (sink, reply) = fake.next_download(cx);
    drop(sink);
    let f = failure("The download was damaged. Try again.", false);
    reply.send(Err(f.clone())).unwrap();
    wait_state(cx, &u, "Available again", |s| {
        matches!(s, UpdateState::Available(_))
    });
    assert_eq!(state(cx, &u), UpdateState::Available(offer("6.0.1", true)));
    assert_eq!(*seen.borrow(), [UpdateEvent::DownloadFailed(f.clone())]);
    support::wait_real(cx, "the error row", |_| !update_rows(&services).is_empty());
    assert_eq!(
        update_rows(&services),
        [(
            LogLevel::Error,
            "update-download".to_owned(),
            f.message.clone()
        )]
    );
    assert!(
        act(cx, &u, |u, cx| u.download(cx)),
        "the player can try again"
    );
}

#[gpui_kit::test]
fn a_download_that_cannot_be_saved_here_falls_back_to_the_release_page(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, true);
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    act(cx, &u, |u, cx| u.download(cx));
    let (sink, reply) = fake.next_download(cx);
    drop(sink);
    reply.send(Err(failure(USE_PAGE, true))).unwrap();
    wait_state(cx, &u, "Available again", |s| {
        matches!(s, UpdateState::Available(_))
    });
    assert_eq!(state(cx, &u), UpdateState::Available(offer("6.0.1", false)));
    assert!(!act(cx, &u, |u, cx| u.download(cx)));
}

#[gpui_kit::test]
fn restart_applies_logs_then_relaunches(cx: &mut TestAppContext) {
    let env = support::env();
    let services = with_services(cx, &env);
    let (u, fake) = fake_updater(cx, true);
    reach_ready(cx, &u, &fake, offer("6.0.1", true));
    assert!(act(cx, &u, |u, cx| u.restart(cx)));
    assert!(matches!(state(cx, &u), UpdateState::Installing(_)));
    assert!(!act(cx, &u, |u, cx| u.restart(cx)));
    support::wait_real(cx, "the relaunch", |_| !fake.restarts.borrow().is_empty());
    assert_eq!(
        *fake.restarts.borrow(),
        [AfterInstall::Relaunch(PathBuf::from(APPIMAGE))]
    );
    let install = (
        LogLevel::Info,
        "update-install".to_owned(),
        "Installed TF2Tools v6.0.1".to_owned(),
    );
    assert!(
        update_rows(&services).contains(&install),
        "the row is written before the relaunch: {:?}",
        update_rows(&services)
    );
}

#[gpui_kit::test]
fn an_installer_update_logs_then_quits(cx: &mut TestAppContext) {
    let env = support::env();
    let services = with_services(cx, &env);
    let (u, fake) = fake_updater(cx, true);
    *fake.apply_result.lock().unwrap() = Ok(AfterInstall::Quit);
    reach_ready(cx, &u, &fake, offer("6.0.1", true));
    assert!(act(cx, &u, |u, cx| u.restart(cx)));
    support::wait_real(cx, "the quit", |_| !fake.restarts.borrow().is_empty());
    assert_eq!(*fake.restarts.borrow(), [AfterInstall::Quit]);
    let started = (
        LogLevel::Info,
        "update-install".to_owned(),
        "Started the TF2Tools v6.0.1 installer".to_owned(),
    );
    assert!(
        update_rows(&services).contains(&started),
        "{:?}",
        update_rows(&services)
    );
}

#[gpui_kit::test]
fn a_relaunch_that_never_quits_asks_the_player_to_reopen(cx: &mut TestAppContext) {
    let env = support::env();
    let services = with_services(cx, &env);
    let (u, fake) = fake_updater(cx, true);
    act(cx, &u, |u, _| u.set_restart_grace(Duration::from_secs(1)));
    reach_ready(cx, &u, &fake, offer("6.0.1", true));
    assert!(act(cx, &u, |u, cx| u.restart(cx)));
    support::wait_real(cx, "the relaunch", |_| !fake.restarts.borrow().is_empty());
    // The recorder never quits the app; within the grace the updater still waits for it.
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(matches!(state(cx, &u), UpdateState::Installing(_)));
    wait_state(cx, &u, "Updated", |s| matches!(s, UpdateState::Updated(_)));
    assert_eq!(state(cx, &u), UpdateState::Updated(offer("6.0.1", true)));
    let reopen = (
        LogLevel::Warning,
        "update-restart".to_owned(),
        "TF2Tools has been updated. Close and reopen it to finish.".to_owned(),
    );
    assert!(
        update_rows(&services).contains(&reopen),
        "{:?}",
        update_rows(&services)
    );
    // The file on disk is already new, so a check would only offer it again.
    assert!(!act(cx, &u, |u, cx| u.check(cx)));
    assert!(!fake.check_pending());
}

#[gpui_kit::test]
fn a_failed_install_returns_to_available_and_raises(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, true);
    let seen = events(cx, &u);
    reach_ready(cx, &u, &fake, offer("6.0.1", true));
    let f = failure(
        "Couldn't install the update. Try again, or download the new version from the release page.",
        false,
    );
    *fake.apply_result.lock().unwrap() = Err(f.clone());
    act(cx, &u, |u, cx| u.restart(cx));
    wait_state(cx, &u, "Available again", |s| {
        matches!(s, UpdateState::Available(_))
    });
    assert_eq!(state(cx, &u), UpdateState::Available(offer("6.0.1", true)));
    assert_eq!(*seen.borrow(), [UpdateEvent::InstallFailed(f)]);
    assert!(fake.restarts.borrow().is_empty());
}

#[gpui_kit::test]
fn an_install_that_cannot_write_here_falls_back_to_the_release_page(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, true);
    reach_ready(cx, &u, &fake, offer("6.0.1", true));
    *fake.apply_result.lock().unwrap() = Err(failure(USE_PAGE, true));
    act(cx, &u, |u, cx| u.restart(cx));
    wait_state(cx, &u, "Available again", |s| {
        matches!(s, UpdateState::Available(_))
    });
    assert_eq!(state(cx, &u), UpdateState::Available(offer("6.0.1", false)));
    assert!(fake.restarts.borrow().is_empty());
}

// A download the server cannot find falls back to the release page, but only for that offer:
// checking again brings back an installable offer and the in-app install.
#[gpui_kit::test]
fn a_recheck_after_a_server_error_restores_the_in_app_install(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, true);
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    act(cx, &u, |u, cx| u.download(cx));
    let (sink, reply) = fake.next_download(cx);
    drop(sink);
    let server = "The update couldn't be downloaded right now. Try again later, or get it from the release page.";
    reply.send(Err(failure(server, true))).unwrap();
    wait_state(cx, &u, "Available again", |s| {
        matches!(s, UpdateState::Available(_))
    });
    assert_eq!(state(cx, &u), UpdateState::Available(offer("6.0.1", false)));

    reach_available(cx, &u, &fake, offer("6.0.1", true));
    assert_eq!(state(cx, &u), UpdateState::Available(offer("6.0.1", true)));
    assert!(act(cx, &u, |u, cx| u.download(cx)));
}
