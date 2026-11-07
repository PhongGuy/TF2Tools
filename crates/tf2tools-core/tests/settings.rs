use crate::common;

use rusqlite::Connection;
use std::path::Path;
use tf2tools_core::{Database, Volume};

fn raw_setting(dir: &tf2tools_core::DataDir, key: &str) -> Option<String> {
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .ok()
}

#[test]
fn fresh_database_has_v5_defaults() {
    let (_tmp, dir) = common::temp_data_dir();
    let s = Database::open(&dir).unwrap().settings().unwrap();
    assert_eq!(s.tf2_path(), None);
    assert_eq!(s.volume().get(), 50);
    assert_eq!(s.theme(), "dark");
    assert_eq!(s.sync_cache_hours(), 168);
    assert!(!s.sidebar_collapsed());
}

#[test]
fn setters_round_trip_and_persist_across_reopen() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    db.set_tf2_path(Some(Path::new("/games/Team Fortress 2")))
        .unwrap();
    db.set_volume(Volume::new(75).unwrap()).unwrap();
    db.set_theme("light").unwrap();
    db.set_sync_cache_hours(24).unwrap();
    drop(db);
    let s = Database::open(&dir).unwrap().settings().unwrap();
    assert_eq!(s.tf2_path(), Some(Path::new("/games/Team Fortress 2")));
    assert_eq!(s.volume().get(), 75);
    assert_eq!(s.theme(), "light");
    assert_eq!(s.sync_cache_hours(), 24);
}

#[test]
fn setters_store_v5_text_values() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    db.set_tf2_path(Some(Path::new("/games/Team Fortress 2")))
        .unwrap();
    db.set_volume(Volume::new(75).unwrap()).unwrap();
    db.set_sync_cache_hours(24).unwrap();
    db.set_theme("light").unwrap();
    drop(db);
    assert_eq!(
        raw_setting(&dir, "tf2_path").as_deref(),
        Some("/games/Team Fortress 2")
    );
    assert_eq!(raw_setting(&dir, "volume").as_deref(), Some("75"));
    assert_eq!(raw_setting(&dir, "sync_cache_hours").as_deref(), Some("24"));
    assert_eq!(raw_setting(&dir, "theme").as_deref(), Some("light"));
}

#[test]
fn clearing_tf2_path_deletes_the_key() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    db.set_tf2_path(Some(Path::new("/games/tf2"))).unwrap();
    db.set_tf2_path(None).unwrap();
    assert_eq!(db.settings().unwrap().tf2_path(), None);
    drop(db);
    assert_eq!(raw_setting(&dir, "tf2_path"), None);
}

#[test]
fn v5_values_are_read_leniently() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).unwrap());
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute_batch(
        "INSERT OR REPLACE INTO settings (key, value) VALUES
            ('volume', '150'), ('sync_cache_hours', 'soon'), ('tf2_path', ''),
            ('installed_hud_folder', 'rayshud');",
    )
    .unwrap();
    drop(conn);
    let s = Database::open(&dir).unwrap().settings().unwrap();
    assert_eq!(s.volume().get(), 100);
    assert_eq!(s.sync_cache_hours(), 168);
    assert_eq!(s.tf2_path(), None);

    let conn = Connection::open(dir.db_path()).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('volume', 'loud')",
        [],
    )
    .unwrap();
    drop(conn);
    assert_eq!(
        Database::open(&dir)
            .unwrap()
            .settings()
            .unwrap()
            .volume()
            .get(),
        50
    );
}

#[test]
fn volume_accepts_0_to_100_only() {
    assert_eq!(Volume::new(0).map(Volume::get), Some(0));
    assert_eq!(Volume::new(100).map(Volume::get), Some(100));
    assert_eq!(Volume::new(101), None);
    assert_eq!(Volume::default().get(), 50);
}

#[cfg(unix)]
#[test]
fn non_utf8_tf2_path_is_rejected() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    let path = Path::new(OsStr::from_bytes(b"/games/\xff"));
    let err = db
        .set_tf2_path(Some(path))
        .expect_err("non-UTF-8 must be refused");
    assert!(
        matches!(err, tf2tools_core::SettingsError::NonUtf8Path { .. }),
        "{err:?}"
    );
    assert_eq!(db.settings().unwrap().tf2_path(), None);
}

#[test]
fn sidebar_collapsed_defaults_false_and_round_trips() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    assert!(!db.settings().unwrap().sidebar_collapsed());
    db.set_sidebar_collapsed(true).unwrap();
    assert_eq!(
        raw_setting(&dir, "sidebar_collapsed").as_deref(),
        Some("true")
    );
    drop(db);
    let db = Database::open(&dir).unwrap();
    assert!(db.settings().unwrap().sidebar_collapsed());
    db.set_sidebar_collapsed(false).unwrap();
    assert_eq!(
        raw_setting(&dir, "sidebar_collapsed").as_deref(),
        Some("false")
    );
    assert!(!db.settings().unwrap().sidebar_collapsed());
}

#[test]
fn sidebar_collapsed_reads_leniently() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    common::set_setting(&dir, "sidebar_collapsed", "1");
    assert!(db.settings().unwrap().sidebar_collapsed());
    common::set_setting(&dir, "sidebar_collapsed", "garbage");
    assert!(!db.settings().unwrap().sidebar_collapsed());
}

