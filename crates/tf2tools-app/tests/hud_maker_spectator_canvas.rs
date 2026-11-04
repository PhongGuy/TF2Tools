//! The spectator canvas: the lines the code writes and the bars it paints.

mod support;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, Tf2Install};

/// A synthetic English file; no Valve bytes.
const ENGLISH: &str = r#""lang" { "Language" "English" "Tokens" {
  "TF_Spectator_ChangeClass" "Press [ %changeclass% ] to pick a class"
  "TF_Spectator_SwitchCamModeKey" "[%jump%]"
  "TF_Spectator_CycleTargetFwdKey" "[%attack%]"
  "TF_Spectator_CycleTargetRevKey" "[%attack2%]"
  "TF_Spectator_SwitchCamMode" "Switch the camera" } }"#;

const TOP: &str =
    "topbar { ControlName Panel fieldName TopBar xpos 0 ypos 0 tall 84 wide f0 visible 1 }";

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    support::click_nav(cx, s.win, tf2tools_app::shell::route::Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn utf16le(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// The stock game with `path` replaced and the synthetic English file seeded, on `scene`'s tab.
fn on_tab(cx: &mut TestAppContext, path: &str, text: &str, scene: Scene) -> support::ShellWindow {
    let env = support::env();
    let english = utf16le(ENGLISH);
    support::seed_stock_game_with_bytes(
        &env.game,
        &[(path, Some(text))],
        &[("resource/tf_english.txt", english.as_slice())],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, scene);
    s
}

fn spectator(cx: &mut TestAppContext, text: &str) -> support::ShellWindow {
    on_tab(cx, "resource/ui/spectator.res", text, Scene::Spectator)
}

fn label(cx: &mut TestAppContext, s: &support::ShellWindow, name: &str) -> Option<String> {
    let key = PartKey::new(format!("Spectator/{name}"));
    support::label_of(cx, s.win, ids::hud_maker_part(&key))
}

fn fills(cx: &mut TestAppContext, s: &support::ShellWindow, name: &str) -> bool {
    let key = PartKey::new(format!("Spectator/{name}"));
    support::present(cx, s.win, ids::hud_maker_fill(&key))
}

#[gpui_kit::test]
fn the_code_writes_the_countdown_the_hints_the_map_and_the_tip(cx: &mut TestAppContext) {
    let s = spectator(cx, support::hud_stock::SPECTATOR);
    let want = [
        ("ReinforcementsLabel", "Respawn in: 7 seconds"),
        ("MapLabel", "Map: 2Fort"),
        ("ClassOrTeamLabel", "Press [ N ] to pick a class"),
        ("SwitchCamModeKeyLabel", "[SPACE]"),
        ("CycleTargetFwdKeyLabel", "[MOUSE1]"),
        ("CycleTargetRevKeyLabel", "[MOUSE2]"),
        // A label the code leaves alone keeps the file's token, resolved through the game text.
        ("SwitchCamModeLabel", "Switch the camera"),
    ];
    for (name, text) in want {
        assert_eq!(label(cx, &s, name).as_deref(), Some(text), "{name}");
    }
    let tip = label(cx, &s, "TipLabel").unwrap_or_default();
    assert!(tip.starts_with("Tip: "), "{tip}");
}

#[gpui_kit::test]
fn both_bars_are_filled_and_only_their_size_can_take_the_fill_away(cx: &mut TestAppContext) {
    let s = spectator(cx, support::hud_stock::SPECTATOR);
    assert!(fills(cx, &s, "topbar") && fills(cx, &s, "bottombarblank"));
    // The frame and the extras panel are not painted by the code.
    assert!(!fills(cx, &s, "BottomBar") && !fills(cx, &s, "spectator_extras"));

    // `visible 0` is overridden by the game: the bar still draws.
    let off = support::hud_stock::SPECTATOR.replace(TOP, &TOP.replace("visible 1", "visible 0"));
    assert_ne!(off, support::hud_stock::SPECTATOR);
    let s = spectator(cx, &off);
    assert!(fills(cx, &s, "topbar"));

    // A zero height takes it away.
    let flat = support::hud_stock::SPECTATOR.replace(TOP, &TOP.replace("tall 84", "tall 0"));
    assert_ne!(flat, support::hud_stock::SPECTATOR);
    let s = spectator(cx, &flat);
    assert!(!fills(cx, &s, "topbar") && fills(cx, &s, "bottombarblank"));
}

#[gpui_kit::test]
fn a_colour_the_file_gives_a_bar_does_not_replace_the_game_s(cx: &mut TestAppContext) {
    // The file paints the top bar fully clear; the game repaints it near-black after loading.
    let clear = support::hud_stock::SPECTATOR.replace(
        TOP,
        &TOP.replace("visible 1", "visible 1 bgcolor_override \"0 0 0 0\""),
    );
    assert_ne!(clear, support::hud_stock::SPECTATOR);
    let s = spectator(cx, &clear);
    assert!(fills(cx, &s, "topbar"));
}

#[gpui_kit::test]
fn the_class_menu_keeps_the_colour_its_file_gives_the_room(cx: &mut TestAppContext) {
    // Unlike the bars, the room is only a stand-in; a HUD's own fill (a clear one here) wins.
    let stock = support::hud_stock::class_select();
    let room = "MenuBG { ControlName CModelPanel xpos 0 ypos 0 zpos 0 wide f0 tall 480 visible 1";
    let tinted = stock.replace(room, &format!("{room} fillcolor \"0 0 0 0\""));
    assert_ne!(tinted, stock);
    let s = on_tab(
        cx,
        "resource/ui/classselection.res",
        &tinted,
        Scene::ClassSelect,
    );
    let bg = PartKey::new("class/MenuBG");
    assert!(!support::present(cx, s.win, ids::hud_maker_fill(&bg)));
    let s = on_tab(
        cx,
        "resource/ui/classselection.res",
        &stock,
        Scene::ClassSelect,
    );
    assert!(support::present(cx, s.win, ids::hud_maker_fill(&bg)));
}
