//! Save as new from a copy, the credits file, and a copy installed over its original.

mod common;

use std::fs::read_to_string;

use common::hud_copy::{game_for, import_stock_hud};
use common::{gb, library_env, tree};
use tf2tools_core::{
    Change, HudStatus, LibraryKind, MakeHudError, MakeTarget, Platform, Provenance, SetKey,
};

const H: &str = "resource/ui/hudplayerhealth.res";
const TERMS: &str = ", changed with TF2Tools. Its author's terms apply; check the HUD's own page before you share it.\n";

fn red() -> Change {
    Change::set(
        1,
        SetKey::new(
            H,
            &["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
            "fgcolor",
            None,
            Some("TanDark"),
            "255 80 80 255",
        ),
    )
}

#[test]
fn save_as_new_from_a_copy_carries_originals_or_refuses_without_room() {
    let mut env = library_env();
    let tf = game_for(&env, &[]);
    let base = import_stock_hud(&env, "somehud", &[("README.txt", "hi")], Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    env.lib
        .save_made_hud(
            &MakeTarget::Existing(copy.id().clone()),
            copy.name(),
            &files,
            Platform::Linux,
            &[red()],
        )
        .unwrap();
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let target = MakeTarget::CopyOf(copy.id().clone());
    let saved = env
        .lib
        .save_made_hud(&target, "Mine 2", &files, Platform::Linux, &[red()])
        .unwrap();
    let d = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name());
    assert_eq!(read_to_string(d.join("README.txt")).unwrap(), "hi");
    assert_eq!(
        read_to_string(d.join(".tf2tools/original").join(H)).unwrap(),
        common::hud_stock::HEALTH
    );
    assert_eq!(
        env.lib
            .copy_base(saved.entry().id())
            .unwrap()
            .unwrap()
            .hud_id(),
        base.id().as_str()
    );
    env.lib.set_free_space_probe(|_| Ok(0));
    let err = env
        .lib
        .save_made_hud(&target, "Mine 3", &files, Platform::Linux, &[])
        .unwrap_err();
    assert!(matches!(err, MakeHudError::NoSpace { free: 0, .. }));
}

#[test]
fn the_credits_name_the_base_its_author_and_its_origin() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let with_author = Provenance::GameBanana {
        gb_id: tf2tools_core::GbId::new(5),
        author: "raysfire".into(),
    };
    for (root, provenance, by) in [
        ("gbhud", with_author, " by raysfire (GameBanana item 5)"),
        ("localhud", gb(6), " (GameBanana item 6)"),
        ("filehud", Provenance::Local, " (a file you imported)"),
    ] {
        let base = import_stock_hud(&env, root, &[], provenance);
        let copy = env.lib.copy_hud(base.id()).unwrap();
        let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
        env.lib
            .save_made_hud(
                &MakeTarget::Existing(copy.id().clone()),
                copy.name(),
                &files,
                Platform::Linux,
                &[red()],
            )
            .unwrap();
        let d = env
            .dir
            .library_dir(LibraryKind::Huds)
            .join(copy.folder_name());
        let expected = format!("This HUD is a copy of \"{}\"{by}{TERMS}", base.name());
        assert_eq!(
            read_to_string(d.join("TF2Tools-credits.txt")).unwrap(),
            expected
        );
    }
}

#[test]
fn installing_the_copy_replaces_an_installed_original() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let menu = (
        "cfg/m0re_menu.cfg",
        "echo #base ../../custom/m0rehud/resource/ui/a.res",
    );
    let base = import_stock_hud(
        &env,
        "m0rehud-4f00092352c34d3317e195db5a5c414b0143a898",
        &[menu],
        Provenance::Local,
    );
    env.lib.install_hud(base.id(), &env.tf2).unwrap();
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    env.lib
        .save_made_hud(
            &MakeTarget::Existing(copy.id().clone()),
            copy.name(),
            &files,
            Platform::Linux,
            &[red()],
        )
        .unwrap();
    assert_eq!(
        env.lib.install_hud(copy.id(), &env.tf2).unwrap().folder(),
        "m0rehud"
    );
    let custom = env.tf2.custom_dir();
    let huds: Vec<String> = tree(&custom)
        .into_iter()
        .filter(|p| p.ends_with("info.vdf"))
        .collect();
    assert_eq!(huds, ["m0rehud/info.vdf"]);
    assert!(
        read_to_string(custom.join("m0rehud").join(H))
            .unwrap()
            .contains("255 80 80 255")
    );
    assert!(!custom.join("m0rehud/.tf2tools").exists());
    match env.lib.hud_status(&env.tf2).unwrap() {
        HudStatus::Managed { id, .. } => assert_eq!(id.as_str(), copy.id().as_str()),
        other => panic!("expected the copy managed, got {other:?}"),
    }
}
