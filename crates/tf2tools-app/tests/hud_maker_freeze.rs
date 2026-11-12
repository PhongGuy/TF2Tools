//! The Death and freeze cam tab: what it draws, its backdrop, and its edits in the shared list.

use crate::support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, Tf2Install};

const BASE: &str = "FreezePanel/FreezePanelBase";
const NAME: &str = "FreezePanel/FreezePanelBase/FreezeLabelKiller";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

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

fn scene(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<Scene> {
    cx.update(|cx| page.read(cx).scene())
}

fn present(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> bool {
    support::present(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
}

#[gpui_kit::test]
fn the_tab_is_last_and_switches_to_the_freeze_cam(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    let tab = ids::hud_maker_scene(Scene::FreezeCam);
    assert!(format!("{tab:?}").contains("death-and-freeze-cam"));
    assert!(support::present(cx, s.win, tab.clone()));
    support::click_scene(cx, s.win, Scene::FreezeCam);
    assert_eq!(scene(cx, &page), Some(Scene::FreezeCam));
    // Its own parts, none of the HUD's, and no crosshair: a dead player has none.
    assert!(present(cx, &s, BASE));
    assert!(!present(cx, &s, HV));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_CROSSHAIR));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
    // What the game hides stays off the canvas; the hint the game shows is on it.
    assert!(!present(
        cx,
        &s,
        "FreezePanel/FreezePanelBase/NemesisSubPanel"
    ));
    assert!(!present(cx, &s, "FreezePanel/itempanel"));
    assert!(present(cx, &s, "FreezePanel/ScreenshotPanel"));
    support::click_scene(cx, s.win, Scene::Hud);
    assert!(present(cx, &s, HV));
    assert!(!present(cx, &s, BASE));
}

#[gpui_kit::test]
fn the_freeze_cam_darkens_the_world_and_keeps_the_chat(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_FREEZE_SHADE));
    support::click_scene(cx, s.win, Scene::FreezeCam);
    // The game dims the world behind the freeze cam and keeps the chat over it.
    assert!(support::present(cx, s.win, ids::HUD_MAKER_FREEZE_SHADE));
    assert!(present(cx, &s, "HudChat"));
    support::click_scene(cx, s.win, Scene::Hud);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_FREEZE_SHADE));
}

#[gpui_kit::test]
fn a_hud_with_a_stub_freeze_file_says_it_cannot_show_the_scene(cx: &mut TestAppContext) {
    let env = support::env();
    let stub = "#base \"freezepanel_basic_base.res\"";
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/freezepanel_basic.res", Some(stub))],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::FreezeCam);
    assert_eq!(scene(cx, &page), Some(Scene::FreezeCam));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCENE_NOTE).as_deref(),
        Some("Can't show the death and freeze cam")
    );
    // The HUD tab is unaffected.
    support::click_scene(cx, s.win, Scene::Hud);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
    assert!(present(cx, &s, HV));
}

#[gpui_kit::test]
fn the_empty_panel_names_the_death_and_freeze_cam(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::FreezeCam);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_EMPTY).as_deref(),
        Some("Click a part of the death and freeze cam to change it.")
    );
}

fn utf16le(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// A synthetic English file; no Valve bytes.
const ENGLISH: &str = r#""lang" { "Language" "English" "Tokens" {
  "FreezePanel_Killer" "You were killed by" } }"#;

