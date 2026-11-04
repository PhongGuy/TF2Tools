//! The made-up killer the freeze cam previews, and the labels it fills.

mod common;

use common::hud_stock::{FREEZE_PANEL, stock_files_with};
use tf2tools_core::{HudModel, PartKey, Platform, SampleSet, Scene, SceneRule, Team, drawn_image};

#[test]
fn the_freeze_cam_previews_one_made_up_killer() {
    assert_eq!(SceneRule::get(Scene::FreezeCam).sample(), SampleSet::Killed);
    let killer = SampleSet::Killed.killer().unwrap();
    assert_eq!(
        (killer.name(), killer.team(), killer.health()),
        ("Captain Crumpet", Team::Blue, 187)
    );
    assert!(SampleSet::Empty.killer().is_none() && SampleSet::Casual.killer().is_none());
    // No scoreboard rows, no match score.
    for team in [Team::Red, Team::Blue] {
        assert!(SampleSet::Killed.players(team).is_empty());
        assert_eq!(SampleSet::Killed.team_score(team), None);
    }
    assert_eq!(SampleSet::Killed.selected(), None);
}

#[test]
fn the_labels_fill_from_the_killer_not_the_live_defaults() {
    let fill = |text| SampleSet::Killed.fill(text);
    assert_eq!(fill("%killername%"), "Captain Crumpet");
    assert_eq!(fill("%KillerName%"), "Captain Crumpet");
    // The killer's health, not the HUD's default 150.
    assert_eq!(fill("%Health%"), "187");
    assert_eq!(fill("%text%"), "[F5] Save this moment!");
    assert_eq!(fill("%nemesisname%%nemesisadvice%"), "");
    assert_eq!(fill("Health left: %Health%"), "Health left: 187");
    // The other samples are untouched.
    assert_eq!(SampleSet::Casual.fill("%Health%"), "150");
    assert_eq!(SampleSet::Empty.fill("%killername%"), "");
}

const BG: &str = "FreezePanel/FreezePanelBase/FreezePanelBG";

fn bg_image(panel: &str, team: Team) -> Option<String> {
    let files = stock_files_with(&[("resource/ui/freezepanel_basic.res", Some(panel))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::FreezeCam).unwrap();
    drawn_image(m.part(&PartKey::new(BG)).unwrap(), team).map(str::to_owned)
}

#[test]
fn the_panel_background_is_the_killers_team_whatever_the_file_says() {
    // The killer is on the other team: BLU for a RED player, RED for a BLU one.
    assert_eq!(
        bg_image(FREEZE_PANEL, Team::Red).as_deref(),
        Some("../hud/color_panel_blu")
    );
    assert_eq!(
        bg_image(FREEZE_PANEL, Team::Blue).as_deref(),
        Some("../hud/color_panel_red")
    );
    // A HUD that restyles `image` or adds `teambg_*` keys still draws the team panel.
    let restyled = FREEZE_PANEL.replacen(
        "image ../hud/color_panel_blu src_corner_height",
        "image ../hud/my_panel teambg_2 ../hud/other teambg_3 ../hud/other src_corner_height",
        1,
    );
    assert_ne!(restyled, FREEZE_PANEL);
    assert_eq!(
        bg_image(&restyled, Team::Red).as_deref(),
        Some("../hud/color_panel_blu")
    );
    assert_eq!(
        bg_image(&restyled, Team::Blue).as_deref(),
        Some("../hud/color_panel_red")
    );
}

#[test]
fn another_panel_keeps_the_file_image() {
    let other = FREEZE_PANEL.replace("FreezePanelBG {", "FreezePanelBG2 {");
    let files = stock_files_with(&[("resource/ui/freezepanel_basic.res", Some(&other))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::FreezeCam).unwrap();
    let part = m
        .part(&PartKey::new("FreezePanel/FreezePanelBase/FreezePanelBG2"))
        .unwrap();
    assert_eq!(drawn_image(part, Team::Red), Some("../hud/color_panel_blu"));
}
