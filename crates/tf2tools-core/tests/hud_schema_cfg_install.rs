//! Settings files a copy's options write: put in `tf/cfg` on install, taken back when the
//! option is turned off or the HUD is uninstalled, and a file that was there is put back.

mod common;

use std::path::PathBuf;

use common::hud_copy::{game_for, import_stock_hud};
use common::hud_switches::save;
use common::{LibraryEnv, library_env};
use tf2tools_core::{
    Change, HudEntry, HudSchemas, LibraryKind, OptionChange, OptionValue, Provenance,
};

const UBER: &str = "e:dh_toggle_centered_ubercharge";
const MEDIC: &str = "resource/ui/hudmediccharge.res";
const BASE_UBER: &str =
    "#base \"../../../../cfg/darhud/ubercharge.txt\"\n\"Resource/UI/HudMedicCharge.res\" { }\n";

fn darhud_copy(env: &mut LibraryEnv) -> HudEntry {
    env.lib.set_hud_schemas(HudSchemas::bundled());
    let base = import_stock_hud(env, "darhud", &[(MEDIC, BASE_UBER)], Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    // A local import records no repo; give the copy darhud's, as a curated copy would have.
    let maker = (env.dir.library_dir(LibraryKind::Huds))
        .join(copy.folder_name())
        .join(".tf2tools/maker.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&maker).unwrap()).unwrap();
    json["base"]["repo"] = "https://github.com/DarSitam/darhud".into();
    std::fs::write(&maker, serde_json::to_vec(&json).unwrap()).unwrap();
    copy
}

fn uber(on: bool) -> Vec<Change> {
    vec![Change::option(
        1,
        OptionChange::new(OptionChange::SCHEMA, UBER, OptionValue::Bool(on)),
    )]
}

fn cfg_file(env: &LibraryEnv) -> PathBuf {
    env.tf2.tf_dir().join("cfg/darhud/ubercharge.txt")
}

#[test]
fn install_writes_the_file_and_turning_the_option_off_takes_it_back() {
    let mut env = library_env();
    let tf = game_for(&env, &[]);
    let copy = darhud_copy(&mut env);
    save(&env, &tf, &copy, &uber(true));
    env.lib.install_hud(copy.id(), &env.tf2).unwrap();
    let text = std::fs::read_to_string(cfg_file(&env)).unwrap();
    assert!(text.ends_with("centered_ubercharge.res"), "{text}");

    save(&env, &tf, &copy, &[]);
    env.lib.reinstall_hud(copy.id(), &env.tf2).unwrap();
    assert!(!cfg_file(&env).exists());
}

#[test]
fn uninstall_takes_the_file_back_and_restores_the_one_that_was_there() {
    let mut env = library_env();
    let tf = game_for(&env, &[]);
    let copy = darhud_copy(&mut env);
    std::fs::create_dir_all(cfg_file(&env).parent().unwrap()).unwrap();
    std::fs::write(cfg_file(&env), "the player's own").unwrap();
    save(&env, &tf, &copy, &uber(true));
    env.lib.install_hud(copy.id(), &env.tf2).unwrap();
    assert_ne!(
        std::fs::read_to_string(cfg_file(&env)).unwrap(),
        "the player's own"
    );
    env.lib.uninstall_hud(&env.tf2).unwrap();
    assert_eq!(
        std::fs::read_to_string(cfg_file(&env)).unwrap(),
        "the player's own"
    );
}

#[test]
fn a_file_the_player_changed_since_is_left_alone() {
    let mut env = library_env();
    let tf = game_for(&env, &[]);
    let copy = darhud_copy(&mut env);
    save(&env, &tf, &copy, &uber(true));
    env.lib.install_hud(copy.id(), &env.tf2).unwrap();
    std::fs::write(cfg_file(&env), "edited in game").unwrap();
    env.lib.uninstall_hud(&env.tf2).unwrap();
    assert_eq!(
        std::fs::read_to_string(cfg_file(&env)).unwrap(),
        "edited in game"
    );
}

#[test]
fn installing_another_hud_takes_the_first_ones_files_back() {
    let mut env = library_env();
    let tf = game_for(&env, &[]);
    let copy = darhud_copy(&mut env);
    save(&env, &tf, &copy, &uber(true));
    env.lib.install_hud(copy.id(), &env.tf2).unwrap();
    let other = import_stock_hud(&env, "other", &[], Provenance::Local);
    env.lib.install_hud(other.id(), &env.tf2).unwrap();
    assert!(!cfg_file(&env).exists());
}
