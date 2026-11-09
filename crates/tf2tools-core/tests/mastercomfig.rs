//! Detecting mastercomfig in tf/custom, reading its version, and removing it.

use crate::common;

use std::fs;
use std::path::PathBuf;

use common::{
    LibraryEnv, library_env, mastercomfig_base, set_mastercomfig_state, tree, write_file,
};
use tf2tools_core::{MastercomfigStatus, Preset, RemoveMastercomfigError};

const BASE: &str = "mastercomfig-base.vpk";
const OURS: &str = "// Written by TF2Tools; changes here are replaced\n";

fn custom(env: &LibraryEnv) -> PathBuf {
    env.tf2.custom_dir()
}

fn status(env: &LibraryEnv) -> MastercomfigStatus {
    env.lib.mastercomfig_status(&env.tf2).unwrap()
}

#[test]
fn preset_names_round_trip_and_the_picker_leaves_out_destitute() {
    use Preset::{Custom, High, Low, Medium, Ultra};
    assert_eq!(Preset::PICKER, [Ultra, High, Medium, Low, Custom]);
    for preset in Preset::PICKER {
        assert_eq!(Preset::from_name(preset.name()), Some(preset));
    }
    assert_eq!(Medium.name(), "medium");
    assert_eq!(Preset::from_name(" High "), Some(High));
    assert_eq!(Preset::from_name("destitute"), None);
}

#[test]
fn without_the_base_vpk_mastercomfig_is_not_installed() {
    let env = library_env();
    assert_eq!(status(&env), MastercomfigStatus::NotInstalled);
    set_mastercomfig_state(&env.dir, "9.100.1", "high");
    assert_eq!(
        status(&env),
        MastercomfigStatus::NotInstalled,
        "a saved state without the file is no install"
    );
}

#[test]
fn a_hand_install_reads_its_version_from_the_vpk() {
    let env = library_env();
    write_file(&custom(&env).join(BASE), &mastercomfig_base("9.100.1"));
    assert_eq!(
        status(&env),
        MastercomfigStatus::Outside {
            version: Some("9.100.1".to_owned())
        }
    );
}

#[test]
fn an_unreadable_base_vpk_is_installed_without_a_version() {
    let env = library_env();
    write_file(&custom(&env).join(BASE), b"not a vpk");
    assert_eq!(status(&env), MastercomfigStatus::Outside { version: None });
}

#[test]
fn a_managed_install_reads_version_and_preset_from_its_state() {
    let env = library_env();
    write_file(&custom(&env).join(BASE), &mastercomfig_base("9.99.0"));
    set_mastercomfig_state(&env.dir, "9.100.1", "low");
    assert_eq!(
        status(&env),
        MastercomfigStatus::Managed {
            version: "9.100.1".to_owned(),
            preset: Preset::Low
        }
    );
}

#[test]
fn remove_deletes_mastercomfig_and_keeps_the_players_files() {
    let env = library_env();
    let custom = custom(&env);
    let overrides = env.tf2.tf_dir().join("cfg/overrides");
    write_file(&custom.join(BASE), &mastercomfig_base("9.100.1"));
    write_file(&custom.join("mastercomfig-base.vpk.sound.cache"), b"c");
    write_file(&custom.join("mastercomfig-addon-override-2.vpk"), b"o");
    write_file(&custom.join("mastercomfig-addon-flat-mouse.vpk"), b"a");
    write_file(
        &custom.join("comfig-custom/cfg/app/setup_hook.cfg"),
        b"preset=high\n",
    );
    write_file(
        &overrides.join("modules.cfg"),
        format!("{OURS}fpscap=120\n").as_bytes(),
    );
    write_file(&overrides.join("autoexec.cfg"), b"echo mine\n");
    write_file(&overrides.join("setup_hook.cfg"), b"echo mine too\n");
    set_mastercomfig_state(&env.dir, "9.100.1", "high");

    env.lib.remove_mastercomfig(&env.tf2).unwrap();

    assert_eq!(tree(&custom), ["mastercomfig-addon-flat-mouse.vpk"]);
    assert_eq!(tree(&overrides), ["autoexec.cfg", "setup_hook.cfg"]);
    assert_eq!(common::count(&env.dir, "mastercomfig_state"), 0);
    assert_eq!(status(&env), MastercomfigStatus::NotInstalled);
}

#[test]
fn remove_keeps_a_modules_cfg_tf2tools_did_not_write() {
    let env = library_env();
    let modules = env.tf2.tf_dir().join("cfg/overrides/modules.cfg");
    write_file(&custom(&env).join(BASE), &mastercomfig_base("9.100.1"));
    write_file(&modules, b"fpscap=240\n");
    set_mastercomfig_state(&env.dir, "9.100.1", "medium");
    env.lib.remove_mastercomfig(&env.tf2).unwrap();
    assert_eq!(fs::read(&modules).unwrap(), b"fpscap=240\n");
}

#[test]
fn remove_refuses_an_install_tf2tools_does_not_manage() {
    let env = library_env();
    let base = write_file(&custom(&env).join(BASE), &mastercomfig_base("9.100.1"));
    let err = env.lib.remove_mastercomfig(&env.tf2).unwrap_err();
    assert!(
        matches!(err, RemoveMastercomfigError::NotManaged),
        "{err:?}"
    );
    assert!(base.is_file());
}
