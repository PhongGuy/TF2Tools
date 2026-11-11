//! Opening a log: a downloaded one reads at once, a pending one is fetched with a loading line.

use std::time::Duration;

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::shell::route::Page;
use tf2tools_core::LogState;

use crate::support;
use crate::support::logstf::{ARIE, SIXES, day_ago, expanded, list_body, open_page, shell, to_end};

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
            == Some("Can't load this log while you're offline.")
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
