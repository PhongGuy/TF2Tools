//! Saving a made HUD: a sparse folder, a local row, unique names, the library boundary and swaps.

mod common;

use common::hud_stock::{HEALTH, stock_files};
use common::{LibraryEnv, import_hud_as, library_env, sql, tmp_is_empty, tree, write_file};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tf2tools_core::{
    Change, HudId, LibraryKind, MakeHudError, MakeTarget, Platform, Provenance, SavedHud, SetKey,
};

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

fn save(
    env: &LibraryEnv,
    target: &MakeTarget,
    name: &str,
    changes: &[Change],
) -> Result<SavedHud, MakeHudError> {
    env.lib
        .save_made_hud(target, name, &stock_files(), Platform::Linux, changes)
}

fn folder(env: &LibraryEnv, saved: &SavedHud) -> PathBuf {
    env.dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name())
}

fn maker_json(env: &LibraryEnv, saved: &SavedHud) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(folder(env, saved).join(".tf2tools/maker.json")).unwrap())
        .unwrap()
}

#[test]
fn a_new_save_writes_a_sparse_folder_and_a_local_row() {
    let env = library_env();
    let saved = save(&env, &MakeTarget::New, "  My HUD ", &[red_health(1)]).unwrap();
    assert_eq!(
        (
            saved.entry().name(),
            saved.entry().provenance(),
            saved.dropped()
        ),
        ("My HUD", &Provenance::Local, 0)
    );
    let dir = folder(&env, &saved);
    assert_eq!(
        tree(&dir),
        [
            ".tf2tools/maker.json",
            "info.vdf",
            "resource/ui/hudplayerhealth.res"
        ]
    );
    let want = HEALTH.replace("fgcolor TanDark", "fgcolor \"255 80 80 255\"");
    assert_eq!(
        std::fs::read_to_string(dir.join("resource/ui/hudplayerhealth.res")).unwrap(),
        want
    );
    let info = std::fs::read_to_string(dir.join("info.vdf")).unwrap();
    assert_eq!(
        info,
        format!(
            "\"{}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n",
            saved.entry().folder_name()
        )
    );
    let json = maker_json(&env, &saved);
    let hash: String = Sha256::digest(HEALTH.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        json["sources"]["resource/ui/hudplayerhealth.res"].as_str(),
        Some(hash.as_str())
    );
    assert_eq!(json["changes"].as_array().unwrap().len(), 1);
    let source: String = sql(&env.dir)
        .query_row(
            "SELECT source FROM library_huds WHERE id = ?1",
            [saved.entry().id().as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(source, "local");
    assert!(tmp_is_empty(&env.dir));
    env.lib.install_hud(saved.entry().id(), &env.tf2).unwrap();
    let installed = env.tf2.custom_dir().join(saved.entry().folder_name());
    assert_eq!(
        tree(&installed),
        ["info.vdf", "resource/ui/hudplayerhealth.res"]
    );
}

#[test]
fn names_are_trimmed_and_unique_ignoring_case() {
    let env = library_env();
    assert!(matches!(
        save(&env, &MakeTarget::New, "   ", &[]),
        Err(MakeHudError::EmptyName)
    ));
    import_hud_as(&env, "rayshud", Provenance::Local);
    assert!(matches!(
        save(&env, &MakeTarget::New, "RaysHUD", &[]),
        Err(MakeHudError::NameTaken { .. })
    ));
    save(&env, &MakeTarget::New, "Mine", &[]).unwrap();
    assert!(matches!(
        save(&env, &MakeTarget::New, "MINE", &[]),
        Err(MakeHudError::NameTaken { .. })
    ));
    let gone = MakeTarget::Existing(HudId::new("no-such-id"));
    assert!(matches!(
        save(&env, &gone, "Other", &[]),
        Err(MakeHudError::NotFound { .. })
    ));
}

#[test]
fn a_hud_not_made_here_is_never_overwritten() {
    let env = library_env();
    let imported = import_hud_as(&env, "rayshud", Provenance::Local);
    let dir = env.dir.library_dir(LibraryKind::Huds).join("rayshud");
    let before = tree(&dir);
    let result = save(
        &env,
        &MakeTarget::Existing(imported.id().clone()),
        "Rays",
        &[red_health(1)],
    );
    assert!(
        matches!(result, Err(MakeHudError::NotMade { .. })),
        "{result:?}"
    );
    assert_eq!(tree(&dir), before);
    assert!(tmp_is_empty(&env.dir));
}

#[test]
fn nothing_is_written_outside_the_library() {
    let env = library_env();
    let escape = Change::set(
        2,
        SetKey::new("../../../../escape.res", &["R"], "k", None, None, "v"),
    );
    let saved = save(&env, &MakeTarget::New, "Mine", &[red_health(1), escape]).unwrap();
    assert_eq!(saved.dropped(), 1);
    assert_eq!(
        maker_json(&env, &saved)["changes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for path in tree(env.tmp.path()) {
        assert!(path.starts_with("com.tf2tools.app/"), "{path}");
    }
    assert!(tree(&env.tf2.custom_dir()).is_empty());
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

fn old_of(dir: &std::path::Path) -> PathBuf {
    dir.with_file_name(format!(
        "{}.old",
        dir.file_name().unwrap().to_string_lossy()
    ))
}

#[test]
fn a_resave_swaps_the_folder_and_carries_tf2tools() {
    let env = library_env();
    let first = save(&env, &MakeTarget::New, "Mine", &[red_health(1)]).unwrap();
    let dir = folder(&env, &first);
    write_file(&dir.join(".tf2tools/history/one.json"), b"{}");
    #[cfg(unix)]
    std::os::unix::fs::symlink(env.tmp.path(), dir.join(".tf2tools/link")).unwrap();
    let second = save(
        &env,
        &MakeTarget::Existing(first.entry().id().clone()),
        "Renamed",
        &[red_health(1), ammo_layer(2)],
    )
    .unwrap();
    assert_eq!(folder(&env, &second), dir);
    assert_eq!(
        tree(&dir),
        [
            ".tf2tools/history/one.json",
            ".tf2tools/maker.json",
            "info.vdf",
            "resource/ui/hudammoweapons.res",
            "resource/ui/hudplayerhealth.res"
        ]
    );
    assert!(
        std::fs::symlink_metadata(dir.join(".tf2tools/link")).is_err(),
        "symlinks are not carried"
    );
    assert_eq!(
        maker_json(&env, &second)["changes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(!old_of(&dir).exists());
    assert_eq!(env.lib.list_huds().unwrap()[0].name(), "Renamed");
    assert!(tmp_is_empty(&env.dir));
}

#[test]
fn a_database_failure_after_the_swap_puts_the_old_folder_back() {
    let env = library_env();
    let first = save(&env, &MakeTarget::New, "Mine", &[red_health(1)]).unwrap();
    let dir = folder(&env, &first);
    let before = std::fs::read(dir.join(".tf2tools/maker.json")).unwrap();
    sql(&env.dir)
        .execute_batch("CREATE TRIGGER refuse BEFORE UPDATE ON library_huds BEGIN SELECT RAISE(ABORT, 'refused'); END;")
        .unwrap();
    let result = save(
        &env,
        &MakeTarget::Existing(first.entry().id().clone()),
        "Renamed",
        &[red_health(1), ammo_layer(2)],
    );
    assert!(
        matches!(result, Err(MakeHudError::Database { .. })),
        "{result:?}"
    );
    assert_eq!(
        tree(&dir),
        [
            ".tf2tools/maker.json",
            "info.vdf",
            "resource/ui/hudplayerhealth.res"
        ]
    );
    assert_eq!(
        std::fs::read(dir.join(".tf2tools/maker.json")).unwrap(),
        before
    );
    assert!(!old_of(&dir).exists());
    assert_eq!(env.lib.list_huds().unwrap()[0].name(), "Mine");
    assert!(tmp_is_empty(&env.dir));
}

/// Makes a folder read-only and restores it on drop, even when the test panics, so the
/// temp dir can still be deleted.
#[cfg(unix)]
struct ReadOnly(PathBuf);

#[cfg(unix)]
impl ReadOnly {
    fn new(dir: PathBuf) -> Self {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        Self(dir)
    }
}

#[cfg(unix)]
impl Drop for ReadOnly {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

#[cfg(unix)]
#[test]
fn a_failed_first_rename_leaves_the_previous_folder() {
    let env = library_env();
    let first = save(&env, &MakeTarget::New, "Mine", &[red_health(1)]).unwrap();
    let huds = env.dir.library_dir(LibraryKind::Huds);
    let before = tree(&folder(&env, &first));
    let read_only = ReadOnly::new(huds.clone());
    if std::fs::write(huds.join("probe"), b"").is_ok() {
        drop(read_only);
        eprintln!("skipped: this process can write a read-only folder (root)");
        return;
    }
    let result = save(
        &env,
        &MakeTarget::Existing(first.entry().id().clone()),
        "Renamed",
        &[ammo_layer(1)],
    );
    drop(read_only);
    assert!(matches!(result, Err(MakeHudError::Io { .. })), "{result:?}");
    assert_eq!(tree(&folder(&env, &first)), before);
    assert_eq!(std::fs::read_dir(&huds).unwrap().count(), 1, "no .old left");
    assert_eq!(env.lib.list_huds().unwrap()[0].name(), "Mine");
    assert!(tmp_is_empty(&env.dir));
}

#[test]
fn a_crash_between_the_renames_is_recovered_from_the_old_folder() {
    let env = library_env();
    let first = save(&env, &MakeTarget::New, "Mine", &[red_health(1)]).unwrap();
    let dir = folder(&env, &first);
    write_file(&dir.join(".tf2tools/history/one.json"), b"{}");
    std::fs::rename(&dir, old_of(&dir)).unwrap();
    save(
        &env,
        &MakeTarget::Existing(first.entry().id().clone()),
        "Mine",
        &[red_health(1)],
    )
    .unwrap();
    assert!(
        dir.join(".tf2tools/history/one.json").exists(),
        "the .old copy was used"
    );
    assert!(!old_of(&dir).exists());
}

#[test]
fn a_crash_after_the_swap_keeps_the_new_folder() {
    let env = library_env();
    let first = save(&env, &MakeTarget::New, "Mine", &[red_health(1)]).unwrap();
    let dir = folder(&env, &first);
    write_file(&dir.join(".tf2tools/history/new.json"), b"{}");
    write_file(&old_of(&dir).join(".tf2tools/history/stale.json"), b"{}");
    save(
        &env,
        &MakeTarget::Existing(first.entry().id().clone()),
        "Mine",
        &[red_health(1)],
    )
    .unwrap();
    assert!(dir.join(".tf2tools/history/new.json").exists());
    assert!(!dir.join(".tf2tools/history/stale.json").exists());
    assert!(!old_of(&dir).exists());
}

#[test]
fn a_failed_new_insert_removes_the_folder() {
    let env = library_env();
    sql(&env.dir)
        .execute_batch("CREATE TRIGGER refuse BEFORE INSERT ON library_huds BEGIN SELECT RAISE(ABORT, 'refused'); END;")
        .unwrap();
    let result = save(&env, &MakeTarget::New, "Mine", &[red_health(1)]);
    assert!(
        matches!(result, Err(MakeHudError::Database { .. })),
        "{result:?}"
    );
    assert!(tree(&env.dir.library_dir(LibraryKind::Huds)).is_empty());
    assert!(tmp_is_empty(&env.dir));
}

#[test]
fn a_very_long_name_still_gets_a_folder_the_disk_accepts() {
    let env = library_env();
    let name = "\u{e9}".repeat(200);
    let saved = save(&env, &MakeTarget::New, &name, &[red_health(1)]).unwrap();
    assert_eq!(saved.entry().name(), name);
    // Room is left for the `.old` suffix a re-save adds.
    assert!(saved.entry().folder_name().len() + ".old".len() <= 255);
    save(
        &env,
        &MakeTarget::Existing(saved.entry().id().clone()),
        &name,
        &[red_health(1)],
    )
    .unwrap();
}
