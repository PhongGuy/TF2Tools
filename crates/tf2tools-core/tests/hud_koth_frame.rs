//! The round timer and the KOTH clocks sit in the match HUD's timer frame; the clocks are
//! coloured as the game's code leaves them once each timer's highlight or dim event settles.

use crate::common;

use common::hud_anims::{ANIMS_PATH, MANIFEST, MANIFEST_PATH};
use common::hud_stock::{KOTH_PANEL, stock_files_with};
use tf2tools_core::{
    GameSituation, HiddenBy, HudModel, PartKey, Platform, Rect, Rgba, Screen, SituationSwitch,
};

const SD: Screen = Screen::new(640, 480);
const FRAME: &str = "HudMatchStatus/BGFrame";
const COUNTER: &str = "HudMatchStatus/RoundCounter";
const PICTURE: &str = "HudMatchStatus/RoundCounter/Background";
const MATCH: &str = "resource/ui/hudmatchstatus.res";
const COUNTER_FILE: &str = "resource/ui/hudroundcounter.res";
const KOTH_FILE: &str = "resource/ui/hudobjectivekothtimepanel.res";
const BLUE_VALUE: &str = "HudKothTimeStatus/BlueTimer/TimePanelValue";
const RED_VALUE: &str = "HudKothTimeStatus/RedTimer/TimePanelValue";

/// The stock timer events' shape: the highlight blinks and ends bright, the dim fades.
const TIMER_ANIMS: &str = r#"event ActiveTimerHighlight
{
    Animate TimePanelValue FgColor "TanLight" Linear 0.1 0.1
    Animate TimePanelValue FgColor "142 134 115 255" Linear 0.3 0.1
    Animate TimePanelValue FgColor "TanLight" Linear 1.3 0.3
}
event ActiveTimerDim
{
    Animate TimePanelValue FgColor "142 134 115 255" Linear 0.0 0.4
}
"#;

/// A round counter drawn as two team bars over a dark backing, its picture sized away.
const BARS_COUNTER: &str = r#""Resource/UI/HudRoundCounter.res"
{
  RoundCounter { xpos cs-0.5 ypos -2 zpos 2 wide 300 tall 25 proportionaltoparent 1 }
  Background { ControlName ImagePanel xpos cs-0.5 ypos 0 wide 115 tall 0 image ../hud/comp_round_timer proportionaltoparent 1 }
  BlueTeamBackground { ControlName ImagePanel xpos c-25 ypos 0 wide 25 tall 10 fillcolor HUDBlueTeamSolid proportionaltoparent 1 }
  RedTeamBackground { ControlName ImagePanel xpos c0 ypos 0 wide 25 tall 10 fillcolor HUDRedTeamSolid proportionaltoparent 1 }
  TimerBG { ControlName ImagePanel xpos c-25 ypos 10 wide 50 tall 15 fillcolor "0 0 0 120" proportionaltoparent 1 }
}
"#;

fn koth() -> GameSituation {
    GameSituation::default().with_switch(SituationSwitch::Koth, true)
}

fn model(overrides: &[(&str, Option<&str>)]) -> HudModel {
    HudModel::load(&stock_files_with(overrides), Platform::Linux).unwrap()
}

fn hidden(m: &HudModel, s: &GameSituation, key: &str) -> Option<HiddenBy> {
    let layout = m.layout_in(SD, s);
    layout.get(&PartKey::new(key)).expect(key).hidden()
}

/// The KOTH file with each clock label naming its own team colour, as some HUDs write it.
fn team_coloured_clocks() -> String {
    let label = "labelText 0:00 textAlignment center";
    KOTH_PANEL
        .replacen(label, &format!("{label} fgcolor HUDBlueTeamSolid"), 1)
        .replacen(
            &format!("{label}\n"),
            &format!("{label} fgcolor HUDRedTeamSolid\n"),
            1,
        )
}

fn stock_match() -> String {
    let files = common::hud_stock::stock();
    files.into_iter().find(|(p, _)| p == MATCH).unwrap().1
}

fn colour(m: &HudModel, key: &str) -> Rgba {
    m.drawn_text_colour(m.part(&PartKey::new(key)).expect(key))
}

#[test]
fn the_frame_shows_around_the_round_timer_and_the_koth_clocks() {
    // The match HUD shows the round counter whenever it is on, KOTH or not (in game on
    // cp_dustbowl and koth_harvest_final, 2026-11-13).
    let m = model(&[]);
    for situation in [GameSituation::default(), koth()] {
        for key in [FRAME, COUNTER, PICTURE] {
            assert_eq!(hidden(&m, &situation, key), None, "{key}");
        }
    }
}

