//! HUD library: import with provenance, listing and deletion.

use crate::common;

use std::fs;
use std::path::PathBuf;

use common::{LibraryEnv, library_env, sql, tree, write_file, zip_bytes};
use tf2tools_core::{GbId, HudId, ImportError, LibraryError, LibraryKind, Provenance};

fn hud_zip(env: &LibraryEnv, file: &str, entries: &[(&str, &[u8])]) -> PathBuf {
    write_file(
        &env.tmp.path().join("downloads").join(file),
        &zip_bytes(entries),
    )
}

fn columns(
    env: &LibraryEnv,
    folder: &str,
) -> (String, String, Option<String>, Option<i64>, String) {
    sql(&env.dir)
        .query_row(
            "SELECT id, source, store_hud_id, store_gb_id, author FROM library_huds WHERE folder_name = ?1",
            [folder],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap()
}

#[test]
fn import_finds_the_info_vdf_folder_and_copies_it_into_the_library() {
    let env = library_env();
    let archive = hud_zip(
        &env,
        "rayshud.zip",
        &[
            ("rayshud-master/rayshud/info.vdf", b"\"rayshud\" {}"),
            (
                "rayshud-master/rayshud/resource/ui/hudlayout.res",
                b"layout",
            ),
            ("rayshud-master/README.md", b"readme"),
        ],
    );
    let entry = env.lib.import_hud(&archive, Provenance::Local).unwrap();
    assert_eq!(entry.folder_name(), "rayshud");
    assert_eq!(entry.name(), "rayshud");
    assert_eq!(entry.author(), "");
    assert_eq!(entry.provenance(), &Provenance::Local);
    assert!(!entry.has_thumbnail());
    assert!(!entry.has_gallery());
    assert!(!entry.created_at().is_empty());

    let dest = env.dir.library_dir(LibraryKind::Huds).join("rayshud");
    assert_eq!(tree(&dest), ["info.vdf", "resource/ui/hudlayout.res"]);
    assert!(dest.join(".tf2tools").is_dir());
    assert_eq!(
        columns(&env, "rayshud"),
        (
            entry.id().as_str().to_owned(),
            "local".into(),
            None,
            None,
            String::new()
        )
    );
    assert_eq!(env.lib.list_huds().unwrap(), vec![entry]);
    assert_eq!(fs::read_dir(env.dir.tmp_dir()).unwrap().count(), 0);
}

#[test]
fn curated_import_records_store_hud_id_and_the_store_rows_name_and_author() {
    let env = library_env();
    sql(&env.dir)
        .execute(
            "INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
             VALUES ('budhud', 'budhud', 'rbjaxter', 'rbjaxter/budhud', 'abc', '{}', '2026-07-04T17:45:05Z')",
            [],
        )
        .unwrap();
    let archive = hud_zip(
        &env,
        "budhud.zip",
        &[
            ("budhud-abc/info.vdf", b"x"),
            ("budhud-abc/scripts/hudlayout.res", b"y"),
        ],
    );
    let curated = Provenance::Curated {
        hud_id: "budhud".into(),
    };
    let entry = env.lib.import_hud(&archive, curated.clone()).unwrap();
    assert_eq!(entry.folder_name(), "budhud-abc");
    assert_eq!(entry.name(), "budhud");
    assert_eq!(entry.author(), "rbjaxter");
    assert_eq!(entry.provenance(), &curated);
    let (_, source, store_hud_id, store_gb_id, author) = columns(&env, "budhud-abc");
    assert_eq!(
        (
            source.as_str(),
            store_hud_id.as_deref(),
            store_gb_id,
            author.as_str()
        ),
        ("store", Some("budhud"), None, "rbjaxter")
    );
}

#[test]
fn gamebanana_import_records_store_gb_and_the_provenance_author() {
    let env = library_env();
    sql(&env.dir)
        .execute(
            "INSERT INTO store_huds_gb (gb_id, name, author, fetched_at)
             VALUES (3, 'Hypnotize HUD', 'store-author', '2026-07-04T17:45:30Z')",
            [],
        )
        .unwrap();
    let archive = hud_zip(&env, "hypno.zip", &[("hypnotize/info.vdf", b"x")]);
    let gb = Provenance::GameBanana {
        gb_id: GbId::new(3),
        author: "Hypnootize".into(),
    };
    let entry = env.lib.import_hud(&archive, gb.clone()).unwrap();
    assert_eq!(entry.name(), "Hypnotize HUD");
    assert_eq!(entry.author(), "Hypnootize");
    assert_eq!(entry.provenance(), &gb);
    let (_, source, store_hud_id, store_gb_id, author) = columns(&env, "hypnotize");
    assert_eq!(
        (source.as_str(), store_hud_id, store_gb_id, author.as_str()),
        ("store_gb", None, Some(3), "Hypnootize")
    );

    // Without a store row the folder name stands in for the display name.
    let other = hud_zip(&env, "other.zip", &[("otherhud/info.vdf", b"x")]);
    let unknown = Provenance::GameBanana {
        gb_id: GbId::new(99),
        author: "a".into(),
    };
    assert_eq!(
        env.lib.import_hud(&other, unknown).unwrap().name(),
        "otherhud"
    );
}

#[test]
fn reimporting_the_same_store_item_replaces_files_and_row() {
    let env = library_env();
    let curated = Provenance::Curated {
        hud_id: "myhud".into(),
    };
    let first = hud_zip(
        &env,
        "v1.zip",
        &[("myhud/info.vdf", b"1"), ("myhud/old.res", b"old")],
    );
    env.lib.import_hud(&first, curated.clone()).unwrap();
    let second = hud_zip(
        &env,
        "v2.zip",
        &[("myhud/info.vdf", b"2"), ("myhud/new.res", b"new")],
    );
    let entry = env.lib.import_hud(&second, curated).unwrap();

    let listed = env.lib.list_huds().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id(), entry.id());
    let dest = env.dir.library_dir(LibraryKind::Huds).join("myhud");
    assert_eq!(tree(&dest), ["info.vdf", "new.res"]);
}

