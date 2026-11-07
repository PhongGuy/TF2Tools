//! Picture copies leave on save unless the HUD or one of its snapshots still names them.

use crate::common;

use common::hud_pictures::{CODE_VMT, folder, kept, pick, save, tex};
use common::library_env;
use tf2tools_core::{HistoryError, HudAssets, MakeTarget};

#[test]
fn an_unreferenced_copy_is_removed_on_save() {
    let env = library_env();
    let mut assets = HudAssets::new();
    let first = pick(1, 1, CODE_VMT, &mut assets);
    let saved = save(&env, &MakeTarget::New, &first, &assets);
    let target = MakeTarget::Existing(saved.entry().id().clone());
    let second = pick(2, 2, CODE_VMT, &mut assets);
    let again = save(&env, &target, &second, &assets);
    let dir = folder(&env, &again);
    assert!(!dir.join(kept(1, CODE_VMT)).exists());
    assert!(
        !dir.join(kept(1, &tex(1))).parent().unwrap().exists(),
        "its folder went too"
    );
    assert!(dir.join(kept(2, CODE_VMT)).exists());
}

#[test]
fn a_snapshot_referenced_copy_is_kept() {
    let env = library_env();
    let mut assets = HudAssets::new();
    let first = pick(1, 1, CODE_VMT, &mut assets);
    let saved = save(&env, &MakeTarget::New, &first, &assets);
    let id = saved.entry().id().clone();
    env.lib.save_hud_snapshot(&id, "Before", &first).unwrap();
    let again = save(&env, &MakeTarget::Existing(id), &[], &assets);
    let dir = folder(&env, &again);
    assert!(dir.join(kept(1, CODE_VMT)).exists());
    assert!(dir.join(kept(1, &tex(1))).exists());
}

#[test]
fn a_snapshot_s_picture_survives_a_later_pick() {
    let env = library_env();
    let mut assets = HudAssets::new();
    let first = pick(1, 1, CODE_VMT, &mut assets);
    let saved = save(&env, &MakeTarget::New, &first, &assets);
    let id = saved.entry().id().clone();
    let entry = env.lib.save_hud_snapshot(&id, "Before", &first).unwrap();
    let mut later = first.clone();
    later.extend(pick(2, 2, CODE_VMT, &mut assets));
    save(&env, &MakeTarget::Existing(id.clone()), &later, &assets);
    let snapshot = env.lib.load_hud_snapshot(&id, entry.file()).unwrap();
    assert_eq!(
        snapshot.assets().get(&kept(1, CODE_VMT)),
        Some(b"vmt 1".as_slice())
    );
}

#[test]
fn a_snapshot_of_an_unsaved_pick_keeps_its_picture_after_undo_and_save() {
    let env = library_env();
    let mut assets = HudAssets::new();
    let saved = save(&env, &MakeTarget::New, &[], &assets);
    let id = saved.entry().id().clone();
    // Pick (bytes only in memory), snapshot, undo (no changes), Save, then restore.
    let picked = pick(1, 1, CODE_VMT, &mut assets);
    let entry = env
        .lib
        .save_hud_snapshot_with_assets(&id, "Picked", &picked, &assets)
        .unwrap();
    save(&env, &MakeTarget::Existing(id.clone()), &[], &assets);
    let snapshot = env.lib.load_hud_snapshot(&id, entry.file()).unwrap();
    assert_eq!(snapshot.skipped(), 0);
    assert_eq!(
        snapshot.assets().get(&kept(1, CODE_VMT)),
        Some(b"vmt 1".as_slice())
    );
    assert_eq!(
        snapshot.assets().get(&kept(1, &tex(1))),
        Some(vec![1u8; 100].as_slice())
    );
}

#[test]
fn a_snapshot_whose_picture_cannot_be_stored_is_not_saved() {
    let env = library_env();
    let mut assets = HudAssets::new();
    let saved = save(&env, &MakeTarget::New, &[], &assets);
    let id = saved.entry().id().clone();
    // A file where the images folder should be makes every picture write fail.
    std::fs::write(folder(&env, &saved).join(".tf2tools/images"), b"in the way").unwrap();
    let picked = pick(1, 1, CODE_VMT, &mut assets);
    let refused = env
        .lib
        .save_hud_snapshot_with_assets(&id, "Picked", &picked, &assets);
    assert!(matches!(refused, Err(HistoryError::Io { .. })));
    assert!(env.lib.list_hud_snapshots(&id).unwrap().is_empty());
}

#[test]
fn a_snapshot_that_does_not_parse_keeps_every_copy() {
    let env = library_env();
    let mut assets = HudAssets::new();
    let first = pick(1, 1, CODE_VMT, &mut assets);
    let saved = save(&env, &MakeTarget::New, &first, &assets);
    let history = folder(&env, &saved).join(".tf2tools/history");
    std::fs::create_dir_all(&history).unwrap();
    std::fs::write(history.join("broken.json"), b"{ not json").unwrap();
    let again = save(
        &env,
        &MakeTarget::Existing(saved.entry().id().clone()),
        &[],
        &assets,
    );
    assert!(folder(&env, &again).join(kept(1, CODE_VMT)).exists());
}
