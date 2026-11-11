//! The app-wide logs.tf sync: one run at a time, cancelled by a newer request, with resumed
//! progress counting the logs already saved.

use std::time::Duration;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::logs_sync::{LogsSync, SyncProgress, downloading, logs_sync};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{LogState, LogsRange, SteamId};

use crate::support;
use crate::support::logstf::{
    ARIE, Gate, HIGHLANDER, SIXES, day_ago, list_body, list_requests, mount_held,
    mount_held_status, serve_two_logs, shell, sid,
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
    let gate = Gate::default();
    mount_held(
        &server,
        &gate,
        "/json/2",
        None,
        HIGHLANDER.as_bytes().to_vec(),
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
    gate.open();
    idle(cx, &sync);
}

/// Serves logs 3 and 2 with log 3's body held until the returned gate opens. While it is
/// held the server answers nothing else, so a run stays mid-download until the test says.
fn serve_held_download(server: &support::StoreServer) -> Gate {
    let gate = Gate::default();
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    mount_held(server, &gate, "/json/3", None, SIXES.as_bytes().to_vec());
    server.mount_bytes("/json/2", HIGHLANDER.as_bytes().to_vec());
    gate
}

// Waits until the run's first download is held at the server.
fn held(cx: &mut TestAppContext, gate: &Gate) {
    support::wait_real(cx, "the held download", |_| gate.arrived() >= 1);
}

#[gpui_kit::test]
fn a_request_for_another_player_cancels_the_run_and_follows_it(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let gate = serve_held_download(&server);
    let s = shell(cx, &env, &server.uri(), &[(1, "other"), (ARIE, "Arie")]);
    let sync = hub(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    held(cx, &gate);
    start(cx, &sync, 1, LogsRange::Months3);
    assert_eq!(
        cx.update(|cx| (sync.read(cx).running(), sync.read(cx).queued())),
        (Some(sid(ARIE)), Some(sid(1))),
        "the cancelled run ends before the next starts"
    );
    gate.open();
    idle(cx, &sync);
    assert_eq!(
        last_synced(&s, sid(ARIE)),
        None,
        "a cancelled sync is not stamped"
    );
    assert!(
        last_synced(&s, sid(1)).is_some(),
        "the queued player synced"
    );
    assert_eq!(cx.update(|cx| sync.read(cx).error(sid(ARIE))), None);
}

#[gpui_kit::test]
fn the_same_request_again_is_ignored(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let gate = serve_held_download(&server);
    let _s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let sync = hub(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    held(cx, &gate);
    start(cx, &sync, ARIE, LogsRange::Months3);
    assert_eq!(cx.update(|cx| sync.read(cx).queued()), None);
    gate.open();
    idle(cx, &sync);
    assert_eq!(list_requests(&server), 1, "one listing for one request");
}

#[gpui_kit::test]
fn a_request_with_another_range_restarts_the_run(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let gate = serve_held_download(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let sync = hub(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    held(cx, &gate);
    start(cx, &sync, ARIE, LogsRange::Year1);
    assert_eq!(cx.update(|cx| sync.read(cx).queued()), Some(sid(ARIE)));
    gate.open();
    idle(cx, &sync);
    assert_eq!(list_requests(&server), 2, "the wider range lists again");
    assert!(
        last_synced(&s, sid(ARIE)).is_some(),
        "the restarted run finished"
    );
}

#[gpui_kit::test]
fn asking_again_for_a_cancelled_player_queues_a_fresh_run(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let gate = serve_held_download(&server);
    let s = shell(cx, &env, &server.uri(), &[(1, "other"), (ARIE, "Arie")]);
    let sync = hub(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    held(cx, &gate);
    start(cx, &sync, 1, LogsRange::Months3);
    start(cx, &sync, ARIE, LogsRange::Months3);
    assert_eq!(cx.update(|cx| sync.read(cx).queued()), Some(sid(ARIE)));
    gate.open();
    idle(cx, &sync);
    assert!(
        last_synced(&s, sid(ARIE)).is_some(),
        "Arie's fresh run finished"
    );
}

#[gpui_kit::test]
fn stopping_a_queued_player_drops_them(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let gate = serve_held_download(&server);
    let s = shell(cx, &env, &server.uri(), &[(1, "other"), (ARIE, "Arie")]);
    let sync = hub(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    held(cx, &gate);
    start(cx, &sync, 1, LogsRange::Months3);
    let _stopped = cx.update(|cx| sync.update(cx, |s, cx| s.stop(sid(1), cx)));
    assert_eq!(cx.update(|cx| sync.read(cx).queued()), None);
    gate.open();
    idle(cx, &sync);
    assert_eq!(
        last_synced(&s, sid(1)),
        None,
        "the dropped player never synced"
    );
}

#[gpui_kit::test]
fn the_header_ring_follows_a_logs_tf_sync_and_opens_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let gate = serve_held_download(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    assert!(
        !support::present(cx, s.win, ids::logstf_sync_ring()),
        "no ring while idle"
    );
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    support::wait_real(cx, "the ring", |cx| {
        support::label_of(cx, s.win, ids::logstf_sync_ring()).as_deref()
            == Some("Syncing logs.tf: 0 / 2")
    });
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(
        support::present(cx, s.win, ids::logstf_sync_ring()),
        "the sync runs on while the page is hidden"
    );
    support::click_id(cx, s.win, ids::logstf_sync_ring());
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::LogsTf);
    gate.open();
    support::wait_real(cx, "the sync", |cx| {
        !support::present(cx, s.win, ids::logstf_sync_ring())
    });
}

#[gpui_kit::test]
fn the_ring_spins_while_the_list_is_read(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let gate = Gate::default();
    mount_held(
        &server,
        &gate,
        "/api/v1/log",
        None,
        list_body(&[(3, day_ago(1))]),
    );
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let sync = hub(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    support::wait_real(cx, "the held listing", |_| gate.arrived() >= 1);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_sync_ring()).as_deref(),
        Some("Syncing logs.tf...")
    );
    gate.open();
    idle(cx, &sync);
}

#[gpui_kit::test]
fn a_stopped_run_keeps_no_error_for_the_player(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // The download in flight when the run stops fails, as it would for a removed player.
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    let gate = Gate::default();
    mount_held_status(&server, &gate, "/json/3", 503);
    let _s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let sync = hub(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    held(cx, &gate);
    let _stopped = cx.update(|cx| sync.update(cx, |s, cx| s.stop(sid(ARIE), cx)));
    gate.open();
    idle(cx, &sync);
    assert_eq!(cx.update(|cx| sync.read(cx).error(sid(ARIE))), None);
}

// Points the app's logs client at `base` with reads that give up after a second, so a reply
// the server holds back fails as a network error.
fn impatient_client(cx: &mut TestAppContext, base: &str) {
    use tf2tools_app::services::Services;
    use tf2tools_core::{LogsCache, LogsClient, LogsEndpoints, StoreConfig};
    cx.update(|cx| {
        let db = cx.global::<Services>().db.clone();
        let logs = LogsClient::new(LogsCache::new(db), LogsEndpoints::loopback(base)).with_config(
            StoreConfig::default()
                .with_request_spacing(Duration::ZERO)
                .with_retry_backoff(Duration::from_millis(1))
                .with_timeouts(Duration::from_secs(10), Duration::from_secs(1)),
        );
        cx.global_mut::<Services>().logs = logs;
    });
}

// Holds back Arie's list replies, all of them or only the first `times`.
fn hold_lists(server: &support::StoreServer, times: Option<u64>) {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    let reply = ResponseTemplate::new(200)
        .set_body_bytes(list_body(&[(3, day_ago(1)), (2, day_ago(2))]))
        .set_delay(Duration::from_secs(30));
    let mut mock = Mock::given(method("GET"))
        .and(path("/api/v1/log"))
        .respond_with(reply);
    if let Some(times) = times {
        mock = mock.up_to_n_times(times);
    }
    server.mount(mock);
}

// The sync with its pause before a retry taken out, so tests wait on no clock.
fn hub_without_retry_pause(cx: &mut TestAppContext) -> Entity<LogsSync> {
    let sync = hub(cx);
    cx.update(|cx| sync.update(cx, |s, _| s.set_retry_delay(Duration::ZERO)));
    sync
}

#[gpui_kit::test]
fn one_unreachable_try_is_retried_and_the_logs_arrive(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    hold_lists(&server, Some(1));
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    impatient_client(cx, &server.uri());
    let sync = hub_without_retry_pause(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    idle(cx, &sync);
    assert_eq!(cx.update(|cx| sync.read(cx).error(sid(ARIE))), None);
    let saved = s.services.logs.cache().logs(sid(ARIE), LogsRange::All);
    assert_eq!(saved.unwrap().len(), 2);
    assert_eq!(list_requests(&server), 2);
}

#[gpui_kit::test]
fn two_unreachable_tries_with_nothing_saved_say_logs_tf_was_not_reached(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    hold_lists(&server, None);
    let _s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    impatient_client(cx, &server.uri());
    let sync = hub_without_retry_pause(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    idle(cx, &sync);
    assert_eq!(
        cx.update(|cx| sync.read(cx).error(sid(ARIE))),
        Some("Couldn't reach logs.tf. Check your connection and try again.")
    );
    assert_eq!(list_requests(&server), 2, "one retry, no more");
}

#[gpui_kit::test]
fn a_retried_sync_counts_the_logs_its_first_try_saved(cx: &mut TestAppContext) {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    let env = support::env();
    let server = support::StoreServer::start();
    // The first try saves log 3, then log 2 never answers in time; the retry's log 2 is held
    // at the gate, where its progress shows.
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let slow = ResponseTemplate::new(200)
        .set_body_bytes(HIGHLANDER.as_bytes().to_vec())
        .set_delay(Duration::from_secs(30));
    server.mount(
        Mock::given(method("GET"))
            .and(path("/json/2"))
            .respond_with(slow)
            .up_to_n_times(1),
    );
    let gate = Gate::default();
    mount_held(
        &server,
        &gate,
        "/json/2",
        None,
        HIGHLANDER.as_bytes().to_vec(),
    );
    let _s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    impatient_client(cx, &server.uri());
    let sync = hub_without_retry_pause(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    held(cx, &gate);
    support::wait_real(cx, "the retry's progress", |cx| {
        cx.update(|cx| sync.read(cx).progress())
            == Some(SyncProgress::Downloading {
                done: 1,
                total: 2,
                left: Duration::ZERO,
            })
    });
    gate.open();
    idle(cx, &sync);
}

#[gpui_kit::test]
fn a_new_run_keeps_the_last_download_count_until_it_downloads(cx: &mut TestAppContext) {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    let env = support::env();
    let server = support::StoreServer::start();
    // The first run lists at once, saves log 3 and fails on log 2; the next run's listing
    // is held at the gate, where the first run's count and estimate must still show.
    server.mount(
        Mock::given(method("GET"))
            .and(path("/api/v1/log"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(list_body(&[(3, day_ago(1)), (2, day_ago(2))])),
            )
            .up_to_n_times(1),
    );
    let gate = Gate::default();
    mount_held(
        &server,
        &gate,
        "/api/v1/log",
        None,
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    server.mount(
        Mock::given(method("GET"))
            .and(path("/json/2"))
            .respond_with(ResponseTemplate::new(500)),
    );
    let _s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    impatient_client(cx, &server.uri());
    let sync = hub_without_retry_pause(cx);
    start(cx, &sync, ARIE, LogsRange::Months3);
    idle(cx, &sync);
    assert!(cx.update(|cx| sync.read(cx).error(sid(ARIE))).is_some());
    start(cx, &sync, ARIE, LogsRange::Months3);
    held(cx, &gate);
    assert_eq!(
        cx.update(|cx| sync.read(cx).progress()),
        Some(SyncProgress::Downloading {
            done: 1,
            total: 2,
            left: Duration::ZERO,
        }),
        "the listing keeps the last run's count"
    );
    gate.open();
    idle(cx, &sync);
}

// Arie's first listing answers at once and log 3's body waits at `download`; every later
// listing waits at `listing`, where a new run's progress shows. Player 1 lists nothing.
fn serve_held_download_then_listing(server: &support::StoreServer) -> (Gate, Gate) {
    let (download, listing) = (Gate::default(), Gate::default());
    support::logstf::mount_list_for(server, 1, &[], Duration::ZERO);
    server.mount(
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/v1/log"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_bytes(list_body(&[(3, day_ago(1)), (2, day_ago(2))])),
            )
            .up_to_n_times(1),
    );
    mount_held(
        server,
        &listing,
        "/api/v1/log",
        None,
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    mount_held(
        server,
        &download,
        "/json/3",
        None,
        SIXES.as_bytes().to_vec(),
    );
    server.mount_bytes("/json/2", HIGHLANDER.as_bytes().to_vec());
    (download, listing)
}

// Starts Arie's Months3 run and returns its download count once it shows.
fn downloading_count(cx: &mut TestAppContext, sync: &Entity<LogsSync>) -> SyncProgress {
    start(cx, sync, ARIE, LogsRange::Months3);
    support::wait_real(cx, "the download count", |cx| {
        matches!(
            cx.update(|cx| sync.read(cx).progress()),
            Some(SyncProgress::Downloading { .. })
        )
    });
    cx.update(|cx| sync.read(cx).progress()).unwrap()
}

// Starts Arie over `range`, past the freshness check, and reads the progress while the
// listing is held.
fn progress_while_listing(
    cx: &mut TestAppContext,
    sync: &Entity<LogsSync>,
    listing: &Gate,
    range: LogsRange,
) -> Option<SyncProgress> {
    let before = listing.arrived();
    cx.update(|cx| sync.update(cx, |s, cx| s.start(sid(ARIE), range, true, cx)));
    support::wait_real(cx, "the held listing", |_| listing.arrived() > before);
    let shown = cx.update(|cx| sync.read(cx).progress());
    listing.open();
    idle(cx, sync);
    shown
}

#[gpui_kit::test]
fn a_finished_run_leaves_no_download_count(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (download, listing) = serve_held_download_then_listing(&server);
    let _s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let sync = hub(cx);
    downloading_count(cx, &sync);
    download.open();
    idle(cx, &sync);
    assert_eq!(cx.update(|cx| sync.read(cx).error(sid(ARIE))), None);
    assert_eq!(
        progress_while_listing(cx, &sync, &listing, LogsRange::Months3),
        None
    );
}

#[gpui_kit::test]
fn a_stopped_run_leaves_no_download_count(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (download, listing) = serve_held_download_then_listing(&server);
    let _s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let sync = hub(cx);
    downloading_count(cx, &sync);
    let _stopped = cx.update(|cx| sync.update(cx, |s, cx| s.stop(sid(ARIE), cx)));
    download.open();
    idle(cx, &sync);
    assert_eq!(
        progress_while_listing(cx, &sync, &listing, LogsRange::Months3),
        None
    );
}

#[gpui_kit::test]
fn a_run_over_another_range_leaves_out_the_old_count(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (download, listing) = serve_held_download_then_listing(&server);
    let _s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let sync = hub(cx);
    downloading_count(cx, &sync);
    start(cx, &sync, ARIE, LogsRange::Year1);
    download.open();
    support::wait_real(cx, "the held listing", |_| listing.arrived() >= 1);
    assert_eq!(
        cx.update(|cx| sync.read(cx).progress()),
        None,
        "the wider range's listing shows no count"
    );
    listing.open();
    idle(cx, &sync);
}

#[gpui_kit::test]
fn switching_players_keeps_each_players_download_count(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (download, listing) = serve_held_download_then_listing(&server);
    let _s = shell(cx, &env, &server.uri(), &[(1, "other"), (ARIE, "Arie")]);
    let sync = hub(cx);
    let arie = downloading_count(cx, &sync);
    start(cx, &sync, 1, LogsRange::Months3);
    download.open();
    idle(cx, &sync);
    assert_eq!(
        progress_while_listing(cx, &sync, &listing, LogsRange::Months3),
        Some(arie),
        "Arie's count is back while Arie lists again"
    );
}
