//! The KOTH timers each load the round timer's file: one part per child, if_match on, shared edits.

use crate::common;

use common::hud_stock::stock_files_with;
use tf2tools_core::{
    GameSituation, HiddenBy, HudModel, PartId, PartKey, Platform, Screen, SituationSwitch,
};

const SD: Screen = Screen::new(640, 480);
const KOTH_FILE: &str = "resource/ui/hudobjectivekothtimepanel.res";
const TIME: &str = "resource/ui/hudobjectivetimepanel.res";
const BLUE_VALUE: &str = "HudKothTimeStatus/BlueTimer/TimePanelValue";
const ROUND_VALUE: &str = "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue";

/// The stock time panel plus a background the game hides in the match HUD.
const TIME_WITH_BG: &str = r#""Resource/UI/HudObjectiveTimePanel.res" {
  TimePanelBG { ControlName ScalableImagePanel xpos 16 ypos 0 wide 78 tall 33 visible 1 image ../hud/objectives_timepanel_blue_bg scaleImage 1 if_match { visible 0 } }
  TimePanelValue { ControlName CExLabel xpos 25 ypos 2 wide 60 tall 20 labelText 0:00 if_match { xpos 35 } } }"#;

fn model(overrides: &[(&str, Option<&str>)]) -> HudModel {
    HudModel::load(&stock_files_with(overrides), Platform::Linux).unwrap()
}

#[test]
fn a_shared_child_is_one_part_read_from_both_files() {
    let m = model(&[]);
    let n = m
        .parts()
        .iter()
        .filter(|p| p.key().as_str().eq_ignore_ascii_case(BLUE_VALUE))
        .count();
    assert_eq!(
        n, 1,
        "the KOTH file's sub-block and the time panel's block are one label"
    );
    let part = m.part(&PartKey::new(BLUE_VALUE)).unwrap();
    let files: Vec<&str> = part.blocks().iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(files, [TIME, KOTH_FILE], "the timer's own file loads first");
    let x = part.value("xpos").unwrap();
    assert_eq!(
        (x.file(), x.text()),
        (KOTH_FILE, "15"),
        "the KOTH file's if_match line wins"
    );
    let label = part.value("labelText").unwrap();
    assert_eq!(label.file(), KOTH_FILE);
    assert_eq!(part.own_block().0, KOTH_FILE, "edits go to the KOTH file");
    assert!(part.is_readable());
}

#[test]
fn both_timers_get_the_time_panel_children_with_if_match_on() {
    let m = model(&[(TIME, Some(TIME_WITH_BG))]);
    // On a KOTH map, so the panel itself shows and only if_match can hide the background.
    let koth = GameSituation::default().with_switch(SituationSwitch::Koth, true);
    let l = m.layout_in(SD, &koth);
    for timer in ["BlueTimer", "RedTimer"] {
        let key = PartKey::new(format!("HudKothTimeStatus/{timer}/TimePanelBG"));
        assert!(m.part(&key).is_some(), "{timer}");
        assert_eq!(
            l.get(&key).unwrap().hidden(),
            Some(HiddenBy::Visible),
            "{timer}: if_match hides it"
        );
    }
    let round = m
        .parts()
        .iter()
        .filter(|p| p.key().as_str() == ROUND_VALUE)
        .count();
    assert_eq!(round, 1, "the round timer keeps its one label");
}

#[test]
fn an_edit_to_a_timer_child_changes_every_timer_that_reads_the_file() {
    let m = model(&[(TIME, Some(TIME_WITH_BG))]);
    let blue = PartKey::new("HudKothTimeStatus/BlueTimer/TimePanelBG");
    let changes = m.set_key_text(&blue, "wide", "60", 1).unwrap();
    let edited = m.with_changes(&changes);
    assert_eq!(edited.touched(), [TIME.to_owned()]);
    for key in [
        "HudKothTimeStatus/RedTimer/TimePanelBG",
        "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelBG",
    ] {
        let wide = edited
            .model()
            .part(&PartKey::new(key))
            .unwrap()
            .value("wide")
            .unwrap();
        assert_eq!(wide.text(), "60", "{key}");
    }
}

// Pins a regression rather than failing first: before each timer read the time panel file, the KOTH file already gave the child its owner;
// a merge that passes no rule to `push_controls` drops it.
#[test]
fn merged_children_keep_their_owner() {
    let m = model(&[]);
    let part = m.part(&PartKey::new(BLUE_VALUE)).unwrap();
    assert_eq!(part.owner(), Some(PartId::KothTimer));
}
