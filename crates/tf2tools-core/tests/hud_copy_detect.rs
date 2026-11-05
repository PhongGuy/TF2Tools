//! A copy learns of a newer base only from its own source; tree hashes are cached.

mod common;

use common::hud_copy::{import_stock_hud, store_item};
use common::hud_stock::HEALTH;
use common::{LibraryEnv, library_env, sql};
use tf2tools_core::{CopyUpdate, LibraryKind, Provenance};

const A: &str = "a22ede014e4f3b1260aa8fd657ff0757d9d006b8";
const B: &str = "b33fef125f5f4c2371ba9fe768ff1868e0e7e119";

fn curated() -> Provenance {
    Provenance::Curated {
        hud_id: "rayshud".into(),
    }
}

fn store(env: &LibraryEnv, hash: &str) {
    sql(&env.dir).execute(
        "INSERT OR REPLACE INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
         VALUES ('rayshud', 'rayshud', 'raysfire', 'raysfire/rayshud', ?1, '{}', '2026-10-28T00:00:00Z')",
        [hash],
    ).unwrap();
}

#[test]
fn a_reinstall_with_new_bytes_is_offered_and_same_bytes_are_not() {
    let env = library_env();
    let base = import_stock_hud(&env, "somehud", &[], store_item());
    let copy = env.lib.copy_hud(base.id()).unwrap();
    assert_eq!(
        env.lib.copy_update(copy.id()).unwrap(),
        CopyUpdate::UpToDate
    );
    import_stock_hud(&env, "somehud", &[], store_item());
    assert_eq!(
        env.lib.copy_update(copy.id()).unwrap(),
        CopyUpdate::UpToDate
    );
    let health = HEALTH.replace("TanDark", "TanLight");
    let newer = import_stock_hud(
        &env,
        "somehud",
        &[("resource/ui/hudplayerhealth.res", &health)],
        store_item(),
    );
    assert_eq!(
        env.lib.copy_update(copy.id()).unwrap(),
        CopyUpdate::InLibrary {
            from: newer.id().clone(),
            name: newer.name().to_owned()
        }
    );
}

#[test]
fn the_store_s_current_commit_is_offered_once_downloaded() {
    let env = library_env();
    store(&env, A);
    let base = import_stock_hud(&env, &format!("rayshud-{A}"), &[], curated());
    let copy = env.lib.copy_hud(base.id()).unwrap();
    store(&env, B);
    assert_eq!(env.lib.copy_update(copy.id()).unwrap(), CopyUpdate::InStore);
    let newer = import_stock_hud(
        &env,
        &format!("rayshud-{B}"),
        &[("NEW.txt", "new")],
        curated(),
    );
    assert!(
        matches!(env.lib.copy_update(copy.id()).unwrap(), CopyUpdate::InLibrary { from, .. } if &from == newer.id())
    );
}

#[test]
fn an_older_download_is_never_offered() {
    let env = library_env();
    store(&env, B);
    let base = import_stock_hud(&env, &format!("rayshud-{B}"), &[], curated());
    let copy = env.lib.copy_hud(base.id()).unwrap();
    import_stock_hud(
        &env,
        &format!("rayshud-{A}"),
        &[("OLD.txt", "old")],
        curated(),
    );
    assert_eq!(
        env.lib.copy_update(copy.id()).unwrap(),
        CopyUpdate::UpToDate
    );
}

#[test]
fn an_unrelated_hud_is_never_offered() {
    let env = library_env();
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    import_stock_hud(&env, "otherhud", &[("NEW.txt", "new")], Provenance::Local);
    assert_eq!(
        env.lib.copy_update(copy.id()).unwrap(),
        CopyUpdate::UpToDate
    );
}

#[test]
fn tree_hashes_are_cached_by_size_and_time() {
    let env = library_env();
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let first = env.lib.hud_tree_hash(base.id()).unwrap();
    assert!(first.files_read() > 10);
    let again = env.lib.hud_tree_hash(base.id()).unwrap();
    assert_eq!((again.hash(), again.files_read()), (first.hash(), 0));
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(base.folder_name());
    std::fs::write(dir.join("README.txt"), "a new file").unwrap();
    let changed = env.lib.hud_tree_hash(base.id()).unwrap();
    assert_eq!(changed.files_read(), 1);
    assert_ne!(changed.hash(), first.hash());
}

#[test]
fn a_damaged_cache_is_rebuilt_and_deleted_folders_are_pruned() {
    let env = library_env();
    let cache = env.dir.root().join("hud_tree_cache.json");
    let keep = import_stock_hud(&env, "keephud", &[], Provenance::Local);
    let gone = import_stock_hud(&env, "gonehud", &[], Provenance::Local);
    env.lib.hud_tree_hash(gone.id()).unwrap();
    std::fs::write(&cache, b"{ not json").unwrap();
    let rebuilt = env.lib.hud_tree_hash(keep.id()).unwrap();
    assert!(rebuilt.files_read() > 10);
    env.lib.delete_hud(gone.id()).unwrap();
    env.lib.hud_tree_hash(keep.id()).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&cache).unwrap()).unwrap();
    let folders: Vec<&String> = json.as_object().unwrap().keys().collect();
    assert_eq!(folders, [keep.folder_name()]);
}

/// One info.vdf for both imports, so the same HUD hashes alike whatever its folder.
const INFO: (&str, &str) = ("info.vdf", "\"rayshud\" { ui_version 3 }");

#[test]
fn a_copy_without_a_known_commit_learns_it_from_a_download_with_its_bytes() {
    let env = library_env();
    store(&env, A);
    let base = import_stock_hud(&env, "rayshud", &[INFO], curated());
    let copy = env.lib.copy_hud(base.id()).unwrap();
    assert_eq!(env.lib.copy_base(copy.id()).unwrap().unwrap().hash(), "");
    assert_eq!(env.lib.copy_update(copy.id()).unwrap(), CopyUpdate::InStore);
    import_stock_hud(&env, &format!("rayshud-{A}"), &[INFO], curated());
    assert_eq!(
        env.lib.copy_update(copy.id()).unwrap(),
        CopyUpdate::UpToDate
    );
    assert_eq!(env.lib.copy_base(copy.id()).unwrap().unwrap().hash(), A);
    store(&env, B);
    assert_eq!(env.lib.copy_update(copy.id()).unwrap(), CopyUpdate::InStore);
}
