//! A player's log list: the window, the cap, and refusing an untracked player.

use tf2tools_core::{LogsCancel, LogsRange, LogsSyncError, SteamId, TrackedPlayer};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, days_ago, list_body, logs_env, mount_any_log, mount_list, paths,
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