fn label(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> Option<String> {
    support::label_of(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
}

#[gpui_kit::test]
fn the_killed_by_panel_draws_the_killer_and_the_game_text(cx: &mut TestAppContext) {
    let env = support::env();
    let english = utf16le(ENGLISH);
    let red = tf2tools_core::RgbaImage::from_pixels(2, 2, [255, 0, 0, 255].repeat(4)).unwrap();
    let vtf = tf2tools_core::rgba_to_vtf(&red).unwrap();
    let vmt = r#""UnlitGeneric" { "$baseTexture" "hud/color_panel_blu" }"#;
    support::seed_stock_game_with_bytes(
        &env.game,
        &[],
        &[
            ("resource/tf_english.txt", english.as_slice()),
            ("materials/hud/color_panel_blu.vmt", vmt.as_bytes()),
            ("materials/hud/color_panel_blu.vtf", &vtf),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::FreezeCam);
    assert_eq!(
        label(cx, &s, "FreezePanel/FreezePanelBase/FreezeLabel").as_deref(),
        Some("You were killed by")
    );
    assert_eq!(label(cx, &s, NAME).as_deref(), Some("Captain Crumpet"));
    assert_eq!(
        label(
            cx,
            &s,
            "FreezePanel/FreezePanelBase/FreezePanelHealth/PlayerStatusHealthValue"
        )
        .as_deref(),
        Some("187")
    );
    assert_eq!(
        label(cx, &s, "FreezePanel/ScreenshotPanel/ScreenshotLabel").as_deref(),
        Some("[F5] Save this moment")
    );
    // The panel's background decodes under its part (the team image the code sets, not the file's), and the avatar is the plain stand-in.
    let bg = PartKey::new("FreezePanel/FreezePanelBase/FreezePanelBG");
    assert!(cx.update(|cx| page.read(cx).image(&bg)).is_some());
    assert!(present(cx, &s, "FreezePanel/FreezePanelBase/AvatarImage"));
}

#[gpui_kit::test]
fn a_hud_that_moves_the_killer_name_moves_it_on_the_canvas(cx: &mut TestAppContext) {
    let env = support::env();
    let moved = support::hud_stock::FREEZE_PANEL.replace("xpos 61 ypos 173", "xpos 100 ypos 200");
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/freezepanel_basic.res", Some(&moved))],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::FreezeCam);
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    let name = support::bounds_of(cx, s.win, ids::hud_maker_part(&PartKey::new(NAME))).unwrap();
    // The page starts at 1920x1080, scale 2.25: the base sits at r270 and the name 100 and 200
    // in from it, so the box starts at those fractions of the canvas.
    let scale = 2.25_f32;
    let fx = f32::from(name.left() - canvas.left()) / f32::from(canvas.size.width);
    let fy = f32::from(name.top() - canvas.top()) / f32::from(canvas.size.height);
    assert!(
        (fx - (1920. + (100. - 270.) * scale) / 1920.).abs() < 0.005,
        "{fx}"
    );
    assert!((fy - (93. + 200.) * scale / 1080.).abs() < 0.005, "{fy}");
}

fn commit(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    field: Field,
    text: &str,
) {
    use gpui_kit::component::input::InputEvent;
    let input = cx.update(|cx| page.read(cx).field(field).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| {
            st.set_value(text, window, cx);
            cx.emit(InputEvent::PressEnter {
                secondary: false,
                shift: false,
            });
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Opens the Parts list if it is folded, then clicks the part's row.
fn click_row(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) {
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::click_part_row(cx, s.win, &PartKey::new(key));
}

fn xpos(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor()?;
        Some(
            e.model()
                .part(&PartKey::new(key))?
                .value("xpos")?
                .text()
                .to_owned(),
        )
    })
}

fn step(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    f: impl FnOnce(&mut HudMakerPage, &mut gpui_kit::Window, &mut gpui_kit::Context<HudMakerPage>),
) -> Option<Scene> {
    let scene = cx
        .update_window(s.win, |_, window, cx| {
            page.update(cx, |p, cx| {
                f(p, window, cx);
                p.scene()
            })
        })
        .unwrap();
    support::settle(cx, s.win);
    scene
}

#[gpui_kit::test]
fn edits_in_the_freeze_cam_and_the_hud_share_one_list_and_undo_switches_tabs(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    // A move on the freeze cam tab, then a move on the HUD tab.
    support::click_scene(cx, s.win, Scene::FreezeCam);
    click_row(cx, &s, NAME);
    // The X field is the screen pixel at 1920x1080 (scale 2.25): the base sits at 1312.5, so
    // 1538 puts the name 100 game units in from it.
    commit(cx, &s, &page, Field::X, "1538");
    assert_eq!(xpos(cx, &page, NAME).as_deref(), Some("100"));
    support::click_scene(cx, s.win, Scene::Hud);
    click_row(cx, &s, HV);
    commit(cx, &s, &page, Field::X, "30");
    // Undo walks the one list backwards: the HUD move stays on the HUD tab, the freeze cam
    // move takes the player back to its tab.
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.undo(w, cx)),
        Some(Scene::Hud)
    );
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.undo(w, cx)),
        Some(Scene::FreezeCam)
    );
    assert_eq!(xpos(cx, &page, NAME).as_deref(), Some("61"));
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.redo(w, cx)),
        Some(Scene::FreezeCam)
    );
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.redo(w, cx)),
        Some(Scene::Hud)
    );
    // The editor holds the open scene's model, so look at the freeze cam again to read it.
    support::click_scene(cx, s.win, Scene::FreezeCam);
    assert_eq!(xpos(cx, &page, NAME).as_deref(), Some("100"));
}

#[gpui_kit::test]
fn a_move_in_the_killer_health_file_undoes_back_to_the_freeze_cam(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    let value = "FreezePanel/FreezePanelBase/FreezePanelHealth/PlayerStatusHealthValue";
    support::click_scene(cx, s.win, Scene::FreezeCam);
    click_row(cx, &s, value);
    // Screen pixel 1358: the health panel is 8 in from the base, the value 12 in from it.
    commit(cx, &s, &page, Field::X, "1358");
    assert_eq!(xpos(cx, &page, value).as_deref(), Some("12"));
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.undo(w, cx)),
        Some(Scene::FreezeCam)
    );
    assert_eq!(xpos(cx, &page, value).as_deref(), Some("6"));
}

fn open_at(cx: &mut TestAppContext, w: f32, h: f32) -> support::ShellWindow {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    cx.simulate_window_resize(s.win, gpui_kit::size(gpui_kit::px(w), gpui_kit::px(h)));
    support::settle(cx, s.win);
    let _page = open_maker(cx, &s);
    s
}

#[gpui_kit::test]
fn a_narrow_window_swaps_the_scene_tabs_for_a_menu_that_switches_scenes(cx: &mut TestAppContext) {
    let s = open_at(cx, 560., 600.);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SCENE_MENU));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_scene(Scene::Hud)
    ));
    // The button names the open scene; picking an entry switches to it.
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCENE_MENU).as_deref(),
        Some("HUD")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_SCENE_MENU);
    support::click_id(cx, s.win, ids::hud_maker_scene(Scene::FreezeCam));
    assert!(present(cx, &s, BASE));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCENE_MENU).as_deref(),
        Some("Death and freeze cam")
    );
}

#[gpui_kit::test]
fn a_wide_window_keeps_the_scene_tabs(cx: &mut TestAppContext) {
    let s = open_at(cx, 1280., 800.);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_MENU));
    for scene in Scene::ALL {
        assert!(support::present(cx, s.win, ids::hud_maker_scene(scene)));
    }
}
