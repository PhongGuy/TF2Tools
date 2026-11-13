//! Where the spectator preview follows the game on a default server: the camera hint under
//! mp_forcecamera 1, the HUD parts kept while dead, Minimal HUD and the target ID's width.

use crate::common;

use common::hud_stock::{hudlayout_with_killfeed, stock_files_with, target_id_files, with_blocks};
use tf2tools_core::{
    GameSituation, HiddenBy, HudModel, PartId, PartKey, Platform, Scene, SceneRule, Screen,
    SituationSwitch,
};

const SD: Screen = Screen::new(640, 480);
const HD: Screen = Screen::new(1920, 1080);
const CHAT_FILE: &str = "resource/ui/basechat.res";
const CHAT: &str = r##""Resource/UI/BaseChat.res"
{
  HudChat { ControlName EditablePanel fieldName HudChat visible 1 enabled 1 xpos 10 ypos 275 wide 280 tall 120 }
  ChatInputLine { ControlName EditablePanel fieldName ChatInputLine visible 1 enabled 1 xpos 10 ypos 395 wide 260 tall 2 }
  ChatFiltersButton { ControlName Button fieldName ChatFiltersButton xpos 225 ypos 2 wide 45 tall 15 visible 1 }
  HudChatHistory { ControlName RichText fieldName HudChatHistory xpos 10 ypos 17 wide 260 tall 75 visible 1 font ChatFont }
}
"##;

fn spectating() -> HudModel {
    let layout = with_blocks(
        &hudlayout_with_killfeed(
            "  HudDeathNotice { fieldName HudDeathNotice xpos r640 ypos 18 wide 628 tall 468 \
             LineHeight 16 LineSpacing 4 TextFont Default }\n",
        ),
        "  HudChat { fieldName HudChat xpos 10 ypos 100 wide 320 tall 120 }\n",
    );
    let files = stock_files_with(&[
        ("scripts/hudlayout.res", Some(layout.as_str())),
        (CHAT_FILE, Some(CHAT)),
    ]);
    HudModel::load_scene(&files, Platform::Linux, Scene::Spectator).unwrap()
}

fn hidden_in(m: &HudModel, key: &str, s: &GameSituation) -> Option<HiddenBy> {
    m.layout_in(SD, s)
        .get(&PartKey::new(key))
        .unwrap_or_else(|| panic!("{key} is not in the spectator scene"))
        .hidden()
}

fn hidden(m: &HudModel, key: &str) -> Option<HiddenBy> {
    hidden_in(m, key, &GameSituation::default())
}

#[test]
fn the_default_server_forces_the_camera_so_its_hint_is_hidden() {
    let m = spectating();
    for key in [
        "Spectator/SwitchCamModeKeyLabel",
        "Spectator/SwitchCamModeLabel",
    ] {
        assert_eq!(hidden(&m, key), Some(HiddenBy::Game), "{key}");
    }
    // Picking who to watch is still allowed.
    for key in [
        "Spectator/CycleTargetFwdKeyLabel",
        "Spectator/CycleTargetRevLabel",
    ] {
        assert_eq!(hidden(&m, key), None, "{key}");
    }
}

#[test]
fn the_dead_player_still_sees_the_killfeed_chat_and_koth_panel() {
    let rule = SceneRule::get(Scene::Spectator);
    let ids: Vec<PartId> = rule.rules().iter().map(|r| r.id()).collect();
    assert_eq!(
        ids,
        [
            PartId::Spectator,
            PartId::SpectatorTargetId,
            PartId::Killfeed,
            PartId::Chat,
            PartId::KothTimer
        ]
    );
    for file in [CHAT_FILE, "resource/ui/hudobjectivekothtimepanel.res"] {
        assert!(rule.files().contains(&file), "{file}");
        assert!(rule.extras().contains(&file), "{file}");
    }
    let m = spectating();
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    for key in ["HudDeathNotice", "HudChat", "HudChat/HudChatHistory"] {
        assert_eq!(hidden(&m, key), None, "{key}");
    }
    assert!(m.death_notices(HD).is_some());
    assert!(m.chat_sample(HD).is_some());
    for key in ["HudChat/ChatInputLine", "HudChat/ChatFiltersButton"] {
        assert_eq!(hidden(&m, key), Some(HiddenBy::Game), "{key}");
    }
    // The KOTH panel only on a KOTH map.
    let koth = GameSituation::default().with_switch(SituationSwitch::Koth, true);
    assert_eq!(hidden(&m, "HudKothTimeStatus"), Some(HiddenBy::Game));
    assert_eq!(hidden_in(&m, "HudKothTimeStatus", &koth), None);
    // The rest of the HUD stays away.
    assert!(m.part(&PartKey::new("HudPlayerHealth")).is_none());
}

#[test]
fn minimal_hud_takes_away_the_key_hints_and_the_bars() {
    let m = spectating().with_minmode(true);
    for key in [
        "Spectator/topbar",
        "Spectator/bottombarblank",
        "Spectator/CycleTargetFwdKeyLabel",
        "Spectator/CycleTargetFwdLabel",
        "Spectator/CycleTargetRevKeyLabel",
        "Spectator/CycleTargetRevLabel",
        "Spectator/SwitchCamModeKeyLabel",
        "Spectator/SwitchCamModeLabel",
    ] {
        assert_eq!(hidden(&m, key), Some(HiddenBy::Game), "{key}");
    }
    for key in ["Spectator/ReinforcementsLabel", "Spectator/TipLabel"] {
        assert_eq!(hidden(&m, key), None, "{key}");
    }
    // The full HUD keeps them.
    let full = spectating();
    for key in ["Spectator/topbar", "Spectator/CycleTargetFwdKeyLabel"] {
        assert_eq!(hidden(&full, key), None, "{key}");
    }
}

#[test]
fn the_target_id_is_sized_around_its_text() {
    let m = HudModel::load_scene(&target_id_files(), Platform::Linux, Scene::Spectator).unwrap();
    let l = m.layout(SD);
    // CSpectatorTargetID::PerformLayout: the health panel (32), XRES(5 + 10) and the text.
    let fitted = l.target_id_around(100);
    let at = |key: &str| {
        let b = fitted.get(&PartKey::new(key)).unwrap();
        (b.x(), b.y(), b.wide(), b.tall())
    };
    let wide = 32 + 15 + 100;
    let x = (640 - wide) / 2;
    assert_eq!(at("CSpectatorTargetID"), (x, 250, wide, 35));
    assert_eq!(
        at("CSpectatorTargetID/TargetIDBG_Spec_Red"),
        (x, 250, wide, 30)
    );
    // The labels and the health panel ride along with the panel.
    assert_eq!(at("CSpectatorTargetID/TargetNameLabel").0, x + 8 + 32);
    assert_eq!(at("CSpectatorTargetID/SpectatorGUIHealth").0, x + 3);
    // XRES follows the screen width.
    let hd = m.layout(HD).target_id_around(200);
    let b = hd.get(&PartKey::new("CSpectatorTargetID")).unwrap();
    assert_eq!(b.wide(), 72 + 45 + 200);
    // A scene without the panel is unchanged.
    let hud = m.with_scene(Scene::Hud).layout(SD);
    assert_eq!(hud.target_id_around(100).boxes().len(), hud.boxes().len());
}
