//! The HUD parts the game keeps on screen at round end: health, ammo, the round timer (the
//! KOTH clocks on a KOTH map) and the chat, with the crosshair; the point icons go.

use crate::common;

use common::hud_stock::{AMMO, BASECHAT, HEALTH, stock_files_with, with_blocks};
use tf2tools_core::{
    GameSituation, HiddenBy, HudModel, PartId, PartKey, Platform, Scene, SceneRule, Screen,
    SituationSwitch,
};

const SD: Screen = Screen::new(640, 480);
const HD: Screen = Screen::new(1920, 1080);
const HEALTH_PANEL: &str = "HudPlayerStatus/HudPlayerHealth";
const TIMER: &str = "HudMatchStatus/ObjectiveStatusTimePanel";

// The stock files with a chat history, and a bleed icon and a no-clip count the files show,
// both of which the game's code turns off for a clip weapon with no effects on the player.
fn win() -> HudModel {
    let health = with_blocks(
        HEALTH,
        "  PlayerStatusBleedImage { ControlName CTFImagePanel xpos 85 ypos 0 wide 20 tall 20 visible 1 }\n",
    );
    let ammo = AMMO.replacen(
        "} }",
        "} AmmoNoClip { ControlName CExLabel xpos 0 ypos 5 wide 55 tall 40 visible 1 labelText %Ammo% } }",
        1,
    );
    assert_ne!(ammo, AMMO, "the stock ammo file should end its root block");
    let chat = with_blocks(
        BASECHAT,
        "  HudChatHistory { ControlName RichText xpos 10 ypos 17 wide 260 tall 75 visible 1 font ChatFont }\n",
    );
    let files = stock_files_with(&[
        ("resource/ui/basechat.res", Some(chat.as_str())),
        ("resource/ui/hudplayerhealth.res", Some(health.as_str())),
        ("resource/ui/hudammoweapons.res", Some(ammo.as_str())),
    ]);
    HudModel::load_scene(&files, Platform::Linux, Scene::WinPanel).unwrap()
}

fn hidden_in(m: &HudModel, key: &str, s: &GameSituation) -> Option<HiddenBy> {
    let l = m.layout_in(SD, s);
    l.get(&PartKey::new(key))
        .unwrap_or_else(|| panic!("{key} is not in the win panel scene"))
        .hidden()
}

fn hidden(m: &HudModel, key: &str) -> Option<HiddenBy> {
    hidden_in(m, key, &GameSituation::default())
}

#[test]
fn the_win_panel_keeps_health_ammo_the_timer_and_chat() {
    let rule = SceneRule::get(Scene::WinPanel);
    let ids: Vec<PartId> = rule.rules().iter().map(|r| r.id()).collect();
    assert_eq!(
        ids,
        [
            PartId::WinPanel,
            PartId::Health,
            PartId::Ammo,
            PartId::RoundTimer,
            PartId::Chat,
            PartId::KothTimer,
        ]
    );
    // The kept parts' files never count as the win panel's problems.
    assert_eq!(
        rule.problem_files(),
        ["scripts/hudlayout.res", "resource/ui/winpanel.res"]
    );
    let m = win();
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    for key in [
        HEALTH_PANEL,
        "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue",
        "HudWeaponAmmo",
        "HudWeaponAmmo/AmmoInClip",
        TIMER,
        "HudChat",
        "WinPanel",
    ] {
        assert_eq!(hidden(&m, key), None, "{key}");
    }
    // The chat's sample lines draw as they do over the HUD; nobody is typing.
    assert!(m.chat_sample(HD).is_some());
    assert_eq!(hidden(&m, "HudChat/ChatInputLine"), Some(HiddenBy::Game));
}

#[test]
fn the_kept_parts_follow_the_game_s_code_as_they_do_over_the_hud() {
    let m = win();
    // A clip weapon hides the no-clip count, and no effect is on the player.
    assert_eq!(hidden(&m, "HudWeaponAmmo/AmmoNoClip"), Some(HiddenBy::Game));
    let bleed = format!("{HEALTH_PANEL}/PlayerStatusBleedImage");
    assert_eq!(hidden(&m, &bleed), Some(HiddenBy::Game));
    // On a King of the Hill map the KOTH clocks take the round timer's place.
    let koth = GameSituation::default().with_switch(SituationSwitch::Koth, true);
    assert_eq!(hidden(&m, "HudKothTimeStatus"), Some(HiddenBy::Game));
    assert_eq!(hidden_in(&m, "HudKothTimeStatus", &koth), None);
    assert!(hidden_in(&m, TIMER, &koth).is_some());
}

#[test]
fn the_crosshair_stays_and_the_rest_of_the_hud_goes() {
    // The scene keeps the crosshair, so it does not hide the HUD's.
    assert!(!SceneRule::get(Scene::WinPanel).hides_hud());
    let m = win();
    // The point icons and every HUD part the scene does not keep are not drawn.
    for key in [
        "HudPlayerStatus/HudPlayerClass",
        "HudMedicCharge",
        "HudItemEffectMeter",
        "HudDeathNotice",
        "CMainTargetID",
    ] {
        assert!(m.part(&PartKey::new(key)).is_none(), "{key}");
    }
    assert!(
        !m.parts()
            .iter()
            .any(|p| p.name().to_ascii_lowercase().contains("controlpoint")),
        "no control point icons"
    );
}
