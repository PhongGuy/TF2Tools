use crate::common;

use rusqlite::Connection;
use tf2tools_core::{Database, OpenError};

/// The tables a v5 database had at `user_version` 1, before any later step ran.
const V1_SCHEMA: &str = "
    CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE library_huds (id TEXT PRIMARY KEY, folder_name TEXT UNIQUE NOT NULL, name TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'local', store_hud_id TEXT, has_thumbnail INTEGER NOT NULL DEFAULT 0,
        has_gallery INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
    CREATE TABLE library_sounds (id TEXT PRIMARY KEY, folder_name TEXT UNIQUE NOT NULL, name TEXT NOT NULL,
        original_filename TEXT NOT NULL, created_at TEXT NOT NULL);
    CREATE TABLE library_crosshairs (id TEXT PRIMARY KEY, folder_name TEXT UNIQUE NOT NULL, name TEXT NOT NULL,
        source_type TEXT NOT NULL, created_at TEXT NOT NULL);
    CREATE TABLE crosshair_assignments (weapon_script TEXT PRIMARY KEY, crosshair_source TEXT NOT NULL,
        crosshair_ref TEXT NOT NULL);
    CREATE TABLE store_huds (hud_id TEXT PRIMARY KEY, name TEXT NOT NULL, author TEXT NOT NULL, repo TEXT,
        hash TEXT, data_json TEXT NOT NULL, fetched_at TEXT NOT NULL);
    CREATE TABLE store_cache_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE logs (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT NOT NULL, level TEXT NOT NULL,
        category TEXT NOT NULL, action TEXT NOT NULL, message TEXT NOT NULL, details TEXT);
    CREATE INDEX idx_logs_timestamp ON logs(timestamp DESC);
    CREATE INDEX idx_logs_category ON logs(category);
    PRAGMA user_version = 1;";

#[test]
fn v5_database_upgrades_to_v16_without_loss() {
    let (_tmp, dir) = common::temp_data_dir();
    common::write_v5_database(&dir);
    let before = common::dump_tables(&Connection::open(dir.db_path()).unwrap());
    assert!(
        before.values().all(|rows| !rows.is_empty()),
        "seed fills every table"
    );
    let db = Database::open(&dir).expect("open v5 database");
    assert_eq!(db.schema_version(), 16);
    drop(db);
    let after = Connection::open(dir.db_path()).unwrap();
    let mut rows = common::dump_tables(&after);
    for table in [
        "store_maps",
        "downloaded_maps",
        "mastercomfig_state",
        "mastercomfig_release",
        "library_gb_addons",
        "logstf_players",
        "logstf_logs",
        "logstf_player_logs",
        "logstf_class_rows",
        "logstf_weapon_rows",
    ] {
        assert_eq!(
            rows.remove(table).map(|r| r.len()),
            Some(0),
            "{table} starts empty"
        );
    }
    assert_eq!(rows, before);
    assert_eq!(common::schema_shape(&after), common::current_shape());
    let version: u32 = after
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 16);
    let analyzed: i64 = after
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'sqlite_stat1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(analyzed, 1, "migration 10 runs ANALYZE");
    for name in [
        "idx_library_sounds_gb",
        "idx_library_crosshairs_gb",
        "idx_library_huds_gb",
    ] {
        let sql: String = after
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = ?1",
                [name],
                |r| r.get(0),
            )
            .unwrap();
        assert!(sql.ends_with("WHERE store_gb_id IS NOT NULL"), "{sql}");
    }
    drop(after);
    let db = Database::open(&dir).expect("reopen at v16");
    assert_eq!(db.schema_version(), 16);
}

