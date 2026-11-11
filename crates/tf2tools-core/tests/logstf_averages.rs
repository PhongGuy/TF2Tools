//! Log rows and 90-day class averages over the cache.

use tf2tools_core::{LOG_CAP, LogState, SteamId, TfClass, TrackedPlayer};

use crate::common;
use crate::common::logstf::{
    ARIE, PlayerSpec, days_ago, list_body, log_body, logs_env, logstf_fixture, mount_list,
    mount_log, now, solo,
};

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[tokio::test]
async fn averages_are_time_weighted_over_the_window() {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    // Log 1: scout 600 s and pyro 120 s. Log 2: scout 1200 s and medic 300 s.
    let log1 = PlayerSpec {
        account: 231_702,
        name: "Arie",
        team: "Red",
        heal: 0,
        ubers: 0,
        drops: 0,
        classes: vec![("scout", 600, 10, 5, 4, 3000), ("pyro", 120, 1, 2, 0, 200)],
    };
    let log2 = PlayerSpec {
        account: 231_702,
        name: "Arie",
        team: "Red",
        heal: 3000,
        ubers: 2,
        drops: 0,
        classes: vec![
            ("scout", 1200, 20, 10, 6, 6000),
            ("medic", 300, 0, 1, 3, 50),
        ],
    };
    // Someone else in log 2 must not count.
    let other = solo(5, ("scout", 1500, 99, 0, 0, 99_999));
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(2, days_ago(1)), (1, days_ago(2))]),
    )
    .await;
    mount_log(&env.server, 1, 200, log_body(720, &[log1])).await;
    mount_log(&env.server, 2, 200, log_body(1500, &[log2, other])).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();

    // A log outside the window, written straight to the cache, must not count either.
    let steam = i64::try_from(ARIE).unwrap();
    common::sql(&env.dir)
        .execute_batch(&format!(
            "INSERT INTO logstf_logs (id, title, map, date, body, state) VALUES (9, 'old', 'm', {}, '{{}}', 0);
             INSERT INTO logstf_player_logs (steam_id, log_id) VALUES ({steam}, 9);
             INSERT INTO logstf_class_rows (log_id, steam_id, class, seconds, kills, deaths, assists, dmg)
                 VALUES (9, {steam}, 'scout', 6000, 500, 0, 0, 1);",
            days_ago(95)
        ))
        .unwrap();

    let avg = env.client.cache().class_averages(arie()).unwrap();
    let classes: Vec<TfClass> = avg.iter().map(|a| a.class()).collect();
    assert_eq!(classes, TfClass::ALL);

    let scout = &avg[0];
    assert_eq!((scout.seconds(), scout.logs()), (1800, 2));
    assert!(close(scout.kills_per_min(), 1.0));
    assert!(close(scout.deaths_per_min(), 0.5));
    assert!(close(scout.assists_per_min(), 10.0 / 30.0));
    assert!(close(scout.dmg_per_min(), 300.0));
    assert!(close(scout.kad(), 40.0 / 15.0));
    assert_eq!(scout.heal_per_min(), None);

    let pyro = &avg[2];
    assert_eq!((pyro.seconds(), pyro.logs()), (120, 1));
    assert!(close(pyro.kills_per_min(), 0.5) && close(pyro.dmg_per_min(), 100.0));
    assert!(close(pyro.kad(), 0.5));

    let medic = &avg[6];
    assert_eq!((medic.seconds(), medic.logs()), (300, 1));
    assert!(close(medic.kad(), 3.0));
    assert!(close(medic.heal_per_min().unwrap(), 600.0));
    assert!(close(medic.ubers_per_10min().unwrap(), 4.0));

    let soldier = &avg[1];
    assert!(!soldier.played());
    assert_eq!((soldier.seconds(), soldier.logs()), (0, 0));
    assert!(close(soldier.kills_per_min(), 0.0) && close(soldier.kad(), 0.0));
    assert_eq!(avg[6].class(), TfClass::Medic);
    assert_eq!(avg[1].ubers_per_10min(), None);

    assert_eq!(env.client.cache().logs(arie()).unwrap().len(), 2);
}

