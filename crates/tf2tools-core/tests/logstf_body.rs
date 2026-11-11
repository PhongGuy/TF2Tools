//! Log bodies are stored compressed, and plain-text bodies from earlier builds still open.

use tf2tools_core::{
    Database, LOG_BODY_LIMIT, LogsCache, LogsCancel, LogsRange, SteamId, TrackedPlayer, parse_log,
};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, days_ago, list_body, log_state, logs_env, logstf_fixture, mount_list, mount_log,
    now, paths,
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

fn storage(dir: &tf2tools_core::DataDir, id: i64) -> (String, i64) {
    common::sql(dir)
        .query_row(
            "SELECT typeof(body), length(body) FROM logstf_logs WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
}

/// Recreates `logstf_logs` as the pre-release step 15 declared it, with a TEXT body column.
fn old_body_column(dir: &tf2tools_core::DataDir) {
    common::sql(dir)
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TABLE logstf_logs;
             CREATE TABLE logstf_logs (
                 id INTEGER PRIMARY KEY, title TEXT NOT NULL, map TEXT NOT NULL,
                 date INTEGER NOT NULL, length INTEGER, body TEXT, fetched_at INTEGER,
                 state INTEGER NOT NULL DEFAULT 0);",
        )
        .unwrap();
}

#[tokio::test]
async fn a_downloaded_body_is_stored_compressed_and_reads_back_whole() {
    let env = tracked_env().await;
    let body = logstf_fixture("log-4137103-6v6.json");
    mount_list(&env.server, ARIE, list_body(&[(3, days_ago(1))])).await;
    mount_log(&env.server, 3, 200, body.clone()).await;
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
    let (kind, bytes) = storage(&env.dir, 3);
    assert_eq!(kind, "blob");
    assert!(
        bytes * 4 < body.len() as i64,
        "{bytes} bytes for {}",
        body.len()
    );
    assert_eq!(env.client.cache().log_body(3).unwrap(), Some(body));
}

#[test]
fn a_fresh_database_declares_the_body_column_blob() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).unwrap());
    let declared: String = common::sql(&dir)
        .query_row(
            "SELECT type FROM pragma_table_info('logstf_logs') WHERE name = 'body'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(declared, "BLOB");
}

#[tokio::test]
async fn plain_text_bodies_from_an_earlier_build_keep_opening_beside_new_ones() {
    let env = tracked_env().await;
    old_body_column(&env.dir);
    let old = logstf_fixture("log-4150405-hl.json");
    let steam = i64::try_from(ARIE).unwrap();
    let conn = common::sql(&env.dir);
    conn.execute(
        "INSERT INTO logstf_logs (id, title, map, date, length, body, fetched_at)
         VALUES (7, 't', 'm', ?1, 2068, ?2, ?3)",
        rusqlite::params![days_ago(2), old, now()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO logstf_player_logs (steam_id, log_id) VALUES (?1, 7)",
        [steam],
    )
    .unwrap();
    // A sync lists both; only the new one downloads, into the TEXT-declared column.
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(8, days_ago(1)), (7, days_ago(2))]),
    )
    .await;
    let new = logstf_fixture("log-4137103-6v6.json");
    mount_log(&env.server, 8, 200, new.clone()).await;
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
    assert_eq!(paths(&env.server).await, ["/api/v1/log", "/json/8"]);
    assert_eq!(storage(&env.dir, 7).0, "text");
    assert_eq!(storage(&env.dir, 8).0, "blob");
    let cache = env.client.cache();
    assert_eq!(cache.log_body(7).unwrap(), Some(old.clone()));
    assert_eq!(cache.log_body(8).unwrap(), Some(new));
    assert_eq!(parse_log(&old).unwrap().players().len(), 18);
    assert_eq!(log_state(&env.dir, 7), (0, true));
}

#[test]
fn a_damaged_blob_is_an_error_not_a_missing_body() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = LogsCache::new(Database::open(&dir).unwrap());
    common::sql(&dir)
        .execute_batch(
            "INSERT INTO logstf_logs (id, title, map, date, body) VALUES (5, 't', 'm', 1, x'00010203');
             INSERT INTO logstf_logs (id, title, map, date) VALUES (6, 't', 'm', 1);",
        )
        .unwrap();
    assert!(cache.log_body(5).is_err());
    assert_eq!(cache.log_body(6).unwrap(), None);
    assert_eq!(cache.log_body(99).unwrap(), None);
}

/// Stores `len` bytes of `a` as log 5's zlib body, as the client would compress it.
fn stored_body_of(dir: &tf2tools_core::DataDir, len: u64) {
    use std::io::Write;
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
    zlib.write_all(&vec![b'a'; usize::try_from(len).unwrap()])
        .unwrap();
    common::sql(dir)
        .execute(
            "INSERT OR REPLACE INTO logstf_logs (id, title, map, date, body)
             VALUES (5, 't', 'm', 1, ?1)",
            [zlib.finish().unwrap()],
        )
        .unwrap();
}

#[test]
fn a_stored_body_reads_up_to_the_size_cap_and_no_further() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = LogsCache::new(Database::open(&dir).unwrap());
    stored_body_of(&dir, LOG_BODY_LIMIT);
    let body = cache.log_body(5).unwrap().unwrap();
    assert_eq!(body.len() as u64, LOG_BODY_LIMIT);
    stored_body_of(&dir, LOG_BODY_LIMIT + 1);
    assert!(cache.log_body(5).is_err());
}
