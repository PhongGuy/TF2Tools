//! An unedited store HUD learns whether the store has a newer version of it.

use crate::common;

use common::hud_copy::{import_stock_hud, store_item};
use common::{LibraryEnv, library_env, sql};
use tf2tools_core::{DownloadUpdate, Provenance};

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
fn a_download_behind_the_store_commit_is_newer() {
    let env = library_env();
    let hud = import_stock_hud(&env, &format!("rayshud-{A}"), &[], curated());
    store(&env, B);
    assert_eq!(
        env.lib.download_update(hud.id()).unwrap(),
        DownloadUpdate::Newer
    );
}

#[test]
fn a_download_at_the_store_commit_is_current() {
    let env = library_env();
    let hud = import_stock_hud(&env, &format!("rayshud-{A}"), &[], curated());
    // The store may hold a full commit where the folder carries a short one.
    store(&env, A);
    assert_eq!(
        env.lib.download_update(hud.id()).unwrap(),
        DownloadUpdate::Current
    );
    let short = import_stock_hud(&env, "rayshud-a22ede0", &[], curated());
    assert_eq!(
        env.lib.download_update(short.id()).unwrap(),
        DownloadUpdate::Current
    );
}

#[test]
fn a_version_that_cannot_be_compared_is_unknown() {
    let env = library_env();
    // No store row yet, so the store's commit is unknown.
    let hud = import_stock_hud(&env, &format!("rayshud-{A}"), &[], curated());
    assert_eq!(
        env.lib.download_update(hud.id()).unwrap(),
        DownloadUpdate::Unknown
    );
    store(&env, B);
    // A folder without a commit in its name gives no version to compare.
    let plain = import_stock_hud(&env, "rayshud", &[], curated());
    let gamebanana = import_stock_hud(&env, &format!("gbhud-{A}"), &[], store_item());
    let local = import_stock_hud(&env, &format!("myhud-{A}"), &[], Provenance::Local);
    for id in [plain.id(), gamebanana.id(), local.id()] {
        assert_eq!(
            env.lib.download_update(id).unwrap(),
            DownloadUpdate::Unknown
        );
    }
}

#[test]
fn an_edited_copy_is_left_to_its_own_update_check() {
    let env = library_env();
    let hud = import_stock_hud(&env, &format!("rayshud-{A}"), &[], curated());
    let copy = env.lib.copy_hud(hud.id()).unwrap();
    store(&env, B);
    assert_eq!(
        env.lib.download_update(copy.id()).unwrap(),
        DownloadUpdate::Unknown
    );
}
