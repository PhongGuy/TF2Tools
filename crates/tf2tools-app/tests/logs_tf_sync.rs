//! The app-wide logs.tf sync: one run at a time, cancelled by a newer request, with resumed
//! progress counting the logs already saved.

use std::time::Duration;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::logs_sync::{LogsSync, SyncProgress, downloading, logs_sync};
use tf2tools_core::{LogState, LogsRange, SteamId};

use crate::support;
use crate::support::logstf::{
    ARIE, HIGHLANDER, SIXES, day_ago, list_body, serve_two_logs, shell, sid,
};

fn hub(cx: &mut TestAppContext) -> Entity<LogsSync> {
    cx.update(logs_sync)
}

fn start(cx: &mut TestAppContext, sync: &Entity<LogsSync>, account: u64, range: LogsRange) {
    cx.update(|cx| sync.update(cx, |s, cx| s.start(sid(account), range, false, cx)));
}

fn idle(cx: &mut TestAppContext, sync: &Entity<LogsSync>) {
    support::wait_real(cx, "the logs.tf sync", |cx| {
        cx.update(|cx| !sync.read(cx).busy())
    });
}

fn last_synced(s: &support::ShellWindow, id: SteamId) -> Option<i64> {
    let players = s.services.logs.cache().players().unwrap();
    players
        .iter()
        .find(|p| p.steam_id() == id)
        .and_then(|p| p.last_synced())
}

#[test]
fn downloading_counts_the_logs_already_saved() {
    assert_eq!(
        downloading(0, 3_212, Duration::from_secs(2), 1_600),
        SyncProgress::Downloading {
            done: 1_600,
            total: 4_812,
            left: Duration::from_secs(6_424)
        }
    );
    assert_eq!(
        downloading(5, 5, Duration::from_secs(1), 0),
        SyncProgress::Downloading {
            done: 5,
            total: 5,
            left: Duration::ZERO
        }
    );
}

#[gpui_kit::test]
fn a_sync_runs_without_the_page_and_ends_idle(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let sync = hub(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    assert_eq!(cx.update(|cx| sync.read(cx).running()), Some(sid(ARIE)));
    idle(cx, &sync);
    assert!(
        last_synced(&s, sid(ARIE)).is_some(),
        "a finished sync is stamped"
    );
    let rows = s
        .services
        .logs
        .cache()
        .logs(sid(ARIE), LogsRange::Months3)
        .unwrap();
    assert!(
        rows.iter().all(|r| r.state() == LogState::Ready),
        "both logs downloaded"
    );
    assert_eq!(cx.update(|cx| sync.read(cx).error(sid(ARIE))), None);
    assert_eq!(cx.update(|cx| sync.read(cx).progress()), None);
}

#[gpui_kit::test]
fn a_resumed_sync_counts_the_logs_already_saved(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // The first run saves log 3, then logs.tf fails on log 2 and the run stops.
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    server.mount_status("/json/2", 503);
    let _s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let sync = hub(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    idle(cx, &sync);
    assert!(
        cx.update(|cx| sync.read(cx).error(sid(ARIE))).is_some(),
        "the failure is kept"
    );

    server.reset();
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    server.mount_bytes_after(
        "/json/2",
        HIGHLANDER.as_bytes().to_vec(),
        Duration::from_secs(1),
    );
    start(cx, &sync, ARIE, LogsRange::Months3);
    assert_eq!(
        cx.update(|cx| sync.read(cx).error(sid(ARIE))),
        None,
        "a new run clears it"
    );
    support::wait_real(cx, "the resumed download", |cx| {
        cx.update(|cx| sync.read(cx).progress())
            == Some(SyncProgress::Downloading {
                done: 1,
                total: 2,
                left: Duration::ZERO,
            })
    });
    idle(cx, &sync);
}
