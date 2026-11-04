//! The activity log over v5's logs table: append, filtered keyset queries, clear and trim.

mod common;

use common::{count, sql, temp_data_dir};
use tempfile::TempDir;
use tf2tools_core::{
    ActivityLog, DataDir, Database, LOG_KEEP, LogEntry, LogLevel, LogQuery, NewLogEntry,
};

fn log_env() -> (TempDir, DataDir, ActivityLog) {
    let (tmp, dir) = temp_data_dir();
    let db = Database::open(&dir).unwrap();
    (tmp, dir, ActivityLog::new(db))
}

fn add(log: &ActivityLog, level: LogLevel, category: &str, message: &str) {
    log.append(&NewLogEntry::new(level, category, "act", message))
        .unwrap();
}

fn messages(rows: &[LogEntry]) -> Vec<&str> {
    rows.iter().map(|r| r.message()).collect()
}

#[test]
fn append_then_query_returns_newest_first_with_every_field() {
    let (_tmp, _dir, log) = log_env();
    add(&log, LogLevel::Info, "hud", "first");
    log.append(
        &NewLogEntry::new(LogLevel::Error, "sound", "install", "second").with_details("boom"),
    )
    .unwrap();
    let rows = log.query(&LogQuery::new(50)).unwrap();
    assert_eq!(messages(&rows), ["second", "first"]);
    let r = &rows[0];
    assert_eq!(
        (r.level(), r.category(), r.action(), r.details()),
        (LogLevel::Error, "sound", "install", Some("boom"))
    );
    assert!(rows[1].details().is_none());
    assert!(r.id() > rows[1].id());
    assert_eq!(r.timestamp().len(), 32);
    assert!(r.timestamp().ends_with("+00:00"));
}

#[test]
fn levels_are_stored_as_v5_text_and_unknown_text_reads_as_info() {
    let (_tmp, dir, log) = log_env();
    add(&log, LogLevel::Info, "hud", "i");
    add(&log, LogLevel::Warning, "hud", "w");
    add(&log, LogLevel::Error, "hud", "e");
    let conn = sql(&dir);
    let mut stmt = conn.prepare("SELECT level FROM logs ORDER BY id").unwrap();
    let levels: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(levels, ["info", "warning", "error"]);
    conn.execute(
        "INSERT INTO logs (timestamp, level, category, action, message)
         VALUES ('2025-01-01T00:00:00+00:00', 'debug', 'hud', 'act', 'odd')",
        [],
    )
    .unwrap();
    let rows = log.query(&LogQuery::new(1)).unwrap();
    assert_eq!(
        (rows[0].message(), rows[0].level()),
        ("odd", LogLevel::Info)
    );
}

#[test]
fn a_category_also_matches_its_store_category_and_levels_filter() {
    let (_tmp, _dir, log) = log_env();
    add(&log, LogLevel::Info, "hud", "a");
    add(&log, LogLevel::Error, "hud_store", "b");
    add(&log, LogLevel::Info, "sound", "c");
    add(&log, LogLevel::Info, "hudx", "d");
    let hud = log.query(&LogQuery::new(50).with_category("hud")).unwrap();
    assert_eq!(messages(&hud), ["b", "a"]);
    let errors = log
        .query(
            &LogQuery::new(50)
                .with_category("hud")
                .with_level(LogLevel::Error),
        )
        .unwrap();
    assert_eq!(messages(&errors), ["b"]);
    let info = log
        .query(&LogQuery::new(50).with_level(LogLevel::Info))
        .unwrap();
    assert_eq!(messages(&info), ["d", "c", "a"]);
}

#[test]
fn pages_follow_the_id_keyset() {
    let (_tmp, _dir, log) = log_env();
    for m in ["1", "2", "3", "4", "5"] {
        add(&log, LogLevel::Info, "hud", m);
    }
    let first = log.query(&LogQuery::new(2)).unwrap();
    assert_eq!(messages(&first), ["5", "4"]);
    let next = log
        .query(&LogQuery::new(2).with_before(first[1].id()))
        .unwrap();
    assert_eq!(messages(&next), ["3", "2"]);
    let last = log
        .query(&LogQuery::new(2).with_before(next[1].id()))
        .unwrap();
    assert_eq!(messages(&last), ["1"]);
}

#[test]
fn clear_removes_every_row() {
    let (_tmp, dir, log) = log_env();
    add(&log, LogLevel::Info, "hud", "a");
    add(&log, LogLevel::Info, "hud", "b");
    log.clear().unwrap();
    assert_eq!(count(&dir, "logs"), 0);
    assert!(log.query(&LogQuery::new(10)).unwrap().is_empty());
}

#[test]
fn trim_keeps_the_newest_rows() {
    let (_tmp, dir, log) = log_env();
    for i in 0..25 {
        add(&log, LogLevel::Info, "hud", &i.to_string());
    }
    assert_eq!(log.trim(10).unwrap(), 15);
    assert_eq!(count(&dir, "logs"), 10);
    let rows = log.query(&LogQuery::new(50)).unwrap();
    assert_eq!(rows.last().unwrap().message(), "15");
    assert_eq!(log.trim(10).unwrap(), 0);
    assert_eq!(log.trim(100).unwrap(), 0);
}

#[test]
fn trim_to_log_keep_removes_exactly_the_excess() {
    let (_tmp, dir, log) = log_env();
    sql(&dir)
        .execute_batch(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 10050)
             INSERT INTO logs (timestamp, level, category, action, message)
             SELECT '2025-01-01T00:00:00+00:00', 'info', 'hud', 'act', 'row ' || i FROM n;",
        )
        .unwrap();
    assert_eq!(log.trim(LOG_KEEP).unwrap(), 50);
    assert_eq!(count(&dir, "logs"), i64::from(LOG_KEEP));
    let oldest: String = sql(&dir)
        .query_row("SELECT message FROM logs ORDER BY id LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(oldest, "row 51");
}
