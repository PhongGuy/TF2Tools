//! Clearing assignments back to the game's default, and telling which weapons an Install
//! would change.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{crosshair_env, set_setting, setting, write_file};
use tf2tools_core::{BuiltinCrosshairs, CrosshairRef, LibraryError};

const SCRIPTS: &str = "tf2tools-crosshairs/scripts";

fn catalog() -> BuiltinCrosshairs {
    let bundled: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/crosshairs");
    BuiltinCrosshairs::load(&bundled).unwrap()
}

fn builtin(name: &str) -> CrosshairRef {
    CrosshairRef::Builtin(name.into())
}

fn assigned(env: &common::LibraryEnv) -> Vec<String> {
    env.lib
        .assignments()
        .unwrap()
        .iter()
        .map(|a| a.weapon_script().to_owned())
        .collect()
}

#[test]
fn unassign_many_clears_only_the_named_weapons() {
    let env = crosshair_env();
    let scripts = ["tf_weapon_bat", "tf_weapon_scattergun", "tf_weapon_minigun"];
    env.lib.assign_many(&scripts, &builtin("cpma_9")).unwrap();
    set_setting(&env.dir, "active_profile_id", "p1");

    env.lib
        .unassign_many(&["tf_weapon_bat", "tf_weapon_scattergun"])
        .unwrap();
    assert_eq!(assigned(&env), ["tf_weapon_minigun"]);
    // A changed loadout is no longer the profile's, as with assign.
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
}

#[test]
fn unassign_many_of_unassigned_weapons_keeps_the_profile() {
    let env = crosshair_env();
    set_setting(&env.dir, "active_profile_id", "p1");
    env.lib.unassign_many(&["tf_weapon_bat"]).unwrap();
    env.lib.unassign_many(&[]).unwrap();
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some("p1")
    );
}

#[test]
fn unassign_many_rejects_a_bad_name_before_clearing_anything() {
    let env = crosshair_env();
    env.lib.assign("tf_weapon_bat", &builtin("cpma_9")).unwrap();
    let err = env
        .lib
        .unassign_many(&["tf_weapon_bat", "../evil"])
        .unwrap_err();
    assert!(matches!(err, LibraryError::InvalidWeaponScript { .. }));
    assert_eq!(assigned(&env), ["tf_weapon_bat"]);
}

#[test]
fn nothing_is_waiting_when_nothing_is_assigned_or_installed() {
    let env = crosshair_env();
    let waiting = env
        .lib
        .crosshairs_not_in_game(&env.tf2, &catalog())
        .unwrap();
    assert!(waiting.is_empty(), "{waiting:?}");
}

#[test]
fn new_assignments_wait_until_installed() {
    let env = crosshair_env();
    env.lib
        .assign_many(
            &["tf_weapon_scattergun", "tf_weapon_bat"],
            &builtin("cpma_9"),
        )
        .unwrap();
    assert_eq!(
        env.lib
            .crosshairs_not_in_game(&env.tf2, &catalog())
            .unwrap(),
        ["tf_weapon_bat", "tf_weapon_scattergun"]
    );
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    assert!(
        env.lib
            .crosshairs_not_in_game(&env.tf2, &catalog())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_changed_or_cleared_assignment_waits_until_installed() {
    let env = crosshair_env();
    let builtins = catalog();
    let other = builtins
        .all()
        .iter()
        .find(|b| b.name() != "cpma_9")
        .unwrap()
        .name()
        .to_owned();
    env.lib
        .assign_many(
            &["tf_weapon_scattergun", "tf_weapon_bat"],
            &builtin("cpma_9"),
        )
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &builtins).unwrap();

    env.lib
        .assign("tf_weapon_scattergun", &builtin(&other))
        .unwrap();
    env.lib.unassign_many(&["tf_weapon_bat"]).unwrap();
    assert_eq!(
        env.lib.crosshairs_not_in_game(&env.tf2, &builtins).unwrap(),
        ["tf_weapon_bat", "tf_weapon_scattergun"]
    );
}

#[test]
fn an_assignment_install_would_skip_never_waits() {
    let env = crosshair_env();
    env.lib.assign("tf_weapon_bat", &builtin("nope")).unwrap();
    assert!(
        env.lib
            .crosshairs_not_in_game(&env.tf2, &catalog())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_removed_texture_or_edited_script_waits_for_a_reinstall() {
    let env = crosshair_env();
    env.lib
        .assign_many(
            &["tf_weapon_scattergun", "tf_weapon_bat"],
            &builtin("cpma_9"),
        )
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join(SCRIPTS).join("tf_weapon_bat.txt"), b"edited");
    assert_eq!(
        env.lib
            .crosshairs_not_in_game(&env.tf2, &catalog())
            .unwrap(),
        ["tf_weapon_bat"]
    );
    fs::remove_file(custom.join("tf2tools-crosshairs/materials/vgui/replay/thumbnails/cpma_9.vtf"))
        .unwrap();
    assert_eq!(
        env.lib
            .crosshairs_not_in_game(&env.tf2, &catalog())
            .unwrap(),
        ["tf_weapon_bat", "tf_weapon_scattergun"]
    );
}