#[test]
fn v1_database_upgrades_to_v16() {
    let (_tmp, dir) = common::temp_data_dir();
    std::fs::create_dir_all(dir.root()).unwrap();
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute_batch(V1_SCHEMA).unwrap();
    conn.execute_batch(
        "INSERT INTO store_cache_meta (key, value) VALUES
            ('last_fetched', '2026-01-01T00:00:00Z'), ('sounds_hitsound_page_2', 'x'),
            ('crosshairs_all_page_1', 'y'), ('addons_installed_tag', 'v1'), ('sounds_sync_page', '4');
        INSERT INTO library_sounds (id, folder_name, name, original_filename, created_at)
            VALUES ('s1', 'boop', 'Boop', 'boop.wav', '2026-01-01T00:00:00Z');",
    )
    .unwrap();
    drop(conn);

    let db = Database::open(&dir).expect("open v1 database");
    assert_eq!(db.schema_version(), 16);
    drop(db);

    let conn = Connection::open(dir.db_path()).unwrap();
    assert_eq!(common::schema_shape(&conn), common::current_shape());
    let keys: Vec<String> = conn
        .prepare("SELECT key FROM store_cache_meta ORDER BY key")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(keys, ["curated_last_synced", "sounds_sync_page"]);
    let (source, sound_type): (String, String) = conn
        .query_row(
            "SELECT source, sound_type FROM library_sounds WHERE id = 's1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((source.as_str(), sound_type.as_str()), ("local", "wav"));
}

#[test]
fn newer_schema_is_refused_and_left_untouched() {
    let (_tmp, dir) = common::temp_data_dir();
    std::fs::create_dir_all(dir.root()).unwrap();
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute_batch("CREATE TABLE future (x); PRAGMA user_version = 17;")
        .unwrap();
    drop(conn);
    let before = std::fs::read(dir.db_path()).unwrap();
    std::fs::create_dir_all(dir.tmp_dir()).unwrap();
    let keep = dir.tmp_dir().join("keep.part");
    std::fs::write(&keep, b"partial").unwrap();

    let err = Database::open(&dir).expect_err("a newer schema must be refused");
    assert!(
        matches!(&err, OpenError::NewerSchema { path, found: 17, supported: 16 } if *path == dir.db_path()),
        "{err:?}"
    );
    assert_eq!(std::fs::read(dir.db_path()).unwrap(), before);
    assert!(keep.exists(), "a refused open must not clear tmp");
}

#[test]
fn non_sqlite_file_is_reported_corrupt_and_kept() {
    let (_tmp, dir) = common::temp_data_dir();
    std::fs::create_dir_all(dir.root()).unwrap();
    let garbage = b"this is not a sqlite database\n".repeat(200);
    std::fs::write(dir.db_path(), &garbage).unwrap();

    let err = Database::open(&dir).expect_err("garbage must be refused");
    match &err {
        OpenError::Corrupt { path, .. } => assert_eq!(path, &dir.db_path()),
        other => panic!("expected Corrupt, got {other:?}"),
    }
    assert!(err.to_string().contains("tf2tools.db"), "{err}");
    assert_eq!(std::fs::read(dir.db_path()).unwrap(), garbage);
}

#[test]
fn failed_step_rolls_back_and_keeps_earlier_steps() {
    // A view named library_sounds makes step 3's ALTER fail; step 2 must stay committed.
    let (_tmp, dir) = common::temp_data_dir();
    std::fs::create_dir_all(dir.root()).unwrap();
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute_batch(V1_SCHEMA).unwrap();
    conn.execute_batch(
        "DROP TABLE library_sounds;
        CREATE TABLE sounds_base (id TEXT, folder_name TEXT, name TEXT, original_filename TEXT, created_at TEXT);
        CREATE VIEW library_sounds AS SELECT * FROM sounds_base;",
    )
    .unwrap();
    drop(conn);

    let err = Database::open(&dir).expect_err("step 3 must fail");
    assert!(
        matches!(&err, OpenError::Migration { step: 3, .. }),
        "{err:?}"
    );
    let conn = Connection::open(dir.db_path()).unwrap();
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2);
    let store_sounds: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'store_sounds'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        store_sounds, 0,
        "step 3's CREATE rolled back with its ALTER"
    );
}

