//! The chat part: basechat.res's HudChat over the HUD scene, its code-hidden children, its
//! sample lines in the chat scheme's colours and font, and the "Chat at the top" option.

use crate::common;

use common::hud_stock::{HUDLAYOUT, stock_files_with, with_blocks};
use tf2tools_core::{
    GameSituation, HiddenBy, HideTechnique, HudModel, OptionValue, PanelRule, PartId, PartKey,
    Platform, Rgba, SampleSet, Scene, SceneRule, Screen, ShowsFor, SituationSwitch,
};

const CHAT_FILE: &str = "resource/ui/basechat.res";
const SD: Screen = Screen::new(640, 480);
const HISTORY: &str = "HudChat/HudChatHistory";

/// A stock-shaped basechat.res, no Valve bytes: the panel, its input line, the filters button
/// and the history.
const CHAT: &str = r##""Resource/UI/BaseChat.res"
{
  HudChat { ControlName EditablePanel fieldName HudChat visible 1 enabled 1 xpos 10 ypos 275 wide 280 tall 120 PaintBackgroundType 2 }
  ChatInputLine { ControlName EditablePanel fieldName ChatInputLine visible 1 enabled 1 xpos 10 ypos 395 wide 260 tall 2 }
  ChatFiltersButton { ControlName Button fieldName ChatFiltersButton xpos 225 ypos 2 wide 45 tall 15 visible 1 labelText "#chat_filterbutton" }
  HudChatHistory { ControlName RichText fieldName HudChatHistory xpos 10 ypos 17 wide 260 tall 75 visible 1 labelText "" font ChatFont }
}
"##;

/// The stock files with `chat` as basechat.res, a hudlayout HudChat block that basechat's
/// own block must override, and `extra` files.
fn chat_with(chat: &str, extra: &[(&str, Option<&str>)]) -> HudModel {
    let layout = with_blocks(
        HUDLAYOUT,
        "  HudChat { fieldName HudChat xpos 10 ypos 100 wide 320 tall 120 }\n",
    );
    let mut files = vec![
        ("scripts/hudlayout.res", Some(layout.as_str())),
        (CHAT_FILE, Some(chat)),
    ];
    files.extend_from_slice(extra);
    HudModel::load(&stock_files_with(&files), Platform::Linux).unwrap()
}

#[test]
fn the_chat_is_a_hud_part_read_from_basechat() {
    let rule = PanelRule::get(PartId::Chat);
    assert_eq!(
        (rule.label(), rule.panel(), rule.parent()),
        ("Chat", "HudChat", None)
    );
    assert_eq!(rule.geometry(), [("scripts/hudlayout.res", "HudChat")]);
    assert_eq!(rule.children_file(), CHAT_FILE);
    assert!(rule.has_children() && rule.clickable());
    assert!(rule.ignores("visible"));
    assert!(rule.shows_when().starts_with("Shows "));
    assert_eq!(rule.hide(), HideTechnique::OffScreenX);
    let hud = SceneRule::get(Scene::Hud);
    // The weapon meters' extra files follow the last rule's file.
    let rule_files = &hud.files()[..hud.files().len() - hud.extras().len()];
    // The KOTH timer's rule and file follow the chat's.
    let koth = "resource/ui/hudobjectivekothtimepanel.res";
    assert!(rule_files.ends_with(&[CHAT_FILE, koth]), "{rule_files:?}");
    let ids: Vec<PartId> = hud.rules().iter().map(PanelRule::id).collect();
    assert!(ids.ends_with(&[PartId::Chat, PartId::KothTimer]), "{ids:?}");
    assert_eq!(Scene::for_file(CHAT_FILE), Some(Scene::Hud));
}

