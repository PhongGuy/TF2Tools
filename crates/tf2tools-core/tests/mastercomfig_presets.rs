//! Picking a preset, taking over a hand install, and writing back cleared files.

use crate::common;

use std::fs;

use common::{LibraryEnv, library_env, mastercomfig_base, set_mastercomfig_state, write_file};
use tf2tools_core::{ApplyPresetError, MastercomfigStatus, Preset, TakeOverError};

const BASE: &str = "mastercomfig-base.vpk";
const HOOK: &str = "comfig-custom/cfg/app/setup_hook.cfg";
const ADDONS_CFG: &str = "comfig-custom/cfg/app/addons.cfg";

fn hook(env: &LibraryEnv) -> Option<String> {
    fs::read_to_string(env.tf2.custom_dir().join(HOOK)).ok()
}

fn managed(env: &LibraryEnv, preset: &str) {
    write_file(
        &env.tf2.custom_dir().join(BASE),
        &mastercomfig_base("9.100.1"),
    );
    set_mastercomfig_state(&env.dir, "9.100.1", preset);
}

fn status_of(version: &str, preset: Preset) -> MastercomfigStatus {
    MastercomfigStatus::Managed {
        version: version.to_owned(),
        preset,
    }
}

#[test]
fn apply_preset_writes_setup_hook_and_saves_the_choice() {
    let env = library_env();
    managed(&env, "medium");
    env.lib.apply_preset(&env.tf2, Preset::Ultra).unwrap();
    assert_eq!(hook(&env).as_deref(), Some("preset=ultra\n"));
    assert_eq!(
        env.lib.mastercomfig_status(&env.tf2).unwrap(),
        status_of("9.100.1", Preset::Ultra)
    );
}

#[test]
fn apply_preset_refuses_an_install_tf2tools_does_not_manage() {
    let env = library_env();
    write_file(
        &env.tf2.custom_dir().join(BASE),
        &mastercomfig_base("9.100.1"),
    );
    let err = env.lib.apply_preset(&env.tf2, Preset::High).unwrap_err();
    assert!(matches!(err, ApplyPresetError::NotManaged), "{err:?}");
    assert_eq!(hook(&env), None);
}

#[test]
fn take_over_records_the_vpk_version_and_the_existing_preset() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join(BASE), &mastercomfig_base("9.99.0"));
    write_file(&custom.join(HOOK), b"// comfig-app\npreset=low\n");
    let autoexec = write_file(
        &custom.join("comfig-custom/cfg/app/autoexec.cfg"),
        b"echo app\n",
    );
    let status = env.lib.take_over_mastercomfig(&env.tf2).unwrap();
    assert_eq!(status, status_of("9.99.0", Preset::Low));
    assert_eq!(env.lib.mastercomfig_status(&env.tf2).unwrap(), status);
    assert_eq!(hook(&env).as_deref(), Some("// comfig-app\npreset=low\n"));
    assert!(
        autoexec.is_file(),
        "take over leaves every file where it is"
    );
    let again = env.lib.take_over_mastercomfig(&env.tf2).unwrap_err();
    assert!(matches!(again, TakeOverError::AlreadyManaged), "{again:?}");
}

#[test]
fn take_over_defaults_to_medium_and_an_unreadable_version_is_unknown() {
    let env = library_env();
    write_file(&env.tf2.custom_dir().join(BASE), b"not a vpk");
    let status = env.lib.take_over_mastercomfig(&env.tf2).unwrap();
    assert_eq!(status, status_of("unknown", Preset::Medium));
}

#[test]
fn take_over_needs_the_base_vpk() {
    let env = library_env();
    let err = env.lib.take_over_mastercomfig(&env.tf2).unwrap_err();
    assert!(matches!(err, TakeOverError::NotInstalled), "{err:?}");
}

#[test]
fn restore_writes_back_files_another_tool_cleared() {
    let env = library_env();
    managed(&env, "high");
    write_file(
        &env.tf2
            .custom_dir()
            .join("mastercomfig-addon-flat-mouse.vpk"),
        b"a",
    );
    assert!(env.lib.restore_mastercomfig_files(&env.tf2).unwrap());
    assert_eq!(hook(&env).as_deref(), Some("preset=high\n"));
    let addons = fs::read_to_string(env.tf2.custom_dir().join(ADDONS_CFG)).unwrap();
    assert_eq!(addons, "exec addons/flat-mouse.cfg\n");
    assert!(
        !env.lib.restore_mastercomfig_files(&env.tf2).unwrap(),
        "nothing is missing the second time"
    );
}

#[test]
fn restore_leaves_an_unmanaged_game_and_an_existing_preset_file_alone() {
    let env = library_env();
    write_file(
        &env.tf2.custom_dir().join(BASE),
        &mastercomfig_base("9.100.1"),
    );
    assert!(!env.lib.restore_mastercomfig_files(&env.tf2).unwrap());
    assert_eq!(hook(&env), None);
    set_mastercomfig_state(&env.dir, "9.100.1", "high");
    write_file(&env.tf2.custom_dir().join(HOOK), b"preset=low\n");
    assert!(!env.lib.restore_mastercomfig_files(&env.tf2).unwrap());
    assert_eq!(hook(&env).as_deref(), Some("preset=low\n"));
}
