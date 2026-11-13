//! Null-Canceling Movement: turning it on binds the player's movement keys in addons.cfg,
//! turning it off (uninstall, delete, removing mastercomfig) puts the stock binds back.

use crate::common;

use std::fs;
use std::path::PathBuf;

use common::{
    LibraryEnv, library_env, mastercomfig_base, set_mastercomfig_state, vpk_bytes, write_file,
};
use tf2tools_core::{AddonName, MoveDirection};

const NULL: &str = "mastercomfig-addon-null-canceling-movement.vpk";
const FLAT: &str = "mastercomfig-addon-flat-mouse.vpk";
const CFG: &str = "comfig-custom/cfg/app/addons.cfg";
const STOCK: &str = "unbindall\r\nbind \"a\" \"+moveleft\"\r\nbind \"d\" \"+moveright\"\r\n\
                     bind \"e\" \"+use\"\r\nbind \"s\" \"+back\"\r\nbind \"w\" \"+forward\"\r\n";
const SAVED: &str = "unbindall\r\nbind \"a\" \"+ml\"\r\nbind \"d\" \"+mr\"\r\n\
                     bind \"e\" \"+use\"\r\nbind \"s\" \"+mb\"\r\nbind \"w\" \"+mf\"\r\n";
const BINDS: &str = "exec addons/null-canceling-movement.cfg\n\
                     // TF2Tools: movement keys for Null-Canceling Movement\n\
                     bind \"w\" \"+mf\"\nbind \"s\" \"+mb\"\nbind \"a\" \"+ml\"\nbind \"d\" \"+mr\"\n";

fn name(s: &str) -> AddonName {
    AddonName::new(s).unwrap()
}

fn config(env: &LibraryEnv) -> PathBuf {
    env.tf2.tf_dir().join("cfg/config.cfg")
}

fn addons_cfg(env: &LibraryEnv) -> String {
    fs::read_to_string(env.tf2.custom_dir().join(CFG)).unwrap()
}

// Imports and installs `asset` with `config.cfg` holding `binds`.
fn env_with(binds: &str, assets: &[&str]) -> LibraryEnv {
    let env = library_env();
    write_file(&config(&env), binds.as_bytes());
    for asset in assets {
        let bytes = vpk_bytes(&[("cfg/addons/x.cfg", b"echo".as_slice())]);
        let path = write_file(&env.tmp.path().join("dl").join(asset), &bytes);
        env.lib.import_addon(&path, &name(asset), "1").unwrap();
    }
    env
}

#[test]
fn turning_it_on_binds_the_movement_keys_and_reports_them() {
    let env = env_with(STOCK, &[NULL]);
    let keys = env.lib.install_addon(&name(NULL), &env.tf2).unwrap();
    let keys = keys.expect("Null-Canceling Movement reports its keys");
    assert_eq!(keys.keys(MoveDirection::Forward), ["w"]);
    assert!(keys.missing().is_empty());
    assert_eq!(addons_cfg(&env), BINDS);
    assert_eq!(fs::read_to_string(config(&env)).unwrap(), STOCK);
}

#[test]
fn other_addons_report_no_keys() {
    let env = env_with(STOCK, &[FLAT]);
    assert_eq!(env.lib.install_addon(&name(FLAT), &env.tf2).unwrap(), None);
    assert_eq!(addons_cfg(&env), "exec addons/flat-mouse.cfg\n");
}

#[test]
fn reinstalling_after_tf2_saved_the_binds_keeps_them() {
    let env = env_with(STOCK, &[NULL, FLAT]);
    env.lib.install_addon(&name(NULL), &env.tf2).unwrap();
    write_file(&config(&env), SAVED.as_bytes());
    env.lib.install_addon(&name(NULL), &env.tf2).unwrap();
    env.lib.install_addon(&name(FLAT), &env.tf2).unwrap();
    assert_eq!(
        addons_cfg(&env),
        format!("exec addons/flat-mouse.cfg\n{BINDS}")
    );
    // Another addon going leaves the movement binds in config.cfg as they are.
    env.lib.uninstall_addon(&name(FLAT), &env.tf2).unwrap();
    assert_eq!(fs::read_to_string(config(&env)).unwrap(), SAVED);
    assert_eq!(addons_cfg(&env), BINDS);
}

#[test]
fn a_missing_direction_is_reported_and_skipped() {
    let env = env_with(
        "bind \"w\" \"+forward\"\nbind \"a\" \"+moveleft\"\n",
        &[NULL],
    );
    let keys = env
        .lib
        .install_addon(&name(NULL), &env.tf2)
        .unwrap()
        .unwrap();
    assert_eq!(keys.missing(), [MoveDirection::Back, MoveDirection::Right]);
    assert!(addons_cfg(&env).ends_with("bind \"w\" \"+mf\"\nbind \"a\" \"+ml\"\n"));
}

#[test]
fn turning_it_off_restores_only_the_addon_binds() {
    let env = env_with(STOCK, &[NULL]);
    env.lib.install_addon(&name(NULL), &env.tf2).unwrap();
    let saved = format!("{SAVED}bind \"q\" \"+mf;+jump\"\r\n");
    write_file(&config(&env), saved.as_bytes());
    env.lib.uninstall_addon(&name(NULL), &env.tf2).unwrap();
    let want = format!("{STOCK}bind \"q\" \"+mf;+jump\"\r\n");
    assert_eq!(fs::read_to_string(config(&env)).unwrap(), want);
    assert!(!env.tf2.custom_dir().join(CFG).exists());
    let leftovers: Vec<_> = fs::read_dir(env.tf2.tf_dir().join("cfg"))
        .unwrap()
        .collect();
    assert_eq!(leftovers.len(), 1, "no temp file is left behind");
}

#[test]
fn deleting_it_restores_the_binds() {
    let env = env_with(STOCK, &[NULL]);
    env.lib.install_addon(&name(NULL), &env.tf2).unwrap();
    write_file(&config(&env), SAVED.as_bytes());
    env.lib.delete_addon(&name(NULL), Some(&env.tf2)).unwrap();
    assert_eq!(fs::read_to_string(config(&env)).unwrap(), STOCK);
}

#[test]
fn turning_it_off_without_a_config_cfg_is_fine() {
    let env = env_with(STOCK, &[NULL]);
    env.lib.install_addon(&name(NULL), &env.tf2).unwrap();
    fs::remove_file(config(&env)).unwrap();
    env.lib.uninstall_addon(&name(NULL), &env.tf2).unwrap();
    assert!(!config(&env).exists());
}

#[test]
fn removing_mastercomfig_restores_the_binds() {
    let env = env_with(SAVED, &[]);
    write_file(
        &env.tf2.custom_dir().join("mastercomfig-base.vpk"),
        &mastercomfig_base("9.100.1"),
    );
    set_mastercomfig_state(&env.dir, "9.100.1", "medium");
    assert!(env.lib.remove_mastercomfig(&env.tf2).unwrap());
    assert_eq!(fs::read_to_string(config(&env)).unwrap(), STOCK);
}

#[test]
fn removing_mastercomfig_without_addon_binds_changes_nothing() {
    let env = env_with(STOCK, &[]);
    write_file(
        &env.tf2.custom_dir().join("mastercomfig-base.vpk"),
        &mastercomfig_base("9.100.1"),
    );
    set_mastercomfig_state(&env.dir, "9.100.1", "medium");
    assert!(!env.lib.remove_mastercomfig(&env.tf2).unwrap());
    assert_eq!(fs::read_to_string(config(&env)).unwrap(), STOCK);
}
