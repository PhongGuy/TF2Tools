//! Opening a trimmed log downloads it again without touching its stats; one logs.tf no longer
//! serves stays trimmed; and opening a log stamps when it was opened.

use tf2tools_core::{
    ClassAverage, ClassWeapons, KeepFullLogs, LOG_BODY_LIMIT, LogState, LogsCancel, LogsClient,
    LogsEndpoints, LogsFetchError, LogsRange, OpenedBody, SteamId, TfClass, TrackedPlayer,
};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, days_ago, list_body, log_state, logs_env, logstf_fixture, mount_list, mount_log,
    now,
};

const DAY: i64 = 86_400;

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

/// One row's stats: id, classes, seconds, kills, deaths, assists, dmg, dt.
type RowStats = (u64, Vec<TfClass>, u32, u32, u32, u32, u32, Option<u32>);

/// What the page shows for Arie: each row's stats, the averages, and each class's weapons.
#[derive(Debug, PartialEq)]
struct Stats {
    rows: Vec<RowStats>,
    averages: [ClassAverage; 9],
    weapons: Vec<ClassWeapons>,
}

fn stats(env: &LogsEnv) -> Stats {
    let cache = env.client.cache();
    let range = LogsRange::Months3;
    let rows = cache
        .logs(arie(), range)
        .unwrap()
        .iter()
        .map(|r| {
            let classes = r.classes().to_vec();
            (
                r.id(),
                classes,
                r.seconds(),
                r.kills(),
                r.deaths(),
                r.assists(),
                r.dmg(),
                r.dt(),
            )
        })
        .collect();
    Stats {
        rows,
        averages: cache.class_averages(arie(), range).unwrap(),
        weapons: TfClass::ALL
            .iter()
            .map(|&class| cache.class_weapons(arie(), range, class).unwrap())
            .collect(),
    }
}

fn states(env: &LogsEnv) -> Vec<LogState> {
    let rows = env.client.cache().logs(arie(), LogsRange::Months3).unwrap();
    rows.iter().map(|r| r.state()).collect()
}

/// (class rows, weapon rows) in the whole cache.
fn row_counts(env: &LogsEnv) -> (i64, i64) {
    (
        common::count(&env.dir, "logstf_class_rows"),
        common::count(&env.dir, "logstf_weapon_rows"),
    )
}

