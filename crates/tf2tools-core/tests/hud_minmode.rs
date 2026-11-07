//! Minimal-HUD mode: a `_minmode` key replaces its bare key in its own layer when the flag is on.

use crate::common;

use common::hud_stock::{HEALTH, HUDLAYOUT, KOTH_PANEL, stock_files_with};
use tf2tools_core::{HiddenBy, HudModel, PartKey, Platform, Screen};

const HEALTH_KEY: &str = "HudPlayerStatus/HudPlayerHealth";
const METER: &str = "HudItemEffectMeter";
const METER_FILE: &str = "resource/ui/huditemeffectmeter.res";
const TIMER_FILE: &str = "resource/ui/hudobjectivetimepanel.res";
const TIMER_VALUE: &str = "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue";
const SD: Screen = Screen::new(640, 480);

fn model(overrides: &[(&str, Option<&str>)]) -> HudModel {
    HudModel::load(&stock_files_with(overrides), Platform::Linux).unwrap()
}

/// The winning line of `k` on part `key` as (its real key, its text).
fn won(m: &HudModel, key: &str, k: &str) -> Option<(String, String)> {
    let line = m.part(&PartKey::new(key))?.value(k)?;
    Some((line.key().to_owned(), line.text().to_owned()))
}

fn pair(k: &str, v: &str) -> Option<(String, String)> {
    Some((k.to_owned(), v.to_owned()))
}

fn meter_file(root: &str) -> String {
    format!(
        r#""Resource/UI/HudItemEffectMeter.res" {{ HudItemEffectMeter {{ {root} }} ItemEffectMeterLabel {{ ControlName CExLabel xpos 5 ypos 5 wide 90 tall 15 }} }}"#
    )
}

#[test]
fn off_by_default_a_minmode_key_is_a_key_of_its_own() {
    let m = model(&[]);
    assert!(!m.minmode());
    assert_eq!(won(&m, HEALTH_KEY, "xpos"), pair("xpos", "0"));
    assert_eq!(
        won(&m, HEALTH_KEY, "xpos_minmode"),
        pair("xpos_minmode", "-5")
    );
}

#[test]
fn on_the_minmode_line_wins_its_layer_whatever_its_order() {
    let m = model(&[]).with_minmode(true);
    assert!(m.minmode());
    assert_eq!(won(&m, HEALTH_KEY, "xpos"), pair("xpos_minmode", "-5"));
    assert_eq!(
        won(&m, HEALTH_KEY, "ypos"),
        pair("ypos", "r120"),
        "no partner, no change"
    );
    let before = meter_file("xpos_minmode r52 xpos r174 ypos r62");
    let m = model(&[(METER_FILE, Some(&before))]).with_minmode(true);
    assert_eq!(won(&m, METER, "xpos"), pair("xpos_minmode", "r52"));
    assert_eq!(won(&m, METER, "xpos_minmode"), pair("xpos_minmode", "r52"));
}

#[test]
fn hudlayout_is_read_the_same_way_and_a_later_plain_key_still_wins() {
    let layout = HUDLAYOUT.replacen(
        "HudItemEffectMeter { xpos r162 ypos r52",
        "HudItemEffectMeter { xpos r162 xpos_minmode r40 ypos r52",
        1,
    );
    let own_has_no_x = meter_file("ypos r62");
    let files = [
        ("scripts/hudlayout.res", Some(layout.as_str())),
        (METER_FILE, Some(own_has_no_x.as_str())),
    ];
    assert_eq!(won(&model(&files), METER, "xpos"), pair("xpos", "r162"));
    let on = model(&files).with_minmode(true);
    assert_eq!(won(&on, METER, "xpos"), pair("xpos_minmode", "r40"));
    // The meter file's own block is a later layer: its plain x beats hudlayout's minimal x.
    let own_has_x = meter_file("xpos r174 ypos r62");
    let files = [
        ("scripts/hudlayout.res", Some(layout.as_str())),
        (METER_FILE, Some(own_has_x.as_str())),
    ];
    assert_eq!(
        won(&model(&files).with_minmode(true), METER, "xpos"),
        pair("xpos", "r174")
    );
}

#[test]
fn an_if_block_is_its_own_layer() {
    let both = r#""Resource/UI/HudObjectiveTimePanel.res" { TimePanelValue { ControlName CExLabel xpos 25 xpos_minmode 30 ypos 2 wide 60 tall 20 labelText 0:00 if_match { xpos 35 xpos_minmode 40 } } }"#;
    assert_eq!(
        won(&model(&[(TIMER_FILE, Some(both))]), TIMER_VALUE, "xpos"),
        pair("xpos", "35")
    );
    let on = model(&[(TIMER_FILE, Some(both))]).with_minmode(true);
    assert_eq!(won(&on, TIMER_VALUE, "xpos"), pair("xpos_minmode", "40"));
    // With no minimal x in the active if_match, its plain x overrides the parent's minimal x.
    let plain_if = r#""Resource/UI/HudObjectiveTimePanel.res" { TimePanelValue { ControlName CExLabel xpos 25 xpos_minmode 30 ypos 2 wide 60 tall 20 labelText 0:00 if_match { xpos 35 } } }"#;
    let on = model(&[(TIMER_FILE, Some(plain_if))]).with_minmode(true);
    assert_eq!(won(&on, TIMER_VALUE, "xpos"), pair("xpos", "35"));
}

