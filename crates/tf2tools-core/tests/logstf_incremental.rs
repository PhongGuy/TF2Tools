//! Repeat syncs: staleness, incremental downloads, provisional logs, pruning, the lock.

use tf2tools_core::{SteamId, TrackedPlayer};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, days_ago, list_body, log_state, logs_env, logstf_fixture, mount_any_log,
    mount_list, mount_log, now, paths,
};

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

async fn env_with(name: &str) -> LogsEnv {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), name))
        .unwrap();
    env
}

fn json_requests(all: &[String]) -> usize {
    all.iter().filter(|p| p.starts_with("/json/")).count()
}

#[tokio::test]
async fn a_fresh_cache_is_skipped_and_a_forced_refresh_fetches_only_the_list() {
    let env = env_with("Arie").await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(2, days_ago(1)), (1, days_ago(2))]),
    )
    .await;
    mount_any_log(&env.server).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    assert_eq!(paths(&env.server).await.len(), 3);

    let again = env
        .client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    assert!(again.skipped());
    assert_eq!(
        paths(&env.server).await.len(),
        3,
        "no request inside 30 minutes"
    );

    let forced = env.client.sync_player(arie(), true, &|_| {}).await.unwrap();
    assert!(!forced.skipped());
    let all = paths(&env.server).await;
    assert_eq!(all.len(), 4, "one list request, no log request");
    assert_eq!(json_requests(&all), 2);
}

#[tokio::test]
async fn a_stale_cache_syncs_without_force() {
    let env = env_with("Arie").await;
    mount_list(&env.server, ARIE, list_body(&[(2, days_ago(1))])).await;
    mount_any_log(&env.server).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    common::sql(&env.dir)
        .execute(
            "UPDATE logstf_players SET last_synced = ?1",
            [now() - 31 * 60],
        )
        .unwrap();
    let summary = env
        .client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    assert!(!summary.skipped());
}

#[tokio::test]
async fn a_provisional_log_is_fetched_again_once() {
    let env = env_with("Arie").await;
    let uploaded = now() - 2 * 3600;
    mount_list(&env.server, ARIE, list_body(&[(20, uploaded)])).await;
    mount_any_log(&env.server).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    // As if it was downloaded ten minutes after upload, while the game may still have been going.
    common::sql(&env.dir)
        .execute(
            "UPDATE logstf_logs SET fetched_at = ?1 WHERE id = 20",
            [uploaded + 600],
        )
        .unwrap();
    env.client.sync_player(arie(), true, &|_| {}).await.unwrap();
    env.client.sync_player(arie(), true, &|_| {}).await.unwrap();
    assert_eq!(json_requests(&paths(&env.server).await), 2);
    assert_eq!(
        common::count(&env.dir, "logstf_class_rows"),
        1,
        "old rows replaced, not doubled"
    );
}

/// A synced log 20 that was fetched ten minutes after upload, so the next sync refetches it,
/// with the refetch answered by `status` and `body`.
async fn provisional_env(status: u16, body: String) -> LogsEnv {
    let env = env_with("Arie").await;
    let uploaded = now() - 2 * 3600;
    mount_list(&env.server, ARIE, list_body(&[(20, uploaded)])).await;
    mount_any_log(&env.server).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    common::sql(&env.dir)
        .execute(
            "UPDATE logstf_logs SET fetched_at = ?1 WHERE id = 20",
            [uploaded + 600],
        )
        .unwrap();
    mount_log(&env.server, 20, status, body).await;
    env
}

/// A refetch that cannot be read keeps the earlier good copy and settles it, so it is not
/// counted as unreadable and is not asked for again.
async fn assert_kept_after_unreadable_refetch(env: &LogsEnv) {
    let summary = env.client.sync_player(arie(), true, &|_| {}).await.unwrap();
    assert_eq!(summary.unreadable(), 0, "a kept log is not unreadable");
    assert_eq!(log_state(&env.dir, 20), (0, true), "the good body stays");
    assert_eq!(
        common::count(&env.dir, "logstf_class_rows"),
        1,
        "the class rows stay"
    );
    env.client.sync_player(arie(), true, &|_| {}).await.unwrap();
    assert_eq!(
        json_requests(&paths(&env.server).await),
        2,
        "settled, not asked again"
    );
}

