//! The class select menu's code toggles, and what they do to a file that disagrees.

use crate::common;

use common::hud_stock::{class_select, stock_files, stock_files_with};
use tf2tools_core::{HiddenBy, HudModel, PartKey, Platform, Scene, SceneRule, Screen};

const SD: Screen = Screen::new(640, 480);

fn menu(files: &tf2tools_core::HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::ClassSelect).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn the_table_has_the_game_s_29_cases_and_none_is_class_or_weapon_driven() {
    let rule = SceneRule::get(Scene::ClassSelect);
    assert_eq!(rule.code_toggled().len(), 29);
    for t in rule.code_toggled() {
        assert!(t.shows_when().starts_with("Shows "), "{}", t.panel());
        assert!(t.shows_for().is_none(), "{}", t.panel());
    }
    let shown = |name: &str| rule.toggled(name).map(|t| t.shown_in_sample());
    for name in [
        "CancelButton",
        "EditLoadoutButton",
        "TFPlayerModel",
        "localPlayerImage",
        "CountLabel",
        "countImage0",
        "countImage10",
    ] {
        assert_eq!(shown(name), Some(true), "{name}");
    }
    for name in [
        "ClassMenuSelect",
        "ResetButton",
        "MvMUpgradeImageSolider",
        "StartExplanation",
    ] {
        assert_eq!(shown(name), Some(false), "{name}");
    }
    // Not toggled: the buttons, their counts, the backdrop and the tips box stay as drawn.
    for name in [
        "soldier",
        "random",
        "numSoldier",
        "MenuBG",
        "ClassTipsPanel",
    ] {
        assert!(rule.toggled(name).is_none(), "{name}");
    }
}

#[test]
fn the_game_shows_the_teammates_and_hides_the_select_label_and_the_mvm_panels() {
    let m = menu(&stock_files());
    let l = m.layout(SD);
    let hidden = |key: &str| l.get(&k(key)).unwrap().hidden();
    // The file says `visible 0` for these; the game shows them.
    for key in [
        "class/countImage0",
        "class/countImage10",
        "class/localPlayerImage",
        "class/localPlayerBG",
    ] {
        assert_eq!(hidden(key), None, "{key}");
    }
    for key in [
        "class/ClassMenuSelect",
        "class/ResetButton",
        "class/MvMUpgradeImageSolider",
        "class/StartExplanation",
    ] {
        assert_eq!(hidden(key), Some(HiddenBy::Game), "{key}");
    }
    for key in [
        "class/CancelButton",
        "class/soldier",
        "class/random",
        "class/ClassTipsPanel",
    ] {
        assert_eq!(hidden(key), None, "{key}");
    }
}

#[test]
fn a_hud_that_shows_the_select_label_still_gets_the_game_s_hide() {
    // The player already has a class, so the game hides the label whatever the file says.
    let text = class_select().replace(
        "ClassMenuSelect { ControlName CExLabel xpos 30 ypos r40 wide 450 tall 30 visible 1",
        "ClassMenuSelect { ControlName CExLabel xpos 30 ypos r40 wide 450 tall 30 visible 1 alpha 255",
    );
    assert_ne!(text, class_select());
    let m = menu(&stock_files_with(&[(
        "resource/ui/classselection.res",
        Some(&text),
    )]));
    let key = k("class/ClassMenuSelect");
    assert_eq!(
        m.layout(SD).get(&key).unwrap().hidden(),
        Some(HiddenBy::Game)
    );
}
