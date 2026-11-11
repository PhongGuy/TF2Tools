//! Fetching one log when it is opened; its spacing against a running sync is checked
//! inside the crate, where the gate's starts can be read.

use tf2tools_core::{LogState, LogsFetchError, LogsRange, SteamId, TrackedPlayer};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, class_rows, days_ago, log_state, logs_env, logstf_fixture, mount_log,
    remove_on_download,
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
async fn a_log_removed_while_it_downloads_is_still_returned() {
    let env = pending_env(&[3]).await;
    let body = logstf_fixture("log-4137103-6v6.json");
    remove_on_download(&env.server, env.client.cache(), arie(), 3, body).await;
    let detail = env.client.fetch_log(3).await.unwrap();
    assert_eq!(detail.players().len(), 12);
    assert_eq!(common::count(&env.dir, "logstf_logs"), 0);
    assert_eq!(common::count(&env.dir, "logstf_class_rows"), 0);
}
