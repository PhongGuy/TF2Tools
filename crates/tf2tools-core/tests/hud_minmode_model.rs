//! The minimal-HUD flag survives every model rebuild, and a HUD without minimal keys says so.

mod common;

use common::hud_stock::{HEALTH, KOTH_PANEL, stock_files_with};
use tf2tools_core::{BoardMode, HudModel, PartKey, Platform, Scene};

fn stock() -> HudModel {
    HudModel::load(&stock_files_with(&[]), Platform::Linux).unwrap()
}

#[test]
fn the_flag_survives_scene_mode_and_changes() {
    let on = stock().with_minmode(true);
    assert!(on.with_scene(Scene::Scoreboard).minmode());
    assert!(on.with_board_mode(BoardMode::Mvm).minmode());
    assert!(on.with_changes(&[]).model().minmode());
    assert!(!on.with_minmode(false).minmode());
    let back = on.with_scene(Scene::Scoreboard).with_scene(Scene::Hud);
    let x = back
        .part(&PartKey::new("HudPlayerStatus/HudPlayerHealth"))
        .unwrap()
        .value("xpos")
        .unwrap()
        .text()
        .to_owned();
    assert_eq!(x, "-5", "the HUD scene re-resolves under the flag");
}

#[test]
fn a_scene_without_minimal_keys_says_so() {
    assert!(
        stock().has_minmode_keys(),
        "the health file has xpos_minmode"
    );
    let plain = HEALTH.replacen("    xpos_minmode -5\n", "", 1);
    let koth = KOTH_PANEL
        .replace(" xpos_minmode 20", "")
        .replace(" xpos_minmode 70", "");
    let files = stock_files_with(&[
        ("resource/ui/hudplayerhealth.res", Some(&plain)),
        ("resource/ui/hudobjectivekothtimepanel.res", Some(&koth)),
    ]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert!(!m.has_minmode_keys());
    assert!(
        !m.with_minmode(true).has_minmode_keys(),
        "the flag does not change the answer"
    );
    assert!(!stock().with_scene(Scene::Scoreboard).has_minmode_keys());
}
