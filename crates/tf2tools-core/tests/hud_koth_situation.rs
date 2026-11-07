//! The King of the Hill switch: the game shows the KOTH timers there and the round timer elsewhere.

use crate::common;

use common::hud_stock::stock_files_with;
use tf2tools_core::{
    GameSituation, HiddenBy, HideTechnique, HudModel, PartKey, Platform, Scene, SceneRule, Screen,
    ShowsFor, SituationSwitch,
};

const SD: Screen = Screen::new(640, 480);
const KOTH: &str = "HudKothTimeStatus";
const ROUND: &str = "HudMatchStatus/ObjectiveStatusTimePanel";

fn hidden(s: &GameSituation, key: &str) -> Option<HiddenBy> {
    let m = HudModel::load(&stock_files_with(&[]), Platform::Linux).unwrap();
    m.layout_in(SD, s).get(&PartKey::new(key)).unwrap().hidden()
}

#[test]
fn the_round_timer_shows_by_default_and_the_koth_timer_on_koth() {
    let usual = GameSituation::default();
    assert!(!usual.is_on(SituationSwitch::Koth));
    assert_eq!(hidden(&usual, KOTH), Some(HiddenBy::Game));
    assert_ne!(hidden(&usual, ROUND), Some(HiddenBy::Game));
    assert_eq!(
        hidden(&usual, &format!("{KOTH}/BlueTimer")),
        Some(HiddenBy::Parent)
    );
    let koth = usual.with_switch(SituationSwitch::Koth, true);
    assert_eq!(hidden(&koth, KOTH), None);
    assert_eq!(hidden(&koth, ROUND), Some(HiddenBy::Game));
    assert_eq!(
        hidden(&koth, &format!("{KOTH}/BlueTimer/TimePanelValue")),
        None
    );
}

#[test]
fn the_toggles_say_when_and_keep_the_timers_hide() {
    let hud = SceneRule::get(Scene::Hud);
    let koth = hud.toggled("HudKothTimeStatus").unwrap();
    assert_eq!(koth.shows_for(), Some(ShowsFor::Mode("koth")));
    assert_eq!(koth.shows_when(), "Shows on King of the Hill maps");
    assert_eq!(koth.hide(), HideTechnique::OffScreenY);
    let round = hud.toggled("ObjectiveStatusTimePanel").unwrap();
    assert_eq!(round.shows_for(), Some(ShowsFor::Mode("not koth")));
    assert_eq!(round.hide(), HideTechnique::OffScreenY);
    assert!(ShowsFor::Mode("not koth").in_default());
    assert!(!ShowsFor::Mode("koth").in_default());
}