#[cfg(unix)]
#[test]
fn unwritable_data_dir_reports_io_with_the_path() {
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, dir) = common::temp_data_dir();
    std::fs::create_dir_all(dir.root()).unwrap();
    std::fs::set_permissions(dir.root(), std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(dir.root().join("probe"), b"").is_ok() {
        return; // running as root: permissions are not enforced
    }
    let result = Database::open(&dir);
    // Restore first so a failing assertion cannot leave an undeletable tempdir behind.
    std::fs::set_permissions(dir.root(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let err = result.expect_err("read-only root must fail");
    match &err {
        OpenError::Io { path, .. } => assert!(path.starts_with(dir.root()), "{path:?}"),
        other => panic!("expected Io, got {other:?}"),
    }
}

#[test]
fn v10_database_marks_the_gamebanana_mod_stores_stale_and_keeps_their_rows() {
    // v10 synced crosshair and HUD galleries missing most screenshots; step 11 forces a full resync.
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).expect("create"));
    let conn = Connection::open(dir.db_path()).unwrap();
    let fresh = common::hours_ago(0);
    for key in ["curated", "huds_gb", "sounds", "crosshairs", "addons"] {
        conn.execute(
            "INSERT OR REPLACE INTO store_cache_meta (key, value) VALUES (?1, ?2)",
            [format!("{key}_last_synced"), fresh.clone()],
        )
        .unwrap();
    }
    conn.execute_batch(
        "INSERT INTO store_cache_meta (key, value) VALUES
            ('crosshairs_sync_page', '3'), ('crosshairs_sync_start', 'x'), ('crosshairs_sync_fetched', '45'),
            ('sounds_sync_page', '2');
        INSERT INTO store_crosshairs (gb_id, name, fetched_at) VALUES (1, 'kept', 'x');
        INSERT INTO store_huds_gb (gb_id, name, fetched_at) VALUES (2, 'kept', 'x');
        PRAGMA user_version = 10;",
    )
    .unwrap();
    drop(conn);

    let db = Database::open(&dir).expect("open v10 database");
    assert_eq!(db.schema_version(), 16);
    drop(db);
    let conn = Connection::open(dir.db_path()).unwrap();
    let keys: Vec<String> = conn
        .prepare("SELECT key FROM store_cache_meta ORDER BY key")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        keys,
        [
            "curated_last_synced",
            "sounds_last_synced",
            "sounds_sync_page"
        ]
    );
    for table in ["store_crosshairs", "store_huds_gb"] {
        assert_eq!(common::count(&dir, table), 1, "{table} rows are kept");
    }
}

#[test]
fn v11_database_gains_the_map_tables() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).expect("create"));
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute_batch(
        "DROP TABLE store_maps; DROP TABLE downloaded_maps; PRAGMA user_version = 11;",
    )
    .unwrap();
    drop(conn);

    let db = Database::open(&dir).expect("open v11 database");
    assert_eq!(db.schema_version(), 16);
    drop(db);
    assert_eq!(common::count(&dir, "store_maps"), 0);
    assert_eq!(common::count(&dir, "downloaded_maps"), 0);
}

fn asset_names(dir: &tf2tools_core::DataDir, table: &str) -> Vec<String> {
    let conn = common::sql(dir);
    let mut stmt = conn
        .prepare(&format!(
            "SELECT asset_name FROM {table} ORDER BY asset_name"
        ))
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    names
}