#[tokio::test]
async fn a_log_row_carries_the_players_line_and_state() {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(3, days_ago(1)), (2, days_ago(2)), (1, days_ago(3))]),
    )
    .await;
    mount_log(&env.server, 3, 200, logstf_fixture("log-4137103-6v6.json")).await;
    mount_log(&env.server, 2, 404, String::new()).await;
    mount_log(&env.server, 1, 200, "{}".into()).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    // A listed log whose download has not happened yet.
    let steam = i64::try_from(ARIE).unwrap();
    common::sql(&env.dir)
        .execute_batch(&format!(
            "INSERT INTO logstf_logs (id, title, map, date) VALUES (4, 'Log 4', 'm', {});
             INSERT INTO logstf_player_logs (steam_id, log_id) VALUES ({steam}, 4);",
            now() - 60
        ))
        .unwrap();

    let rows = env.client.cache().logs(arie()).unwrap();
    let ids: Vec<u64> = rows.iter().map(|r| r.id()).collect();
    assert_eq!(ids, [4, 3, 2, 1]);
    let states: Vec<LogState> = rows.iter().map(|r| r.state()).collect();
    assert_eq!(
        states,
        [
            LogState::Pending,
            LogState::Ready,
            LogState::Gone,
            LogState::Unreadable
        ]
    );

    let ready = &rows[1];
    assert_eq!(ready.title(), "Log 3");
    assert_eq!(ready.length(), Some(1714));
    assert_eq!(
        ready.classes(),
        [TfClass::Scout, TfClass::Pyro, TfClass::Heavy]
    );
    assert_eq!(
        (ready.kills(), ready.deaths(), ready.assists(), ready.dmg()),
        (7, 21, 6, 3896)
    );
    assert_eq!(ready.seconds(), 1714);
    assert!((ready.dmg_per_min().unwrap() - 136.38).abs() < 0.01);
    assert_eq!(rows[0].dmg_per_min(), None);
    assert!(env.client.cache().log_body(3).unwrap().is_some());
    assert_eq!(env.client.cache().log_body(2).unwrap(), None);
}

#[test]
fn rows_and_averages_share_the_cap() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = tf2tools_core::Database::open(&dir).unwrap();
    let cache = tf2tools_core::LogsCache::new(db);
    let steam = i64::try_from(ARIE).unwrap();
    let base = now() - 3600;
    let mut sql = String::new();
    // LOG_CAP + 1 ready logs, one scout minute each; the oldest falls outside the cap.
    for i in 0..=LOG_CAP as i64 {
        sql.push_str(&format!(
            "INSERT INTO logstf_logs (id, title, map, date, body, state) VALUES ({id}, 't', 'm', {date}, '{{}}', 0);
             INSERT INTO logstf_player_logs (steam_id, log_id) VALUES ({steam}, {id});
             INSERT INTO logstf_class_rows (log_id, steam_id, class, seconds, kills, deaths, assists, dmg)
                 VALUES ({id}, {steam}, 'scout', 60, 1, 0, 0, 0);",
            id = i + 1,
            date = base - i,
        ));
    }
    common::sql(&dir).execute_batch(&sql).unwrap();
    assert_eq!(cache.logs(arie()).unwrap().len(), LOG_CAP);
    let avg = cache.class_averages(arie()).unwrap();
    assert_eq!(avg[0].logs() as usize, LOG_CAP);
    assert_eq!(avg[0].seconds(), 60 * LOG_CAP as u64);
}

#[tokio::test]
async fn classes_with_equal_time_list_by_class_name_in_every_row() {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    // Three classes tied at 300 s in five logs: row order would only match by chance.
    let tied = PlayerSpec {
        account: 231_702,
        name: "Arie",
        team: "Red",
        heal: 0,
        ubers: 0,
        drops: 0,
        classes: vec![
            ("soldier", 300, 1, 1, 1, 100),
            ("medic", 300, 1, 1, 1, 100),
            ("scout", 300, 1, 1, 1, 100),
        ],
    };
    let ids: Vec<(u64, i64)> = (1..=5).map(|i| (i, days_ago(i as i64))).collect();
    mount_list(&env.server, ARIE, list_body(&ids)).await;
    for (id, _) in &ids {
        mount_log(
            &env.server,
            *id,
            200,
            log_body(900, std::slice::from_ref(&tied)),
        )
        .await;
    }
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    let rows = env.client.cache().logs(arie()).unwrap();
    assert_eq!(rows.len(), 5);
    for row in rows {
        assert_eq!(
            row.classes(),
            [TfClass::Medic, TfClass::Scout, TfClass::Soldier],
            "log {}",
            row.id()
        );
    }
}
