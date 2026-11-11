//! The window is the 90 days ending at the player's newest log, however old that log is.

use tf2tools_core::{LogsRange, PRUNE_SECS, SteamId, TrackedPlayer};

use crate::common;
use crate::common::logstf::{
    ARIE, days_ago, list_body, logs_env, mount_any_log, mount_list, now, paths,
};

const DAY: i64 = 86_400;

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

fn ids(rows: &[tf2tools_core::LogRow]) -> Vec<u64> {
    rows.iter().map(|r| r.id()).collect()
}

/// Logs for a player who last played `anchor_days` ago: two inside the window, one a day
/// inside its far edge, and one a day past it.
async fn sync_from(anchor_days: i64) -> (Vec<u64>, u32, usize, usize) {
    let env = logs_env().await;
    let cache = env.client.cache();
    cache
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    let window_days = LogsRange::Months3.reach_secs().unwrap() / DAY;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[
            (4, days_ago(anchor_days)),
            (3, days_ago(anchor_days + 30)),
            (2, days_ago(anchor_days + window_days - 1)),
            (1, days_ago(anchor_days + window_days + 1)),
        ]),
    )
    .await;
    mount_any_log(&env.server).await;
    let summary = env
        .client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    let rows = cache.logs(arie(), LogsRange::Months3).unwrap();
    let averages = cache.class_averages(arie(), LogsRange::Months3).unwrap();
    let fetched = paths(&env.server)
        .await
        .iter()
        .filter(|p| p.starts_with("/json/"))
        .count();
    (ids(&rows), averages[0].logs(), summary.listed(), fetched)
}

#[tokio::test]
async fn a_player_who_stopped_months_ago_keeps_their_last_90_days() {
    assert_eq!(sync_from(150).await, (vec![4, 3, 2], 3, 3, 3));
}

#[tokio::test]
async fn a_player_who_stopped_years_ago_keeps_their_last_90_days() {
    assert_eq!(sync_from(1_200).await, (vec![4, 3, 2], 3, 3, 3));
}

/// Seeds `(id, days ago, downloaded)` logs for Arie straight into the cache, synced just now.
fn seed(dir: &tf2tools_core::DataDir, logs: &[(i64, i64, bool)]) {
    let steam = i64::try_from(ARIE).unwrap();
    let mut sql = format!(
        "INSERT INTO logstf_players (steam_id, name, position, last_synced)
         VALUES ({steam}, 'Arie', 0, {});",
        now()
    );
    for &(id, days, ready) in logs {
        let body = if ready { "'{}'" } else { "NULL" };
        sql.push_str(&format!(
            "INSERT INTO logstf_logs (id, title, map, date, body, state)
                 VALUES ({id}, 't', 'm', {}, {body}, 0);
             INSERT INTO logstf_player_logs (steam_id, log_id) VALUES ({steam}, {id});",
            days_ago(days)
        ));
        if ready {
            sql.push_str(&format!(
                "INSERT INTO logstf_class_rows (log_id, steam_id, class, seconds, kills, deaths, assists, dmg)
                     VALUES ({id}, {steam}, 'scout', 60, 1, 1, 1, 100);"
            ));
        }
    }
    common::sql(dir).execute_batch(&sql).unwrap();
}

#[tokio::test]
async fn the_list_averages_and_pending_check_share_one_window() {
    // A pending log past the window's far edge does not hold a fresh player's sync open.
    let env = logs_env().await;
    seed(&env.dir, &[(3, 150, true), (2, 200, true), (1, 260, false)]);
    let cache = env.client.cache();
    assert_eq!(
        ids(&cache.logs(arie(), LogsRange::Months3).unwrap()),
        [3, 2]
    );
    let averages = cache.class_averages(arie(), LogsRange::Months3).unwrap();
    assert_eq!((averages[0].logs(), averages[0].seconds()), (2, 120));
    let summary = env
        .client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    assert!(summary.skipped());

    // One inside the window does: the list shows it pending and the sync goes on.
    let env = logs_env().await;
    seed(&env.dir, &[(3, 150, true), (2, 220, false)]);
    mount_list(&env.server, ARIE, list_body(&[])).await;
    let cache = env.client.cache();
    assert_eq!(
        ids(&cache.logs(arie(), LogsRange::Months3).unwrap()),
        [3, 2]
    );
    let summary = env
        .client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    assert!(!summary.skipped());
}

#[tokio::test]
async fn pruning_follows_each_players_own_window() {
    // Arie stopped 1,200 days ago; Bea plays today and shares one of Arie's old logs.
    let env = logs_env().await;
    let cache = env.client.cache();
    let anchor = 1_200;
    let slack = PRUNE_SECS / DAY;
    seed(
        &env.dir,
        &[
            (1, anchor, true),
            (2, anchor + 80, true),
            (3, anchor + slack - 1, true),
            (4, anchor + slack + 1, true),
        ],
    );
    let bea = SteamId::from_id64(ARIE + 1).unwrap();
    cache.add_player(&TrackedPlayer::new(bea, "Bea")).unwrap();
    let bea_sql = i64::try_from(ARIE + 1).unwrap();
    common::sql(&env.dir)
        .execute_batch(&format!(
            "INSERT INTO logstf_logs (id, title, map, date) VALUES (5, 't', 'm', {});
             INSERT INTO logstf_logs (id, title, map, date) VALUES (6, 't', 'm', {});
             INSERT INTO logstf_player_logs (steam_id, log_id) VALUES
                 ({bea_sql}, 5), ({bea_sql}, 6), ({bea_sql}, 2);",
            days_ago(1),
            days_ago(slack + 2),
        ))
        .unwrap();

    cache.prune(now()).unwrap();

    let left: Vec<i64> = common::sql(&env.dir)
        .prepare("SELECT id FROM logstf_logs ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    // 4 is past Arie's slack and 6 past Bea's; 2 stays for Arie although Bea's window is long past it.
    assert_eq!(left, [1, 2, 3, 5]);
    assert_eq!(
        ids(&cache.logs(arie(), LogsRange::Months3).unwrap()),
        [1, 2]
    );
}
