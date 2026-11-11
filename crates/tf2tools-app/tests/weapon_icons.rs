//! The session's game weapon icons: one load per found TF2 folder, off the UI thread,
//! following the shared TF2 state.

use crate::support;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::services::{Services, Tf2State, install_tf2_state};
use tf2tools_app::weapon_icons::{IconLoader, WeaponIcons};
use tf2tools_core::Tf2Install;

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn setup(
    cx: &mut TestAppContext,
    env: &support::Env,
    state: Tf2State,
    skip: &'static [&'static str],
) -> (Entity<Tf2State>, Entity<WeaponIcons>, support::IconLoads) {
    support::init(cx);
    support::install_services(cx, support::services(env, support::no_steam()));
    let tf2 = cx.update(|cx| install_tf2_state(cx, state));
    let (loader, loads) = support::fake_icon_loader(support::icon_png(env), skip);
    let icons = cx.update(|cx| WeaponIcons::install_with(cx, loader));
    (tf2, icons, loads)
}

fn icon_of(cx: &mut TestAppContext, icons: &Entity<WeaponIcons>, script: &str) -> Option<PathBuf> {
    cx.update(|cx| icons.read(cx).icon(script).map(Path::to_path_buf))
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
fn a_found_install_loads_its_icons_once(cx: &mut TestAppContext) {
    let env = support::env();
    let (_tf2, icons, loads) = setup(cx, &env, found(&env), &[]);
    support::wait_real(cx, "the icons", |cx| {
        icon_of(cx, &icons, "tf_weapon_scattergun").is_some()
    });
    assert_eq!(
        icon_of(cx, &icons, "tf_weapon_scattergun"),
        Some(env.tmp.path().join("icon.png"))
    );
    let again = cx.update(WeaponIcons::global);
    assert_eq!(again.entity_id(), icons.entity_id());
    cx.run_until_parked();
    assert_eq!(*loads.lock().unwrap(), vec![env.game.join("tf")]);
    assert_eq!(cx.update(|cx| icons.read(cx).loads()), 1);
}

#[gpui_kit::test]
fn a_weapon_the_game_lacks_has_no_icon(cx: &mut TestAppContext) {
    let env = support::env();
    let (_tf2, icons, _) = setup(cx, &env, found(&env), &["tf_weapon_bat"]);
    support::wait_real(cx, "the icons", |cx| {
        icon_of(cx, &icons, "tf_weapon_scattergun").is_some()
    });
    assert_eq!(icon_of(cx, &icons, "tf_weapon_bat"), None);
}

#[gpui_kit::test]
fn nothing_loads_before_tf2_is_found(cx: &mut TestAppContext) {
    let env = support::env();
    let (_tf2, icons, loads) = setup(cx, &env, Tf2State::Missing { saved: None }, &[]);
    cx.run_until_parked();
    assert!(loads.lock().unwrap().is_empty());
    assert_eq!(icon_of(cx, &icons, "tf_weapon_scattergun"), None);
}

#[gpui_kit::test]
fn finding_tf2_later_loads_and_losing_it_clears(cx: &mut TestAppContext) {
    let env = support::env();
    let (tf2, icons, loads) = setup(cx, &env, Tf2State::Detecting, &[]);
    cx.run_until_parked();
    assert!(loads.lock().unwrap().is_empty());

    set_state(cx, &tf2, found(&env));
    support::wait_real(cx, "the icons", |cx| {
        icon_of(cx, &icons, "tf_weapon_scattergun").is_some()
    });
    // A notify that leaves the folder as it was starts no second load.
    set_state(cx, &tf2, found(&env));
    cx.run_until_parked();
    assert_eq!(loads.lock().unwrap().len(), 1);

    set_state(cx, &tf2, Tf2State::Missing { saved: None });
    cx.run_until_parked();
    assert_eq!(icon_of(cx, &icons, "tf_weapon_scattergun"), None);
}

#[gpui_kit::test]
fn without_a_tf2_state_nothing_loads(cx: &mut TestAppContext) {
    let env = support::env();
    support::init(cx);
    support::install_services(cx, support::services(&env, support::no_steam()));
    let icons = cx.update(WeaponIcons::global);
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| icons.read(cx).loads()), 0);
    assert_eq!(icon_of(cx, &icons, "tf_weapon_scattergun"), None);
}

// Each folder's load pictures every weapon with `<tf>/icon.png`, so a test can tell whose
// result is shown.
fn per_folder_loader() -> IconLoader {
    Arc::new(|_: &Services, tf_dir: &Path| {
        tf2tools_core::weapons()
            .iter()
            .map(|w| (w.script_name(), tf_dir.join("icon.png")))
            .collect::<HashMap<_, _>>()
    })
}

#[gpui_kit::test]
fn moving_tf2_mid_load_shows_only_the_new_folders_icons(cx: &mut TestAppContext) {
    let env = support::env();
    let other = env.tmp.path().join("Other TF2");
    std::fs::create_dir_all(other.join("tf")).unwrap();
    support::init(cx);
    support::install_services(cx, support::services(&env, support::no_steam()));
    let tf2 = cx.update(|cx| install_tf2_state(cx, Tf2State::Detecting));
    let icons = cx.update(|cx| WeaponIcons::install_with(cx, per_folder_loader()));

    // The old folder's load is still in flight when TF2 moves.
    set_state(cx, &tf2, found(&env));
    set_state(
        cx,
        &tf2,
        Tf2State::Found(Tf2Install::from_path(&other).unwrap()),
    );
    cx.run_until_parked();

    assert_eq!(
        icon_of(cx, &icons, "tf_weapon_scattergun"),
        Some(other.join("tf/icon.png"))
    );
    assert_eq!(cx.update(|cx| icons.read(cx).loads()), 2);
}

// The landing guard on its own: a result for a folder no longer shown is dropped even if
// its task was not.
#[gpui_kit::test]
fn a_late_result_for_an_old_folder_is_dropped(cx: &mut TestAppContext) {
    let env = support::env();
    let (_tf2, icons, _) = setup(cx, &env, Tf2State::Detecting, &[]);
    let old = env.game.join("tf");
    let late = HashMap::from([("tf_weapon_scattergun", env.tmp.path().join("old.png"))]);
    let shown = cx.update(|cx| {
        icons.update(cx, |icons, cx| {
            icons.show_game(Some(env.tmp.path().join("Other TF2/tf")), cx);
            icons.land(&old, late, cx);
            icons.icon("tf_weapon_scattergun").map(Path::to_path_buf)
        })
    });
    assert_eq!(shown, None);
}
