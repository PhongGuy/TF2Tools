//! The session's weapon names for logs.tf: one load per found TF2 folder, off the UI thread.

use crate::support;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::services::{Services, Tf2State, install_tf2_state};
use tf2tools_app::weapon_names::{LoadedWeaponNames, NamesLoader};
use tf2tools_core::{Tf2Install, WeaponNames};

fn found(game: &Path) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(game).unwrap())
}

// Names quake_rl after the folder's game directory, so a test can tell whose load shows.
fn per_folder_loader() -> (NamesLoader, Arc<Mutex<Vec<PathBuf>>>) {
    let loads: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let log = Arc::clone(&loads);
    let loader: NamesLoader = Arc::new(move |_: &Services, tf_dir: &Path| {
        log.lock().unwrap().push(tf_dir.to_path_buf());
        let game = tf_dir
            .parent()
            .and_then(Path::file_name)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let lang = format!("\"lang\" {{ \"Tokens\" {{ \"TF_TheOriginal\" \"{game}\" }} }}");
        WeaponNames::from_files(support::logstf::NAMES_ITEMS, lang.as_bytes())
    });
    (loader, loads)
}

fn setup(
    cx: &mut TestAppContext,
    env: &support::Env,
    state: Tf2State,
) -> (
    Entity<Tf2State>,
    Entity<LoadedWeaponNames>,
    Arc<Mutex<Vec<PathBuf>>>,
) {
    support::init(cx);
    support::install_services(cx, support::services(env, support::no_steam()));
    let tf2 = cx.update(|cx| install_tf2_state(cx, state));
    let (loader, loads) = per_folder_loader();
    let names = cx.update(|cx| LoadedWeaponNames::install_with(cx, loader));
    (tf2, names, loads)
}

fn name_of(
    cx: &mut TestAppContext,
    names: &Entity<LoadedWeaponNames>,
    raw: &str,
) -> Option<String> {
    cx.update(|cx| names.read(cx).names().get(raw).map(str::to_owned))
}

fn set_state(cx: &mut TestAppContext, tf2: &Entity<Tf2State>, state: Tf2State) {
    cx.update(|cx| {
        tf2.update(cx, |t, cx| {
            *t = state;
            cx.notify();
        })
    });
}

#[gpui_kit::test]
fn a_found_install_loads_its_names_once(cx: &mut TestAppContext) {
    let env = support::env();
    let (_tf2, names, loads) = setup(cx, &env, found(&env.game));
    support::wait_real(cx, "the names", |cx| {
        name_of(cx, &names, "quake_rl").is_some()
    });
    assert_eq!(
        name_of(cx, &names, "quake_rl").as_deref(),
        Some("The Team Fortress 2")
    );
    let again = cx.update(LoadedWeaponNames::global);
    assert_eq!(again.entity_id(), names.entity_id());
    cx.run_until_parked();
    assert_eq!(*loads.lock().unwrap(), vec![env.game.join("tf")]);
}

#[gpui_kit::test]
fn finding_tf2_later_loads_and_losing_it_clears(cx: &mut TestAppContext) {
    let env = support::env();
    let (tf2, names, loads) = setup(cx, &env, Tf2State::Detecting);
    cx.run_until_parked();
    assert!(loads.lock().unwrap().is_empty());
    set_state(cx, &tf2, found(&env.game));
    support::wait_real(cx, "the names", |cx| {
        name_of(cx, &names, "quake_rl").is_some()
    });
    set_state(cx, &tf2, Tf2State::Missing { saved: None });
    cx.run_until_parked();
    assert_eq!(name_of(cx, &names, "quake_rl"), None);
}

#[gpui_kit::test]
fn moving_tf2_mid_load_shows_only_the_new_folders_names(cx: &mut TestAppContext) {
    let env = support::env();
    let other = env.tmp.path().join("Other TF2");
    std::fs::create_dir_all(other.join("tf")).unwrap();
    let (tf2, names, _) = setup(cx, &env, Tf2State::Detecting);
    set_state(cx, &tf2, found(&env.game));
    set_state(cx, &tf2, found(&other));
    cx.run_until_parked();
    support::wait_real(cx, "the names", |cx| {
        name_of(cx, &names, "quake_rl").is_some()
    });
    assert_eq!(
        name_of(cx, &names, "quake_rl").as_deref(),
        Some("The Other TF2")
    );
    assert_eq!(cx.update(|cx| names.read(cx).loads()), 2);
}

// The production loader on env's game folder, waited on until its read has landed.
fn load_for_real(cx: &mut TestAppContext, env: &support::Env) -> Entity<LoadedWeaponNames> {
    support::init(cx);
    support::install_services(cx, support::services(env, support::no_steam()));
    cx.update(|cx| install_tf2_state(cx, found(&env.game)));
    let names = cx.update(LoadedWeaponNames::global);
    support::wait_real(cx, "the load", |cx| {
        cx.update(|cx| names.read(cx).loads() == 1 && !names.read(cx).loading())
    });
    names
}

#[gpui_kit::test]
fn the_games_files_give_its_weapon_names(cx: &mut TestAppContext) {
    let env = support::env();
    let tf = env.game.join("tf");
    std::fs::create_dir_all(tf.join("scripts/items")).unwrap();
    std::fs::create_dir_all(tf.join("resource")).unwrap();
    std::fs::write(
        tf.join("scripts/items/items_game.txt"),
        support::logstf::NAMES_ITEMS,
    )
    .unwrap();
    std::fs::write(
        tf.join("resource/tf_english.txt"),
        support::logstf::NAMES_LANG,
    )
    .unwrap();
    let names = load_for_real(cx, &env);
    assert_eq!(
        name_of(cx, &names, "quake_rl").as_deref(),
        Some("The Original")
    );
    assert_eq!(
        cx.update(|cx| names.read(cx).names().clone()),
        support::logstf::weapon_names()
    );
}

#[gpui_kit::test]
fn a_game_without_items_game_leaves_log_names_raw(cx: &mut TestAppContext) {
    let env = support::env();
    let names = load_for_real(cx, &env);
    assert_eq!(
        cx.update(|cx| names.read(cx).names().clone()),
        WeaponNames::default()
    );
}