#[test]
fn set_tf2_path_if_unset_never_replaces_a_saved_path() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    assert!(db.set_tf2_path_if_unset(Path::new("/games/first")).unwrap());
    assert!(
        !db.set_tf2_path_if_unset(Path::new("/games/second"))
            .unwrap()
    );
    assert_eq!(
        db.settings().unwrap().tf2_path(),
        Some(Path::new("/games/first"))
    );
}

#[test]
fn set_tf2_path_if_unset_fills_an_empty_v5_value() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    drop(db);
    Connection::open(dir.db_path())
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('tf2_path', '')",
            [],
        )
        .unwrap();
    let db = Database::open(&dir).unwrap();
    assert!(db.set_tf2_path_if_unset(Path::new("/games/tf2")).unwrap());
    assert_eq!(raw_setting(&dir, "tf2_path").as_deref(), Some("/games/tf2"));
}

#[test]
fn font_check_dismissal_round_trips_per_hud() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    assert!(db.font_check_dismissed("budhud").unwrap().is_empty());
    db.set_font_check_dismissed(
        "budhud",
        &[
            "font:lato".to_owned(),
            "issue:uppercase:resource/A.ttf".to_owned(),
        ],
    )
    .unwrap();
    db.set_font_check_dismissed("m0rehud", &["font:surface".to_owned()])
        .unwrap();
    drop(db);
    let db = Database::open(&dir).unwrap();
    assert_eq!(
        db.font_check_dismissed("budhud").unwrap(),
        ["font:lato", "issue:uppercase:resource/A.ttf"]
    );
    assert_eq!(
        db.font_check_dismissed("m0rehud").unwrap(),
        ["font:surface"]
    );
    let raw: serde_json::Value =
        serde_json::from_str(&common::setting(&dir, "font_check_dismissed").unwrap()).unwrap();
    assert_eq!(
        raw,
        serde_json::json!({
            "budhud": ["font:lato", "issue:uppercase:resource/A.ttf"],
            "m0rehud": ["font:surface"],
        })
    );
}

#[test]
fn a_malformed_dismissal_reads_empty_and_is_replaced() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    for bad in [
        "not json",
        "[1, 2]",
        r#"{"x": "not a list"}"#,
        r#"{"x": [1]}"#,
    ] {
        common::set_setting(&dir, "font_check_dismissed", bad);
        assert!(db.font_check_dismissed("x").unwrap().is_empty(), "{bad}");
    }
    db.set_font_check_dismissed("x", &["font:a".to_owned()])
        .unwrap();
    assert_eq!(db.font_check_dismissed("x").unwrap(), ["font:a"]);
}

#[test]
fn the_dismissal_key_leaves_the_other_settings_alone() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    db.set_font_check_dismissed("x", &["font:a".to_owned()])
        .unwrap();
    assert_eq!(db.settings().unwrap().theme(), "dark");
}

#[test]
fn maker_grid_lines_default_off_and_round_trip() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    assert!(!db.settings().unwrap().maker_grid_lines());
    db.set_maker_grid_lines(true).unwrap();
    assert_eq!(
        raw_setting(&dir, "maker_grid_lines").as_deref(),
        Some("true")
    );
    drop(db);
    let db = Database::open(&dir).unwrap();
    assert!(db.settings().unwrap().maker_grid_lines());
    db.set_maker_grid_lines(false).unwrap();
    assert_eq!(
        raw_setting(&dir, "maker_grid_lines").as_deref(),
        Some("false")
    );
    assert!(!db.settings().unwrap().maker_grid_lines());
}

#[test]
fn hud_maker_panel_defaults_to_none_and_round_trips() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    assert_eq!(db.settings().unwrap().hud_maker_panel(), None);
    db.set_hud_maker_panel("right 16 16 collapsed").unwrap();
    assert_eq!(
        raw_setting(&dir, "hud_maker_panel").as_deref(),
        Some("right 16 16 collapsed")
    );
    drop(db);
    let db = Database::open(&dir).unwrap();
    assert_eq!(
        db.settings().unwrap().hud_maker_panel(),
        Some("right 16 16 collapsed")
    );
}

#[test]
fn hud_maker_backdrop_defaults_to_none_round_trips_and_clears() {
    let (_tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    assert_eq!(db.settings().unwrap().hud_maker_backdrop(), None);
    let shot = Path::new("/home/player/Pictures/koth_shot.jpg");
    db.set_hud_maker_backdrop(Some(shot)).unwrap();
    assert_eq!(
        raw_setting(&dir, "hud_maker_backdrop").as_deref(),
        Some("/home/player/Pictures/koth_shot.jpg")
    );
    drop(db);
    let db = Database::open(&dir).unwrap();
    assert_eq!(db.settings().unwrap().hud_maker_backdrop(), Some(shot));
    db.set_hud_maker_backdrop(None).unwrap();
    assert_eq!(raw_setting(&dir, "hud_maker_backdrop"), None);
    assert_eq!(db.settings().unwrap().hud_maker_backdrop(), None);
}
