//! The GameBanana add-on rows: migration 14's table and the library's listing of it.

use crate::common::{self, library_env, write_file};

use rusqlite::Connection;
use tf2tools_core::{Database, GbId};

#[allow(clippy::too_many_arguments)]
fn insert_row(
    env: &common::LibraryEnv,
    gb_id: i64,
    section: &str,
    name: &str,
    file_date: i64,
    latest: i64,
    files: &[&str],
    installed: bool,
    gone: bool,
) {
    common::sql(&env.dir)
        .execute(
            "INSERT INTO library_gb_addons (gb_id, section, name, author, file_id, file_date,
             latest_file_date, checked_at, payloads, files, installed, gone)
             VALUES (?1, ?2, ?3, 'vex', 1, ?4, ?5, 0, '[\"a.vpk\"]', ?6, ?7, ?8)",
            rusqlite::params![
                gb_id,
                section,
                name,
                file_date,
                latest,
                serde_json::to_string(files).unwrap(),
                installed,
                gone
            ],
        )
        .unwrap();
}

#[test]
fn migration_14_creates_the_table_with_the_spec_columns() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).unwrap());
    let conn = Connection::open(dir.db_path()).unwrap();
    let mut stmt = conn
        .prepare("SELECT name, type, \"notnull\" FROM pragma_table_info('library_gb_addons') ORDER BY cid")
        .unwrap();
    let cols: Vec<(String, String, bool)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let expected: Vec<(String, String, bool)> = [
        ("gb_id", "INTEGER", false),
        ("section", "TEXT", true),
        ("name", "TEXT", true),
        ("author", "TEXT", true),
        ("file_id", "INTEGER", true),
        ("file_date", "INTEGER", true),
        ("latest_file_date", "INTEGER", true),
        ("checked_at", "INTEGER", true),
        ("payloads", "TEXT", true),
        ("files", "TEXT", true),
        ("installed", "INTEGER", true),
        ("gone", "INTEGER", true),
    ]
    .into_iter()
    .map(|(n, t, nn)| (n.to_owned(), t.to_owned(), nn))
    .collect();
    assert_eq!(cols, expected);
}

#[test]
fn the_list_is_ordered_and_carries_update_gone_and_installed_state() {
    let env = library_env();
    write_file(&env.tf2.custom_dir().join("tf2tools-gb-1-zeta.vpk"), b"v");
    insert_row(
        &env,
        1,
        "skins",
        "Zeta",
        100,
        200,
        &["custom/tf2tools-gb-1-zeta.vpk"],
        true,
        false,
    );
    insert_row(
        &env,
        2,
        "effects",
        "alpha",
        100,
        100,
        &["custom/tf2tools-gb-2-alpha"],
        true,
        false,
    );
    insert_row(&env, 3, "effects", "Beta", 100, 100, &[], false, true);
    let list = env.lib.list_gb_addons(Some(&env.tf2)).unwrap();
    let summary: Vec<(u64, &str, bool, bool, bool)> = list
        .iter()
        .map(|e| {
            (
                e.gb_id().get(),
                e.name(),
                e.is_installed(),
                e.has_update(),
                e.is_gone(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (2, "alpha", false, false, false),
            (3, "Beta", false, false, true),
            (1, "Zeta", true, true, false),
        ]
    );
    assert_eq!(list[2].files(), ["custom/tf2tools-gb-1-zeta.vpk"]);
    assert_eq!(list[2].payloads(), ["a.vpk"]);
    assert_eq!(list[2].section(), "skins");
    assert_eq!(list[2].gb_id(), GbId::new(1));
}

#[test]
fn without_tf2_nothing_counts_as_installed() {
    let env = library_env();
    write_file(&env.tf2.custom_dir().join("tf2tools-gb-1-zeta.vpk"), b"v");
    insert_row(
        &env,
        1,
        "skins",
        "Zeta",
        100,
        100,
        &["custom/tf2tools-gb-1-zeta.vpk"],
        true,
        false,
    );
    let list = env.lib.list_gb_addons(None).unwrap();
    assert!(!list[0].is_installed());
}

#[test]
fn a_row_whose_install_did_not_finish_is_not_installed_even_with_its_files() {
    let env = library_env();
    write_file(&env.tf2.custom_dir().join("tf2tools-gb-1-zeta.vpk"), b"v");
    insert_row(
        &env,
        1,
        "skins",
        "Zeta",
        100,
        100,
        &["custom/tf2tools-gb-1-zeta.vpk"],
        false,
        false,
    );
    assert!(!env.lib.list_gb_addons(Some(&env.tf2)).unwrap()[0].is_installed());
}
