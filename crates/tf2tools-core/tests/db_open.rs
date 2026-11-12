use crate::common;

use rusqlite::Connection;
use tf2tools_core::{Database, LibraryKind};

#[test]
fn fresh_open_creates_layout_and_reaches_v18() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).expect("open");
    assert_eq!(db.schema_version(), 18);
    assert!(dir.db_path().is_file());
    assert!(dir.tmp_dir().is_dir());
    for kind in LibraryKind::ALL {
        assert!(dir.library_dir(kind).is_dir(), "{kind:?}");
    }
}

#[test]
fn fresh_database_is_wal_at_schema_v18() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).expect("open"));
    let conn = Connection::open(dir.db_path()).unwrap();
    let mode: String = conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 18);
}

#[test]
fn fresh_schema_matches_the_current_shape() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).expect("open"));
    let fresh = Connection::open(dir.db_path()).unwrap();
    assert_eq!(common::schema_shape(&fresh), common::current_shape());
}

#[test]
fn reopen_keeps_rows_and_version() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).expect("first open"));
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute(
        "INSERT INTO store_cache_meta (key, value) VALUES ('sounds_sync_page', '7')",
        [],
    )
    .unwrap();
    drop(conn);
    let db = Database::open(&dir).expect("second open");
    assert_eq!(db.schema_version(), 18);
    drop(db);
    let conn = Connection::open(dir.db_path()).unwrap();
    let page: String = conn
        .query_row(
            "SELECT value FROM store_cache_meta WHERE key = 'sounds_sync_page'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(page, "7");
}

#[test]
fn open_clears_leftover_temp_files_but_keeps_the_library() {
    let (_tmp, dir) = common::temp_data_dir();
    std::fs::create_dir_all(dir.tmp_dir().join("nested")).unwrap();
    std::fs::write(dir.tmp_dir().join("download.part"), b"partial").unwrap();
    std::fs::write(dir.tmp_dir().join("nested/x"), b"x").unwrap();
    let hud = dir.library_dir(LibraryKind::Huds).join("rayshud/info.vdf");
    std::fs::create_dir_all(hud.parent().unwrap()).unwrap();
    std::fs::write(&hud, b"\"rayshud\" {}").unwrap();
    let _db = Database::open(&dir).expect("open");
    assert!(dir.tmp_dir().is_dir());
    assert_eq!(std::fs::read_dir(dir.tmp_dir()).unwrap().count(), 0);
    assert!(hud.is_file());
}

#[test]
fn database_is_clone_send_sync_debug() {
    fn assert_traits<T: Clone + Send + Sync + std::fmt::Debug + 'static>() {}
    assert_traits::<Database>();
}
