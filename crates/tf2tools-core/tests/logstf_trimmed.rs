//! The trimmed state: a log whose full body was dropped keeps its stats, is never pending, and
//! is pruned like any other log.

use tf2tools_core::{
    Database, LogState, LogsCache, LogsCancel, LogsRange, SteamId, TfClass, TrackedPlayer,
};

use crate::common;
use crate::common::logstf::{
    ARIE, days_ago, list_body, log_state, logs_env, mount_any_log, mount_list, paths, seed_stored,
    trim_by_hand,
};

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

fn tracked(dir: &tf2tools_core::DataDir) -> LogsCache {
    let cache = LogsCache::new(Database::open(dir).unwrap());
    cache
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    cache
}

#[test]
fn a_trimmed_log_lists_as_trimmed_and_keeps_its_stats() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    seed_stored(&dir, 5, days_ago(3), days_ago(2));
    let range = LogsRange::Months3;
    let averages = cache.class_averages(arie(), range).unwrap();
    let weapons = cache.class_weapons(arie(), range, TfClass::Scout).unwrap();
    assert_eq!(
        cache.logs(arie(), range).unwrap()[0].state(),
        LogState::Ready
    );

    trim_by_hand(&dir, 5);
    let rows = cache.logs(arie(), range).unwrap();
    assert_eq!(rows[0].state(), LogState::Trimmed);
    assert_eq!(rows[0].classes(), [TfClass::Scout]);
    let row = &rows[0];
    assert_eq!(
        (
            row.seconds(),
            row.kills(),
            row.deaths(),
            row.assists(),
            row.dmg(),
            row.dt()
        ),
        (600, 5, 2, 3, 2000, Some(1500))
    );
    assert_eq!(cache.class_averages(arie(), range).unwrap(), averages);
    assert_eq!(
        cache.class_weapons(arie(), range, TfClass::Scout).unwrap(),
        weapons
    );
    assert_eq!(cache.log_body(5).unwrap(), None);
}

#[tokio::test]
async fn a_sync_after_a_trim_downloads_nothing_and_finds_nothing_pending() {
    let env = logs_env().await;
    let cache = env.client.cache();
    cache
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    let date = days_ago(3);
    seed_stored(&env.dir, 5, date, date + 7_200);
    trim_by_hand(&env.dir, 5);
    mount_list(&env.server, ARIE, list_body(&[(5, date)])).await;
    mount_any_log(&env.server).await;
    let first = env
        .client
        .sync_player(
            arie(),
            LogsRange::Months3,
            true,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    assert_eq!(paths(&env.server).await, ["/api/v1/log"]);
    assert_eq!(first.downloaded(), 0);
    assert_eq!(log_state(&env.dir, 5), (3, false));
    // Freshly synced over the same range with nothing pending, so the next sync is skipped.
    let again = env
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
    assert!(again.skipped());
}

#[test]
fn the_backfills_skip_a_trimmed_log() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    seed_stored(&dir, 5, days_ago(3), days_ago(2));
    common::sql(&dir)
        .execute_batch("UPDATE logstf_logs SET weapons_counted = 0, dt_counted = 0 WHERE id = 5")
        .unwrap();
    trim_by_hand(&dir, 5);
    let weapons = cache.count_weapons(10).unwrap();
    assert_eq!((weapons.counted(), weapons.left()), (0, 0));
    let taken = cache.fill_damage_taken(10).unwrap();
    assert_eq!((taken.counted(), taken.left()), (0, 0));
}

#[test]
fn removing_the_last_player_prunes_a_trimmed_log() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    seed_stored(&dir, 5, days_ago(3), days_ago(2));
    trim_by_hand(&dir, 5);
    cache.remove_player(arie()).unwrap();
    assert_eq!(common::count(&dir, "logstf_logs"), 0);
    assert_eq!(common::count(&dir, "logstf_class_rows"), 0);
    assert_eq!(common::count(&dir, "logstf_weapon_rows"), 0);
}
