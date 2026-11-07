//! A HUD arriving under a name the library already uses: the same store item updates in
//! place, anything else is kept beside the old one under a numbered name.

use crate::common;

use std::fs;
use std::path::PathBuf;

use common::{LibraryEnv, library_env, sql, write_file, zip_bytes};
use tf2tools_core::{GbId, HudEntry, LibraryKind, Provenance};

fn hud_zip(env: &LibraryEnv, file: &str, folder: &str, layout: &[u8]) -> PathBuf {
    let info = format!("{folder}/info.vdf");
    let res = format!("{folder}/resource/ui/hudlayout.res");
    write_file(
        &env.tmp.path().join("downloads").join(file),
        &zip_bytes(&[(&info, b"x"), (&res, layout)]),
    )
}

fn import(env: &LibraryEnv, file: &str, folder: &str, layout: &[u8], p: Provenance) -> HudEntry {
    let archive = hud_zip(env, file, folder, layout);
    env.lib.import_hud(&archive, p).unwrap()
}

fn gb(id: u64) -> Provenance {
    Provenance::GameBanana {
        gb_id: GbId::new(id),
        author: "someone".into(),
    }
}

fn curated(hud_id: &str) -> Provenance {
    Provenance::Curated {
        hud_id: hud_id.into(),
    }
}

fn store_gb(env: &LibraryEnv, gb_id: i64, name: &str) {
    sql(&env.dir)
        .execute(
            "INSERT INTO store_huds_gb (gb_id, name, author, fetched_at)
             VALUES (?1, ?2, 'someone', '2026-07-04T17:45:30Z')",
            rusqlite::params![gb_id, name],
        )
        .unwrap();
}

fn layout(env: &LibraryEnv, folder: &str) -> Vec<u8> {
    let dir = env.dir.library_dir(LibraryKind::Huds).join(folder);
    fs::read(dir.join("resource/ui/hudlayout.res")).unwrap()
}

/// Each library HUD as (folder, name), sorted by folder.
fn shelf(env: &LibraryEnv) -> Vec<(String, String)> {
    let mut huds: Vec<_> = env
        .lib
        .list_huds()
        .unwrap()
        .iter()
        .map(|h| (h.folder_name().to_owned(), h.name().to_owned()))
        .collect();
    huds.sort();
    huds
}

#[test]
fn a_manual_import_with_a_taken_name_keeps_both() {
    let env = library_env();
    let first = import(&env, "a.zip", "rayshud", b"old", Provenance::Local);
    assert_eq!(first.wanted_name(), None);
    let second = import(&env, "b.zip", "rayshud", b"new", Provenance::Local);

    assert_eq!(second.name(), "rayshud (2)");
    assert_eq!(second.folder_name(), "rayshud-2");
    assert_eq!(second.wanted_name(), Some("rayshud"));
    assert_ne!(second.id(), first.id());
    assert_eq!(
        shelf(&env),
        [
            ("rayshud".into(), "rayshud".into()),
            ("rayshud-2".into(), "rayshud (2)".into())
        ]
    );
    assert_eq!(layout(&env, "rayshud"), b"old");
    assert_eq!(layout(&env, "rayshud-2"), b"new");
}

#[test]
fn a_third_import_takes_the_next_number() {
    let env = library_env();
    import(&env, "a.zip", "rayshud", b"1", Provenance::Local);
    import(&env, "b.zip", "rayshud", b"2", Provenance::Local);
    let third = import(&env, "c.zip", "rayshud", b"3", Provenance::Local);
    assert_eq!(third.name(), "rayshud (3)");
    assert_eq!(third.folder_name(), "rayshud-3");
    assert_eq!(shelf(&env).len(), 3);
    assert_eq!(layout(&env, "rayshud"), b"1");
}

