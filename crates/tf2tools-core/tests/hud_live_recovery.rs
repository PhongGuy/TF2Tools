//! The live link's staging folder: HUD scans skip it, recovery and installs clear it.

mod common;

use std::fs;

use common::{LibraryEnv, library_env, tree, write_file, zip_bytes};
use tf2tools_core::{HudEntry, HudStatus, Provenance, Recovery};

const STAGING: &str = ".tf2tools-reinstall";

fn import(env: &LibraryEnv, folder: &str) -> HudEntry {
    let vdf = format!("{folder}/info.vdf");
    let res = format!("{folder}/resource/clientscheme.res");
    let bytes = zip_bytes(&[
        (vdf.as_str(), b"hud".as_slice()),
        (res.as_str(), b"scheme".as_slice()),
    ]);
    let archive = write_file(&env.tmp.path().join(format!("{folder}.zip")), &bytes);
    env.lib.import_hud(&archive, Provenance::Local).unwrap()
}

#[test]
fn hud_scans_ignore_the_staging_folder() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let staging = env.tf2.custom_dir().join(STAGING);
    write_file(&staging.join("new/rayshud/info.vdf"), b"hud");
    write_file(&staging.join("old/rayshud/info.vdf"), b"hud");
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Managed {
            folder_name: "rayshud".to_owned(),
            id: hud.id().clone()
        }
    );
}

#[test]
fn recovery_with_no_staging_folder_does_nothing() {
    let env = library_env();
    write_file(&env.tf2.custom_dir().join("rayshud/info.vdf"), b"hud");
    assert_eq!(
        env.lib.recover_reinstall(&env.tf2).unwrap(),
        Recovery::Clean
    );
    assert_eq!(tree(&env.tf2.custom_dir()), ["rayshud/info.vdf"]);
}

#[test]
fn a_marked_swap_puts_the_old_copy_back_and_drops_the_new_one() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    let staging = custom.join(STAGING);
    write_file(&staging.join("swapping"), b"rayshud");
    write_file(&staging.join("old/rayshud/info.vdf"), b"old");
    write_file(&staging.join("new/rayshud/info.vdf"), b"new");
    assert_eq!(
        env.lib.recover_reinstall(&env.tf2).unwrap(),
        Recovery::Restored {
            folder: "rayshud".to_owned()
        }
    );
    assert_eq!(tree(&custom), ["rayshud/info.vdf"]);
    assert_eq!(fs::read(custom.join("rayshud/info.vdf")).unwrap(), b"old");
}

#[test]
fn without_the_marker_a_set_aside_copy_never_comes_back() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    // A finished swap whose staging delete failed, then the player deleted the HUD by hand.
    write_file(&custom.join(STAGING).join("old/rayshud/info.vdf"), b"old");
    assert_eq!(
        env.lib.recover_reinstall(&env.tf2).unwrap(),
        Recovery::Cleared
    );
    assert!(tree(&custom).is_empty(), "{:?}", tree(&custom));
}

#[test]
fn uninstall_clears_a_leftover_so_nothing_comes_back() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join(STAGING).join("old/rayshud/info.vdf"), b"old");
    write_file(&custom.join(STAGING).join("swapping"), b"rayshud");
    env.lib.uninstall_hud(&env.tf2).unwrap();
    assert!(tree(&custom).is_empty(), "{:?}", tree(&custom));
    assert_eq!(
        env.lib.recover_reinstall(&env.tf2).unwrap(),
        Recovery::Clean
    );
    assert!(tree(&custom).is_empty());
}

#[test]
fn install_clears_a_leftover_staging_folder() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    let custom = env.tf2.custom_dir();
    write_file(&custom.join(STAGING).join("old/budhud/info.vdf"), b"old");
    write_file(&custom.join(STAGING).join("swapping"), b"budhud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        tree(&custom),
        ["rayshud/info.vdf", "rayshud/resource/clientscheme.res"]
    );
}

#[test]
fn recovery_clears_a_leftover_once_the_hud_is_in_place() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("rayshud/info.vdf"), b"new");
    write_file(&custom.join(STAGING).join("old/rayshud/info.vdf"), b"old");
    assert_eq!(
        env.lib.recover_reinstall(&env.tf2).unwrap(),
        Recovery::Cleared
    );
    assert_eq!(tree(&custom), ["rayshud/info.vdf"]);
    assert_eq!(fs::read(custom.join("rayshud/info.vdf")).unwrap(), b"new");

    write_file(&custom.join(STAGING).join("new/rayshud/info.vdf"), b"half");
    assert_eq!(
        env.lib.recover_reinstall(&env.tf2).unwrap(),
        Recovery::Cleared,
        "a copy cut short"
    );
    assert_eq!(tree(&custom), ["rayshud/info.vdf"]);
}