#[tokio::test]
async fn a_provisional_log_keeps_its_body_when_the_refetch_will_not_parse() {
    let env = provisional_env(200, "not a log".to_owned()).await;
    assert_kept_after_unreadable_refetch(&env).await;
}

#[tokio::test]
async fn a_provisional_log_keeps_its_body_when_the_refetch_is_too_large() {
    let limit = usize::try_from(tf2tools_core::LOG_BODY_LIMIT).unwrap();
    let env = provisional_env(200, "x".repeat(limit + 1)).await;
    assert_kept_after_unreadable_refetch(&env).await;
}

#[tokio::test]
async fn a_provisional_log_that_is_now_404_is_gone() {
    let env = provisional_env(404, String::new()).await;
    let summary = env.client.sync_player(arie(), true, &|_| {}).await.unwrap();
    assert_eq!(summary.gone(), 1);
    assert_eq!(log_state(&env.dir, 20), (1, false));
    assert_eq!(common::count(&env.dir, "logstf_class_rows"), 0);
}

#[tokio::test]
async fn a_placeholder_name_is_replaced_from_the_log() {
    let env = env_with("76561197960497430").await;
    mount_list(&env.server, ARIE, list_body(&[(3, days_ago(1))])).await;
    mount_log(&env.server, 3, 200, logstf_fixture("log-4137103-6v6.json")).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    assert_eq!(
        env.client.cache().players().unwrap()[0].name(),
        "Arie - serveme.tf"
    );
}

#[tokio::test]
async fn a_real_name_is_kept() {
    let env = env_with("Arie").await;
    mount_list(&env.server, ARIE, list_body(&[(3, days_ago(1))])).await;
    mount_log(&env.server, 3, 200, logstf_fixture("log-4137103-6v6.json")).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    assert_eq!(env.client.cache().players().unwrap()[0].name(), "Arie");
}

#[tokio::test]
async fn a_finished_sync_prunes_logs_past_120_days() {
    let env = env_with("Arie").await;
    let steam = i64::try_from(ARIE).unwrap();
    common::sql(&env.dir)
        .execute_batch(&format!(
            "INSERT INTO logstf_logs (id, title, map, date) VALUES (99, 'old', 'm', {});
             INSERT INTO logstf_player_logs (steam_id, log_id) VALUES ({steam}, 99);",
            days_ago(130)
        ))
        .unwrap();
    mount_list(&env.server, ARIE, list_body(&[(2, days_ago(1))])).await;
    mount_any_log(&env.server).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    assert_eq!(common::count(&env.dir, "logstf_logs"), 1);
    assert_eq!(common::count(&env.dir, "logstf_player_logs"), 1);
}

#[tokio::test]
async fn concurrent_syncs_run_one_at_a_time() {
    let env = env_with("Arie").await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(2, days_ago(1)), (1, days_ago(2))]),
    )
    .await;
    mount_any_log(&env.server).await;
    let other = env.client.clone();
    let (a, b) = tokio::join!(
        env.client.sync_player(arie(), true, &|_| {}),
        other.sync_player(arie(), true, &|_| {}),
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(
        json_requests(&paths(&env.server).await),
        2,
        "the second found both cached"
    );
}

#[tokio::test]
async fn a_teammates_cached_log_costs_no_request() {
    let env = env_with("Arie").await;
    mount_list(&env.server, ARIE, list_body(&[(3, days_ago(1))])).await;
    mount_log(&env.server, 3, 200, logstf_fixture("log-4137103-6v6.json")).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    // A Red teammate in the same log.
    let mate = SteamId::from_steam3("[U:1:103548491]").unwrap();
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(mate, "Emiel"))
        .unwrap();
    mount_list(&env.server, mate.id64(), list_body(&[(3, days_ago(1))])).await;
    env.client.sync_player(mate, false, &|_| {}).await.unwrap();
    assert_eq!(json_requests(&paths(&env.server).await), 1);
}
