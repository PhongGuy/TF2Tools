//! The spectator bar's code toggles, and what they do to a file that disagrees.

mod common;

use common::hud_stock::{SPECTATOR, stock_files, stock_files_with};
use tf2tools_core::{
    ChangeTarget, EditRefused, HiddenBy, HudFiles, HudModel, PartKey, Platform, Scene, SceneRule,
    Screen,
};

const FILE: &str = "resource/ui/spectator.res";
const TOP: &str =
    "topbar { ControlName Panel fieldName TopBar xpos 0 ypos 0 tall 84 wide f0 visible 1 }";
const BOTTOM: &str = "bottombarblank { ControlName Panel fieldName bottombarblank xpos 0 ypos r70 tall 70 wide f0 visible 1 }";
const KEY: &str =
    "SwitchCamModeKeyLabel { ControlName CExLabel xpos 5 ypos 10 wide 60 tall 20 visible 1";
const SD: Screen = Screen::new(640, 480);

fn bar(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::Spectator).unwrap()
}

fn hidden(files: &HudFiles, key: &str) -> Option<HiddenBy> {
    bar(files)
        .layout(SD)
        .get(&PartKey::new(key))
        .unwrap()
        .hidden()
}

fn with(from: &str, to: &str) -> HudFiles {
    let text = SPECTATOR.replace(from, to);
    assert_ne!(text, SPECTATOR, "{from}");
    stock_files_with(&[(FILE, Some(&text))])
}

#[test]
fn the_table_has_the_game_s_23_cases() {
    let rule = SceneRule::get(Scene::Spectator);
    assert_eq!(rule.code_toggled().len(), 23);
    for t in rule.code_toggled() {
        assert!(t.shows_when().starts_with("Shows "), "{}", t.panel());
        assert!(t.shows_for().is_none(), "{}", t.panel());
    }
    let shown = |name: &str| rule.toggled(name).map(|t| t.shown_in_sample());
    for name in [
        "Spectator",
        "topbar",
        "bottombarblank",
        "ReinforcementsLabel",
        "SwitchCamModeKeyLabel",
        "SwitchCamModeLabel",
        "CycleTargetFwdKeyLabel",
        "CycleTargetFwdLabel",
        "CycleTargetRevKeyLabel",
        "CycleTargetRevLabel",
    ] {
        assert_eq!(shown(name), Some(true), "{name}");
    }
    for name in ["BuyBackLabel", "itempanel"] {
        assert_eq!(shown(name), Some(false), "{name}");
    }
    // Not toggled: the file's own visibility stands.
    for name in [
        "BottomBar",
        "MapLabel",
        "ClassOrTeamLabel",
        "TipLabel",
        "spectator_extras",
    ] {
        assert!(rule.toggled(name).is_none(), "{name}");
    }
}

#[test]
fn the_game_hides_the_buy_back_line_and_the_item_panel_and_shows_the_rest() {
    let files = stock_files();
    for key in ["Spectator/BuyBackLabel", "Spectator/itempanel"] {
        assert_eq!(hidden(&files, key), Some(HiddenBy::Game), "{key}");
    }
    for key in [
        "Spectator/topbar",
        "Spectator/ReinforcementsLabel",
        "Spectator/TipLabel",
        "Spectator/BottomBar",
    ] {
        assert_eq!(hidden(&files, key), None, "{key}");
    }
}

#[test]
fn a_bar_the_file_marks_not_visible_is_still_drawn_but_a_zero_height_hides_it() {
    // The game turns both bars on after the file loads, so only their size can hide them.
    assert_eq!(
        hidden(
            &with(TOP, &TOP.replace("visible 1", "visible 0")),
            "Spectator/topbar"
        ),
        None
    );
    assert_eq!(
        hidden(
            &with(TOP, &TOP.replace("tall 84", "tall 0")),
            "Spectator/topbar"
        ),
        Some(HiddenBy::SizeZero)
    );
}

#[test]
fn the_bottom_bar_is_the_game_s_size_and_cannot_be_hidden_or_moved() {
    let m = bar(&stock_files());
    let key = PartKey::new("Spectator/bottombarblank");
    let caps = m.capabilities(&key).unwrap();
    assert!(!caps.hide() && !caps.move_x() && !caps.move_y());
    assert!(!caps.resize_wide() && !caps.resize_tall());
    assert_eq!(m.hide_part(&key, 1), Err(EditRefused::NotOffered));
    assert_eq!(m.move_part(&key, 4, 0, SD, 1), Err(EditRefused::NotOffered));
    // A zero tall and a zero wide do not hide it: the game fits it from its ypos to the bottom.
    let flat = BOTTOM
        .replace("tall 70 wide f0", "tall 0 wide 0")
        .replace("ypos r70", "ypos r55");
    let files = with(BOTTOM, &flat);
    let l = bar(&files).layout(SD);
    let b = l.get(&key).unwrap();
    assert_eq!((b.x(), b.y(), b.wide(), b.tall()), (0, 425, 640, 55));
    assert_eq!(b.hidden(), None);
    // A ypos at the screen bottom leaves it no area.
    let gone = with(BOTTOM, &BOTTOM.replace("ypos r70", "ypos 9999"));
    assert!(bar(&gone).layout(SD).get(&key).unwrap().hidden().is_some());
}

#[test]
fn the_top_bar_hides_by_tall_alone_and_owns_its_width_and_place() {
    let m = bar(&stock_files());
    let key = PartKey::new("Spectator/topbar");
    let caps = m.capabilities(&key).unwrap();
    assert!(caps.hide() && caps.resize_tall());
    assert!(!caps.move_x() && !caps.move_y() && !caps.resize_wide());
    let keys: Vec<String> = m
        .hide_part(&key, 1)
        .unwrap()
        .iter()
        .map(|c| match c.target() {
            ChangeTarget::Set(s) => s.key().to_owned(),
            _ => panic!("not a set"),
        })
        .collect();
    assert_eq!(keys, ["tall"]);
    // The game resets the width and the place, so a zero wide or a far-off box does not hide it.
    let odd = TOP.replace(
        "xpos 0 ypos 0 tall 84 wide f0",
        "xpos 9999 ypos 9999 tall 84 wide 0",
    );
    let l = bar(&with(TOP, &odd)).layout(SD);
    let b = l.get(&key).unwrap();
    assert_eq!((b.x(), b.y(), b.wide(), b.tall()), (0, 0, 640, 84));
    assert_eq!(b.hidden(), None);
}

#[test]
fn a_camera_label_the_file_marks_not_visible_is_still_drawn() {
    let files = with(KEY, &KEY.replace("visible 1", "visible 0"));
    assert_eq!(hidden(&files, "Spectator/SwitchCamModeKeyLabel"), None);
}
