//! Making an editable copy of a library HUD.

use crate::common;

use common::hud_copy::{game_for, import_stock_hud, snapshot};
use common::hud_stock::{HEALTH, stock_files};
use common::{library_env, tree};
// Only the Unix permission and link tests use these.
#[cfg(unix)]
use common::{tmp_is_empty, write_file};
use tf2tools_core::{
    CopyHudError, HudId, HudModel, LibraryKind, MakeTarget, PartKey, Platform, Provenance,
};

const RAYS: &str = "rayshud-a22ede014e4f3b1260aa8fd657ff0757d9d006b8";

#[test]
fn a_copy_holds_the_base_files_and_a_copy_record() {
    let env = library_env();
    let base = import_stock_hud(
        &env,
        RAYS,
        &[("LICENSE", "MIT")],
        Provenance::Curated {
            hud_id: "rayshud".into(),
        },
    );
    let huds = env.dir.library_dir(LibraryKind::Huds);
    let before = snapshot(&huds.join(base.folder_name()));
    let copy = env.lib.copy_hud(base.id()).unwrap();
    assert_eq!(copy.name(), format!("{} (copy)", base.name()));
    assert!(
        matches!(copy.provenance(), Provenance::Local) && env.lib.is_made_hud(copy.id()).unwrap()
    );
    let copied: Vec<_> = snapshot(&huds.join(copy.folder_name()))
        .into_iter()
        .filter(|(p, _)| !p.starts_with(".tf2tools"))
        .collect();
    assert_eq!(copied, before);
    assert_eq!(snapshot(&huds.join(base.folder_name())), before);
    let r = env.lib.copy_base(copy.id()).unwrap().expect("a copy");
    assert_eq!(
        (r.hud_id(), r.store_id(), r.origin()),
        (base.id().as_str(), "hud-db:rayshud", "hud-db rayshud")
    );
    assert_eq!(
        (r.hash(), r.base_folder(), r.install_folder()),
        (
            "a22ede014e4f3b1260aa8fd657ff0757d9d006b8",
            RAYS,
            Some("rayshud")
        )
    );
    let second = env.lib.copy_hud(base.id()).unwrap();
    assert_eq!(second.name(), format!("{} (copy 2)", base.name()));
}

#[cfg(unix)]
#[test]
fn a_copy_shares_no_file_with_its_base() {
    use std::os::unix::fs::MetadataExt;
    let env = library_env();
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let huds = env.dir.library_dir(LibraryKind::Huds);
    let ino = |folder: &str| {
        std::fs::metadata(huds.join(folder).join("resource/ui/hudplayerhealth.res"))
            .unwrap()
            .ino()
    };
    assert_ne!(ino(base.folder_name()), ino(copy.folder_name()));
}

#[test]
fn made_unknown_and_missing_huds_are_refused() {
    let env = library_env();
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
    assert!(matches!(
        env.lib.copy_hud(made.entry().id()),
        Err(CopyHudError::MadeHud { .. })
    ));
    assert!(matches!(
        env.lib.copy_hud(&HudId::new("nope")),
        Err(CopyHudError::NotFound { .. })
    ));
    let gone = import_stock_hud(&env, "gonehud", &[], Provenance::Local);
    std::fs::remove_dir_all(
        env.dir
            .library_dir(LibraryKind::Huds)
            .join(gone.folder_name()),
    )
    .unwrap();
    assert!(matches!(
        env.lib.copy_hud(gone.id()),
        Err(CopyHudError::BaseMissing { .. })
    ));
}

#[cfg(unix)]
#[test]
fn a_linked_base_folder_or_file_is_never_copied_through() {
    let env = library_env();
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let huds = env.dir.library_dir(LibraryKind::Huds);
    let secret = write_file(&env.tmp.path().join("secret.txt"), b"no");
    std::os::unix::fs::symlink(&secret, huds.join(base.folder_name()).join("link.txt")).unwrap();
    let copy = env.lib.copy_hud(base.id()).unwrap();
    assert!(!huds.join(copy.folder_name()).join("link.txt").exists());
    let real = huds.join(base.folder_name());
    let moved = env.tmp.path().join("moved");
    std::fs::rename(&real, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &real).unwrap();
    assert!(matches!(
        env.lib.copy_hud(base.id()),
        Err(CopyHudError::BaseMissing { .. })
    ));
}

#[test]
fn a_copy_without_room_is_refused_before_anything_is_written() {
    let mut env = library_env();
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    env.lib.set_free_space_probe(|_| Ok(1024));
    let before = tree(&env.dir.library_dir(LibraryKind::Huds));
    let err = env.lib.copy_hud(base.id()).unwrap_err();
    assert!(
        matches!(err, CopyHudError::NoSpace { free: 1024, needed } if needed > 64 * 1024 * 1024)
    );
    assert_eq!(tree(&env.dir.library_dir(LibraryKind::Huds)), before);
    assert_eq!(env.lib.list_huds().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn a_failure_part_way_leaves_nothing_behind() {
    use std::os::unix::fs::PermissionsExt;
    let env = library_env();
    let base = import_stock_hud(&env, "somehud", &[("zz_last.res", "x")], Provenance::Local);
    let locked = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(base.folder_name())
        .join("zz_last.res");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let before = tree(&env.dir.library_dir(LibraryKind::Huds));
    let err = env.lib.copy_hud(base.id());
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(err, Err(CopyHudError::Io { .. })));
    assert_eq!(tree(&env.dir.library_dir(LibraryKind::Huds)), before);
    assert!(tmp_is_empty(&env.dir) && env.lib.list_huds().unwrap().len() == 1);
}

#[test]
fn made_hud_files_reads_a_copy_as_its_base() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let health = HEALTH.replace("TanDark", "TanLight");
    let base = import_stock_hud(
        &env,
        "somehud",
        &[("resource/ui/hudplayerhealth.res", &health)],
        Provenance::Local,
    );
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    assert_eq!(files.install_folder(), Some("somehud"));
    let part = HudModel::load(&files, Platform::Linux).unwrap();
    let hv = part
        .part(&PartKey::new(
            "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue",
        ))
        .unwrap();
    assert_eq!(hv.value("fgcolor").unwrap().text(), "TanLight");
}
