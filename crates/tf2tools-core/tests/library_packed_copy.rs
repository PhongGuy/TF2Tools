//! Copies made and updated from a packed base, and the Maker's refusal of packed HUDs.

use crate::common;

use common::hud_copy::{game_for, import_stock_hud, snapshot, store_item};
use common::library_env;
use common::packed::{convert, hud_dir, loose_files, packed_files, zip_path};
use tf2tools_core::{
    CopyHudError, HudFiles, HudLoadError, MakeHudError, MakeTarget, Platform, Provenance,
};

#[test]
fn a_copy_of_a_packed_hud_holds_its_files_loose() {
    let env = library_env();
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    env.lib.unpack_hud_for_tests(base.id()).unwrap();
    let hash = env.lib.hud_tree_hash(base.id()).unwrap();
    convert(&env);
    assert!(zip_path(&env, &base).is_file());
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let copied: Vec<(String, Vec<u8>)> = snapshot(&hud_dir(&env, &copy))
        .into_iter()
        .filter(|(rel, _)| !rel.starts_with(".tf2tools/"))
        .collect();
    assert_eq!(copied, packed_files(&env, &base));
    assert!(!zip_path(&env, &copy).exists());
    let recorded = env.lib.copy_base(copy.id()).unwrap().unwrap();
    assert_eq!(recorded.tree(), hash.hash());
}

#[test]
fn a_copy_of_a_packed_hud_checks_space_for_its_unpacked_size() {
    let mut env = library_env();
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    env.lib.unpack_hud_for_tests(base.id()).unwrap();
    env.lib.set_free_space_probe(|_| Ok(1));
    let needed = |env: &common::LibraryEnv| match env.lib.copy_hud(base.id()).unwrap_err() {
        CopyHudError::NoSpace { needed, .. } => needed,
        other => panic!("expected NoSpace, got {other:?}"),
    };
    let loose = needed(&env);
    convert(&env);
    assert!(zip_path(&env, &base).is_file());
    assert_eq!(
        needed(&env),
        loose,
        "a packed base needs the room its files take unpacked"
    );
}

#[test]
fn a_copy_updates_from_a_packed_newer_base() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let base = import_stock_hud(&env, "somehud", &[], store_item());
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let newer = import_stock_hud(&env, "somehud", &[("NEW.txt", "new")], store_item());
    env.lib.unpack_hud_for_tests(newer.id()).unwrap();
    convert(&env);
    assert!(zip_path(&env, &newer).is_file());
    env.lib
        .update_copy(copy.id(), newer.id(), &tf, Platform::Linux)
        .unwrap();
    assert_eq!(
        std::fs::read(hud_dir(&env, &copy).join("NEW.txt")).unwrap(),
        b"new"
    );
    assert!(!zip_path(&env, &copy).exists());
}

#[test]
fn the_maker_refuses_a_packed_hud() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let hud = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    env.lib.unpack_hud_for_tests(hud.id()).unwrap();
    convert(&env);
    let opened = env.lib.made_hud_files(hud.id(), &tf);
    assert!(
        matches!(opened, Err(HudLoadError::NotMade { .. })),
        "{opened:?}"
    );
    let files = HudFiles::open_game(&tf).unwrap();
    for target in [
        MakeTarget::Existing(hud.id().clone()),
        MakeTarget::CopyOf(hud.id().clone()),
    ] {
        let err = env
            .lib
            .save_made_hud(&target, "Mine", &files, Platform::Linux, &[])
            .unwrap_err();
        assert!(matches!(err, MakeHudError::NotMade { .. }), "{err:?}");
    }
    assert!(zip_path(&env, &hud).is_file());
    assert_eq!(loose_files(&env, &hud), Vec::<String>::new());
}
