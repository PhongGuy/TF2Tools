//! Edits made while the minimal HUD is on write the `_minmode` line where the part has one.

use crate::common;

use common::hud_stock::stock_files_with;
use tf2tools_core::{Change, ChangeTarget, HudModel, PartKey, Platform, Screen, SetKey};

const HEALTH_KEY: &str = "HudPlayerStatus/HudPlayerHealth";
const HP: &str = "resource/ui/hudplayerhealth.res";
const TIMER_FILE: &str = "resource/ui/hudobjectivetimepanel.res";
const TIMER_VALUE: &str = "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue";
const SD: Screen = Screen::new(640, 480);

fn model(overrides: &[(&str, Option<&str>)]) -> HudModel {
    HudModel::load(&stock_files_with(overrides), Platform::Linux).unwrap()
}

fn sets(changes: &[Change]) -> Vec<&SetKey> {
    (changes.iter())
        .filter_map(|c| match c.target() {
            ChangeTarget::Set(s) => Some(s),
            _ => None,
        })
        .collect()
}

fn text(m: &HudModel, path: &str) -> String {
    String::from_utf8(m.document(path).unwrap().as_bytes().to_vec()).unwrap()
}

#[test]
fn a_move_in_minimal_mode_leaves_the_plain_lines() {
    let m = model(&[]).with_minmode(true);
    let changes = m
        .move_part(&PartKey::new(HEALTH_KEY), 10, 0, SD, 1)
        .unwrap();
    let xs: Vec<&str> = (sets(&changes).into_iter())
        .map(SetKey::key)
        .filter(|k| k.to_ascii_lowercase().starts_with("xpos"))
        .collect();
    assert_eq!(xs, ["xpos_minmode"]);
    let edited = m.with_changes(&changes);
    assert_eq!(edited.dropped(), 0);
    let file = text(edited.model(), HP);
    assert!(file.contains("xpos_minmode 5"), "{file}");
    assert!(file.contains("xpos 0 [$WIN32]") && file.contains("xpos 32 [$X360]"));
    let normal = edited.model().with_minmode(false);
    let x = normal
        .part(&PartKey::new(HEALTH_KEY))
        .unwrap()
        .value("xpos")
        .unwrap();
    assert_eq!(x.text(), "0", "the normal HUD did not move");
}

#[test]
fn a_key_without_a_partner_is_written_plain_in_both_modes() {
    let m = model(&[]).with_minmode(true);
    let changes = m
        .set_key_text(&PartKey::new(HEALTH_KEY), "ypos", "r100", 1)
        .unwrap();
    assert_eq!(
        sets(&changes).iter().map(|s| s.key()).collect::<Vec<_>>(),
        ["ypos"]
    );
    let off = model(&[])
        .move_part(&PartKey::new(HEALTH_KEY), 10, 0, SD, 1)
        .unwrap();
    assert!(
        sets(&off).iter().all(|s| !s.key().ends_with("_minmode")),
        "with the switch off nothing changes"
    );
}

#[test]
fn a_minimal_line_in_an_if_block_is_written_in_place() {
    let both = r#""Resource/UI/HudObjectiveTimePanel.res" { TimePanelValue { ControlName CExLabel xpos 25 xpos_minmode 30 ypos 2 wide 60 tall 20 labelText 0:00 if_match { xpos 35 xpos_minmode 40 } } }"#;
    let m = model(&[(TIMER_FILE, Some(both))]).with_minmode(true);
    let changes = m
        .set_key_text(&PartKey::new(TIMER_VALUE), "xpos", "50", 1)
        .unwrap();
    let s = sets(&changes);
    assert!(!s.is_empty() && s.iter().all(|s| s.key() == "xpos_minmode"));
    assert!(
        s.iter()
            .any(|s| s.block().last().is_some_and(|b| b == "if_match"))
    );
    let edited = m.with_changes(&changes);
    let won = edited
        .model()
        .part(&PartKey::new(TIMER_VALUE))
        .unwrap()
        .value("xpos")
        .unwrap();
    assert_eq!((won.key(), won.text()), ("xpos_minmode", "50"));
    let normal = edited.model().with_minmode(false);
    let won = normal
        .part(&PartKey::new(TIMER_VALUE))
        .unwrap()
        .value("xpos")
        .unwrap();
    assert_eq!(won.text(), "35");
}
