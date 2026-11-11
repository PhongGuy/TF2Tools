//! Opening a log: a downloaded one reads at once, a pending one is fetched with a loading line.

use std::time::Duration;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::logs_sync::{LogsSyncEvent, logs_sync};
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::shell::route::Page;
use tf2tools_core::LogState;

use crate::support;
use crate::support::logstf::{
    ARIE, Gate, SIXES, day_ago, download_beside, expanded, list_body, mount_held, open_page, shell,
    sid, to_end,
};

#[gpui_kit::test]
fn a_pending_log_opens_with_a_loading_line_then_its_tables(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_bytes_after("/json/3", SIXES.as_bytes().to_vec(), Duration::from_secs(1));
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx.update(|cx| s.shell.read(cx).logs_tf_page()).unwrap();
    support::wait_real(cx, "the pending row", |cx| {
        cx.update(|cx| {
            page.read(cx)
                .rows()
                .iter()
                .any(|r| r.id() == 3 && r.state() == LogState::Pending)
        })
    });
    to_end(cx, &page);
    support::click_id(cx, s.win, ids::logstf_log(3));
    assert_eq!(expanded(cx, &page), Some(3));
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_log_status(3)).as_deref(),
        Some("Loading log...")
    );
    support::wait_real(cx, "the fetched log", |cx| {
        cx.update(|cx| page.read(cx).detail().is_some())
    });
    assert!(
        !support::present(cx, s.win, ids::logstf_log_status(3)),
        "the loading line goes"
    );
    support::wait_real(cx, "the sync", |cx| {
        cx.update(|cx| !page.read(cx).is_syncing(cx))
    });
}

#[gpui_kit::test]
fn a_pending_log_that_cannot_load_says_why_and_stays_pending(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_status("/json/3", 503);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    to_end(cx, &page);
    support::click_id(cx, s.win, ids::logstf_log(3));
    support::wait_real(cx, "the failure line", |cx| {
        support::label_of(cx, s.win, ids::logstf_log_status(3)).as_deref()
            == Some("logs.tf isn't responding right now. Try again later.")
    });
    assert_eq!(expanded(cx, &page), Some(3));
    let state = cx.update(|cx| page.read(cx).rows()[0].state());
    assert_eq!(state, LogState::Pending);
}

#[gpui_kit::test]
fn a_pending_log_logs_tf_removed_turns_into_a_removed_row(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_status("/json/3", 503);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    server.reset();
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_status("/json/3", 404);
    to_end(cx, &page);
    support::click_id(cx, s.win, ids::logstf_log(3));
    support::wait_real(cx, "the removed row", |cx| {
        support::label_of(cx, s.win, ids::logstf_log_note(3)).as_deref()
            == Some("This log was removed from logs.tf")
    });
    assert_eq!(expanded(cx, &page), None);
}

// Log 3's row state as the page holds it.
fn state_of(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) -> Option<LogState> {
    cx.update(|cx| {
        page.read(cx)
            .rows()
            .iter()
            .find(|r| r.id() == 3)
            .map(|r| r.state())
    })
}

// A shell whose sync is done with log 3 left pending, since logs.tf answered 503 for it.
fn pending_page(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> (support::ShellWindow, Entity<LogsTfPage>) {
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_status("/json/3", 503);
    let s = shell(cx, env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    assert_eq!(
        state_of(cx, &page),
        Some(LogState::Pending),
        "the 503 leaves log 3 pending"
    );
    (s, page)
}

#[gpui_kit::test]
fn a_fetch_that_lands_after_its_row_closed_still_updates_the_row(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (s, page) = pending_page(cx, &env, &server);
    server.reset();
    let gate = Gate::default();
    mount_held(&server, &gate, "/json/3", None, SIXES.as_bytes().to_vec());
    to_end(cx, &page);
    support::click_id(cx, s.win, ids::logstf_log(3));
    support::wait_real(cx, "the held fetch", |_| gate.arrived() == 1);
    support::click_id(cx, s.win, ids::logstf_log(3));
    assert_eq!(expanded(cx, &page), None);
    gate.open();
    support::wait_real(cx, "the downloaded row", |cx| {
        state_of(cx, &page) == Some(LogState::Ready)
    });
    assert_eq!(
        expanded(cx, &page),
        None,
        "the late fetch does not reopen the row"
    );
    assert!(cx.update(|cx| page.read(cx).detail().is_none()));
}

#[gpui_kit::test]
fn a_loading_row_stays_open_when_the_sync_downloads_it(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let beside = support::StoreServer::start();
    beside.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let (s, page) = pending_page(cx, &env, &server);
    server.reset();
    let gate = Gate::default();
    mount_held(&server, &gate, "/json/3", None, SIXES.as_bytes().to_vec());
    to_end(cx, &page);
    support::click_id(cx, s.win, ids::logstf_log(3));
    support::wait_real(cx, "the held fetch", |_| gate.arrived() == 1);
    // The sync's download lands while the fetch is still out.
    download_beside(cx, &beside, 3);
    let sync = cx.update(logs_sync);
    sync.update(cx, |_, cx| cx.emit(LogsSyncEvent::Progress(sid(ARIE))));
    support::wait_real(cx, "the downloaded row", |cx| {
        state_of(cx, &page) == Some(LogState::Ready)
    });
    assert_eq!(expanded(cx, &page), Some(3));
    // The class grid above grows now that a log is downloaded, so the row is scrolled back.
    to_end(cx, &page);
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_log_status(3)).as_deref(),
        Some("Loading log...")
    );
    gate.open();
    support::wait_real(cx, "the fetched log", |cx| {
        cx.update(|cx| page.read(cx).detail().is_some())
    });
    assert_eq!(expanded(cx, &page), Some(3));
}

#[gpui_kit::test]
fn retry_fetches_a_log_that_failed_to_load(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (s, page) = pending_page(cx, &env, &server);
    to_end(cx, &page);
    support::click_id(cx, s.win, ids::logstf_log(3));
    support::wait_real(cx, "the retry button", |cx| {
        support::present(cx, s.win, ids::logstf_log_retry(3))
    });
    server.reset();
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    support::click_id(cx, s.win, ids::logstf_log_retry(3));
    support::wait_real(cx, "the fetched log", |cx| {
        cx.update(|cx| page.read(cx).detail().is_some())
    });
    assert_eq!(expanded(cx, &page), Some(3));
    assert!(!support::present(cx, s.win, ids::logstf_log_retry(3)));
}

#[gpui_kit::test]
fn a_failed_log_shows_once_the_sync_downloads_it(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (s, page) = pending_page(cx, &env, &server);
    to_end(cx, &page);
    support::click_id(cx, s.win, ids::logstf_log(3));
    support::wait_real(cx, "the failure line", |cx| {
        support::present(cx, s.win, ids::logstf_log_status(3))
    });
    server.reset();
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    support::click_id(cx, s.win, ids::LOGSTF_REFRESH);
    support::wait_real(cx, "the log after the sync", |cx| {
        cx.update(|cx| page.read(cx).detail().is_some())
    });
    assert_eq!(expanded(cx, &page), Some(3));
    support::wait_real(cx, "the sync", |cx| {
        cx.update(|cx| !page.read(cx).is_syncing(cx))
    });
}
