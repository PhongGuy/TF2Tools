//! The HUD parts the game keeps on screen during the freeze cam: the killfeed, the chat and
//! the KOTH panel, with the KOTH timers' digits hidden.

use crate::common;

use common::hud_stock::{hudlayout_with_killfeed, stock_files_with, with_blocks};
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

fn freeze() -> HudModel {
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
    HudModel::load_scene(&files, Platform::Linux, Scene::FreezeCam).unwrap()
}

fn hidden(m: &HudModel, key: &str) -> Option<HiddenBy> {
    hidden_in(m, key, &GameSituation::default())
}

fn koth() -> GameSituation {
    GameSituation::default().with_switch(SituationSwitch::Koth, true)
}

fn hidden_in(m: &HudModel, key: &str, s: &GameSituation) -> Option<HiddenBy> {
    let l = m.layout_in(SD, s);
    l.get(&PartKey::new(key))
        .unwrap_or_else(|| panic!("{key} is not in the freeze cam"))
        .hidden()
}

#[test]
fn the_freeze_cam_keeps_the_killfeed_chat_and_koth_panel() {
    let rule = SceneRule::get(Scene::FreezeCam);
    let ids: Vec<PartId> = rule.rules().iter().map(|r| r.id()).collect();
    assert_eq!(
        ids,
        [
            PartId::FreezeCam,
            PartId::Killfeed,
            PartId::Chat,
            PartId::KothTimer
        ]
    );
    // Their files never count as the freeze cam's problems.
    for file in [CHAT_FILE, "resource/ui/hudobjectivekothtimepanel.res"] {
        assert!(rule.files().contains(&file), "{file}");
        assert!(rule.extras().contains(&file), "{file}");
    }
    let m = freeze();
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    for key in ["HudDeathNotice", "HudChat", "HudChat/HudChatHistory"] {
        assert_eq!(hidden(&m, key), None, "{key}");
    }
    // The sample lines draw as they do over the HUD.
    assert!(m.death_notices(HD).is_some());
    assert!(m.chat_sample(HD).is_some());
    // Nobody types during the sample, so the input line and filters stay hidden.
    for key in ["HudChat/ChatInputLine", "HudChat/ChatFiltersButton"] {
        assert_eq!(hidden(&m, key), Some(HiddenBy::Game), "{key}");
    }
    // The freeze panel itself is unchanged.
    assert_eq!(hidden(&m, "FreezePanel"), None);
}

#[test]
fn the_koth_panel_shows_only_on_a_koth_map() {
    let m = freeze();
    // Off a King of the Hill map the game has no KOTH panel at all.
    assert_eq!(hidden(&m, "HudKothTimeStatus"), Some(HiddenBy::Game));
    assert_eq!(hidden_in(&m, "HudKothTimeStatus", &koth()), None);
}

#[test]
fn the_koth_timers_hide_their_digits_during_the_freeze_cam() {
    let m = freeze();
    for timer in ["BlueTimer", "RedTimer"] {
        let value = format!("HudKothTimeStatus/{timer}/TimePanelValue");
        assert_eq!(
            hidden_in(&m, &value, &koth()),
            Some(HiddenBy::Game),
            "{value}"
        );
        let panel = format!("HudKothTimeStatus/{timer}");
        assert_eq!(hidden_in(&m, &panel, &koth()), None);
    }
    // Only the freeze cam hides the digits; the HUD scene never toggles them.
    let hud = m.with_scene(Scene::Hud);
    let value = PartKey::new("HudKothTimeStatus/BlueTimer/TimePanelValue");
    assert!(hud.part(&value).unwrap().code_toggled().is_none());
}

#[test]
fn the_freeze_cam_still_hides_the_rest_of_the_hud() {
    let m = freeze();
    for key in ["HudPlayerHealth", "HudWeaponAmmo", "HudMedicCharge"] {
        assert!(m.part(&PartKey::new(key)).is_none(), "{key}");
    }
    assert!(SceneRule::get(Scene::FreezeCam).hides_hud());
}
