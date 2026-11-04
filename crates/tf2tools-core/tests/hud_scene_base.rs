//! Scene files that are #base stubs: drawn from the merged tree, read-only.

mod common;

use common::hud_stock::{
    FREEZE_PANEL, HEALTH, MAINMENU, freeze_stub, stock_files, stock_files_with,
};
use tf2tools_core::{HudFiles, HudModel, PartKey, Platform, Scene, Screen};

const BASIC: &str = "resource/ui/freezepanel_basic.res";
const SD: Screen = Screen::new(640, 480);
const NAME: &str = "FreezePanel/FreezePanelBase/FreezeLabelKiller";

fn freeze(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::FreezeCam).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn a_stub_scene_file_shows_its_parts_from_the_merged_tree_read_only() {
    let m = freeze(&freeze_stub());
    assert!(m.problems().is_empty());
    assert_eq!(m.stub_files(), [BASIC]);
    let stock = freeze(&stock_files());
    assert_eq!(stock.parts().len(), m.parts().len());
    let part = m.part(&k(NAME)).expect("the label is drawn");
    assert!(part.is_from_base() && !part.is_readable() && !part.base_incomplete());
    assert_eq!(m.layout(SD).get(&k(NAME)), stock.layout(SD).get(&k(NAME)));
}

#[test]
fn a_file_with_its_own_root_and_a_base_line_stays_editable_and_round_trips() {
    let with_base = format!("#base \"nowhere.res\"\n{MAINMENU}");
    let files = stock_files_with(&[("resource/ui/mainmenuoverride.res", Some(&with_base))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    assert!(m.stub_files().is_empty());
    let friends = k("MainMenuOverride/FriendsContainer");
    assert!(m.part(&friends).unwrap().is_readable());
    // An edit writes the file with its #base line intact, and a reload of those bytes sees it.
    let changes = m.move_part(&friends, 10, 0, SD, 1).unwrap();
    let edited = m.with_changes(&changes);
    assert_eq!(
        (edited.applied().len(), edited.dropped()),
        (changes.len(), 0)
    );
    let doc = edited
        .model()
        .document("resource/ui/mainmenuoverride.res")
        .unwrap();
    let text = String::from_utf8(doc.as_bytes().to_vec()).unwrap();
    assert!(text.starts_with("#base \"nowhere.res\""));
    let again = stock_files_with(&[("resource/ui/mainmenuoverride.res", Some(&text))]);
    let reopened = HudModel::load_scene(&again, Platform::Linux, Scene::MainMenu).unwrap();
    assert_eq!(
        reopened.layout(SD).get(&friends).unwrap().x(),
        edited.model().layout(SD).get(&friends).unwrap().x()
    );
}

#[test]
fn a_stub_with_one_base_present_and_one_absent_merges_and_says_so() {
    let files = stock_files_with(&[
        (
            "resource/ui/freezepanel_basic_variant.res",
            Some(FREEZE_PANEL),
        ),
        (
            BASIC,
            Some("#base \"freezepanel_basic_variant.res\"\n#base \"gone.res\"\n"),
        ),
    ]);
    let m = freeze(&files);
    let part = m.part(&k(NAME)).unwrap();
    assert!(part.is_from_base() && part.base_incomplete());
}

#[test]
fn a_stub_whose_bases_are_all_missing_is_still_a_problem() {
    let files = stock_files_with(&[(BASIC, Some("#base \"nowhere.res\"\n"))]);
    let m = freeze(&files);
    assert_eq!(m.problems(), [BASIC]);
    assert!(m.stub_files().is_empty());
}

#[test]
fn only_the_parts_of_a_stub_go_read_only_in_the_hud_scene() {
    let files = stock_files_with(&[
        ("resource/ui/hudplayerhealth_variant.res", Some(HEALTH)),
        (
            "resource/ui/hudplayerhealth.res",
            Some("#base \"hudplayerhealth_variant.res\"\n"),
        ),
    ]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::Hud).unwrap();
    let health = m
        .part(&k(
            "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue",
        ))
        .unwrap();
    assert!(health.is_from_base() && !health.is_readable());
    let ammo = m.part(&k("HudWeaponAmmo")).unwrap();
    assert!(ammo.is_readable() && !ammo.is_from_base());
}

#[test]
fn a_change_aimed_at_a_stub_is_dropped_and_touches_nothing() {
    use tf2tools_core::{Change, SetKey};
    let m = freeze(&freeze_stub());
    let change = Change::set(
        1,
        SetKey::new(
            BASIC,
            &["Resource/UI/FreezePanel_Basic.res", "FreezePanelBase"],
            "xpos",
            None,
            Some("r270"),
            "r200",
        ),
    );
    let edited = m.with_changes(&[change]);
    assert_eq!((edited.dropped(), edited.applied().len()), (1, 0));
    assert!(edited.touched().is_empty());
    assert!(edited.model().document(BASIC).is_none());
}

#[test]
fn the_edit_builders_refuse_a_stub_part_and_a_scene_switch_keeps_the_mark() {
    use tf2tools_core::EditRefused;
    let m = freeze(&freeze_stub());
    assert_eq!(
        m.move_part(&k(NAME), 4, 0, SD, 1),
        Err(EditRefused::ReadOnly)
    );
    assert_eq!(m.hide_part(&k(NAME), 1), Err(EditRefused::ReadOnly));
    let other = HudModel::load_scene(&freeze_stub(), Platform::Linux, Scene::Hud)
        .unwrap()
        .with_scene(Scene::FreezeCam);
    assert!(other.part(&k(NAME)).unwrap().is_from_base());
    assert_eq!(other.stub_files(), [BASIC]);
}

#[test]
fn a_scoreboard_zeroed_in_a_base_variant_is_still_hidden_by_the_hud() {
    let zeroed = common::hud_stock::SCOREBOARD
        .replace("    wide 640\n    tall 448\n", "    wide 0\n    tall 0\n");
    let files = stock_files_with(&[
        ("resource/ui/scoreboard_variant.res", Some(&zeroed)),
        (
            "resource/ui/scoreboard.res",
            Some("#base \"scoreboard_variant.res\"\n"),
        ),
    ]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::Scoreboard).unwrap();
    assert!(m.hidden_by_hud().is_some());
    assert!(m.parts().iter().all(|p| !p.is_readable()));
}
