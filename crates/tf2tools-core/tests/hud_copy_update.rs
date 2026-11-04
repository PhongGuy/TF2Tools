//! Updating a copy: replay over the new base, report what no longer fits, refuse and fail safely.

mod common;

use common::hud_copy::{game_for, import_stock_hud, snapshot};
use common::hud_stock::{HEALTH, stock_files};
use common::{library_env, sql, tmp_is_empty, tree, write_file};
use tf2tools_core::{
    Change, CopyUpdate, HudId, LibraryKind, MakeTarget, Platform, Provenance, SetKey,
    UpdateCopyError,
};

fn set(group: u32, block: &str, key: &str, old: &str, new: &str) -> Change {
    Change::set(
        group,
        SetKey::new(
            "resource/ui/hudplayerhealth.res",
            &["Resource/UI/HudPlayerHealth.res", block],
            key,
            None,
            Some(old),
            new,
        ),
    )
}

#[test]
fn an_update_replays_reports_and_keeps_the_copy_s_metadata() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let changes = [
        set(
            1,
            "PlayerStatusHealthValue",
            "fgcolor",
            "TanDark",
            "255 80 80 255",
        ),
        set(2, "PlayerStatusHealthImage", "xpos", "73", "80"),
    ];
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    env.lib
        .save_made_hud(
            &MakeTarget::Existing(copy.id().clone()),
            copy.name(),
            &files,
            Platform::Linux,
            &changes,
        )
        .unwrap();
    let huds = env.dir.library_dir(LibraryKind::Huds);
    let d = huds.join(copy.folder_name());
    write_file(&d.join(".tf2tools/borrowed/materials/x.vmt"), b"kept");
    let (image, value) = (
        HEALTH.find("  PlayerStatusHealthImage").unwrap(),
        HEALTH.find("  PlayerStatusHealthValue").unwrap(),
    );
    let v2 = format!("{}{}", &HEALTH[..image], &HEALTH[value..]);
    let newer = import_stock_hud(
        &env,
        "somehud",
        &[("resource/ui/hudplayerhealth.res", &v2), ("NEW.txt", "new")],
        Provenance::Local,
    );
    let newer_before = snapshot(&huds.join(newer.folder_name()));
    let done = env
        .lib
        .update_copy(copy.id(), newer.id(), &tf, Platform::Linux)
        .unwrap();
    let left: Vec<(&str, Option<&str>)> = done
        .left_out()
        .iter()
        .map(|n| (n.part(), n.key()))
        .collect();
    assert_eq!(left, [("PlayerStatusHealthImage", Some("xpos"))]);
    assert!(done.both_changed().is_empty());
    assert_eq!(snapshot(&huds.join(newer.folder_name())), newer_before);
    assert_eq!(std::fs::read_to_string(d.join("NEW.txt")).unwrap(), "new");
    assert_eq!(
        std::fs::read(d.join(".tf2tools/borrowed/materials/x.vmt")).unwrap(),
        b"kept"
    );
    let health = std::fs::read_to_string(d.join("resource/ui/hudplayerhealth.res")).unwrap();
    assert!(health.contains("255 80 80 255") && !health.contains("PlayerStatusHealthImage"));
    assert_eq!(
        std::fs::read_to_string(d.join(".tf2tools/original/resource/ui/hudplayerhealth.res"))
            .unwrap(),
        v2
    );
    assert_eq!(
        env.lib.copy_base(copy.id()).unwrap().unwrap().hud_id(),
        newer.id().as_str()
    );
    assert_eq!(
        env.lib.copy_update(copy.id()).unwrap(),
        CopyUpdate::UpToDate
    );
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let made = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap();
    assert_eq!((made.changes().len(), made.dropped()), (1, 0));
}

#[test]
fn an_update_from_another_source_or_a_made_hud_is_refused() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let other = import_stock_hud(&env, "otherhud", &[("NEW.txt", "new")], Provenance::Local);
    let made = env
        .lib
        .save_made_hud(
            &MakeTarget::New,
            "Made",
            &stock_files(),
            Platform::Linux,
            &[],
        )
        .unwrap();
    let d = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name());
    let before = tree(&d);
    assert!(matches!(
        env.lib
            .update_copy(copy.id(), other.id(), &tf, Platform::Linux),
        Err(UpdateCopyError::NotSameSource { .. })
    ));
    assert!(matches!(
        env.lib
            .update_copy(copy.id(), made.entry().id(), &tf, Platform::Linux),
        Err(UpdateCopyError::MadeHud { .. })
    ));
    assert!(matches!(
        env.lib
            .update_copy(copy.id(), &HudId::new("gone"), &tf, Platform::Linux),
        Err(UpdateCopyError::SourceGone { .. })
    ));
    assert!(matches!(
        env.lib
            .update_copy(base.id(), base.id(), &tf, Platform::Linux),
        Err(UpdateCopyError::NotACopy { .. })
    ));
    assert_eq!(tree(&d), before);
}

#[test]
fn an_update_without_room_changes_nothing() {
    let mut env = library_env();
    let tf = game_for(&env, &[]);
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let newer = import_stock_hud(&env, "somehud", &[("NEW.txt", "new")], Provenance::Local);
    env.lib.set_free_space_probe(|_| Ok(0));
    let d = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name());
    let before = tree(&d);
    let err = env
        .lib
        .update_copy(copy.id(), newer.id(), &tf, Platform::Linux)
        .unwrap_err();
    assert!(matches!(err, UpdateCopyError::NoSpace { free: 0, .. }));
    assert_eq!(tree(&d), before);
}

#[test]
fn a_failed_swap_rolls_back_to_the_copy_as_it_was() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let newer = import_stock_hud(&env, "somehud", &[("NEW.txt", "new")], Provenance::Local);
    let huds = env.dir.library_dir(LibraryKind::Huds);
    let d = huds.join(copy.folder_name());
    let before = snapshot(&d);
    // The row update after the folder swap fails, so the swap must be undone.
    sql(&env.dir)
        .execute_batch("CREATE TRIGGER no_update BEFORE UPDATE ON library_huds BEGIN SELECT RAISE(ABORT, 'no'); END;")
        .unwrap();
    let err = env
        .lib
        .update_copy(copy.id(), newer.id(), &tf, Platform::Linux);
    sql(&env.dir)
        .execute_batch("DROP TRIGGER no_update;")
        .unwrap();
    assert!(err.is_err());
    assert_eq!(snapshot(&d), before);
    let mut old = d.as_os_str().to_owned();
    old.push(".old");
    assert!(!std::path::Path::new(&old).exists() && tmp_is_empty(&env.dir));
}

#[test]
fn an_update_finishes_a_swap_a_crash_interrupted() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let newer = import_stock_hud(&env, "somehud", &[("NEW.txt", "new")], Provenance::Local);
    let d = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name());
    let mut old = d.as_os_str().to_owned();
    old.push(".old");
    std::fs::rename(&d, &old).unwrap();
    env.lib
        .update_copy(copy.id(), newer.id(), &tf, Platform::Linux)
        .unwrap();
    assert_eq!(std::fs::read_to_string(d.join("NEW.txt")).unwrap(), "new");
    assert!(!std::path::Path::new(&old).exists());
}