#[test]
fn basechat_places_the_chat_over_hudlayout_and_holds_its_children() {
    let m = chat_with(CHAT, &[]);
    let keys: Vec<&str> = (m.parts().iter())
        .map(|p| p.key().as_str())
        .filter(|k| k.starts_with("HudChat"))
        .collect();
    assert_eq!(
        keys,
        [
            "HudChat",
            "HudChat/ChatInputLine",
            "HudChat/ChatFiltersButton",
            HISTORY
        ]
    );
    let chat = m.part(&PartKey::new("HudChat")).unwrap();
    assert!(chat.is_root() && chat.is_readable());
    assert_eq!(chat.owner(), Some(PartId::Chat));
    let l = m.layout(SD);
    let at = |key: &str| {
        let b = l.get(&PartKey::new(key)).unwrap();
        (b.x(), b.y(), b.wide(), b.tall())
    };
    // basechat.res's own block comes after hudlayout's, so its place and width win.
    assert_eq!(at("HudChat"), (10, 275, 280, 120));
    assert_eq!(at(HISTORY), (20, 292, 260, 75));
}

#[test]
fn the_input_line_and_filters_button_show_only_while_typing() {
    let hud = SceneRule::get(Scene::Hud);
    for panel in ["ChatInputLine", "ChatFiltersButton"] {
        let t = hud.toggled(panel).unwrap();
        assert_eq!(t.shows_for(), Some(ShowsFor::Mode("typing")), "{panel}");
        assert!(!t.shown_in_sample(), "{panel}");
        assert_eq!(t.shows_when(), "Shows while you type a chat message");
    }
    let l = chat_with(CHAT, &[]).layout(SD);
    for key in ["HudChat/ChatInputLine", "HudChat/ChatFiltersButton"] {
        let b = l.get(&PartKey::new(key)).unwrap();
        assert_eq!(b.hidden(), Some(HiddenBy::Game), "{key}");
    }
    assert_eq!(l.get(&PartKey::new(HISTORY)).unwrap().hidden(), None);
}

#[test]
fn a_click_on_the_history_picks_it_and_beside_it_the_chat() {
    let l = chat_with(CHAT, &[]).layout(SD);
    assert_eq!(l.hit(150, 300).map(PartKey::as_str), Some(HISTORY));
    assert_eq!(l.hit(150, 280).map(PartKey::as_str), Some("HudChat"));
}

#[test]
fn chat_at_the_top_moves_the_chat_to_the_top_edge() {
    let m = chat_with(CHAT, &[]);
    let changes = (m.set_option("chat_top", OptionValue::Bool(true), &[], 1)).unwrap();
    let e = m.with_changes(&changes);
    let l = e.model().layout(SD);
    let b = l.get(&PartKey::new("HudChat")).unwrap();
    assert_eq!((b.x(), b.y()), (10, 0));
    assert_eq!(e.touched(), [CHAT_FILE]);
}

#[test]
fn a_missing_or_broken_chat_file_is_a_hud_problem() {
    for chat in [
        None,
        Some("}"),
        Some("\"Resource/UI/BaseChat.res\" \"flat\""),
    ] {
        let files = stock_files_with(&[(CHAT_FILE, chat)]);
        let m = HudModel::load(&files, Platform::Linux).unwrap();
        assert_eq!(m.problems(), [CHAT_FILE], "{chat:?}");
        let part = m.part(&PartKey::new("HudChat")).unwrap();
        assert!(!part.is_readable(), "{chat:?}");
        assert!(
            !m.options().iter().any(|o| o.id() == "chat_top"),
            "{chat:?}"
        );
        let board = m.with_scene(Scene::Scoreboard);
        assert_eq!(board.problems(), [] as [&str; 0], "{chat:?}");
    }
}

const CHAT_SCHEME_FILE: &str = "resource/chatscheme.res";

/// The stock chat scheme's colour lines: the yellow the code reads and a team colour it does not.
const STOCK_BASE: &str =
    r#""TFColors.ChatTextYellow" "251 235 202 255" "TFColors.ChatTextTeamRed" "1 2 3 255""#;

/// A chat scheme with DullWhite plus `colours` in Colors, `base` in BaseSettings, and a
/// 12-unit Verdana ChatFont.
fn chat_scheme(colours: &str, base: &str) -> String {
    format!(
        r#""Scheme"
{{
  Colors {{ "DullWhite" "142 142 142 255" {colours} }}
  BaseSettings {{ {base} }}
  Fonts {{ "ChatFont" {{ "1" {{ "name" "Verdana" "tall" "12" "weight" "700" "dropshadow" "1" }} }} }}
}}
"#
    )
}

