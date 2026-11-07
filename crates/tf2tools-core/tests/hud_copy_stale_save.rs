//! A save from a page opened before an Update is refused and writes nothing.

use crate::common;

use common::hud_copy::{game_for, import_stock_hud, snapshot, store_item};
use common::hud_switches::{choices, copy_with, dir, switch_layer};
use common::library_env;
use tf2tools_core::{HudAssets, MakeHudError, MakeTarget, Platform};

#[test]
fn a_save_opened_before_an_update_is_refused() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let copy = copy_with(&env, &switch_layer(&[]));
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let opened = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap()
        .base()
        .clone();
    let layer = switch_layer(&[("NEW.txt", "new")]);
    let extra: Vec<(&str, &str)> = layer
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let newer = import_stock_hud(&env, "somehud", &extra, store_item());
    env.lib
        .update_copy(copy.id(), newer.id(), &tf, Platform::Linux)
        .unwrap();
    let before = snapshot(&dir(&env, &copy));
    let target = MakeTarget::Existing(copy.id().clone());
    let refused = env.lib.save_made_hud_opened(
        &target,
        copy.name(),
        &files,
        Platform::Linux,
        &choices(),
        &HudAssets::new(),
        &opened,
    );
    assert!(matches!(refused, Err(MakeHudError::BaseChanged)));
    assert_eq!(snapshot(&dir(&env, &copy)), before);
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let current = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap()
        .base()
        .clone();
    assert!(
        env.lib
            .save_made_hud_opened(
                &target,
                copy.name(),
                &files,
                Platform::Linux,
                &choices(),
                &HudAssets::new(),
                &current
            )
            .is_ok()
    );
}
