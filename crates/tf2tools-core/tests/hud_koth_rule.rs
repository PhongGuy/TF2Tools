//! The KOTH timer: its rule, its parts and their boxes in a casual match.

mod common;

use common::hud_stock::stock_files_with;
use tf2tools_core::{HideTechnique, HudModel, PanelRule, PartId, PartKey, Platform, Scene, Screen};

const SD: Screen = Screen::new(640, 480);
const KOTH_FILE: &str = "resource/ui/hudobjectivekothtimepanel.res";
const TIME: &str = "resource/ui/hudobjectivetimepanel.res";
/// Like the game's time panel file: no TimePanelValue, so the KOTH file's labels stand once
/// each timer also reads this file.
const STOCK_TIME: &str = r#""Resource/UI/HudObjectiveTimePanel.res" { TimePanelBG { ControlName ScalableImagePanel xpos 16 ypos 0 wide 78 tall 33 image ../hud/objectives_timepanel_blue_bg if_match { visible 0 } } }"#;

fn stock() -> HudModel {
    HudModel::load(
        &stock_files_with(&[(TIME, Some(STOCK_TIME))]),
        Platform::Linux,
    )
    .unwrap()
}

#[test]
fn the_rule_names_the_game_panel_and_files() {
    let r = PanelRule::get(PartId::KothTimer);
    assert_eq!(
        (r.id(), r.label(), r.panel()),
        (PartId::KothTimer, "KOTH timer", "HudKothTimeStatus")
    );
    assert_eq!(r.shows_when(), "Shows on King of the Hill maps");
    assert_eq!(r.parent(), None);
    assert_eq!(
        r.geometry(),
        [("scripts/hudlayout.res", "HudKothTimeStatus")]
    );
    assert_eq!(r.children_file(), KOTH_FILE);
    assert!(r.ignores("visible") && !r.ignores("xpos"));
    assert!(r.has_children() && r.clickable() && !r.fills_screen());
    assert_eq!(r.conditions(), ["if_match"]);
    assert_eq!(r.hide(), HideTechnique::OffScreenY);
    assert!(PanelRule::all().iter().any(|p| p.id() == PartId::KothTimer));
    assert_eq!(Scene::for_file(KOTH_FILE), Some(Scene::Hud));
}

#[test]
fn the_timers_sit_in_the_panel_as_the_match_hud_places_them() {
    let l = stock().layout(SD);
    let b = |k: &str| {
        let b = l.get(&PartKey::new(k)).unwrap_or_else(|| panic!("{k}"));
        (b.x(), b.y(), b.wide(), b.tall())
    };
    assert_eq!(b("HudKothTimeStatus"), (220, 0, 200, 160));
    assert_eq!(
        b("HudKothTimeStatus/BlueTimer"),
        (270, 0, 100, 150),
        "cs-0.5 in if_match"
    );
    assert_eq!(b("HudKothTimeStatus/RedTimer"), (270, 0, 100, 150));
    assert_eq!(
        b("HudKothTimeStatus/BlueTimer/TimePanelValue"),
        (285, 12, 35, 10)
    );
    assert_eq!(
        b("HudKothTimeStatus/RedTimer/TimePanelValue"),
        (320, 12, 35, 10),
        "rs1-15"
    );
    assert!(
        l.get(&PartKey::new("HudKothTimeStatus/ActiveTimerBG"))
            .is_some()
    );
}

#[test]
fn a_timer_move_writes_the_koth_file() {
    let m = stock();
    let key = PartKey::new("HudKothTimeStatus/RedTimer/TimePanelValue");
    let changes = m.move_part(&key, 0, 2, SD, 1).unwrap();
    let edited = m.with_changes(&changes);
    assert_eq!(edited.dropped(), 0);
    assert_eq!(edited.touched(), [KOTH_FILE.to_owned()]);
    let y = edited.model().part(&key).unwrap().value("ypos").unwrap();
    assert_eq!((y.file(), y.text()), (KOTH_FILE, "14"));
}
