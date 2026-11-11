//! The logs.tf gate checked from inside: the pacer records every start it gives out, so the
//! spacing is asserted exactly instead of by when requests happen to reach the mock server.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{LogsCancel, LogsClient, LogsEndpoints, LogsProgress};
use crate::db::Database;
use crate::logstf::cache::ListedLog;
use crate::logstf::{LOGS_LONG_SPACING, LogsCache, LogsRange, PlayerInput, SteamId, TrackedPlayer};
use crate::paths::DataDir;
use crate::store::StoreConfig;
use crate::store::pacer::PacerStats;

const ARIE: u64 = 76_561_197_960_497_430;
const DAY: i64 = 86_400;
const FOUND_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
    <profile><steamID64>76561197960287930</steamID64><steamID><![CDATA[Rabscuttle]]></steamID>\
    </profile>";

struct Env {
    _tmp: TempDir,
    server: MockServer,
    client: LogsClient,
}

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Arie tracked, and a client on a new wiremock server whose gate is `spacing` apart.
async fn env(spacing: Duration) -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&DataDir::at(tmp.path().join("data"))).unwrap();
    let server = MockServer::start().await;
    let config = StoreConfig::default()
        .with_request_spacing(spacing)
        .with_retry_backoff(Duration::from_millis(1));
    let client = LogsClient::new(LogsCache::new(db), LogsEndpoints::loopback(&server.uri()))
        .with_config(config);
    client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    Env {
        _tmp: tmp,
        server,
        client,
    }
}

/// A list page of `logs` as (id, upload date), claiming exactly that many in all.
fn list_body(logs: &[(i64, i64)]) -> String {
    let logs: Vec<_> = logs
        .iter()
        .map(|&(id, date)| json!({"id": id, "title": "t", "map": "m", "date": date}))
        .collect();
    json!({"total": logs.len(), "logs": logs}).to_string()
}

/// A one-scout log `parse_log` accepts.
fn log_body() -> String {
    let stats = json!([{"type": "scout", "total_time": 600, "kills": 1, "deaths": 1,
                        "assists": 1, "dmg": 100}]);
    json!({"version": 3, "length": 600, "teams": {"Red": {}, "Blue": {}},
           "players": {"[U:1:231702]": {"team": "Red", "kills": 1, "deaths": 1, "assists": 1,
               "dmg": 100, "heal": 0, "ubers": 0, "drops": 0, "class_stats": stats}},
           "names": {"[U:1:231702]": "solo"}, "info": {}})
    .to_string()
}

async fn mount(server: &MockServer, at: &str, body: String) {
    Mock::given(method("GET"))
        .and(path_regex(at))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(server)
        .await;
}

/// Logs 3, 2 and 1 listed a day apart from yesterday, and every log body answered.
async fn mount_sync(server: &MockServer) {
    let list = list_body(&[(3, now() - DAY), (2, now() - 2 * DAY), (1, now() - 3 * DAY)]);
    mount(server, r"^/api/v1/log$", list).await;
    mount(server, r"^/json/\d+$", log_body()).await;
}

/// The start-to-start gaps the gate gave out, in order.
fn start_gaps(client: &LogsClient) -> Vec<Duration> {
    let starts = client.pacer.starts();
    starts.windows(2).map(|w| w[1].0 - w[0].0).collect()
}

fn slots(interactive: u64, background: u64) -> PacerStats {
    PacerStats {
        interactive,
        background,
        empty: 0,
        throttled: 0,
    }
}

