//! Which picture a part draws in its scene: the code's, the button's default, or the file's.

mod common;

use common::hud_stock::{class_select, stock_files, stock_files_with};
use tf2tools_core::{HudFiles, HudModel, PartKey, Platform, Scene, Team, drawn_image, scene_image};

fn menu(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::ClassSelect).unwrap()
}

fn drawn(m: &HudModel, key: &str) -> Option<String> {
    let part = m
        .part(&PartKey::new(key))
        .unwrap_or_else(|| panic!("{key}"));
    scene_image(m, part, Team::Red).map(str::to_owned)
}

#[test]
fn the_selected_button_and_the_teammate_row_draw_the_code_s_pictures() {
    let m = menu(&stock_files());
    assert_eq!(
        drawn(&m, "class/soldier/SubImage").as_deref(),
        Some("class_sel_sm_soldier_red")
    );
    assert_eq!(
        drawn(&m, "class/localPlayerImage").as_deref(),
        Some("class_sel_sm_soldier_red")
    );
    assert_eq!(
        drawn(&m, "class/countImage0").as_deref(),
        Some("class_sel_sm_scout_red")
    );
    assert_eq!(
        drawn(&m, "class/countImage8").as_deref(),
        Some("class_sel_sm_medic_red")
    );
    // The file's own picture for a part the code does not name.
    assert_eq!(
        drawn(&m, "class/localPlayerBG").as_deref(),
        Some("../hud/color_panel_clear")
    );
}

#[test]
fn an_unselected_button_draws_its_default_picture() {
    let m = menu(&stock_files());
    assert_eq!(
        drawn(&m, "class/scout/SubImage").as_deref(),
        Some("class_sel_sm_scout_inactive")
    );
    // A HUD that sets the button's `image_default` (two corpus HUDs do) wins over the SubImage's key.
    let text = class_select().replace(
        "image_default class_sel_sm_scout_inactive",
        "image_default class_sel_sm_pyro_inactive",
    );
    assert_ne!(text, class_select());
    let other = menu(&stock_files_with(&[(
        "resource/ui/classselection.res",
        Some(&text),
    )]));
    assert_eq!(
        drawn(&other, "class/scout/SubImage").as_deref(),
        Some("class_sel_sm_pyro_inactive")
    );
    // The selected button still takes the code's picture over its default.
    assert_eq!(
        drawn(&other, "class/soldier/SubImage").as_deref(),
        Some("class_sel_sm_soldier_red")
    );
}

#[test]
fn the_buttons_are_found_by_name_whatever_control_the_file_calls_them() {
    let text = class_select().replace(
        "soldier { ControlName CExImageButton",
        "soldier { ControlName Button",
    );
    assert_ne!(text, class_select());
    let m = menu(&stock_files_with(&[(
        "resource/ui/classselection.res",
        Some(&text),
    )]));
    assert_eq!(
        drawn(&m, "class/soldier/SubImage").as_deref(),
        Some("class_sel_sm_soldier_red")
    );
}

#[test]
fn outside_the_class_menu_it_is_drawn_image() {
    let files = stock_files();
    let freeze = HudModel::load_scene(&files, Platform::Linux, Scene::FreezeCam).unwrap();
    let mut seen = 0;
    for part in freeze.parts() {
        assert_eq!(
            scene_image(&freeze, part, Team::Red),
            drawn_image(part, Team::Red),
            "{}",
            part.key().as_str()
        );
        seen += usize::from(drawn_image(part, Team::Red).is_some());
    }
    assert!(
        seen > 0,
        "the freeze cam has at least one picture to compare"
    );
}
