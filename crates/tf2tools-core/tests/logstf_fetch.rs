//! Fetching one log when it is opened, alongside a running sync.

use std::time::Duration;

use tf2tools_core::{LogState, LogsCancel, LogsFetchError, LogsRange, SteamId, TrackedPlayer};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, ResponseTemplate};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, Stamps, class_rows, days_ago, list_body, log_body, log_state, logs_env,
    logstf_fixture, mount_log, solo,
};

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

/// Arie tracked, with pending logs `ids` listed one day apart from yesterday.
async fn pending_env(ids: &[i64]) -> LogsEnv {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    let steam = i64::try_from(ARIE).unwrap();
    for (n, id) in ids.iter().enumerate() {
        common::sql(&env.dir)
            .execute_batch(&format!(
                "INSERT INTO logstf_logs (id, title, map, date) VALUES ({id}, 't', 'm', {});
                 INSERT INTO logstf_player_logs (steam_id, log_id) VALUES ({steam}, {id});",
                days_ago(1 + n as i64)
            ))
            .unwrap();
    }
    env
}

#[tokio::test]
async fn opening_a_pending_log_downloads_stores_and_returns_it() {
    let env = pending_env(&[3]).await;
    mount_log(&env.server, 3, 200, logstf_fixture("log-4137103-6v6.json")).await;
    let detail = env.client.fetch_log(3).await.unwrap();
    assert_eq!(detail.players().len(), 12);
    assert_eq!(log_state(&env.dir, 3), (0, true));
    assert_eq!(class_rows(&env.dir, 3), 29);
    let rows = env.client.cache().logs(arie(), LogsRange::Months3).unwrap();
    assert_eq!(rows[0].state(), LogState::Ready);
}

#[tokio::test]
async fn a_missing_or_unreadable_log_is_marked_as_the_sync_would() {
    let env = pending_env(&[3, 2, 1, 4]).await;
    mount_log(&env.server, 3, 404, String::new()).await;
    mount_log(&env.server, 2, 200, "not a log".to_owned()).await;
    mount_log(&env.server, 1, 500, String::new()).await;
    mount_log(&env.server, 4, 429, String::new()).await;
    assert!(matches!(
        env.client.fetch_log(3).await,
        Err(LogsFetchError::Gone)
    ));
    assert_eq!(log_state(&env.dir, 3), (1, false));
    assert!(matches!(
        env.client.fetch_log(2).await,
        Err(LogsFetchError::Unreadable)
    ));
    assert_eq!(log_state(&env.dir, 2), (2, false));
    assert!(matches!(
        env.client.fetch_log(1).await,
        Err(LogsFetchError::Network { .. })
    ));
    assert_eq!(
        log_state(&env.dir, 1),
        (0, false),
        "a failed fetch leaves the log pending"
    );
    // fast_config retries a 429 after 1 ms, 2 ms and 4 ms, then gives up.
    assert!(matches!(
        env.client.fetch_log(4).await,
        Err(LogsFetchError::RateLimited { .. })
    ));
    assert_eq!(log_state(&env.dir, 4), (0, false));
}

#[tokio::test]
async fn an_offline_fetch_is_a_network_error_and_sends_nothing() {
    let env = pending_env(&[3]).await;
    // The real logs.tf address with loopback-only on: refused before anything is sent.
    let remote = tf2tools_core::LogsClient::new(
        env.client.cache().clone(),
        tf2tools_core::LogsEndpoints::default(),
    )
    .with_config(common::fast_config().with_loopback_only(true));
    assert!(matches!(
        remote.fetch_log(3).await,
        Err(LogsFetchError::Network { .. })
    ));
    assert_eq!(log_state(&env.dir, 3), (0, false));
}

#[tokio::test]
async fn a_fetch_during_a_sync_never_sends_two_requests_inside_one_spacing() {
    let env = pending_env(&[9]).await;
    let spacing = Duration::from_millis(90);
    let client = env
        .client
        .clone()
        .with_config(common::fast_config().with_request_spacing(spacing));
    let stamps = Stamps::default();
    let list = list_body(&[(3, days_ago(2)), (2, days_ago(3)), (1, days_ago(4))]);
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
    let cancel = LogsCancel::new();
    let (synced, fetched) = tokio::join!(
        client.sync_player(arie(), LogsRange::Months3, true, &cancel, &|_| {}),
        async {
            tokio::time::sleep(Duration::from_millis(130)).await;
            client.fetch_log(9).await
        },
    );
    synced.unwrap();
    fetched.unwrap();
    assert_eq!(
        stamps.arrivals(),
        5,
        "the list, three sync downloads and the fetch"
    );
    assert!(
        stamps.min_gap() >= Duration::from_millis(60),
        "{:?}",
        stamps.min_gap()
    );
}