async fn arrivals(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

#[tokio::test]
async fn a_fetch_during_a_sync_never_starts_two_requests_inside_one_gap() {
    let spacing = Duration::from_millis(90);
    let env = env(spacing).await;
    mount_sync(&env.server).await;
    let pending = ListedLog {
        id: 9,
        title: "t".to_owned(),
        map: "m".to_owned(),
        date: now() - 4 * DAY,
    };
    env.client
        .cache()
        .record_list(arie(), &[pending], now())
        .unwrap();
    let (cancel, client) = (LogsCancel::new(), &env.client);
    let (synced, fetched) = tokio::join!(
        client.sync_player(arie(), LogsRange::Months3, true, &cancel, &|_| {}),
        async {
            // Lands while the sync is downloading, so the two contend for the gate.
            tokio::time::sleep(Duration::from_millis(130)).await;
            client.fetch_log(9).await
        },
    );
    synced.unwrap();
    fetched.unwrap();
    assert_eq!(
        arrivals(&env.server).await,
        5,
        "the list, three sync downloads and the fetch"
    );
    assert_eq!(client.pacer.stats(), slots(1, 4));
    let gaps = start_gaps(client);
    assert_eq!(gaps.len(), 4);
    assert!(gaps.iter().all(|gap| *gap >= spacing), "{gaps:?}");
}

#[tokio::test]
async fn every_request_of_a_sync_and_a_lookup_waits_its_turn() {
    let spacing = Duration::from_millis(90);
    let env = env(spacing).await;
    mount_sync(&env.server).await;
    mount(&env.server, r"^/steam/profiles/\d+/$", FOUND_XML.to_owned()).await;
    let (cancel, input, client) = (LogsCancel::new(), PlayerInput::Id(arie()), &env.client);
    let (synced, found) = tokio::join!(
        client.sync_player(arie(), LogsRange::Months3, true, &cancel, &|_| {}),
        client.resolve_player(&input),
    );
    synced.unwrap();
    found.unwrap();
    assert_eq!(arrivals(&env.server).await, 5);
    assert_eq!(client.pacer.stats(), slots(1, 4));
    let gaps = start_gaps(client);
    assert_eq!(gaps.len(), 4);
    assert!(gaps.iter().all(|gap| *gap >= spacing), "{gaps:?}");
}

/// The spacing the first Downloading tick reports for a pull of `count` hourly logs; the
/// sync is cancelled at that tick, so nothing downloads.
async fn first_spacing(env: &Env, client: &LogsClient, count: i64) -> Duration {
    env.server.reset().await;
    let newest = now() - DAY;
    let logs: Vec<_> = (0..count)
        .map(|i| (100_000 + count - i, newest - 3_600 * i))
        .collect();
    mount(&env.server, r"^/api/v1/log$", list_body(&logs)).await;
    let cancel = LogsCancel::new();
    let seen = Mutex::new(None);
    client
        .sync_player(arie(), LogsRange::Months3, true, &cancel, &|p| {
            if let LogsProgress::Downloading { spacing, .. } = p {
                seen.lock().unwrap().get_or_insert(spacing);
                cancel.cancel();
            }
        })
        .await
        .unwrap();
    seen.into_inner().unwrap().unwrap()
}

#[tokio::test]
async fn a_pull_over_400_logs_uses_the_long_spacing_and_then_restores_the_short() {
    let env = env(Duration::ZERO).await;
    let long = Duration::from_millis(300);
    let client = env.client.clone().with_long_spacing(long);
    assert_eq!(first_spacing(&env, &client, 401).await, long);
    // Back on the configured zero gap, lookups after the long pull start with no wait: every
    // start reserved a zero gap after itself. A Steam 404 still resolves a known id.
    assert_eq!(client.pacer.gap(), Duration::ZERO);
    for _ in 0..3 {
        client
            .resolve_player(&PlayerInput::Id(arie()))
            .await
            .unwrap();
    }
    let starts = client.pacer.starts();
    assert_eq!(starts.len(), 4, "the list and three lookups");
    assert!(starts.iter().all(|(_, gap)| gap.is_zero()), "{starts:?}");
    // The 400 ids overlap the cancelled pull's, all still pending, so exactly 400 are wanted.
    assert_eq!(first_spacing(&env, &client, 400).await, Duration::ZERO);
}

#[tokio::test]
async fn a_long_pull_reserves_the_long_gap_after_each_download_but_not_after_the_list() {
    let env = env(Duration::ZERO).await;
    let long = Duration::from_millis(60);
    let client = env.client.clone().with_long_spacing(long);
    let logs: Vec<_> = (0..401)
        .map(|i| (100_401 - i, now() - DAY - 3_600 * i))
        .collect();
    mount(&env.server, r"^/api/v1/log$", list_body(&logs)).await;
    mount(&env.server, r"^/json/\d+$", log_body()).await;
    // Two downloads go through, so the second shows the first one's gap was kept.
    let cancel = LogsCancel::new();
    client
        .sync_player(arie(), LogsRange::Months3, true, &cancel, &|p| {
            if let LogsProgress::Downloading { done: 2, .. } = p {
                cancel.cancel();
            }
        })
        .await
        .unwrap();
    let starts = client.pacer.starts();
    let reserved: Vec<_> = starts.iter().map(|(_, gap)| *gap).collect();
    assert_eq!(
        reserved,
        [Duration::ZERO, long, long],
        "the list, then two downloads"
    );
    assert!(starts[2].0 - starts[1].0 >= long, "{starts:?}");
}

#[tokio::test]
async fn a_client_from_new_slows_a_long_pull_to_the_long_spacing() {
    // `env` builds on `new` and only swaps the config, so the long gap is the default one.
    let env = env(Duration::ZERO).await;
    assert_eq!(
        first_spacing(&env, &env.client, 401).await,
        LOGS_LONG_SPACING
    );
}

/// Syncs 22 hourly logs `spacing` apart whose first download is answered 429 once, and
/// returns the gap each start reserved with the spacing every Downloading tick reported.
async fn sync_through_one_rate_limit(spacing: Duration) -> (Vec<Duration>, Vec<Duration>) {
    let env = env(spacing).await;
    let logs: Vec<_> = (0..22)
        .map(|i| (1_000 - i, now() - DAY - 3_600 * i))
        .collect();
    mount(&env.server, r"^/api/v1/log$", list_body(&logs)).await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/json/\d+$"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&env.server)
        .await;
    mount(&env.server, r"^/json/\d+$", log_body()).await;
    let reported = Mutex::new(Vec::new());
    env.client
        .sync_player(arie(), LogsRange::Months3, true, &LogsCancel::new(), &|p| {
            if let LogsProgress::Downloading { spacing, .. } = p {
                reported.lock().unwrap().push(spacing);
            }
        })
        .await
        .unwrap();
    let reserved = env
        .client
        .pacer
        .starts()
        .iter()
        .map(|(_, gap)| *gap)
        .collect();
    (reserved, reported.into_inner().unwrap())
}

#[tokio::test]
async fn a_429_widens_the_sync_gap_until_twenty_answers_ease_it_back() {
    let base = Duration::from_millis(2);
    let (reserved, _) = sync_through_one_rate_limit(base).await;
    // The list and the rate-limited download start at the base; the next twenty answered
    // downloads run at double it, and the last one is back at the base.
    let mut expected = vec![base, base];
    expected.extend([base * 2; 20]);
    expected.push(base);
    assert_eq!(reserved, expected);
}

#[tokio::test]
async fn sync_progress_reports_the_widened_gap() {
    let base = Duration::from_millis(2);
    let (_, reported) = sync_through_one_rate_limit(base).await;
    assert_eq!(reported.len(), 23, "the start and one tick per log");
    assert_eq!((reported[0], reported[1]), (base, base * 2));
    assert_eq!(reported[22], base);
}