#[test]
fn a_stray_library_folder_is_never_overwritten() {
    let env = library_env();
    let stray = env.dir.library_dir(LibraryKind::Huds).join("rayshud");
    write_file(&stray.join("mine.txt"), b"mine");
    let hud = import(&env, "a.zip", "rayshud", b"new", Provenance::Local);
    assert_eq!(hud.folder_name(), "rayshud-2");
    assert!(stray.join("mine.txt").is_file());
}

#[test]
fn a_different_store_item_with_the_same_name_keeps_both() {
    let env = library_env();
    store_gb(&env, 7, "FlawHUD");
    store_gb(&env, 8, "FlawHUD");
    import(&env, "7.zip", "flawhud", b"seven", gb(7));
    let other = import(&env, "8.zip", "flawhud", b"eight", gb(8));
    assert_eq!(other.name(), "FlawHUD (2)");
    assert_eq!(other.folder_name(), "flawhud-2");
    assert_eq!(other.wanted_name(), Some("FlawHUD"));
    assert_eq!(layout(&env, "flawhud"), b"seven");

    // A curated HUD shipping the same folder is a different item too.
    let curated = import(&env, "c.zip", "flawhud", b"curated", curated("flawhud"));
    assert_eq!(curated.folder_name(), "flawhud-3");
    assert_eq!(shelf(&env).len(), 3);
}

#[test]
fn a_store_item_whose_display_name_is_taken_gets_a_number() {
    let env = library_env();
    store_gb(&env, 7, "FlawHUD");
    store_gb(&env, 8, "FlawHUD");
    import(&env, "7.zip", "flawhud", b"seven", gb(7));
    let other = import(&env, "8.zip", "flaw-fork", b"eight", gb(8));
    assert_eq!(other.name(), "FlawHUD (2)");
    assert_eq!(other.wanted_name(), Some("FlawHUD"));
}

#[test]
fn the_same_store_item_again_updates_in_place() {
    let env = library_env();
    store_gb(&env, 7, "FlawHUD");
    import(&env, "7.zip", "flawhud", b"v1", gb(7));
    let again = import(&env, "7b.zip", "flawhud", b"v2", gb(7));
    assert_eq!(again.name(), "FlawHUD");
    assert_eq!(again.wanted_name(), None);
    assert_eq!(shelf(&env), [("flawhud".to_owned(), "FlawHUD".to_owned())]);
    assert_eq!(layout(&env, "flawhud"), b"v2");

    import(&env, "b1.zip", "budhud", b"v1", curated("budhud"));
    let again = import(&env, "b2.zip", "budhud", b"v2", curated("budhud"));
    assert_eq!(again.folder_name(), "budhud");
    assert_eq!(again.wanted_name(), None);
    assert_eq!(shelf(&env).len(), 2);
    assert_eq!(layout(&env, "budhud"), b"v2");
}

#[test]
fn a_hud_kept_from_tf_custom_survives_an_import_and_a_store_install() {
    let env = library_env();
    let custom = env.tf2.custom_dir().join("rayshud");
    write_file(&custom.join("info.vdf"), b"x");
    write_file(&custom.join("resource/ui/hudlayout.res"), b"tweaked");
    let found = env.lib.external_huds(&env.tf2).unwrap();
    let kept = env.lib.keep_external_huds(&found, &env.tf2).unwrap();
    assert_eq!(kept[0].folder_name(), "rayshud");

    let manual = import(&env, "a.zip", "rayshud", b"manual", Provenance::Local);
    assert_eq!(manual.name(), "rayshud (2)");
    store_gb(&env, 7, "rayshud");
    let store = import(&env, "7.zip", "rayshud", b"store", gb(7));
    assert_eq!(store.name(), "rayshud (3)");

    assert_eq!(layout(&env, "rayshud"), b"tweaked");
    let listed = env.lib.list_huds().unwrap();
    assert!(listed.iter().any(|h| h.id() == kept[0].id()));
    assert_eq!(listed.len(), 3);
}
