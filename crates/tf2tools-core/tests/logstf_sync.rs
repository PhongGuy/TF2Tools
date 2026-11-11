//! A player's log list: the window, every log in it, each body downloaded once, and refusing
//! an untracked player.

use tf2tools_core::{LogsCancel, LogsRange, LogsSyncError, SteamId, TrackedPlayer};

use std::time::Duration;

use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, days_ago, list_body, log_body, logs_env, mount_any_log, mount_list, paths, solo,
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

#[tokio::test]
async fn the_list_is_cut_at_the_window_with_one_list_request() {
    let env = tracked_env().await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[
            (5, days_ago(1)),
            (4, days_ago(89)),
            (3, days_ago(92)),
            (2, days_ago(100)),
        ]),
    )
    .await;
    mount_any_log(&env.server).await;
    let summary = env
        .client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    assert_eq!(
        paths(&env.server).await,
        ["/api/v1/log", "/json/5", "/json/4"]
    );
    assert_eq!(summary.listed(), 2);
    assert_eq!(common::count(&env.dir, "logstf_logs"), 2);
}

#[tokio::test]
async fn every_log_in_the_window_is_listed() {
    let env = tracked_env().await;
    let logs: Vec<(u64, i64)> = (0..405u64)
        .map(|i| (10_000 - i, days_ago(1) - i as i64))
        .collect();
    mount_list(&env.server, ARIE, list_body(&logs)).await;
    mount_any_log(&env.server).await;
    let summary = env
        .client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    assert_eq!((summary.listed(), summary.downloaded()), (405, 405));
    assert_eq!(common::count(&env.dir, "logstf_logs"), 405);
}

#[tokio::test]
async fn an_untracked_player_is_refused_without_a_request() {
    let env = logs_env().await;
    let err = env
        .client
        .sync_player(
            arie(),
            LogsRange::Months3,
            true,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LogsSyncError::NotTracked { .. }), "{err:?}");
    assert!(paths(&env.server).await.is_empty());
}

#[tokio::test]
async fn a_log_fetched_while_the_sync_runs_is_not_downloaded_again() {
    let env = tracked_env().await;
    let list = list_body(&[(3, days_ago(1)), (2, days_ago(2)), (1, days_ago(3))]);
    mount_list(&env.server, ARIE, list).await;
    mount_any_log(&env.server).await;
    // Log 3 answers slowly, so the player opens log 1 while the sync still waits on it.
    let body = log_body(600, &[solo(231_702, ("scout", 600, 1, 1, 1, 100))]);
    Mock::given(method("GET"))
        .and(path("/json/3"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(body)
                .set_delay(Duration::from_millis(300)),
        )
        .with_priority(1)
        .mount(&env.server)
        .await;
    let (cancel, client) = (LogsCancel::new(), &env.client);
    let (synced, fetched) = tokio::join!(
        client.sync_player(arie(), LogsRange::Months3, false, &cancel, &|_| {}),
        async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            client.fetch_log(1).await
        },
    );
    fetched.unwrap();
    assert_eq!(synced.unwrap().downloaded(), 2, "logs 3 and 2");
    let downloads: Vec<_> = paths(&env.server)
        .await
        .into_iter()
        .filter(|p| p == "/json/1")
        .collect();
    assert_eq!(downloads.len(), 1, "only the fetch asked for log 1");
}