#[test]
fn the_chat_reads_its_own_scheme_beside_the_client_scheme() {
    let base = format!(r#"{STOCK_BASE} "RichText.InsetX" "5""#);
    let scheme = chat_scheme("", &base);
    let m = chat_with(CHAT, &[(CHAT_SCHEME_FILE, Some(scheme.as_str()))]);
    let chat = m.chat_scheme();
    assert_eq!(
        chat.named_colour("TFColors.ChatTextYellow"),
        Some(Rgba::new(251, 235, 202, 255))
    );
    assert_eq!(
        chat.font("ChatFont", 480).map(|f| f.family()),
        Some("Verdana")
    );
    assert_eq!(chat.resource_string("richtext.insetx"), Some("5"));
    assert_eq!(chat.resource_string("RichText.InsetY"), None);
    // The client scheme is a different file: it has no chat colours.
    assert_eq!(m.scheme().named_colour("TFColors.ChatTextYellow"), None);
    // Switching scenes and replaying edits keep the chat scheme.
    let board = m.with_scene(Scene::Scoreboard);
    assert!(board.chat_scheme().font("ChatFont", 480).is_some());
    let edited = m.with_changes(&[]);
    assert!(edited.model().chat_scheme().font("ChatFont", 480).is_some());
    // A HUD without a chat scheme reads an empty one.
    let plain = chat_with(CHAT, &[]);
    assert!(plain.chat_scheme().font("ChatFont", 480).is_none());
    assert!(plain.chat_scheme().colour_names().is_empty());
}

#[test]
fn three_sample_lines_in_team_colours_and_the_chat_font() {
    let scheme = chat_scheme("", STOCK_BASE);
    let m = chat_with(CHAT, &[(CHAT_SCHEME_FILE, Some(scheme.as_str()))]);
    let s = m.chat_sample(SD).unwrap();
    assert_eq!(s.history().as_str(), HISTORY);
    assert_eq!(s.font().map(|f| f.family()), Some("Verdana"));
    assert_eq!(s.line_tall(), 12);
    // RichText's offsets at 640 wide: 3 across, 1 down, and 1 more between lines.
    assert_eq!(s.x(), 3);
    let lines: Vec<(i32, &str, &str)> = (s.lines().iter())
        .map(|l| (l.y(), l.name(), l.text()))
        .collect();
    assert_eq!(
        lines,
        [
            (1, "Rocket Jumper", " :  Nice shot!"),
            (14, "Kritz", " :  Push the cart!"),
            (27, "You", " :  Thanks, Medic!"),
        ]
    );
    // The stock scheme names only ChatTextTeamRed, which the game never reads, so the names
    // take the code's own red and blue; the rest of each line takes ChatTextYellow.
    let (red, blue) = (Rgba::new(255, 63, 63, 255), Rgba::new(153, 204, 255, 255));
    let text = Rgba::new(251, 235, 202, 255);
    let colours: Vec<(Rgba, Rgba)> = (s.lines().iter())
        .map(|l| (l.name_colour(), l.text_colour()))
        .collect();
    assert_eq!(colours, [(red, text), (blue, text), (red, text)]);
    // The font and the offsets grow with the screen: 27 px, and 9 and 3 at 1920 wide.
    let hd = m.chat_sample(Screen::new(1920, 1080)).unwrap();
    assert_eq!(hd.line_tall(), 27);
    assert_eq!(hd.x(), 9);
    let ys: Vec<i32> = hd.lines().iter().map(|l| l.y()).collect();
    assert_eq!(ys, [3, 33, 63]);
}

#[test]
fn a_chat_scheme_inset_replaces_the_offsets_unscaled() {
    let base = format!(r#"{STOCK_BASE} "RichText.InsetX" "5" "RichText.InsetY" "2""#);
    let scheme = chat_scheme("", &base);
    let m = chat_with(CHAT, &[(CHAT_SCHEME_FILE, Some(scheme.as_str()))]);
    let hd = m.chat_sample(Screen::new(1920, 1080)).unwrap();
    assert_eq!(hd.x(), 5);
    let ys: Vec<i32> = hd.lines().iter().map(|l| l.y()).collect();
    assert_eq!(ys, [2, 31, 60]);
    // An InsetX alone sets the y offset to nothing.
    let base = format!(r#"{STOCK_BASE} "RichText.InsetX" "4""#);
    let scheme = chat_scheme("", &base);
    let m = chat_with(CHAT, &[(CHAT_SCHEME_FILE, Some(scheme.as_str()))]);
    let s = m.chat_sample(SD).unwrap();
    assert_eq!(s.x(), 4);
    let ys: Vec<i32> = s.lines().iter().map(|l| l.y()).collect();
    assert_eq!(ys, [0, 12, 24]);
}

#[test]
fn the_names_take_the_chat_schemes_team_colours_and_the_text_falls_back_to_dull_white() {
    let scheme = chat_scheme(
        r#""TFColors.ChatTextRed" "10 20 30 255" "TFColors.ChatTextBlue" "40 50 60 128""#,
        "",
    );
    let m = chat_with(CHAT, &[(CHAT_SCHEME_FILE, Some(scheme.as_str()))]);
    let s = m.chat_sample(SD).unwrap();
    let dull = Rgba::new(142, 142, 142, 255);
    assert_eq!(
        (s.lines()[0].name_colour(), s.lines()[0].text_colour()),
        (Rgba::new(10, 20, 30, 255), dull)
    );
    // The game draws chat colours opaque.
    assert_eq!(s.lines()[1].name_colour(), Rgba::new(40, 50, 60, 255));
    // With no chat scheme at all: no font, the 12-unit fallback height, white text.
    let plain = chat_with(CHAT, &[]).chat_sample(SD).unwrap();
    assert!(plain.font().is_none());
    assert_eq!(plain.line_tall(), 12);
    assert_eq!(plain.lines()[0].text_colour(), Rgba::WHITE);
}

#[test]
fn no_sample_lines_where_the_chat_is_not_drawn() {
    let m = chat_with(CHAT, &[]);
    assert!(m.with_scene(Scene::Scoreboard).chat_sample(SD).is_none());
    let quiet = CHAT.replace("tall 75 visible 1", "tall 75 visible 0");
    assert!(chat_with(&quiet, &[]).chat_sample(SD).is_none());
    let away = CHAT.replace("xpos 10 ypos 275", "xpos 9999 ypos 275");
    assert!(chat_with(&away, &[]).chat_sample(SD).is_none());
    let files = stock_files_with(&[(CHAT_FILE, None)]);
    let missing = HudModel::load(&files, Platform::Linux).unwrap();
    assert!(missing.chat_sample(SD).is_none());
}

#[test]
fn while_typing_the_input_line_sits_where_the_game_puts_it_and_says_say() {
    // CBaseHudChat::OnTick ignores the file's ypos and tall: the line is one chat-scheme
    // Default font plus 2 px tall, its top 1.75 such heights above the chat's bottom.
    let typing = GameSituation::default().with_switch(SituationSwitch::Typing, true);
    let line = PartKey::new("HudChat/ChatInputLine");
    let plain = chat_with(CHAT, &[]).layout_in(SD, &typing);
    let b = plain.get(&line).unwrap();
    assert_eq!(b.hidden(), None);
    // No Default font: a 12-unit one, so 14 px tall and 120 - 24.5 down, truncated.
    assert_eq!(
        (b.x() - 10, b.y() - 275, b.wide(), b.tall()),
        (10, 95, 260, 14)
    );
    let scheme = chat_scheme("", STOCK_BASE).replace(
        "Fonts {",
        r#"Fonts { "Default" { "1" { "name" "Verdana" "tall" "14" } }"#,
    );
    let m = chat_with(CHAT, &[(CHAT_SCHEME_FILE, Some(scheme.as_str()))]);
    let b = m.layout_in(SD, &typing).get(&line).unwrap().clone();
    assert_eq!((b.y() - 275, b.tall()), (92, 16));
    // The line shows the code's prompt over the file's empty panel.
    assert_eq!(SampleSet::Empty.code_text("ChatInputLine"), Some("Say :"));
}
