//! Reopening a made HUD: replay, changes TF2 broke, hand edits and files TF2Tools cannot open.

use crate::common;

use common::hud_stock::{HEALTH, stock_files, stock_files_with};
use common::{LibraryEnv, import_hud_as, library_env, tree, write_file};
use tf2tools_core::{
    Change, HudId, HudLoadError, LibraryKind, MakeTarget, PartKey, Platform, Provenance, SavedHud,
    Scene, SetKey,
};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn red_health(group: u32) -> Change {
    Change::set(
        group,
        SetKey::new(
            "resource/ui/hudplayerhealth.res",
            &["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
            "fgcolor",
            None,
            Some("TanDark"),
            "255 80 80 255",
        ),
    )
}

fn ammo_layer(group: u32) -> Change {
    Change::set(
        group,
        SetKey::new(
            "resource/ui/hudammoweapons.res",
            &["Resource/UI/HudAmmoWeapons.res", "AmmoInClip"],
            "zpos",
            None,
            None,
            "3",
        ),
    )
}

fn saved(env: &LibraryEnv, changes: &[Change]) -> SavedHud {
    env.lib
        .save_made_hud(
            &MakeTarget::New,
            "Mine",
            &stock_files(),
            Platform::Linux,
            changes,
        )
        .unwrap()
}

fn maker_path(env: &LibraryEnv, s: &SavedHud) -> std::path::PathBuf {
    env.dir
        .library_dir(LibraryKind::Huds)
        .join(s.entry().folder_name())
        .join(".tf2tools/maker.json")
}

#[test]
fn reopening_replays_the_saved_changes() {
    let env = library_env();
    let s = saved(&env, &[red_health(1), ammo_layer(2)]);
    let made = env
        .lib
        .open_made_hud(s.entry().id(), &stock_files(), Platform::Linux)
        .unwrap();
    assert_eq!(made.changes(), [red_health(1), ammo_layer(2)]);
    assert_eq!((made.dropped(), made.tf2_updated()), (0, false));
    let edited = made.pristine().with_changes(made.changes());
    let part = edited.model().part(&PartKey::new(HV)).unwrap();
    assert_eq!(part.value("fgcolor").unwrap().text(), "255 80 80 255");
}

#[test]
fn a_tf2_update_drops_and_counts_changes_that_no_longer_resolve() {
    let env = library_env();
    let s = saved(&env, &[red_health(1), ammo_layer(2)]);
    let renamed = HEALTH.replace("PlayerStatusHealthValue\n", "PlayerStatusHealthNumber\n");
    let updated = stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(renamed.as_str()))]);
    let made = env
        .lib
        .open_made_hud(s.entry().id(), &updated, Platform::Linux)
        .unwrap();
    assert_eq!(
        (made.changes(), made.dropped(), made.tf2_updated()),
        ([ammo_layer(2)].as_slice(), 1, true)
    );
}

#[test]
fn a_hand_edited_change_with_a_bad_path_is_dropped() {
    let env = library_env();
    let s = saved(&env, &[red_health(1)]);
    let path = maker_path(&env, &s);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    json["changes"].as_array_mut().unwrap().push(serde_json::json!({ "group": 9, "set": {
        "file": "cfg/autoexec.cfg", "block": ["R"], "key": "k", "cond": null, "old": null, "new": "v" } }));
    write_file(&path, &serde_json::to_vec(&json).unwrap());
    let made = env
        .lib
        .open_made_hud(s.entry().id(), &stock_files(), Platform::Linux)
        .unwrap();
    assert_eq!((made.changes().len(), made.dropped()), (1, 1));
}

#[test]
fn damaged_or_newer_maker_files_refuse_editing() {
    let env = library_env();
    let s = saved(&env, &[red_health(1)]);
    let path = maker_path(&env, &s);
    write_file(&path, b"{");
    let damaged = env
        .lib
        .open_made_hud(s.entry().id(), &stock_files(), Platform::Linux);
    assert!(
        matches!(damaged, Err(HudLoadError::Damaged { .. })),
        "{damaged:?}"
    );
    write_file(
        &path,
        br#"{ "version": 2, "base": { "kind": "default" }, "sources": {}, "changes": [] }"#,
    );
    let newer = env
        .lib
        .open_made_hud(s.entry().id(), &stock_files(), Platform::Linux);
    assert!(matches!(newer, Err(HudLoadError::Newer)), "{newer:?}");
}

