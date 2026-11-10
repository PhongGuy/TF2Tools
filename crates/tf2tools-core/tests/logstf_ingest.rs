//! What a first sync writes: order, progress, every player's class rows, medic totals.

use std::sync::Mutex;

use tf2tools_core::{SteamId, TrackedPlayer};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, PlayerSpec, class_rows, days_ago, list_body, log_body, log_state, logs_env,
    logstf_fixture, mount_list, mount_log, paths, solo,
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
async fn first_sync_downloads_newest_first_with_progress_and_every_players_rows() {
    let env = tracked_env().await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(3, days_ago(1)), (2, days_ago(2)), (1, days_ago(3))]),
    )
    .await;
    mount_log(&env.server, 3, 200, logstf_fixture("log-4137103-6v6.json")).await;
    let small = log_body(600, &[solo(231_702, ("scout", 600, 1, 1, 1, 100))]);
    mount_log(&env.server, 2, 200, small.clone()).await;
    mount_log(&env.server, 1, 200, small).await;

    let seen = Mutex::new(Vec::new());
    let summary = env
        .client
        .sync_player(arie(), false, &|p| {
            seen.lock().unwrap().push((p.done(), p.total()))
        })
        .await
        .unwrap();
    assert_eq!(
        paths(&env.server).await,
        ["/api/v1/log", "/json/3", "/json/2", "/json/1"]
    );
    assert_eq!(*seen.lock().unwrap(), [(1, 3), (2, 3), (3, 3)]);
    assert_eq!(
        (summary.listed(), summary.downloaded(), summary.capped()),
        (3, 3, false)
    );
    assert_eq!(log_state(&env.dir, 3), (0, true));
    assert_eq!(
        class_rows(&env.dir, 3),
        29,
        "every player's classes in the 6v6 log"
    );
    assert!(
        env.client.cache().players().unwrap()[0]
            .last_synced()
            .is_some()
    );
}

#[tokio::test]
async fn medic_heals_go_on_the_medic_row_only_and_unknown_classes_are_dropped() {
    let env = tracked_env().await;
    mount_list(&env.server, ARIE, list_body(&[(30, days_ago(1))])).await;
    let medic = PlayerSpec {
        account: 231_702,
        name: "Arie",
        team: "Blue",
        heal: 5000,
        ubers: 3,
        drops: 1,
        classes: vec![
            ("medic", 500, 0, 1, 4, 80),
            ("scout", 100, 2, 1, 0, 300),
            ("undefined", 20, 0, 0, 0, 0),
        ],
    };
    mount_log(&env.server, 30, 200, log_body(620, &[medic])).await;
    env.client
        .sync_player(arie(), false, &|_| {})
        .await
        .unwrap();
    let rows: Vec<(String, i64, i64, i64)> = common::sql(&env.dir)
        .prepare("SELECT class, heal, ubers, drops FROM logstf_class_rows WHERE log_id = 30 ORDER BY class")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        [
            ("medic".to_owned(), 5000, 3, 1),
            ("scout".to_owned(), 0, 0, 0)
        ]
    );
}