#[test]
fn v12_database_gains_the_mastercomfig_tables_and_drops_override_addons() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).expect("create"));
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute_batch(
        "DROP TABLE mastercomfig_state; DROP TABLE mastercomfig_release;
        INSERT INTO library_addons (asset_name, display_name, installed_tag, created_at, updated_at) VALUES
            ('mastercomfig-addon-override-1.vpk', 'Config Override 1', '9.100.1', 'x', 'x'),
            ('mastercomfig-addon-override-3.vpk', 'Config Override 3', '9.100.1', 'x', 'x'),
            ('mastercomfig-addon-flat-mouse.vpk', 'Flat Mouse', '9.100.1', 'x', 'x');
        INSERT INTO store_addons (asset_name, display_name, download_url, release_tag, fetched_at) VALUES
            ('mastercomfig-addon-override-2.vpk', 'Config Override 2', 'u', '9.100.1', 'x'),
            ('mastercomfig-addon-lowmem.vpk', 'Low Memory', 'u', '9.100.1', 'x');
        PRAGMA user_version = 12;",
    )
    .unwrap();
    drop(conn);

    let db = Database::open(&dir).expect("open v12 database");
    assert_eq!(db.schema_version(), 16);
    drop(db);
    assert_eq!(
        asset_names(&dir, "library_addons"),
        ["mastercomfig-addon-flat-mouse.vpk"]
    );
    assert_eq!(
        asset_names(&dir, "store_addons"),
        ["mastercomfig-addon-lowmem.vpk"]
    );
    assert_eq!(common::count(&dir, "mastercomfig_state"), 0);
    assert_eq!(common::count(&dir, "mastercomfig_release"), 0);
    let conn = common::sql(&dir);
    assert_eq!(common::schema_shape(&conn), common::current_shape());
    let second = conn.execute(
        "INSERT INTO mastercomfig_state (id, version, preset, modules) VALUES (2, 'v', 'medium', '{}')",
        [],
    );
    assert!(second.is_err(), "the state table holds one row");
}

#[test]
fn v12_database_forgets_the_addon_sync_so_the_release_is_fetched() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).expect("create"));
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute_batch(
        "DROP TABLE mastercomfig_state; DROP TABLE mastercomfig_release;
        INSERT OR REPLACE INTO store_cache_meta (key, value) VALUES
            ('addons_last_synced', '2026-11-08T10:00:00.000000+00:00'),
            ('curated_last_synced', '2026-11-08T10:00:00.000000+00:00');
        PRAGMA user_version = 12;",
    )
    .unwrap();
    drop(conn);
    drop(Database::open(&dir).expect("open v12 database"));
    assert_eq!(common::meta(&dir, "addons_last_synced"), None);
    assert_eq!(
        common::meta(&dir, "curated_last_synced").as_deref(),
        Some("2026-11-08T10:00:00.000000+00:00"),
        "only the addon store is forgotten"
    );
}

#[test]
fn v14_database_gains_the_logstf_tables() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).expect("create"));
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute_batch(
        "DROP TABLE logstf_weapon_rows;
         DROP TABLE logstf_class_rows; DROP TABLE logstf_player_logs;
         DROP TABLE logstf_logs; DROP TABLE logstf_players;
         PRAGMA user_version = 14;",
    )
    .unwrap();
    drop(conn);

    let db = Database::open(&dir).expect("open v14 database");
    assert_eq!(db.schema_version(), 16);
    drop(db);
    for table in [
        "logstf_players",
        "logstf_logs",
        "logstf_player_logs",
        "logstf_class_rows",
    ] {
        assert_eq!(common::count(&dir, table), 0, "{table}");
    }
}

#[test]
fn v15_database_gains_weapon_rows_and_the_counted_marker() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).expect("create"));
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute_batch(
        "DROP TABLE logstf_weapon_rows;
         ALTER TABLE logstf_logs DROP COLUMN weapons_counted;
         INSERT INTO logstf_logs (id, title, map, date, body, state)
             VALUES (7, 'Log 7', 'm', 1, x'00', 0);
         PRAGMA user_version = 15;",
    )
    .unwrap();
    drop(conn);

    let db = Database::open(&dir).expect("open v15 database");
    assert_eq!(db.schema_version(), 16);
    drop(db);
    assert_eq!(common::count(&dir, "logstf_weapon_rows"), 0);
    let conn = common::sql(&dir);
    let counted: i64 = conn
        .query_row(
            "SELECT weapons_counted FROM logstf_logs WHERE id = 7",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        counted, 0,
        "a log stored before step 16 waits for the backfill"
    );
    assert_eq!(common::schema_shape(&conn), common::current_shape());
}