#[test]
fn only_made_huds_open() {
    let env = library_env();
    let imported = import_hud_as(&env, "rayshud", Provenance::Local);
    assert!(!env.lib.is_made_hud(imported.id()).unwrap());
    let not_made = env
        .lib
        .open_made_hud(imported.id(), &stock_files(), Platform::Linux);
    assert!(
        matches!(not_made, Err(HudLoadError::NotMade { .. })),
        "{not_made:?}"
    );
    let s = saved(&env, &[]);
    assert!(env.lib.is_made_hud(s.entry().id()).unwrap());
    env.lib.delete_hud(s.entry().id()).unwrap();
    let deleted = env
        .lib
        .open_made_hud(s.entry().id(), &stock_files(), Platform::Linux);
    assert!(
        matches!(deleted, Err(HudLoadError::NotFound { .. })),
        "{deleted:?}"
    );
    let unknown = env
        .lib
        .open_made_hud(&HudId::new("no-such-id"), &stock_files(), Platform::Linux);
    assert!(matches!(unknown, Err(HudLoadError::NotFound { .. })));
}

/// Every file under `root` with its bytes, to prove a read changed nothing.
fn snapshot(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    tree(root)
        .into_iter()
        .map(|rel| {
            let bytes = std::fs::read(root.join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect()
}

fn assert_reads_leave_the_tree_alone(env: &LibraryEnv, s: &SavedHud) {
    let huds = env.dir.library_dir(LibraryKind::Huds);
    let before = snapshot(&huds);
    assert!(env.lib.is_made_hud(s.entry().id()).unwrap());
    let made = env
        .lib
        .open_made_hud(s.entry().id(), &stock_files(), Platform::Linux)
        .unwrap();
    assert_eq!(made.changes(), [red_health(1)]);
    assert!(
        snapshot(&huds) == before,
        "a read changed the library folder"
    );
}

#[test]
fn reads_see_a_lone_old_folder_without_recovering_it() {
    let env = library_env();
    let s = saved(&env, &[red_health(1)]);
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(s.entry().folder_name());
    let old = dir.with_file_name(format!("{}.old", s.entry().folder_name()));
    std::fs::rename(&dir, &old).unwrap();
    assert_reads_leave_the_tree_alone(&env, &s);
    assert!(!dir.exists() && old.exists());
}

#[test]
fn reads_leave_both_swap_folders_in_place() {
    let env = library_env();
    let s = saved(&env, &[red_health(1)]);
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(s.entry().folder_name());
    let old = dir.with_file_name(format!("{}.old", s.entry().folder_name()));
    for rel in tree(&dir) {
        write_file(&old.join(&rel), &std::fs::read(dir.join(&rel)).unwrap());
    }
    assert_reads_leave_the_tree_alone(&env, &s);
    assert!(dir.exists() && old.exists());
}

#[test]
fn a_source_file_the_game_no_longer_ships_counts_as_a_tf2_update() {
    let env = library_env();
    let s = saved(&env, &[red_health(1), ammo_layer(2)]);
    let gone = stock_files_with(&[("resource/ui/hudplayerhealth.res", None)]);
    let made = env
        .lib
        .open_made_hud(s.entry().id(), &gone, Platform::Linux)
        .unwrap();
    assert_eq!(
        (made.changes(), made.dropped(), made.tf2_updated()),
        ([ammo_layer(2)].as_slice(), 1, true)
    );
}

#[test]
fn an_oversized_maker_file_is_damaged_without_being_read_whole() {
    let env = library_env();
    let s = saved(&env, &[red_health(1)]);
    // A sparse file far past the 8 MiB cap: only the cap plus one byte is ever read.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(maker_path(&env, &s))
        .unwrap();
    file.set_len(256 * 1024 * 1024).unwrap();
    drop(file);
    let result = env
        .lib
        .open_made_hud(s.entry().id(), &stock_files(), Platform::Linux);
    assert!(
        matches!(result, Err(HudLoadError::Damaged { .. })),
        "{result:?}"
    );
}

fn board_layer(group: u32) -> Change {
    Change::set(
        group,
        SetKey::new(
            "resource/ui/scoreboard.res",
            &["Resource/UI/Scoreboard.res", "BlueTeamScore"],
            "zpos",
            None,
            None,
            "4",
        ),
    )
}

#[test]
fn scoreboard_changes_save_and_reopen_beside_hud_changes() {
    let env = library_env();
    let s = saved(&env, &[red_health(1), board_layer(2)]);
    let folder = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(s.entry().folder_name());
    assert!(folder.join("resource/ui/scoreboard.res").is_file());
    let maker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(maker_path(&env, &s)).unwrap()).unwrap();
    assert!(maker["sources"].get("resource/ui/scoreboard.res").is_some());
    let made = env
        .lib
        .open_made_hud(s.entry().id(), &stock_files(), Platform::Linux)
        .unwrap();
    assert_eq!(made.changes(), [red_health(1), board_layer(2)]);
    assert_eq!((made.dropped(), made.tf2_updated()), (0, false));
    let board = made
        .pristine()
        .with_changes(made.changes())
        .into_model()
        .with_scene(Scene::Scoreboard);
    let score = board.part(&PartKey::new("scores/BlueTeamScore")).unwrap();
    assert_eq!(score.value("zpos").unwrap().text(), "4");
}