#[test]
fn the_round_timer_draws_over_the_backing_the_counters_file_raises() {
    // The match file sorts RoundCounter at zpos 1 under the timer's 2; the counter's own file
    // raises it to 2 later, which moves it past no sibling of equal zpos.
    let m = model(&[]);
    let layout = m.layout_in(SD, &GameSituation::default());
    let rank = |key: &str| layout.paint_rank(&PartKey::new(key)).expect(key);
    let timer = "HudMatchStatus/ObjectiveStatusTimePanel";
    assert!(rank(timer) > rank(PICTURE));
    assert!(rank(COUNTER) > rank(FRAME));
}

#[test]
fn the_frame_is_placed_from_the_huds_own_files() {
    // The frame sized away, as some HUDs drop it.
    let matched = stock_match().replace("wide 365 tall 28", "wide 0 tall 0");
    let m = model(&[(MATCH, Some(&matched)), (COUNTER_FILE, Some(BARS_COUNTER))]);
    let layout = m.layout_in(SD, &koth());
    let rect = |key: &str| layout.get(&PartKey::new(key)).expect(key).rect();
    // The counter's own file wins over the match file's 100-tall block.
    assert_eq!(rect(COUNTER), Rect::new(170, -2, 300, 25));
    let blue = format!("{COUNTER}/BlueTeamBackground");
    let red = format!("{COUNTER}/RedTeamBackground");
    let backing = format!("{COUNTER}/TimerBG");
    assert_eq!(rect(&blue), Rect::new(295, -2, 25, 10));
    assert_eq!(rect(&red), Rect::new(320, -2, 25, 10));
    assert_eq!(rect(&backing), Rect::new(295, 8, 50, 15));
    for key in [&blue, &red, &backing] {
        assert_eq!(hidden(&m, &koth(), key), None, "{key}");
    }
    assert_eq!(hidden(&m, &koth(), PICTURE), Some(HiddenBy::SizeZero));
    assert_eq!(hidden(&m, &koth(), FRAME), Some(HiddenBy::SizeZero));
}

#[test]
fn a_frame_the_hud_turns_off_stays_off_on_koth() {
    let matched = stock_match().replace("if_match { visible 1 }", "");
    let m = model(&[(MATCH, Some(&matched))]);
    assert_eq!(hidden(&m, &koth(), FRAME), Some(HiddenBy::Visible));
    assert_eq!(hidden(&m, &koth(), COUNTER), None);
}

#[test]
fn the_clocks_take_the_colour_the_timer_events_leave() {
    let clocks = team_coloured_clocks();
    let m = model(&[
        (KOTH_FILE, Some(&clocks)),
        (MANIFEST_PATH, Some(MANIFEST)),
        (ANIMS_PATH, Some(TIMER_ANIMS)),
    ]);
    let file = |key: &str| m.text_colour(m.part(&PartKey::new(key)).unwrap());
    assert_eq!(file(BLUE_VALUE), Rgba::new(104, 124, 155, 255));
    // BLU holds the point in the sample, so its clock ends highlighted and RED's dimmed.
    assert_eq!(colour(&m, BLUE_VALUE), Rgba::new(235, 226, 202, 255));
    assert_eq!(colour(&m, RED_VALUE), Rgba::new(142, 134, 115, 255));
}

#[test]
fn a_hud_that_recolours_the_timer_events_is_drawn_in_its_colour() {
    let anims = TIMER_ANIMS.replace(
        r#""TanLight" Linear 1.3 0.3"#,
        r#""HUDRedTeamSolid" Linear 1.3 0.3"#,
    );
    let m = model(&[(MANIFEST_PATH, Some(MANIFEST)), (ANIMS_PATH, Some(&anims))]);
    assert_eq!(colour(&m, BLUE_VALUE), Rgba::new(180, 92, 77, 255));
}

#[test]
fn without_the_timer_events_the_clocks_keep_their_files_colour() {
    let clocks = team_coloured_clocks();
    let m = model(&[(KOTH_FILE, Some(&clocks))]);
    assert_eq!(colour(&m, BLUE_VALUE), Rgba::new(104, 124, 155, 255));
    assert_eq!(colour(&m, RED_VALUE), Rgba::new(180, 92, 77, 255));
    // Only the KOTH clocks take a code colour.
    let round = "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue";
    let part = m.part(&PartKey::new(round)).unwrap();
    assert_eq!(m.drawn_text_colour(part), m.text_colour(part));
}
