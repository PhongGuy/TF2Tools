//! Logs that are gone or unreadable, and failures that stop a sync.

use tf2tools_core::{LogsCancel, LogsRange, LogsSyncError, SteamId, TrackedPlayer};

use crate::common::logstf::{
    ARIE, LogsEnv, class_rows, days_ago, list_body, log_body, log_state, logs_env, mount_any_log,
    mount_list, mount_log, paths, solo,
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
async fn a_404_is_gone_and_never_retried() {
    let env = tracked_env().await;
    mount_list(&env.server, ARIE, list_body(&[(7, days_ago(1))])).await;
    mount_log(
        &env.server,
        7,
        404,
        r#"{"success": false, "error": "Log not found."}"#.into(),
    )
    .await;
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
    assert_eq!(summary.gone(), 1);
    assert_eq!(log_state(&env.dir, 7), (1, false));
    env.client
        .sync_player(
            arie(),
            LogsRange::Months3,
            true,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    let asked = paths(&env.server)
        .await
        .iter()
        .filter(|p| *p == "/json/7")
        .count();
    assert_eq!(asked, 1);
}

#[tokio::test]
async fn unreadable_and_oversized_bodies_are_marked_and_the_rest_continue() {
    let env = tracked_env().await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(9, days_ago(1)), (8, days_ago(2)), (6, days_ago(3))]),
    )
    .await;
    mount_log(&env.server, 9, 200, r#"{"success": true}"#.into()).await;
    mount_log(
        &env.server,
        8,
        200,
        format!("{{\"pad\":\"{}\"}}", "x".repeat(2 * 1024 * 1024)),
    )
    .await;
    mount_log(
        &env.server,
        6,
        200,
        log_body(600, &[solo(231_702, ("scout", 600, 1, 1, 1, 100))]),
    )
    .await;
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
    assert_eq!((summary.unreadable(), summary.downloaded()), (2, 1));
    assert_eq!(log_state(&env.dir, 9), (2, false));
    assert_eq!(log_state(&env.dir, 8), (2, false));
    assert_eq!(class_rows(&env.dir, 9), 0);
    assert_eq!(log_state(&env.dir, 6), (0, true));
}

#[tokio::test]
async fn a_server_error_stops_the_sync_and_keeps_what_was_saved() {
    let env = tracked_env().await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(12, days_ago(1)), (11, days_ago(2)), (10, days_ago(3))]),
    )
    .await;
    mount_any_log(&env.server).await;
    mount_log(&env.server, 11, 503, String::new()).await;
    let err = env
        .client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, LogsSyncError::Http { status: 503, .. }),
        "{err:?}"
    );
    assert_eq!(log_state(&env.dir, 12), (0, true));
    assert!(!paths(&env.server).await.contains(&"/json/10".to_owned()));
    assert_eq!(env.client.cache().players().unwrap()[0].last_synced(), None);
}

#[tokio::test]
async fn a_list_that_will_not_parse_is_malformed() {
    let env = tracked_env().await;
    mount_list(&env.server, ARIE, "not json".into()).await;
    let err = env
        .client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap_err();
    assert!(matches!(err, LogsSyncError::Malformed { .. }), "{err:?}");
}

#[tokio::test]
async fn a_forced_refresh_that_fails_partway_lets_the_next_open_finish_it() {
    let env = tracked_env().await;
    mount_list(&env.server, ARIE, list_body(&[(20, days_ago(2))])).await;
    mount_any_log(&env.server).await;
    env.client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();

    // A new log appears and the forced refresh stops on a server error before reaching it.
    env.server.reset().await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(21, days_ago(1)), (20, days_ago(2))]),
    )
    .await;
    wiremock::Mock::given(wiremock::matchers::path("/json/21"))
        .respond_with(wiremock::ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&env.server)
        .await;
    mount_any_log(&env.server).await;
    env.client
        .sync_player(
            arie(),
            LogsRange::Months3,
            true,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap_err();
    assert_eq!(log_state(&env.dir, 21), (0, false));

    let next = env
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
    assert!(!next.skipped(), "a pending log is not a fresh cache");
    assert_eq!(next.downloaded(), 1);
    assert_eq!(log_state(&env.dir, 21), (0, true));

    let after = env
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
    assert!(after.skipped(), "nothing pending and freshly synced");
}

#[tokio::test]
async fn gone_and_unreadable_logs_do_not_keep_a_fresh_cache_syncing() {
    let env = tracked_env().await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(31, days_ago(1)), (30, days_ago(2))]),
    )
    .await;
    mount_log(&env.server, 31, 404, r#"{"success": false}"#.into()).await;
    mount_log(&env.server, 30, 200, r#"{"success": true}"#.into()).await;
    env.client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    assert_eq!(log_state(&env.dir, 31), (1, false));
    assert_eq!(log_state(&env.dir, 30), (2, false));

    // Neither log has a body, but neither is pending, so a normal open skips the list.
    env.server.reset().await;
    let next = env
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
    assert!(next.skipped(), "gone and unreadable logs are settled");
    assert!(paths(&env.server).await.is_empty());
}
