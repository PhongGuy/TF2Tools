//! One gate for every logs.tf request, and the slower spacing for long pulls.

use std::sync::Mutex;
use std::time::Duration;

use tf2tools_core::{LogsCancel, LogsProgress, LogsRange, PlayerInput, SteamId, TrackedPlayer};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, ResponseTemplate};

use crate::common;
use crate::common::logstf::{
    ARIE, FOUND_XML, LogsEnv, Stamps, days_ago, hourly, list_body, log_body, logs_env, mount_list,
    solo,
};

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

async fn tracked_env() -> LogsEnv {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    env
}

/// The spacing the first Downloading tick reports for a pull of `count` logs; the sync is
/// cancelled at that tick, so nothing downloads.
async fn first_spacing(
    client: &tf2tools_core::LogsClient,
    server: &wiremock::MockServer,
    count: usize,
) -> Duration {
    server.reset().await;
    mount_list(
        server,
        ARIE,
        list_body(&hourly(100_000 + count as u64, days_ago(1), count)),
    )
    .await;
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
    let env = tracked_env().await;
    let long = Duration::from_millis(300);
    let client = env.client.clone().with_long_spacing(long);
    assert_eq!(first_spacing(&client, &env.server, 401).await, long);
    // Back on the configured zero gap, three lookups in a row never wait a long gap. A Steam
    // 404 still resolves a known id, so no mock is needed.
    let start = std::time::Instant::now();
    for _ in 0..3 {
        client
            .resolve_player(&PlayerInput::Id(arie()))
            .await
            .unwrap();
    }
    assert!(
        start.elapsed() < Duration::from_millis(250),
        "{:?}",
        start.elapsed()
    );
    // The 400 ids overlap the cancelled pull's, all still pending, so exactly 400 are wanted.
    assert_eq!(
        first_spacing(&client, &env.server, 400).await,
        Duration::ZERO
    );
}

#[tokio::test]
async fn every_request_of_a_sync_and_a_lookup_waits_its_turn() {
    let env = tracked_env().await;
    let spacing = Duration::from_millis(90);
    let client = env
        .client
        .clone()
        .with_config(common::fast_config().with_request_spacing(spacing));
    let stamps = Stamps::default();
    let list = list_body(&[(3, days_ago(1)), (2, days_ago(2)), (1, days_ago(3))]);
    Mock::given(method("GET"))
        .and(path("/api/v1/log"))
        .respond_with(stamps.answer(ResponseTemplate::new(200).set_body_string(list)))
        .mount(&env.server)
        .await;
    let log = log_body(600, &[solo(231_702, ("scout", 600, 1, 1, 1, 100))]);
    Mock::given(method("GET"))
        .and(path_regex(r"^/json/\d+$"))
        .respond_with(stamps.answer(ResponseTemplate::new(200).set_body_string(log)))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/steam/profiles/\d+/$"))
        .respond_with(stamps.answer(ResponseTemplate::new(200).set_body_string(FOUND_XML)))
        .mount(&env.server)
        .await;
    let (cancel, input) = (LogsCancel::new(), PlayerInput::Id(arie()));
    let (synced, found) = tokio::join!(
        client.sync_player(arie(), LogsRange::Months3, true, &cancel, &|_| {}),
        client.resolve_player(&input),
    );
    synced.unwrap();
    found.unwrap();
    assert_eq!(stamps.arrivals(), 5);
    // The gate's gap is 90 ms; a third of it is left for arrival jitter on a loaded runner.
    assert!(
        stamps.min_gap() >= Duration::from_millis(60),
        "{:?}",
        stamps.min_gap()
    );
}
