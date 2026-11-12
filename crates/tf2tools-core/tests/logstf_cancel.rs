//! Cancelling a sync: it stops before its next request and keeps what it wrote.

use tf2tools_core::{LogsCancel, LogsProgress, LogsRange, SteamId, TrackedPlayer};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, days_ago, list_body, log_state, logs_env, mount_any_log, mount_list, paths,
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
async fn a_cancelled_sync_stops_before_its_next_request_and_the_next_one_resumes() {
    let env = tracked_env().await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(3, days_ago(1)), (2, days_ago(2)), (1, days_ago(3))]),
    )
    .await;
    mount_any_log(&env.server).await;
    common::sql(&env.dir)
        .execute(
            "INSERT INTO logstf_logs (id, title, map, date) VALUES (50, 't', 'm', 1)",
            [],
        )
        .unwrap();
    let cancel = LogsCancel::new();
    let summary = env
        .client
        .sync_player(arie(), LogsRange::Months3, false, &cancel, &|p| {
            if let LogsProgress::Downloading { done: 1, .. } = p {
                cancel.cancel();
            }
        })
        .await
        .unwrap();
    assert!(summary.cancelled());
    assert_eq!(summary.downloaded(), 1);
    assert_eq!(paths(&env.server).await, ["/api/v1/log", "/json/3"]);
    assert_eq!(log_state(&env.dir, 3), (0, true));
    assert_eq!(log_state(&env.dir, 2), (0, false));
    assert_eq!(env.client.cache().players().unwrap()[0].last_synced(), None);
    assert_eq!(
        common::count(&env.dir, "logstf_logs"),
        4,
        "no prune: 50 stays"
    );

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
    assert!(!summary.cancelled());
    assert_eq!(summary.downloaded(), 2);
    assert!(
        env.client.cache().players().unwrap()[0]
            .last_synced()
            .is_some()
    );
    assert_eq!(
        common::count(&env.dir, "logstf_logs"),
        3,
        "pruned: 50 is gone"
    );
}

#[tokio::test]
async fn a_sync_cancelled_before_it_starts_sends_nothing() {
    let env = tracked_env().await;
    let cancel = LogsCancel::new();
    let other = cancel.clone();
    other.cancel();
    assert!(cancel.is_cancelled());
    let summary = env
        .client
        .sync_player(arie(), LogsRange::All, true, &cancel, &|_| {})
        .await
        .unwrap();
    assert!(summary.cancelled());
    assert!(paths(&env.server).await.is_empty());
}