#[test]
fn lines_lists_the_minmode_entries_in_place_of_the_plain_ones() {
    let keys = |m: &HudModel| -> Vec<String> {
        (m.part(&PartKey::new(HEALTH_KEY))
            .unwrap()
            .lines("xpos")
            .into_iter())
        .map(|l| l.key().to_owned())
        .collect()
    };
    assert_eq!(
        keys(&model(&[])),
        ["xpos", "xpos"],
        "the $WIN32 and $X360 lines"
    );
    assert_eq!(keys(&model(&[]).with_minmode(true)), ["xpos_minmode"]);
}

#[test]
fn of_two_minimal_keys_the_last_wins_as_in_the_engine() {
    // CosmosHUD's hudrocketpack.res sets wide_minmode 20 then 48; the game draws 48.
    let dup = meter_file("wide 100 wide_minmode 20 wide_minmode 48 ypos r62");
    let on = model(&[(METER_FILE, Some(&dup))]).with_minmode(true);
    assert_eq!(won(&on, METER, "wide"), pair("wide_minmode", "48"));
    let lines: Vec<String> = (on
        .part(&PartKey::new(METER))
        .unwrap()
        .lines("wide")
        .into_iter())
    .map(|l| l.text().to_owned())
    .collect();
    assert_eq!(
        lines,
        ["48"],
        "the first minimal line was deleted by the second"
    );
    assert_eq!(
        won(&model(&[(METER_FILE, Some(&dup))]), METER, "wide"),
        pair("wide", "100")
    );
}

#[test]
fn a_rename_deletes_only_the_first_plain_duplicate() {
    // The engine deletes the first `xpos`, so the second plain one is found first.
    let dup = meter_file("xpos 1 xpos 2 xpos_minmode 3 ypos r62");
    let on = model(&[(METER_FILE, Some(&dup))]).with_minmode(true);
    assert_eq!(won(&on, METER, "xpos"), pair("xpos", "2"));
}

#[test]
fn a_minmode_suffix_must_end_the_name_and_ignores_case() {
    let odd = meter_file("xpos r174 xpos_minmode_lodef r10 YPOS_MINMODE r20 ypos r62");
    let on = model(&[(METER_FILE, Some(&odd))]).with_minmode(true);
    assert_eq!(won(&on, METER, "xpos"), pair("xpos", "r174"));
    assert_eq!(won(&on, METER, "ypos"), pair("YPOS_MINMODE", "r20"));
}

#[test]
fn visible_minmode_hides_a_part_only_in_minimal_mode() {
    let health = HEALTH.replacen("    tall 51\n", "    tall 51\n    visible_minmode 0\n", 1);
    let files = [("resource/ui/hudplayerhealth.res", Some(health.as_str()))];
    let key = PartKey::new(format!("{HEALTH_KEY}/PlayerStatusHealthImage"));
    let off = model(&files).layout(SD);
    assert_eq!(off.get(&key).unwrap().hidden(), None);
    let on = model(&files).with_minmode(true).layout(SD);
    assert_eq!(on.get(&key).unwrap().hidden(), Some(HiddenBy::Visible));
}

#[test]
fn a_nested_timer_child_reads_its_minimal_key() {
    // The KOTH timers' labels are merged from two files by the nested pass, which must read too.
    const KOTH_FILE: &str = "resource/ui/hudobjectivekothtimepanel.res";
    const BLUE_VALUE: &str = "HudKothTimeStatus/BlueTimer/TimePanelValue";
    let koth = KOTH_PANEL.replacen("xpos 15 ypos 12", "xpos 15 xpos_minmode 18 ypos 12", 1);
    let files = [(KOTH_FILE, Some(koth.as_str()))];
    assert_eq!(won(&model(&files), BLUE_VALUE, "xpos"), pair("xpos", "15"));
    let on = model(&files).with_minmode(true);
    assert_eq!(won(&on, BLUE_VALUE, "xpos"), pair("xpos_minmode", "18"));
}

#[test]
fn the_settings_row_of_two_minimal_keys_shows_the_one_the_engine_keeps() {
    let dup = meter_file("wide 100 wide_minmode 20 wide_minmode 48 ypos r62");
    let on = model(&[(METER_FILE, Some(&dup))]).with_minmode(true);
    assert_eq!(won(&on, METER, "wide_minmode"), pair("wide_minmode", "48"));
}