/// (weapons_counted, dt_counted) of every log.
fn counted(env: &LogsEnv) -> Vec<(i64, i64)> {
    let conn = common::sql(&env.dir);
    let mut stmt = conn
        .prepare("SELECT weapons_counted, dt_counted FROM logstf_logs ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn opened_at(env: &LogsEnv, id: i64) -> Option<i64> {
    common::sql(&env.dir)
        .query_row(
            "SELECT opened_at FROM logstf_logs WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}

async fn sync(env: &LogsEnv) -> tf2tools_core::LogsSyncSummary {
    env.client
        .sync_player(
            arie(),
            LogsRange::Months3,
            true,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap()
}

/// Arie tracked with logs 4 and 3, both the 6v6 fixture, synced; uploaded two and three days ago.
async fn synced_env() -> LogsEnv {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(4, days_ago(2)), (3, days_ago(3))]),
    )
    .await;
    let body = logstf_fixture("log-4137103-6v6.json");
    mount_log(&env.server, 3, 200, body.clone()).await;
    mount_log(&env.server, 4, 200, body).await;
    assert_eq!(sync(&env).await.downloaded(), 2, "both logs download");
    env
}

/// A pass a month from now with the shortest choice, which trims every stored log.
fn trim_all(env: &LogsEnv) -> usize {
    env.client
        .cache()
        .trim(KeepFullLogs::Day1, now() + 30 * DAY)
        .unwrap()
        .trimmed()
}

#[tokio::test]
async fn stats_stay_identical_through_trim_restore_and_resync() {
    let env = synced_env().await;
    let first = stats(&env);
    let rows = row_counts(&env);
    assert_eq!(first.rows.len(), 2);
    assert!(
        first.rows.iter().all(|r| !r.1.is_empty()),
        "{:?}",
        first.rows
    );

    assert_eq!(trim_all(&env), 2);
    assert_eq!(states(&env), [LogState::Trimmed, LogState::Trimmed]);
    assert_eq!(stats(&env), first);

    let detail = env.client.fetch_log(3).await.unwrap();
    assert_eq!(detail.players().len(), 12);
    assert_eq!(log_state(&env.dir, 3), (0, true));
    assert_eq!(states(&env), [LogState::Trimmed, LogState::Ready]);
    assert_eq!(stats(&env), first);
    assert_eq!(row_counts(&env), rows);

    assert_eq!(trim_all(&env), 1);
    assert_eq!(
        sync(&env).await.downloaded(),
        0,
        "a sync never downloads a trimmed log"
    );
    env.client.fetch_log(4).await.unwrap();
    assert_eq!(states(&env), [LogState::Ready, LogState::Trimmed]);
    assert_eq!(stats(&env), first);
    assert_eq!(row_counts(&env), rows);
    assert!(
        counted(&env).iter().all(|&c| c == (1, 1)),
        "{:?}",
        counted(&env)
    );
}

#[tokio::test]
async fn a_restore_puts_back_only_the_body() {
    let env = synced_env().await;
    trim_all(&env);
    // Marks the rows, so a restore that rewrote them would show.
    common::sql(&env.dir)
        .execute_batch(
            "UPDATE logstf_class_rows SET kills = kills + 100 WHERE log_id = 3;
             UPDATE logstf_weapon_rows SET dmg = dmg + 100 WHERE log_id = 3;
             UPDATE logstf_logs SET fetched_at = 1234 WHERE id = 3;",
        )
        .unwrap();
    let marked = stats(&env);
    let rows = row_counts(&env);
    let before = now();
    env.client.fetch_log(3).await.unwrap();
    assert_eq!(log_state(&env.dir, 3), (0, true));
    assert_eq!(stats(&env), marked);
    assert_eq!(row_counts(&env), rows);
    let fetched: i64 = common::sql(&env.dir)
        .query_row("SELECT fetched_at FROM logstf_logs WHERE id = 3", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(fetched, 1234, "the restore keeps the fetch time");
    assert!(opened_at(&env, 3).is_some_and(|at| at >= before));
}

#[tokio::test]
async fn a_trimmed_log_logs_tf_no_longer_serves_stays_trimmed() {
    let env = synced_env().await;
    trim_all(&env);
    let first = stats(&env);
    let rows = row_counts(&env);
    env.server.reset().await;
    mount_log(&env.server, 3, 404, String::new()).await;
    mount_log(&env.server, 4, 200, "not a log".to_owned()).await;
    assert!(matches!(
        env.client.fetch_log(3).await,
        Err(LogsFetchError::FullLogGone)
    ));
    assert!(matches!(
        env.client.fetch_log(4).await,
        Err(LogsFetchError::FullLogGone)
    ));
    env.server.reset().await;
    let huge = "a".repeat(usize::try_from(LOG_BODY_LIMIT).unwrap() + 1);
    mount_log(&env.server, 3, 200, huge).await;
    assert!(matches!(
        env.client.fetch_log(3).await,
        Err(LogsFetchError::FullLogGone)
    ));
    assert_eq!(log_state(&env.dir, 3), (3, false));
    assert_eq!(log_state(&env.dir, 4), (3, false));
    assert_eq!(states(&env), [LogState::Trimmed, LogState::Trimmed]);
    assert_eq!(stats(&env), first);
    assert_eq!(row_counts(&env), rows);
}

#[tokio::test]
async fn a_trimmed_log_opened_offline_stays_trimmed() {
    let env = synced_env().await;
    trim_all(&env);
    let remote = LogsClient::new(env.client.cache().clone(), LogsEndpoints::default())
        .with_config(common::fast_config().with_loopback_only(true));
    assert!(matches!(
        remote.fetch_log(3).await,
        Err(LogsFetchError::Network { .. })
    ));
    assert_eq!(log_state(&env.dir, 3), (3, false));
}

#[tokio::test]
async fn open_body_stamps_the_open_and_says_what_it_found() {
    let env = synced_env().await;
    let cache = env.client.cache();
    let body = logstf_fixture("log-4137103-6v6.json");
    assert_eq!(opened_at(&env, 3), None, "a sync is not an open");
    assert_eq!(cache.open_body(3, 1_000).unwrap(), OpenedBody::Body(body));
    assert_eq!(opened_at(&env, 3), Some(1_000));

    trim_all(&env);
    assert_eq!(cache.open_body(3, 2_000).unwrap(), OpenedBody::Trimmed);
    assert_eq!(opened_at(&env, 3), Some(2_000));
    assert_eq!(cache.open_body(99, 2_000).unwrap(), OpenedBody::Missing);

    common::sql(&env.dir)
        .execute(
            "UPDATE logstf_logs SET body = x'00010203', state = 0 WHERE id = 4",
            [],
        )
        .unwrap();
    assert_eq!(cache.open_body(4, 3_000).unwrap(), OpenedBody::Damaged);
}

#[tokio::test]
async fn fetching_a_pending_log_stamps_it_opened() {
    let env = synced_env().await;
    let steam = i64::try_from(ARIE).unwrap();
    common::sql(&env.dir)
        .execute_batch(&format!(
            "INSERT INTO logstf_logs (id, title, map, date) VALUES (5, 't', 'm', {});
             INSERT INTO logstf_player_logs (steam_id, log_id) VALUES ({steam}, 5);",
            days_ago(1)
        ))
        .unwrap();
    mount_log(&env.server, 5, 200, logstf_fixture("log-4137103-6v6.json")).await;
    let before = now();
    env.client.fetch_log(5).await.unwrap();
    assert!(opened_at(&env, 5).is_some_and(|at| at >= before));
    assert_eq!(opened_at(&env, 4), None);
}