#[test]
fn a_hud_at_the_archive_root_is_named_after_the_archive() {
    let env = library_env();
    let archive = hud_zip(
        &env,
        "flawhud.zip",
        &[("info.vdf", b"x"), ("resource/a.res", b"a")],
    );
    let entry = env.lib.import_hud(&archive, Provenance::Local).unwrap();
    assert_eq!(entry.folder_name(), "flawhud");
    let dest = env.dir.library_dir(LibraryKind::Huds).join("flawhud");
    assert_eq!(tree(&dest), ["info.vdf", "resource/a.res"]);
}

#[test]
fn the_shallowest_info_vdf_wins() {
    let env = library_env();
    let archive = hud_zip(
        &env,
        "pack.zip",
        &[
            ("pack/hud/customizations/alt/info.vdf", b"alt"),
            ("pack/hud/info.vdf", b"main"),
            ("pack/hud/a.res", b"a"),
        ],
    );
    let entry = env.lib.import_hud(&archive, Provenance::Local).unwrap();
    assert_eq!(entry.folder_name(), "hud");
    let dest = env.dir.library_dir(LibraryKind::Huds).join("hud");
    assert_eq!(
        tree(&dest),
        ["a.res", "customizations/alt/info.vdf", "info.vdf"]
    );
}

#[test]
fn metadata_folders_in_the_archive_are_not_copied() {
    let env = library_env();
    let archive = hud_zip(
        &env,
        "m.zip",
        &[
            ("m/info.vdf", b"x"),
            ("m/.tf2tools/thumbnail.webp", b"stale"),
        ],
    );
    env.lib.import_hud(&archive, Provenance::Local).unwrap();
    let dest = env.dir.library_dir(LibraryKind::Huds).join("m");
    assert_eq!(tree(&dest), ["info.vdf"]);
}

#[test]
fn an_archive_without_info_vdf_is_not_a_hud_and_leaves_nothing_behind() {
    let env = library_env();
    let archive = hud_zip(&env, "nothud.zip", &[("x/readme.txt", b"r")]);
    let err = env.lib.import_hud(&archive, Provenance::Local).unwrap_err();
    assert!(matches!(err, ImportError::NotAHud { .. }), "{err:?}");
    assert_eq!(
        fs::read_dir(env.dir.library_dir(LibraryKind::Huds))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(fs::read_dir(env.dir.tmp_dir()).unwrap().count(), 0);
    assert!(env.lib.list_huds().unwrap().is_empty());
}

#[test]
fn a_bare_file_is_not_a_hud() {
    let env = library_env();
    let file = write_file(&env.tmp.path().join("hud.vdf"), b"not an archive");
    let err = env.lib.import_hud(&file, Provenance::Local).unwrap_err();
    assert!(matches!(err, ImportError::NotAHud { .. }), "{err:?}");
}

#[test]
fn a_rar_hud_imports_like_a_zip() {
    let env = library_env();
    let file = write_file(
        &env.tmp.path().join("downloads").join("hud.rar"),
        &common::rar_bytes(
            rars::ArchiveVersion::Rar50,
            &[
                ("myhud/info.vdf", b"\"myhud\" {}"),
                ("myhud/resource/ui/a.res", b"res"),
            ],
            None,
        ),
    );
    let entry = env.lib.import_hud(&file, Provenance::Local).unwrap();
    assert_eq!(entry.folder_name(), "myhud");
    let dest = env.dir.library_dir(LibraryKind::Huds).join("myhud");
    assert_eq!(tree(&dest), ["info.vdf", "resource/ui/a.res"]);
}

#[test]
fn delete_removes_files_and_row() {
    let env = library_env();
    let archive = hud_zip(&env, "d.zip", &[("delme/info.vdf", b"x")]);
    let entry = env.lib.import_hud(&archive, Provenance::Local).unwrap();
    env.lib.delete_hud(entry.id()).unwrap();
    assert!(env.lib.list_huds().unwrap().is_empty());
    assert!(
        !env.dir
            .library_dir(LibraryKind::Huds)
            .join("delme")
            .exists()
    );
}

#[test]
fn an_unsafe_folder_name_from_the_database_touches_no_files() {
    let env = library_env();
    let sentinel = write_file(&env.dir.root().join("library/escape/keep.txt"), b"keep");
    sql(&env.dir)
        .execute(
            "INSERT INTO library_huds (id, folder_name, name, created_at, updated_at)
             VALUES ('bad', '../escape', 'bad', 't', 't')",
            [],
        )
        .unwrap();
    env.lib.delete_hud(&HudId::new("bad")).unwrap();
    assert!(sentinel.is_file());
    assert!(env.lib.list_huds().unwrap().is_empty());
}

#[test]
fn deleting_an_unknown_id_is_not_found() {
    let env = library_env();
    let err = env.lib.delete_hud(&HudId::new("nope")).unwrap_err();
    assert!(matches!(err, LibraryError::NotFound { .. }), "{err:?}");
}
