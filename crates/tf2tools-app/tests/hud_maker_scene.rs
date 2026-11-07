//! The HUD Maker's scenes: the tab bar, the scoreboard preview and its panel rows.

use crate::support;

use gpui_kit::component::color_picker::ColorPickerEvent;
use gpui_kit::component::input::InputEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, Entity, Pixels, ScrollDelta, TestAppContext, point};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshair_maker::model::hsla_of;
use tf2tools_app::pages::hud_maker::view::{Panel, list_font_px};
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_app::services::Tf2State;
use tf2tools_core::{ColumnKind, PlayerListLayout};
use tf2tools_core::{PartKey, Scene, Tf2Install};

// The side panel's least height, as the page's `SIDE_PANEL_MIN` keeps it.
const SIDE_PANEL_MIN: f32 = 240.;
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

/// The stock game with each `(path, Some(text))` replaced and each `(path, None)` removed.
fn shell_with(
    cx: &mut TestAppContext,
    env: &support::Env,
    overrides: &[(&str, Option<&str>)],
) -> support::ShellWindow {
    support::seed_stock_game_with(&env.game, overrides);
    support::shell_window(cx, env, found(env))
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

fn selection(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| Some(page.read(cx).editor()?.selected()?.as_str().to_owned()))
}

fn to_scene(cx: &mut TestAppContext, s: &support::ShellWindow, scene: Scene) {
    // In a window too narrow for the tabs the scenes sit behind the scene menu.
    if !support::present(cx, s.win, ids::hud_maker_scene(scene)) {
        support::click_id(cx, s.win, ids::HUD_MAKER_SCENE_MENU);
    }
    support::click_id(cx, s.win, ids::hud_maker_scene(scene));
}

/// Switches through the page, so the pointer stays where it is.
fn set_scene(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    scene: Scene,
) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.set_scene(scene, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn show_parts(cx: &mut TestAppContext, s: &support::ShellWindow) {
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
}

fn hover(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    cx.update_window(s.win, |_, w, cx| w.hover(id, cx)).unwrap();
    support::settle(cx, s.win);
}

fn bounds(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<gpui_kit::ElementId>,
) -> Bounds<Pixels> {
    let id = id.into();
    support::bounds_of(cx, s.win, id.clone()).unwrap_or_else(|| panic!("no {id:?}"))
}

fn f(p: Pixels) -> f32 {
    f32::from(p)
}

#[gpui_kit::test]
fn the_tabs_switch_what_the_canvas_draws(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    assert_eq!(scene(cx, &page), Some(Scene::Hud));
    let tabs = bounds(cx, &s, ids::HUD_MAKER_SCENES);
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    assert!(
        f(area.top() - tabs.bottom()).abs() <= 1.,
        "{area:?} under {tabs:?}"
    );
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(scene(cx, &page), Some(Scene::Scoreboard));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new("scores"))
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new(HV))
    ));
    to_scene(cx, &s, Scene::Hud);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new(HV))
    ));
}

#[gpui_kit::test]
fn a_switch_drops_the_selection_and_the_hover_outline(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    show_parts(cx, &s);
    let row = ids::hud_maker_row(&PartKey::new(HV));
    support::click_id(cx, s.win, row.clone());
    hover(cx, &s, row.clone());
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_HOVER));
    // The list may stay put across the switch, so the pointer waits on its title, off every row.
    let title = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL_TITLE);
    point_at(
        cx,
        &s,
        point(title.left() + gpui_kit::px(8.), title.center().y),
    );
    set_scene(cx, &s, &page, Scene::Scoreboard);
    assert_eq!(selection(cx, &page), None);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HOVER));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_row(&PartKey::new("scores"))
    ));
    // The pointer then goes onto a row of the list where it now stands, as the list keeps its
    // spot back in the HUD when that spot is clear there too.
    let on_board = bounds(cx, &s, ids::hud_maker_row(&PartKey::new("scores"))).center();
    point_at(cx, &s, on_board);
    // Back in the HUD the rows are built afresh under the still pointer, and gpui hovers the
    // one it is on, so the outline shows over that row's part. The list keeps off what each
    // scene draws, so it may stand elsewhere than on the board and the row need not be HV's.
    set_scene(cx, &s, &page, Scene::Hud);
    let pointer = cx
        .update_window(s.win, |_, w, _| w.mouse_position())
        .unwrap();
    let keys: Vec<PartKey> = cx.update(|cx| {
        let model = page.read(cx).editor().unwrap().model();
        model.parts().iter().map(|p| p.key().clone()).collect()
    });
    let mut under = None;
    for key in keys {
        let row = support::bounds_of(cx, s.win, ids::hud_maker_row(&key));
        if row.is_some_and(|b| b.contains(&pointer)) {
            under = Some(key);
        }
    }
    let key = under.unwrap_or_else(|| panic!("{pointer:?} is on no row"));
    let outline = bounds(cx, &s, ids::HUD_MAKER_HOVER);
    assert_eq!(
        outline,
        bounds(cx, &s, ids::hud_maker_part(&key)),
        "{key:?}"
    );
}

#[gpui_kit::test]
fn a_switch_clears_the_hover_before_any_frame(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    show_parts(cx, &s);
    hover(cx, &s, ids::hud_maker_row(&PartKey::new(HV)));
    let hv = PartKey::new(HV);
    assert_eq!(cx.update(|cx| page.read(cx).hovered().cloned()), Some(hv));
    // A frame may hover a row afresh under the pointer, so read before one is drawn.
    let hovered = cx
        .update_window(s.win, |_, window, cx| {
            page.update(cx, |p, cx| {
                p.set_scene(Scene::Scoreboard, window, cx);
                p.hovered().cloned()
            })
        })
        .unwrap();
    assert_eq!(hovered, None);
}

fn pick(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    let state = cx.update(|cx| page.read(cx).colour_state().clone());
    cx.update(|cx| {
        state.update(cx, |_, cx| {
            cx.emit(ColorPickerEvent::Change(Some(hsla_of([255, 80, 80]))))
        })
    });
    support::settle(cx, s.win);
}

/// What the page hovers and whether a picker run is open, read in the same update as `step`.
fn after(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    step: impl FnOnce(&mut HudMakerPage, &mut gpui_kit::Window, &mut gpui_kit::Context<HudMakerPage>),
) -> (Option<Scene>, Option<PartKey>, bool) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            step(p, window, cx);
            (p.scene(), p.hovered().cloned(), p.is_picking())
        })
    })
    .unwrap()
}

#[gpui_kit::test]
fn undo_and_redo_into_the_other_scene_clear_the_hover_and_the_picker_run(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    click_row(cx, &s, BLUE);
    commit(cx, &s, &page, Field::RowHeight, "24");
    set_scene(cx, &s, &page, Scene::Hud);
    click_row(cx, &s, HV);
    pick(cx, &s, &page);
    hover(cx, &s, ids::hud_maker_row(&PartKey::new(HV)));
    let hv = Some(PartKey::new(HV));
    // Undoing the colour stays in the HUD; undoing the spacing goes to the scoreboard.
    let stay = after(cx, &s, &page, |p, w, cx| p.undo(w, cx));
    assert_eq!(stay, (Some(Scene::Hud), hv.clone(), true));
    let left = after(cx, &s, &page, |p, w, cx| p.undo(w, cx));
    assert_eq!(left, (Some(Scene::Scoreboard), None, false));
    support::settle(cx, s.win);
    hover(cx, &s, ids::hud_maker_row(&PartKey::new(BLUE)));
    let blue = Some(PartKey::new(BLUE));
    let stay = after(cx, &s, &page, |p, w, cx| p.redo(w, cx));
    assert_eq!(stay, (Some(Scene::Scoreboard), blue, false));
    let left = after(cx, &s, &page, |p, w, cx| p.redo(w, cx));
    assert_eq!(left, (Some(Scene::Hud), None, false));
}

#[gpui_kit::test]
fn every_scenes_images_and_the_ping_bars_decode(cx: &mut TestAppContext) {
    let env = support::env();
    let board = support::hud_stock::SCOREBOARD.replace(
        "BlueTeamImage { ControlName ImagePanel",
        "BlueTeamImage { ControlName ImagePanel image ../hud/health_bg",
    );
    let red = tf2tools_core::RgbaImage::from_pixels(2, 2, [255, 0, 0, 255].repeat(4)).unwrap();
    let vtf = tf2tools_core::rgba_to_vtf(&red).unwrap();
    let vmt = |name: &str| format!(r#""UnlitGeneric" {{ "$baseTexture" "hud/{name}" }}"#);
    let (bg, ping) = (vmt("health_bg"), vmt("scoreboard_ping_low"));
    support::seed_stock_game_with_bytes(
        &env.game,
        &[("resource/ui/scoreboard.res", Some(&board))],
        &[
            ("materials/hud/health_bg.vmt", bg.as_bytes()),
            ("materials/hud/health_bg.vtf", &vtf),
            ("materials/hud/scoreboard_ping_low.vmt", ping.as_bytes()),
            ("materials/hud/scoreboard_ping_low.vtf", &vtf),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    for key in ["scores/BlueTeamImage", "../hud/scoreboard_ping_low"] {
        let image = cx.update(|cx| page.read(cx).image(&PartKey::new(key)));
        assert!(image.is_some(), "{key}");
    }
}

const BLUE: &str = "scores/BluePlayerList";

fn click_row(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) {
    show_parts(cx, s);
    support::click_part_row(cx, s.win, &PartKey::new(key));
}

/// Clicks `id` in the side panel, scrolling the panel's body until `id` is wholly in view.
fn click_in_panel(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    for _ in 0..40 {
        let seen = cx
            .update_window(s.win, |_, w, _| {
                let body = w.try_find(ids::HUD_MAKER_PANEL_BODY).map(|b| b.bounds());
                w.try_find(id.clone()).is_some_and(|e| {
                    let b = e.bounds();
                    e.visible() && body.is_none_or(|body| b.bottom() < body.bottom())
                })
            })
            .unwrap();
        if seen {
            break;
        }
        let down = ScrollDelta::Lines(point(0., -3.));
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::HUD_MAKER_PANEL_BODY, down, cx)
        })
        .unwrap();
    }
    support::click_id(cx, s.win, id);
}

fn part_label(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> Option<String> {
    support::label_of(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
}

fn utf16le(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// A synthetic English file; no Valve bytes.
const ENGLISH: &str = r#""lang" { "Language" "English" "Tokens" { "TF_Test_Server" "Test Server" "TF_Scoreboard_Name" "Player" "TF_Scoreboard_Score" "Points" } }"#;

/// The stock game whose ServerLabel reads `label`, with `tf_english.txt` from ENGLISH when `english`.
fn shell_with_server_label(
    cx: &mut TestAppContext,
    env: &support::Env,
    label: &str,
    english: bool,
) -> support::ShellWindow {
    let scoreboard = support::hud_stock::SCOREBOARD.replacen(
        "labelText %server%",
        &format!("labelText \"{label}\""),
        1,
    );
    let bytes = utf16le(ENGLISH);
    let extra: Vec<(&str, &[u8])> = if english {
        vec![("resource/tf_english.txt", bytes.as_slice())]
    } else {
        Vec::new()
    };
    support::seed_stock_game_with_bytes(
        &env.game,
        &[("resource/ui/scoreboard.res", Some(scoreboard.as_str()))],
        &extra,
    );
    support::shell_window(cx, env, found(env))
}

#[gpui_kit::test]
fn a_token_label_draws_the_games_english_text(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_server_label(cx, &env, "#tf_test_SERVER", true);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(
        part_label(cx, &s, "scores/ServerLabel").as_deref(),
        Some("Test Server")
    );
}

#[gpui_kit::test]
fn an_unknown_token_draws_as_written(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_server_label(cx, &env, "#TF_Not_There", true);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(
        part_label(cx, &s, "scores/ServerLabel").as_deref(),
        Some("#TF_Not_There")
    );
}

#[gpui_kit::test]
fn without_the_games_text_a_token_draws_as_written(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_server_label(cx, &env, "#TF_Test_Server", false);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(
        part_label(cx, &s, "scores/ServerLabel").as_deref(),
        Some("#TF_Test_Server")
    );
}

fn title(cx: &mut TestAppContext, s: &support::ShellWindow, name: &str) -> Option<String> {
    support::label_of(
        cx,
        s.win,
        ids::hud_maker_list_title(&PartKey::new(BLUE), name),
    )
}

#[gpui_kit::test]
fn list_titles_come_from_the_games_text(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_server_label(cx, &env, "%server%", true);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(title(cx, &s, "name").as_deref(), Some("Player"));
    assert_eq!(title(cx, &s, "score").as_deref(), Some("Points"));
}

#[gpui_kit::test]
fn list_titles_fall_back_to_name_and_score(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(title(cx, &s, "name").as_deref(), Some("Name"));
    assert_eq!(title(cx, &s, "score").as_deref(), Some("Score"));
}

#[gpui_kit::test]
fn a_list_title_hides_its_hotkey_marker_and_resolves_only_once(cx: &mut TestAppContext) {
    let env = support::env();
    let english = r##""lang" { "Tokens" { "TF_Scoreboard_Name" "&Player && Co"
      "TF_Scoreboard_Score" "#TF_Scoreboard_Name" } }"##;
    let bytes = utf16le(english);
    support::seed_stock_game_with_bytes(
        &env.game,
        &[],
        &[("resource/tf_english.txt", bytes.as_slice())],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(title(cx, &s, "name").as_deref(), Some("Player & Co"));
    assert_eq!(
        title(cx, &s, "score").as_deref(),
        Some("#TF_Scoreboard_Name")
    );
}

#[gpui_kit::test]
fn an_empty_title_falls_back_and_a_hash_title_stays(cx: &mut TestAppContext) {
    let env = support::env();
    let english =
        r##""lang" { "Tokens" { "TF_Scoreboard_Name" "" "TF_Scoreboard_Score" "#1 Score" } }"##;
    let bytes = utf16le(english);
    support::seed_stock_game_with_bytes(
        &env.game,
        &[],
        &[("resource/tf_english.txt", bytes.as_slice())],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(title(cx, &s, "name").as_deref(), Some("Name"));
    assert_eq!(title(cx, &s, "score").as_deref(), Some("#1 Score"));
}

#[gpui_kit::test]
fn the_scoreboard_draws_alone_with_its_sample_text(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_CROSSHAIR));
    assert_eq!(part_label(cx, &s, HV).as_deref(), Some("150"));
    to_scene(cx, &s, Scene::Scoreboard);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_CROSSHAIR));
    assert_eq!(
        part_label(cx, &s, "scores/BlueTeamScore").as_deref(),
        Some("2")
    );
    assert_eq!(
        part_label(cx, &s, "scores/ServerLabel").as_deref(),
        Some("TF2Tools Preview")
    );
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new("scores/classmodelpanel"))
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new("scores/ClassImage"))
    ));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
}

#[gpui_kit::test]
fn a_scoreboard_the_hud_hides_says_so_and_stays_read_only(cx: &mut TestAppContext) {
    let env = support::env();
    let board = support::hud_stock::SCOREBOARD
        .replace("    wide 640\n    tall 448\n", "    wide 0\n    tall 0\n");
    let s = shell_with(cx, &env, &[("resource/ui/scoreboard.res", Some(&board))]);
    let _page = open_maker(cx, &s);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
    to_scene(cx, &s, Scene::Scoreboard);
    let hides = Some("This HUD hides the scoreboard");
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCENE_NOTE).as_deref(),
        hides
    );
    click_row(cx, &s, BLUE);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_UNREADABLE).as_deref(),
        hides
    );
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HIDE));
}

#[gpui_kit::test]
fn a_missing_scoreboard_says_so_and_leaves_the_hud_alone(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[("resource/ui/scoreboard.res", None)]);
    let _page = open_maker(cx, &s);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new(HV))
    ));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCENE_NOTE).as_deref(),
        Some("Can't show the scoreboard")
    );
}

#[gpui_kit::test]
fn an_unparsable_scoreboard_says_so_too(cx: &mut TestAppContext) {
    // A UTF-16 file is a parse error (`KvParseError::Utf16`), which core files under
    // `problems()` as it does a missing file.
    let env = support::env();
    let utf16: &[u8] = &[0xFF, 0xFE, b'{', 0, b'}', 0];
    support::seed_stock_game_with_bytes(
        &env.game,
        &[("resource/ui/scoreboard.res", None)],
        &[("resource/ui/scoreboard.res", utf16)],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCENE_NOTE).as_deref(),
        Some("Can't show the scoreboard")
    );
}

const RED: &str = "scores/RedPlayerList";

fn list_layout(
    cx: &mut TestAppContext,
    page: &Entity<HudMakerPage>,
    key: &str,
) -> PlayerListLayout {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        let font = list_font_px(e.model().scheme(), e.screen());
        e.model()
            .player_list(&PartKey::new(key), e.screen(), font)
            .unwrap()
    })
}

fn list_row(key: &str, row: usize) -> gpui_kit::ElementId {
    ids::hud_maker_list_row(&PartKey::new(key), row)
}

#[gpui_kit::test]
fn player_rows_sit_where_the_game_lays_them_out(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let l = list_layout(cx, &page, BLUE);
    let list = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(BLUE)));
    let scale = f(bounds(cx, &s, ids::HUD_MAKER_CANVAS).size.width) / 1920.;
    let near = |a: Pixels, b: f32| (f(a) - b).abs() <= 1.;
    let (x, wide) = (l.row_x() as f32 * scale, l.row_wide() as f32 * scale);
    let (hy, htall) = l.header();
    let header = bounds(cx, &s, ids::hud_maker_list_header(&PartKey::new(BLUE)));
    assert!(
        near(header.left(), f(list.left()) + x)
            && near(header.top(), f(list.top()) + hy as f32 * scale)
            && near(header.size.width, wide)
            && near(header.size.height, htall as f32 * scale),
        "{header:?} in {list:?}"
    );
    for (i, (y, tall)) in l.rows(3).into_iter().enumerate() {
        let row = bounds(cx, &s, list_row(BLUE, i));
        assert!(
            near(row.left(), f(list.left()) + x)
                && near(row.top(), f(list.top()) + y as f32 * scale)
                && near(row.size.width, wide)
                && near(row.size.height, tall as f32 * scale),
            "row {i}: {row:?} in {list:?}"
        );
    }
    assert_eq!(
        support::label_of(cx, s.win, list_row(BLUE, 0)).as_deref(),
        Some("Blue Rocket")
    );
    assert_eq!(
        support::label_of(cx, s.win, list_row(RED, 0)).as_deref(),
        Some("You")
    );
}

#[gpui_kit::test]
fn the_score_title_shows_in_full_at_its_columns_right(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let l = list_layout(cx, &page, BLUE);
    let score = *l
        .columns()
        .iter()
        .find(|c| c.kind() == ColumnKind::Score)
        .unwrap();
    let key = PartKey::new(BLUE);
    let scale = f(bounds(cx, &s, ids::HUD_MAKER_CANVAS).size.width) / 1920.;
    let list = bounds(cx, &s, ids::hud_maker_part(&key));
    let header = bounds(cx, &s, ids::hud_maker_list_header(&key));
    let title = bounds(cx, &s, ids::hud_maker_list_title(&key, "score"));
    let right = f(list.left()) + (l.row_x() + score.x() + score.wide()) as f32 * scale;
    assert!(
        (f(title.right()) - right).abs() <= 1.,
        "{title:?} ends at {right}"
    );
    // Wider than its column, the title runs left over the empty columns instead of clipping.
    assert!(
        f(title.size.width) > score.wide() as f32 * scale,
        "setup: {title:?} fits {}",
        score.wide()
    );
    assert!(
        title.left() >= header.left() && title.right() <= header.right(),
        "{title:?} in {header:?}"
    );
}

#[gpui_kit::test]
fn the_avatar_stays_square(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let l = list_layout(cx, &page, BLUE);
    let avatar = *l
        .columns()
        .iter()
        .find(|c| c.kind() == ColumnKind::Avatar)
        .unwrap();
    let scale = f(bounds(cx, &s, ids::HUD_MAKER_CANVAS).size.width) / 1920.;
    let (_, tall) = l.rows(1)[0];
    let side = tall.min(avatar.wide()) as f32 * scale - 2.;
    let b = bounds(cx, &s, ids::hud_maker_list_avatar(&PartKey::new(BLUE), 0));
    let near = |a: Pixels| (f(a) - side).abs() <= 0.5;
    assert!(
        near(b.size.width) && near(b.size.height),
        "{b:?} for {side}"
    );
}

#[gpui_kit::test]
fn rows_past_the_lists_bottom_are_not_drawn(cx: &mut TestAppContext) {
    let env = support::env();
    let board = support::hud_stock::SCOREBOARD.replacen("linespacing 18", "linespacing 60", 1);
    let s = shell_with(cx, &env, &[("resource/ui/scoreboard.res", Some(&board))]);
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let tall = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.layout().get(&PartKey::new(BLUE)).unwrap().tall()
    });
    let rows = list_layout(cx, &page, BLUE).rows(12);
    let out = rows
        .iter()
        .position(|&(y, _)| y >= tall)
        .expect("setup: a row past the bottom");
    assert!(out > 0, "setup: {rows:?}");
    assert!(support::present(cx, s.win, list_row(BLUE, out - 1)));
    assert!(!support::present(cx, s.win, list_row(BLUE, out)));
}

#[gpui_kit::test]
fn negative_row_heights_draw_the_header_and_no_rows(cx: &mut TestAppContext) {
    let env = support::env();
    let board = support::hud_stock::SCOREBOARD.replacen("linespacing 18", "linespacing -10", 1);
    let s = shell_with(cx, &env, &[("resource/ui/scoreboard.res", Some(&board))]);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_list_header(&PartKey::new(BLUE))
    ));
    assert!(!support::present(cx, s.win, list_row(BLUE, 0)));
    assert!(support::present(cx, s.win, list_row(RED, 0)));
}

#[gpui_kit::test]
fn a_list_that_is_not_a_sectioned_list_says_rows_draw_in_game(cx: &mut TestAppContext) {
    let env = support::env();
    let board = support::hud_stock::SCOREBOARD.replace(
        "BluePlayerList { ControlName SectionedListPanel",
        "BluePlayerList { ControlName EditablePanel",
    );
    let s = shell_with(cx, &env, &[("resource/ui/scoreboard.res", Some(&board))]);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_part(&PartKey::new(BLUE))).as_deref(),
        Some("Player rows draw in game")
    );
    assert!(!support::present(cx, s.win, list_row(BLUE, 0)));
    assert!(support::present(cx, s.win, list_row(RED, 0)));
}

#[gpui_kit::test]
fn ping_bars_draw_from_the_games_textures(cx: &mut TestAppContext) {
    let env = support::env();
    let red = tf2tools_core::RgbaImage::from_pixels(2, 2, [255, 0, 0, 255].repeat(4)).unwrap();
    let vtf = tf2tools_core::rgba_to_vtf(&red).unwrap();
    let vmt: &[u8] = br#""UnlitGeneric" { "$baseTexture" "hud/scoreboard_ping_low" }"#;
    support::seed_stock_game_with_bytes(
        &env.game,
        &[],
        &[
            ("materials/hud/scoreboard_ping_low.vmt", vmt),
            ("materials/hud/scoreboard_ping_low.vtf", &vtf),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let blue = PartKey::new(BLUE);
    // Blue Rocket is alive at 45 ms; Backburner is dead, so his bar is the unseeded `_d` one.
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_list_ping(&blue, 0)
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_list_ping(&blue, 3)
    ));
}

// The note lives in the tab strip, so no floating panel or the toolbar can cover it.
/// The note's bounds once checked clear at `w` x `h`.
fn the_note_keeps_clear_at(cx: &mut TestAppContext, w: f32, h: f32) -> Bounds<Pixels> {
    let env = support::env();
    let board = support::hud_stock::SCOREBOARD
        .replace("    wide 640\n    tall 448\n", "    wide 0\n    tall 0\n");
    let s = shell_with(cx, &env, &[("resource/ui/scoreboard.res", Some(&board))]);
    cx.simulate_window_resize(s.win, gpui_kit::size(gpui_kit::px(w), gpui_kit::px(h)));
    support::settle(cx, s.win);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    to_scene(cx, &s, Scene::Scoreboard);
    click_row(cx, &s, BLUE);
    let note = bounds(cx, &s, ids::HUD_MAKER_SCENE_NOTE);
    let strip = bounds(cx, &s, ids::HUD_MAKER_SCENES);
    assert!(
        note.top() >= strip.top() && note.bottom() <= strip.bottom(),
        "{w}x{h}: {note:?} in {strip:?}"
    );
    let view = support::viewport(cx, s.win);
    assert!(
        note.left() >= gpui_kit::px(0.)
            && note.right() <= view.width
            && note.bottom() <= view.height,
        "{w}x{h}: {note:?} inside {view:?}"
    );
    for other in [
        ids::HUD_MAKER_PANEL,
        ids::HUD_MAKER_PARTS_PANEL,
        ids::HUD_MAKER_TOOLBAR,
    ] {
        let them = bounds(cx, &s, other);
        assert!(
            !note.intersects(&them),
            "{w}x{h}: {note:?} under {other} {them:?}"
        );
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCENE_NOTE).as_deref(),
        Some("This HUD hides the scoreboard")
    );
    note
}

#[gpui_kit::test]
fn the_scene_note_keeps_clear_at_800x600(cx: &mut TestAppContext) {
    the_note_keeps_clear_at(cx, 800., 600.);
}

#[gpui_kit::test]
fn the_scene_note_keeps_clear_at_1024x768(cx: &mut TestAppContext) {
    the_note_keeps_clear_at(cx, 1024., 768.);
}

#[gpui_kit::test]
fn a_narrow_strip_cuts_the_note_short_on_one_line(cx: &mut TestAppContext) {
    let wide = the_note_keeps_clear_at(cx, 1024., 768.);
    let narrow = the_note_keeps_clear_at(cx, 560., 600.);
    assert!(
        narrow.size.width < wide.size.width && narrow.size.height == wide.size.height,
        "{narrow:?} against {wide:?}"
    );
}

fn commit(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    field: Field,
    text: &str,
) {
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

fn field_text(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, field: Field) -> String {
    cx.update(|cx| page.read(cx).field(field).read(cx).value().to_string())
}

fn spacing(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.model()
            .part(&PartKey::new(BLUE))?
            .value("linespacing")
            .map(|l| l.text().to_owned())
    })
}

#[gpui_kit::test]
fn a_player_list_offers_spacing_rows_and_no_colour(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    click_row(cx, &s, BLUE);
    // The open list makes way rather than leave the side panel no room beside the board's list.
    assert!(
        !cx.update(|cx| page.read(cx).is_folded(Panel::Options)),
        "the side panel folded"
    );
    for key in ["linespacing", "linegap", "sectiongap"] {
        assert!(
            support::present(cx, s.win, ids::hud_maker_number(key)),
            "{key}"
        );
    }
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COLOUR));
    assert_eq!(field_text(cx, &page, Field::RowHeight), "18");
    assert_eq!(field_text(cx, &page, Field::SectionGap), "");
    commit(cx, &s, &page, Field::RowHeight, "24");
    assert_eq!(spacing(cx, &page).as_deref(), Some("24"));
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::hud_maker_reset("linespacing")
    ));
    for refused in ["2000", "-3"] {
        commit(cx, &s, &page, Field::RowHeight, refused);
        assert_eq!(spacing(cx, &page).as_deref(), Some("24"), "{refused}");
        assert_eq!(field_text(cx, &page, Field::RowHeight), "24", "{refused}");
    }
    // The side panel keeps off the selected list, so its body may need scrolling.
    click_in_panel(cx, &s, ids::hud_maker_reset("linespacing"));
    assert_eq!(spacing(cx, &page).as_deref(), Some("18"));
}

#[gpui_kit::test]
fn a_label_offers_no_number_rows(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    click_row(cx, &s, HV);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_LAYER));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_number("linespacing")
    ));
}

#[gpui_kit::test]
fn a_part_the_game_hides_says_when_it_shows_and_hides_by_size(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    click_row(cx, &s, "scores/ClassImage");
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_SHOWS).as_deref(),
        Some("Shows when the 3D class model is off")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_HIDE);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SIZE_HIDDEN).as_deref(),
        Some("Hidden")
    );
    click_row(cx, &s, "scores/BlueTeamScore");
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_SHOWS).as_deref(),
        Some("Shows while you hold the scoreboard key")
    );
}

// The target ID is hidden in the default situation, so these image parts sit in the health panel.
const TARGET_BG: &str = "HudPlayerStatus/HudPlayerHealth/TargetIDBG";

/// A `size` x `size` opaque texture at `materials/hud/<name>`, with its UnlitGeneric material.
fn texture(name: &str, size: u32) -> (String, Vec<u8>) {
    let image = tf2tools_core::RgbaImage::from_pixels(
        size,
        size,
        [200, 180, 150, 255].repeat((size * size) as usize),
    )
    .unwrap();
    let vmt = format!(r#""UnlitGeneric" {{ "$baseTexture" "hud/{name}" }}"#);
    (vmt, tf2tools_core::rgba_to_vtf(&image).unwrap())
}

/// The stock game with `blocks` added to the health panel, plus `extra` files.
fn shell_with_health_blocks(
    cx: &mut TestAppContext,
    env: &support::Env,
    blocks: &str,
    extra: &[(&str, &[u8])],
) -> support::ShellWindow {
    let health = support::hud_stock::with_blocks(support::hud_stock::HEALTH, blocks);
    support::seed_stock_game_with_bytes(
        &env.game,
        &[("resource/ui/hudplayerhealth.res", Some(&health))],
        extra,
    );
    support::shell_window(cx, env, found(env))
}

fn bg_block(class: &str) -> String {
    format!(
        "TargetIDBG {{ ControlName {class} xpos 0 ypos 0 zpos -1 wide 252 tall 50 image ../hud/color_panel_brown src_corner_height 2 src_corner_width 2 draw_corner_width 5 draw_corner_height 5 }}"
    )
}

fn inside(cell: Bounds<Pixels>, part: Bounds<Pixels>) -> bool {
    f(cell.left()) >= f(part.left()) - 0.5
        && f(cell.top()) >= f(part.top()) - 0.5
        && f(cell.right()) <= f(part.right()) + 0.5
        && f(cell.bottom()) <= f(part.bottom()) + 0.5
}

#[gpui_kit::test]
fn a_square_texture_stays_inside_a_wide_part(cx: &mut TestAppContext) {
    let env = support::env();
    let (vmt, vtf) = texture("color_panel_brown", 8);
    let s = shell_with_health_blocks(
        cx,
        &env,
        &bg_block("CTFImagePanel"),
        &[
            ("materials/hud/color_panel_brown.vmt", vmt.as_bytes()),
            ("materials/hud/color_panel_brown.vtf", &vtf),
        ],
    );
    let _page = open_maker(cx, &s);
    let key = PartKey::new(TARGET_BG);
    let part = bounds(cx, &s, ids::hud_maker_part(&key));
    let scale = f(bounds(cx, &s, ids::HUD_MAKER_CANVAS).size.width) / 1920.;
    for i in 0..9 {
        let cell = bounds(cx, &s, ids::hud_maker_image_cell(&key, i));
        assert!(inside(cell, part), "cell {i} {cell:?} leaves {part:?}");
    }
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_image_cell(&key, 9)
    ));
    let corner = bounds(cx, &s, ids::hud_maker_image_cell(&key, 0));
    assert!((f(corner.size.width) - 11. * scale).abs() <= 0.5);
    assert!((f(corner.size.height) - 11. * scale).abs() <= 0.5);
    let last = bounds(cx, &s, ids::hud_maker_image_cell(&key, 8));
    assert!((f(last.bottom()) - f(part.bottom())).abs() <= 0.5);
    assert!((f(last.right()) - f(part.right())).abs() <= 0.5);
}

#[gpui_kit::test]
fn a_plain_image_fills_its_box_in_one_piece(cx: &mut TestAppContext) {
    let env = support::env();
    let (vmt, vtf) = texture("color_panel_brown", 8);
    let s = shell_with_health_blocks(
        cx,
        &env,
        &bg_block("ImagePanel"),
        &[
            ("materials/hud/color_panel_brown.vmt", vmt.as_bytes()),
            ("materials/hud/color_panel_brown.vtf", &vtf),
        ],
    );
    let _page = open_maker(cx, &s);
    let key = PartKey::new(TARGET_BG);
    let part = bounds(cx, &s, ids::hud_maker_part(&key));
    let cell = bounds(cx, &s, ids::hud_maker_image_cell(&key, 0));
    assert!(
        inside(cell, part) && inside(part, cell),
        "{cell:?} vs {part:?}"
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_image_cell(&key, 1)
    ));
}

#[gpui_kit::test]
fn a_game_filled_image_draws_a_plain_stand_in(cx: &mut TestAppContext) {
    let env = support::env();
    let avatar =
        r#"AvatarImage { ControlName CAvatarImagePanel xpos 5 ypos 5 wide 10 tall 10 image "" }"#;
    let s = shell_with_health_blocks(cx, &env, avatar, &[]);
    let _page = open_maker(cx, &s);
    let key = PartKey::new("HudPlayerStatus/HudPlayerHealth/AvatarImage");
    let part = bounds(cx, &s, ids::hud_maker_part(&key));
    let stand_in = bounds(cx, &s, ids::hud_maker_stand_in(&key));
    assert!(inside(stand_in, part) && inside(part, stand_in));
}

#[gpui_kit::test]
fn a_hidden_game_filled_image_draws_no_stand_in(cx: &mut TestAppContext) {
    let env = support::env();
    let health = support::hud_stock::HEALTH.replacen(
        "  HealthBonusSettings",
        "  PlayerStatus_SpyMarked { ControlName ImagePanel xpos 120 ypos 0 wide 30 tall 30 visible 1 image \"\" }\n  HealthBonusSettings",
        1,
    );
    let s = shell_with(
        cx,
        &env,
        &[("resource/ui/hudplayerhealth.res", Some(health.as_str()))],
    );
    let _page = open_maker(cx, &s);
    let key = PartKey::new("HudPlayerStatus/HudPlayerHealth/PlayerStatus_SpyMarked");
    assert!(
        !support::present(cx, s.win, ids::hud_maker_part(&key)),
        "the default situation hides it"
    );
    assert!(!support::present(cx, s.win, ids::hud_maker_stand_in(&key)));
}

#[gpui_kit::test]
fn the_class_model_draws_the_soldier_portrait_at_its_foot(cx: &mut TestAppContext) {
    let env = support::env();
    let (vmt, vtf) = texture("class_soldierred", 4);
    support::seed_stock_game_with_bytes(
        &env.game,
        &[],
        &[
            ("materials/hud/class_soldierred.vmt", vmt.as_bytes()),
            ("materials/hud/class_soldierred.vtf", &vtf),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let key = PartKey::new("scores/classmodelpanel");
    let part = bounds(cx, &s, ids::hud_maker_part(&key));
    let portrait = bounds(cx, &s, ids::hud_maker_portrait(&key));
    assert!(inside(portrait, part));
    assert!(
        (f(portrait.size.width) - f(part.size.width)).abs() <= 0.5,
        "as wide as the model box"
    );
    assert!((f(portrait.size.height) - f(portrait.size.width)).abs() <= 0.5);
    assert!((f(portrait.bottom()) - f(part.bottom())).abs() <= 0.5);
}

#[gpui_kit::test]
fn without_the_portrait_the_class_model_draws_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let key = PartKey::new("scores/classmodelpanel");
    assert!(support::present(cx, s.win, ids::hud_maker_part(&key)));
    assert!(!support::present(cx, s.win, ids::hud_maker_portrait(&key)));
}

#[gpui_kit::test]
fn a_loose_english_file_names_the_labels(cx: &mut TestAppContext) {
    let env = support::env();
    let scoreboard = support::hud_stock::SCOREBOARD.replacen(
        "labelText %server%",
        "labelText #TF_Test_Server",
        1,
    );
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/scoreboard.res", Some(scoreboard.as_str()))],
    );
    let loose = env.game.join("tf/resource/tf_english.txt");
    std::fs::create_dir_all(loose.parent().unwrap()).unwrap();
    std::fs::write(&loose, utf16le(ENGLISH)).unwrap();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(
        part_label(cx, &s, "scores/ServerLabel").as_deref(),
        Some("Test Server")
    );
}

const TEAM_BG: &str = "HudPlayerStatus/HudPlayerHealth/TeamBG";

fn team_bg(class: &str) -> String {
    format!(
        "TeamBG {{ ControlName {class} xpos 0 ypos 0 wide 90 tall 45 image ../hud/blue_bg teambg_2 ../hud/red_bg teambg_3 ../hud/blue_bg }}"
    )
}

#[gpui_kit::test]
fn a_team_panel_draws_the_red_players_background(cx: &mut TestAppContext) {
    let env = support::env();
    let (vmt, vtf) = texture("red_bg", 4);
    let s = shell_with_health_blocks(
        cx,
        &env,
        &team_bg("CTFImagePanel"),
        &[
            ("materials/hud/red_bg.vmt", vmt.as_bytes()),
            ("materials/hud/red_bg.vtf", &vtf),
        ],
    );
    let _page = open_maker(cx, &s);
    let key = PartKey::new(TEAM_BG);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_image_cell(&key, 0)
    ));
}

#[gpui_kit::test]
fn a_plain_image_panel_keeps_its_image_whatever_the_team(cx: &mut TestAppContext) {
    let env = support::env();
    let (vmt, vtf) = texture("red_bg", 4);
    let s = shell_with_health_blocks(
        cx,
        &env,
        &team_bg("ImagePanel"),
        &[
            ("materials/hud/red_bg.vmt", vmt.as_bytes()),
            ("materials/hud/red_bg.vtf", &vtf),
        ],
    );
    let _page = open_maker(cx, &s);
    let key = PartKey::new(TEAM_BG);
    assert!(support::present(cx, s.win, ids::hud_maker_part(&key)));
    assert!(
        !support::present(cx, s.win, ids::hud_maker_image_cell(&key, 0)),
        "its blue image is not seeded"
    );
}

#[gpui_kit::test]
fn the_health_cross_draws_the_texture_its_code_paints(cx: &mut TestAppContext) {
    let env = support::env();
    let (vmt, vtf) = texture("health_color", 4);
    let s = shell_with_health_blocks(
        cx,
        &env,
        "",
        &[
            ("materials/hud/health_color.vmt", vmt.as_bytes()),
            ("materials/hud/health_color.vtf", &vtf),
        ],
    );
    let _page = open_maker(cx, &s);
    let key = PartKey::new("HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage");
    let part = bounds(cx, &s, ids::hud_maker_part(&key));
    let cell = bounds(cx, &s, ids::hud_maker_image_cell(&key, 0));
    assert!(
        inside(cell, part) && inside(part, cell),
        "{cell:?} vs {part:?}"
    );
}

const MAIN_BG: &str = "scores/MainBG";

/// The scoreboard with a MainBG whose scheme border is a scalable image, that image seeded
/// when `seeded`.
fn shell_with_main_bg(
    cx: &mut TestAppContext,
    env: &support::Env,
    seeded: bool,
) -> support::ShellWindow {
    let board = support::hud_stock::SCOREBOARD.replacen(
        "  MvMScoreboard",
        "  MainBG { ControlName EditablePanel xpos 0 ypos 38 zpos -1 wide 640 tall 410 border ThinBorder }\n  MvMScoreboard",
        1,
    );
    let scheme = support::hud_stock::SCHEME.replacen(
        "  Fonts\n",
        "  Borders { ThinBorder { bordertype scalable_image image ../hud/panel_brown src_corner_height 2 src_corner_width 2 draw_corner_width 8 draw_corner_height 8 } }\n  Fonts\n",
        1,
    );
    let (vmt, vtf) = texture("panel_brown", 8);
    let extra: Vec<(&str, &[u8])> = if seeded {
        vec![
            ("materials/hud/panel_brown.vmt", vmt.as_bytes()),
            ("materials/hud/panel_brown.vtf", &vtf),
        ]
    } else {
        Vec::new()
    };
    support::seed_stock_game_with_bytes(
        &env.game,
        &[
            ("resource/ui/scoreboard.res", Some(&board)),
            ("resource/clientscheme.res", Some(&scheme)),
        ],
        &extra,
    );
    support::shell_window(cx, env, found(env))
}

#[gpui_kit::test]
fn a_scalable_border_draws_its_image_in_nine_pieces(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_main_bg(cx, &env, true);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let key = PartKey::new(MAIN_BG);
    let part = bounds(cx, &s, ids::hud_maker_part(&key));
    let scale = f(bounds(cx, &s, ids::HUD_MAKER_CANVAS).size.width) / 1920.;
    for i in 0..9 {
        let cell = bounds(cx, &s, ids::hud_maker_border_cell(&key, i));
        assert!(inside(cell, part), "cell {i} {cell:?} leaves {part:?}");
    }
    // Corners are 8 units, 18 px at 1080.
    let corner = bounds(cx, &s, ids::hud_maker_border_cell(&key, 0));
    assert!(
        (f(corner.size.width) - 18. * scale).abs() <= 0.5,
        "{corner:?}"
    );
    let last = bounds(cx, &s, ids::hud_maker_border_cell(&key, 8));
    assert!((f(last.right()) - f(part.right())).abs() <= 0.5);
    assert!((f(last.bottom()) - f(part.bottom())).abs() <= 0.5);
}

#[gpui_kit::test]
fn a_border_whose_image_is_missing_draws_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_main_bg(cx, &env, false);
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let key = PartKey::new(MAIN_BG);
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_border_cell(&key, 0)
    ));
}

#[gpui_kit::test]
fn the_portrait_stays_under_the_player_lists(cx: &mut TestAppContext) {
    let env = support::env();
    let (vmt, vtf) = texture("class_soldierred", 4);
    // The real file's model box: it reaches up under the lists' last rows.
    let board = support::hud_stock::SCOREBOARD.replacen(
        "classmodelpanel { ControlName CTFPlayerModelPanel xpos 10 ypos 340 wide 80 tall 100 }",
        "classmodelpanel { ControlName CTFPlayerModelPanel xpos -10 ypos 185 zpos 10 wide 130 tall 260 }",
        1,
    );
    assert_ne!(board, support::hud_stock::SCOREBOARD);
    let board = board.replace("SectionedListPanel xpos", "SectionedListPanel zpos 20 xpos");
    support::seed_stock_game_with_bytes(
        &env.game,
        &[("resource/ui/scoreboard.res", Some(&board))],
        &[
            ("materials/hud/class_soldierred.vmt", vmt.as_bytes()),
            ("materials/hud/class_soldierred.vtf", &vtf),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let key = PartKey::new("scores/classmodelpanel");
    let part = bounds(cx, &s, ids::hud_maker_part(&key));
    let portrait = bounds(cx, &s, ids::hud_maker_portrait(&key));
    let list = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(BLUE)));
    let scale = f(bounds(cx, &s, ids::HUD_MAKER_CANVAS).size.width) / 1920.;
    assert!(
        f(portrait.top()) >= f(list.bottom()) - 0.5,
        "{portrait:?} under {list:?}"
    );
    // 1037 - 816 = 221 px at 1080, square, still standing on the box's foot.
    assert!(
        (f(portrait.size.height) - 221. * scale).abs() <= 0.5,
        "{portrait:?}"
    );
    assert!((f(portrait.size.width) - f(portrait.size.height)).abs() <= 0.5);
    assert!((f(portrait.bottom()) - f(part.bottom())).abs() <= 0.5);
}

#[gpui_kit::test]
fn your_teams_rows_draw_class_icons(cx: &mut TestAppContext) {
    let env = support::env();
    let (vmt, vtf) = texture("leaderboard_class_soldier", 4);
    support::seed_stock_game_with_bytes(
        &env.game,
        &[],
        &[
            (
                "materials/hud/leaderboard_class_soldier.vmt",
                vmt.as_bytes(),
            ),
            ("materials/hud/leaderboard_class_soldier.vtf", &vtf),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::Scoreboard);
    let (red, blue) = (PartKey::new(RED), PartKey::new(BLUE));
    // You (RED row 0) and Just Joined (row 11) play Soldier; Rocket Jumper (row 1) too.
    let icon = bounds(cx, &s, ids::hud_maker_list_class(&red, 0));
    let row = bounds(cx, &s, list_row(RED, 0));
    let scale = f(bounds(cx, &s, ids::HUD_MAKER_CANVAS).size.width) / 1920.;
    assert!(
        (f(icon.size.width) - 29. * scale).abs() <= 0.5,
        "13 units: {icon:?}"
    );
    assert!((f(icon.size.height) - f(icon.size.width)).abs() <= 0.5);
    let layout = list_layout(cx, &page, RED);
    let column = layout
        .columns()
        .iter()
        .find(|c| c.kind() == ColumnKind::Class)
        .unwrap();
    let column_right = f(row.left()) + (column.x() + column.wide()) as f32 * scale;
    assert!(
        (f(icon.right()) - column_right).abs() <= 0.5,
        "right-aligned"
    );
    assert!(
        !support::present(cx, s.win, ids::hud_maker_list_class(&red, 2)),
        "demo not seeded"
    );
    assert!(
        (0..12).all(|i| !support::present(cx, s.win, ids::hud_maker_list_class(&blue, i))),
        "the other team's classes stay hidden"
    );
}

#[gpui_kit::test]
fn killfeed_lines_draw_the_games_weapon_icons(cx: &mut TestAppContext) {
    let env = support::env();
    let table = r#""sprites/640_hud" { TextureFileRefs { dfile { prefix d_ } }
      TextureData { tf_projectile_rocket { dfile HUD/d_images x 0 y 0 width 6 height 2 } } }"#;
    let sheet =
        tf2tools_core::RgbaImage::from_pixels(8, 4, [235, 226, 202, 255].repeat(32)).unwrap();
    let vtf = tf2tools_core::rgba_to_vtf(&sheet).unwrap();
    let vmt: &[u8] = br#""UnlitGeneric" { "$baseTexture" "HUD/d_images" }"#;
    support::seed_stock_game_with_bytes(
        &env.game,
        &[("scripts/mod_textures.txt", Some(table))],
        &[
            ("materials/hud/d_images.vmt", vmt),
            ("materials/hud/d_images.vtf", &vtf),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    let scale = f(bounds(cx, &s, ids::HUD_MAKER_CANVAS).size.width) / 1920.;
    // Line height 36 px at 1080, the icon 31 px tall; the 6x2 region is 93 px wide.
    for line in [0, 1] {
        let icon = bounds(cx, &s, ids::hud_maker_killfeed_icon(line));
        assert!(
            (f(icon.size.height) - 31. * scale).abs() <= 0.5,
            "{line}: {icon:?}"
        );
        assert!(
            (f(icon.size.width) - 93. * scale).abs() <= 0.5,
            "{line}: {icon:?}"
        );
    }
    // Sticky Situation's pipe is not in the table: the outline box, twice as wide as tall.
    let pipe = bounds(cx, &s, ids::hud_maker_killfeed_icon(2));
    assert!((f(pipe.size.width) - 62. * scale).abs() <= 0.5, "{pipe:?}");
}

// The RED team logo where the stock file mirrors the BLU one, at the board's top right.
fn board_with_red_logo() -> String {
    support::hud_stock::with_blocks(
        support::hud_stock::SCOREBOARD,
        "  RedTeamImage { ControlName ImagePanel xpos 573 ypos 9 wide 60 tall 60 image \"\" }\n",
    )
}

// At 1920x1080 the screen reaches the area's top, so the collapsed list's home lies on the
// logo; it moves to a spot over nothing drawn and stays there.
#[gpui_kit::test]
fn the_collapsed_parts_list_keeps_off_the_red_team_logo(cx: &mut TestAppContext) {
    let env = support::env();
    let board = board_with_red_logo();
    let s = shell_with(cx, &env, &[("resource/ui/scoreboard.res", Some(&board))]);
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(1920.), gpui_kit::px(1080.)),
    );
    support::settle(cx, s.win);
    let page = open_maker(cx, &s);
    set_scene(cx, &s, &page, Scene::Scoreboard);
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_PARTS),
        "the list starts collapsed"
    );
    let logo = bounds(
        cx,
        &s,
        ids::hud_maker_part(&PartKey::new("scores/RedTeamImage")),
    );
    let header = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    assert!(
        !header.intersects(&logo),
        "{header:?} covers the logo {logo:?}"
    );
    let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
    assert!(
        header.left() >= area.left()
            && header.right() <= area.right()
            && header.top() >= area.top(),
        "{header:?} outside {area:?}"
    );
    // No drawn spot depends on the panels, so the layout comes to rest.
    let relayouts = cx.update(|cx| page.read(cx).relayouts());
    for _ in 0..3 {
        support::settle(cx, s.win);
    }
    assert_eq!(bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL), header);
    assert_eq!(cx.update(|cx| page.read(cx).relayouts()), relayouts);
}

const FRIENDS: &str = "MainMenuOverride/FriendsContainer";

#[gpui_kit::test]
fn the_main_menu_tab_shows_the_menu_without_the_hud(cx: &mut TestAppContext) {
    assert_eq!(
        ids::hud_maker_scene(Scene::MainMenu),
        gpui_kit::ElementId::Name("hud-maker-scene:main-menu".into())
    );
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    let friends = ids::hud_maker_part(&PartKey::new(FRIENDS));
    assert!(!support::present(cx, s.win, friends.clone()));
    to_scene(cx, &s, Scene::MainMenu);
    assert_eq!(scene(cx, &page), Some(Scene::MainMenu));
    assert!(support::present(cx, s.win, friends.clone()));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new(HV))
    ));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
    // The friends list is a part like any other: a click on its row selects it.
    click_row(cx, &s, FRIENDS);
    assert_eq!(selection(cx, &page).as_deref(), Some(FRIENDS));
    to_scene(cx, &s, Scene::Hud);
    assert!(!support::present(cx, s.win, friends));
}

#[gpui_kit::test]
fn the_empty_panel_names_the_scene_its_tab_shows(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    let hint = |cx: &mut TestAppContext| support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_EMPTY);
    assert_eq!(
        hint(cx).as_deref(),
        Some("Click a part of the HUD to change it.")
    );
    to_scene(cx, &s, Scene::MainMenu);
    assert_eq!(
        hint(cx).as_deref(),
        Some("Click a part of the main menu to change it.")
    );
    to_scene(cx, &s, Scene::Scoreboard);
    assert_eq!(
        hint(cx).as_deref(),
        Some("Click a part of the scoreboard to change it.")
    );
}

#[gpui_kit::test]
fn a_missing_menu_file_says_the_menu_cannot_show(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[("resource/ui/mainmenuoverride.res", None)]);
    let _page = open_maker(cx, &s);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
    to_scene(cx, &s, Scene::MainMenu);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SCENE_NOTE).as_deref(),
        Some("Can't show the main menu")
    );
}

#[gpui_kit::test]
fn hiding_the_friends_list_on_the_main_menu_tab_hides_it_there(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::MainMenu);
    let friends = ids::hud_maker_part(&PartKey::new(FRIENDS));
    assert!(support::present(cx, s.win, friends.clone()));
    let option = ids::hud_maker_option("hide_friends");
    assert_eq!(
        support::label_of(cx, s.win, option.clone()).as_deref(),
        Some("Hide the friends list")
    );
    click_in_panel(cx, &s, option);
    assert!(!support::present(cx, s.win, friends.clone()));
    // An option is not a part edit, so undoing it from the HUD tab stays there; the main
    // menu shows the friends list again.
    set_scene(cx, &s, &page, Scene::Hud);
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.undo(window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert_eq!(scene(cx, &page), Some(Scene::Hud));
    to_scene(cx, &s, Scene::MainMenu);
    assert!(support::present(cx, s.win, friends));
}

#[gpui_kit::test]
fn undoing_a_main_menu_part_edit_from_another_tab_goes_to_the_main_menu(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    let wide = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            let model = page.read(cx).editor().unwrap().model();
            let part = model.part(&PartKey::new(FRIENDS)).unwrap();
            part.value("wide").map(|v| v.text().to_owned())
        })
    };
    to_scene(cx, &s, Scene::MainMenu);
    click_row(cx, &s, FRIENDS);
    let before = wide(cx);
    commit(cx, &s, &page, Field::Wide, "123");
    let edited = wide(cx);
    assert_ne!(edited, before, "setup: the edit changes the width");
    // A part edit belongs to its file's scene, so undo and redo from the HUD tab both go to
    // the main menu, dropping the hover there as a scene switch does.
    set_scene(cx, &s, &page, Scene::Hud);
    hover(cx, &s, ids::hud_maker_row(&PartKey::new(HV)));
    let undone = after(cx, &s, &page, |p, w, cx| p.undo(w, cx));
    assert_eq!(undone, (Some(Scene::MainMenu), None, false));
    support::settle(cx, s.win);
    assert_eq!(wide(cx), before);
    set_scene(cx, &s, &page, Scene::Hud);
    let redone = after(cx, &s, &page, |p, w, cx| p.redo(w, cx));
    assert_eq!(redone, (Some(Scene::MainMenu), None, false));
    support::settle(cx, s.win);
    assert_eq!(wide(cx), edited);
}

/// The stock game with a chapter list, a character list and their one-colour textures: the
/// plain background 2x2, the wide one 4x2 and the class picture 2x4. `picture` is extra keys
/// in the class picture's block.
fn shell_with_menu_art(
    cx: &mut TestAppContext,
    env: &support::Env,
    picture: &str,
) -> support::ShellWindow {
    let vtf = |wide: u32, tall: u32| {
        let pixels = [40, 80, 120, 255].repeat((wide * tall) as usize);
        let image = tf2tools_core::RgbaImage::from_pixels(wide, tall, pixels).unwrap();
        tf2tools_core::rgba_to_vtf(&image).unwrap()
    };
    let (plain, wide, face) = (vtf(2, 2), vtf(4, 2), vtf(2, 4));
    let vmt = |name: &str| format!(r#""UnlitGeneric" {{ "$baseTexture" "console/{name}" }}"#);
    let (plain_vmt, wide_vmt) = (
        vmt("background_upward"),
        vmt("background_upward_widescreen"),
    );
    let face_vmt = vmt("characters/demoman");
    let menu = support::hud_stock::MAINMENU.replace(
        "  FriendsContainer\n",
        &format!("  TFCharacterImage {{ xpos c-250 ypos -80 zpos -99 wide 600 tall 600 {picture} }}\n  FriendsContainer\n"),
    );
    let chapters = r#""chapters" { 1 "background_upward" }"#;
    let characters =
        r#""characters" { "basic_demoman" { "image" "../console/characters/demoman" } }"#;
    support::seed_stock_game_with_bytes(
        &env.game,
        &[
            ("resource/ui/mainmenuoverride.res", Some(&menu)),
            ("scripts/chapterbackgrounds.txt", Some(chapters)),
            ("scripts/characterbackgrounds.txt", Some(characters)),
        ],
        &[
            (
                "materials/console/background_upward.vmt",
                plain_vmt.as_bytes(),
            ),
            ("materials/console/background_upward.vtf", &plain),
            (
                "materials/console/background_upward_widescreen.vmt",
                wide_vmt.as_bytes(),
            ),
            ("materials/console/background_upward_widescreen.vtf", &wide),
            (
                "materials/console/characters/demoman.vmt",
                face_vmt.as_bytes(),
            ),
            ("materials/console/characters/demoman.vtf", &face),
        ],
    );
    support::shell_window(cx, env, found(env))
}

fn size_of(image: Option<std::sync::Arc<gpui_kit::RenderImage>>) -> Option<(i32, i32)> {
    let size = image?.size(0);
    Some((i32::from(size.width), i32::from(size.height)))
}

#[gpui_kit::test]
fn the_main_menu_draws_the_games_background_and_class_picture(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_menu_art(cx, &env, "");
    let page = open_maker(cx, &s);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MENU_BACKGROUND));
    to_scene(cx, &s, Scene::MainMenu);
    // Stretched over the whole screen box, under the parts; wide at 16:9.
    assert_eq!(
        support::bounds_of(cx, s.win, ids::HUD_MAKER_MENU_BACKGROUND),
        support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS)
    );
    let background =
        |cx: &mut TestAppContext| size_of(cx.update(|cx| page.read(cx).menu_background()));
    assert_eq!(background(cx), Some((4, 2)));
    let picture = PartKey::new("MainMenuOverride/TFCharacterImage");
    assert_eq!(
        size_of(cx.update(|cx| page.read(cx).image(&picture))),
        Some((2, 4))
    );
    assert!(support::present(cx, s.win, ids::hud_maker_part(&picture)));
    // A 4:3 screen takes the plain picture.
    cx.update(|cx| {
        page.update(cx, |p, _| {
            p.editor_mut()
                .unwrap()
                .set_screen(tf2tools_core::Screen::new(1440, 1080))
        })
    });
    support::settle(cx, s.win);
    assert_eq!(background(cx), Some((2, 2)));
    to_scene(cx, &s, Scene::Hud);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MENU_BACKGROUND));
}

#[gpui_kit::test]
fn the_menu_backgrounds_decode_only_once_the_main_menu_opens(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_menu_art(cx, &env, "");
    let page = open_maker(cx, &s);
    let background =
        |cx: &mut TestAppContext| size_of(cx.update(|cx| page.read(cx).menu_background()));
    let reads = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).menu_background_reads());
    assert_eq!(background(cx), None, "the HUD tab decodes no menu picture");
    assert_eq!(reads(cx), 0);
    to_scene(cx, &s, Scene::MainMenu);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_MENU_BACKGROUND));
    assert_eq!(background(cx), Some((4, 2)));
    // Kept for the rest of this HUD: a second visit reads nothing again.
    to_scene(cx, &s, Scene::Hud);
    to_scene(cx, &s, Scene::MainMenu);
    assert_eq!(background(cx), Some((4, 2)));
    assert_eq!(reads(cx), 1);
}

/// Opens the Main menu and draws one frame without letting the background read finish, the
/// moment a player sees right after the click.
fn open_menu_mid_read(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.set_scene(Scene::MainMenu, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
}

fn loading_line_shown(cx: &mut TestAppContext, s: &support::ShellWindow) -> bool {
    cx.update_window(s.win, |_, w, _| {
        w.try_find(ids::HUD_MAKER_MENU_LOADING).is_some()
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_main_menu_says_it_is_loading_the_picture_until_it_lands(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_menu_art(cx, &env, "");
    let page = open_maker(cx, &s);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MENU_LOADING));
    open_menu_mid_read(cx, &s, &page);
    assert!(loading_line_shown(cx, &s), "the wait shows a line");
    let label = cx
        .update_window(s.win, |_, w, _| {
            w.try_find(ids::HUD_MAKER_MENU_LOADING)
                .and_then(|e| e.label().map(str::to_owned))
        })
        .unwrap();
    assert_eq!(label.as_deref(), Some("Loading the menu picture\u{2026}"));
    support::wait_real(cx, "the menu picture", |cx| {
        cx.update(|cx| page.read(cx).menu_background()).is_some()
    });
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MENU_LOADING));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_MENU_BACKGROUND));
}

#[gpui_kit::test]
fn a_hud_without_menu_pictures_never_shows_the_loading_line(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    open_menu_mid_read(cx, &s, &page);
    assert!(!loading_line_shown(cx, &s));
    support::settle(cx, s.win);
    assert!(!loading_line_shown(cx, &s));
}

#[gpui_kit::test]
fn the_loading_line_goes_when_the_read_fails(cx: &mut TestAppContext) {
    let env = support::env();
    let vmt = r#""UnlitGeneric" { "$baseTexture" "console/background_upward_widescreen" }"#;
    support::seed_stock_game_with_bytes(
        &env.game,
        &[(
            "scripts/chapterbackgrounds.txt",
            Some(r#""chapters" { 1 "background_upward" }"#),
        )],
        &[
            (
                "materials/console/background_upward_widescreen.vmt",
                vmt.as_bytes(),
            ),
            (
                "materials/console/background_upward_widescreen.vtf",
                b"not a texture",
            ),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    open_menu_mid_read(cx, &s, &page);
    assert!(loading_line_shown(cx, &s));
    support::settle(cx, s.win);
    assert!(!loading_line_shown(cx, &s));
}

#[gpui_kit::test]
fn only_the_picture_for_the_screen_shape_is_decoded_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_menu_art(cx, &env, "");
    let page = open_maker(cx, &s);
    let held = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).menu_backgrounds_held());
    to_scene(cx, &s, Scene::MainMenu);
    // The default screen is 16:9, so only the wide picture was read.
    assert_eq!(held(cx), 1);
    // A 4:3 screen reads the plain one when it is needed.
    cx.update(|cx| {
        page.update(cx, |p, _| {
            p.editor_mut()
                .unwrap()
                .set_screen(tf2tools_core::Screen::new(1440, 1080))
        })
    });
    support::wait_real(cx, "the plain picture", |cx| {
        support::settle(cx, s.win);
        cx.update(|cx| page.read(cx).menu_background())
            .and_then(|i| size_of(Some(i)))
            == Some((2, 2))
    });
    assert_eq!(held(cx), 2);
    assert_eq!(cx.update(|cx| page.read(cx).menu_background_reads()), 2);
}

#[gpui_kit::test]
fn a_menu_background_that_fails_to_read_settles_on_the_dark_screen(cx: &mut TestAppContext) {
    // Both backgrounds name a texture that is not a VTF at all.
    let env = support::env();
    let vmt = |name: &str| format!(r#""UnlitGeneric" {{ "$baseTexture" "console/{name}" }}"#);
    let (plain_vmt, wide_vmt) = (
        vmt("background_upward"),
        vmt("background_upward_widescreen"),
    );
    support::seed_stock_game_with_bytes(
        &env.game,
        &[(
            "scripts/chapterbackgrounds.txt",
            Some(r#""chapters" { 1 "background_upward" }"#),
        )],
        &[
            (
                "materials/console/background_upward.vmt",
                plain_vmt.as_bytes(),
            ),
            ("materials/console/background_upward.vtf", b"not a texture"),
            (
                "materials/console/background_upward_widescreen.vmt",
                wide_vmt.as_bytes(),
            ),
            (
                "materials/console/background_upward_widescreen.vtf",
                b"not a texture",
            ),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::MainMenu);
    assert!(cx.update(|cx| page.read(cx).menu_background()).is_none());
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MENU_BACKGROUND));
    // Settled: more frames on the tab start no new read.
    support::settle(cx, s.win);
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| page.read(cx).menu_background_reads()), 1);
}

#[gpui_kit::test]
fn a_colour_blind_view_simulates_the_menu_background_once_it_is_read(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_menu_art(cx, &env, "");
    let page = open_maker(cx, &s);
    // The view's images land on the HUD tab, before any menu picture is read.
    let runs = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).simulation_runs());
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.set_vision(tf2tools_core::Vision::Protanopia, cx)
        })
    });
    support::wait_real(cx, "the red-blind images", |cx| runs(cx) == 1);
    support::settle(cx, s.win);
    to_scene(cx, &s, Scene::MainMenu);
    let first = |cx: &mut TestAppContext| {
        let image = cx.update(|cx| page.read(cx).menu_background())?;
        let bytes = image.as_bytes(0)?.to_vec();
        Some([bytes[2], bytes[1], bytes[0], bytes[3]])
    };
    let plain = [40, 80, 120, 255];
    let mut seen = plain.to_vec();
    tf2tools_core::Vision::Protanopia.simulate_rgba(&mut seen);
    support::wait_real(cx, "the red-blind menu picture", |cx| {
        first(cx).map(|px| px.to_vec()) == Some(seen.clone())
    });
    assert_ne!(seen, plain.to_vec());
    assert_eq!(runs(cx), 1, "the part images were not simulated again");
}

#[gpui_kit::test]
fn a_class_picture_the_hud_names_wins_over_the_character_list(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_menu_art(cx, &env, "image ../console/background_upward");
    let page = open_maker(cx, &s);
    let picture = PartKey::new("MainMenuOverride/TFCharacterImage");
    assert_eq!(
        size_of(cx.update(|cx| page.read(cx).image(&picture))),
        Some((2, 2))
    );
}

#[gpui_kit::test]
fn a_hud_without_menu_pictures_draws_the_menu_on_the_dark_screen(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::MainMenu);
    assert!(cx.update(|cx| page.read(cx).menu_background()).is_none());
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MENU_BACKGROUND));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new("MainMenuOverride/FriendsContainer"))
    ));
}

/// A menu footer drawing a colour and holding a label, with a button over it that draws
/// nothing but its icon.
const FOOTER_MENU: &str = r#""Resource/UI/MainMenuOverride.res"
{
  MainMenuOverride { ControlName EditablePanel xpos 0 ypos 0 wide f0 tall f0 }
  FooterBar { ControlName EditablePanel xpos 0 ypos r60 zpos 1 wide f0 tall 60 bgcolor_override "40 40 40 255"
    VersionLabel { ControlName CExLabel xpos 500 ypos 10 wide 80 tall 20 labelText "v1.0" } }
  AchievementsButton { ControlName CExImageButton xpos 300 ypos r50 zpos 3 wide 60 tall 40 labelText ""
    SubImage { ControlName ImagePanel xpos 5 ypos 5 zpos 1 wide 15 tall 15 image glyph_achievements scaleImage 1 } }
}
"#;
const FOOTER: &str = "MainMenuOverride/FooterBar";
const ACHIEVEMENTS: &str = "MainMenuOverride/AchievementsButton";
const ICON: &str = "MainMenuOverride/AchievementsButton/SubImage";
const VERSION: &str = "MainMenuOverride/FooterBar/VersionLabel";

/// The main menu tab of a game whose menu is FOOTER_MENU.
fn footer_menu(cx: &mut TestAppContext) -> (support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    let s = shell_with(
        cx,
        &env,
        &[("resource/ui/mainmenuoverride.res", Some(FOOTER_MENU))],
    );
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::MainMenu);
    (s, page)
}

fn screen_box(
    cx: &mut TestAppContext,
    page: &Entity<HudMakerPage>,
    key: &str,
) -> (i32, i32, i32, i32) {
    cx.update(|cx| {
        let b = page.read(cx).editor().unwrap();
        let b = b.layout().get(&PartKey::new(key)).unwrap();
        (b.x(), b.y(), b.wide(), b.tall())
    })
}

/// The window point over screen pixel (x, y), and window pixels per screen pixel.
fn window_at(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    (x, y): (i32, i32),
) -> (gpui_kit::Point<Pixels>, f32) {
    let canvas = bounds(cx, s, ids::HUD_MAKER_CANVAS);
    let wide = cx.update(|cx| page.read(cx).editor().unwrap().screen().wide());
    let k = f(canvas.size.width) / wide as f32;
    let at = canvas.origin + point(gpui_kit::px(x as f32 * k), gpui_kit::px(y as f32 * k));
    (at, k)
}

/// Dispatches one pointer event to the shell window, then draws a frame.
fn send(cx: &mut TestAppContext, s: &support::ShellWindow, event: impl gpui_kit::InputEvent) {
    cx.update_window(s.win, |_, w, cx| {
        let _ = w.dispatch_event(event.to_platform_input(), cx);
        w.render_frame(cx);
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Presses at `from`, moves by `by` window pixels with Ctrl held so nothing snaps, and lets go;
/// a zero `by` is a plain click, the `click_count`th of a quick run.
fn press_and_move(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    from: gpui_kit::Point<Pixels>,
    by: (f32, f32),
    modifiers: gpui_kit::Modifiers,
    click_count: usize,
) {
    let button = gpui_kit::MouseButton::Left;
    let to = from + point(gpui_kit::px(by.0), gpui_kit::px(by.1));
    send(
        cx,
        s,
        gpui_kit::MouseDownEvent {
            button,
            position: from,
            modifiers,
            click_count,
            first_mouse: false,
        },
    );
    if by != (0., 0.) {
        for step in 1..=4 {
            let t = step as f32 / 4.;
            send(
                cx,
                s,
                gpui_kit::MouseMoveEvent {
                    position: from + point(gpui_kit::px(by.0 * t), gpui_kit::px(by.1 * t)),
                    pressed_button: Some(button),
                    modifiers,
                },
            );
        }
    }
    send(
        cx,
        s,
        gpui_kit::MouseUpEvent {
            button,
            position: to,
            modifiers,
            click_count,
        },
    );
}

fn click_at(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    at: gpui_kit::Point<Pixels>,
    modifiers: gpui_kit::Modifiers,
    click_count: usize,
) {
    press_and_move(cx, s, at, (0., 0.), modifiers, click_count);
}

fn alt() -> gpui_kit::Modifiers {
    gpui_kit::Modifiers {
        alt: true,
        ..Default::default()
    }
}

#[gpui_kit::test]
fn a_press_in_the_selected_parts_empty_area_drags_it_not_the_part_behind(cx: &mut TestAppContext) {
    let (s, page) = footer_menu(cx);
    click_row(cx, &s, ACHIEVEMENTS);
    let (x, y, w, h) = screen_box(cx, &page, ACHIEVEMENTS);
    let footer = screen_box(cx, &page, FOOTER);
    // Bottom right of the button, clear of its icon, over the drawn footer.
    let (from, k) = window_at(cx, &s, &page, (x + w - 4, y + h - 4));
    let free = gpui_kit::Modifiers::secondary_key();
    press_and_move(cx, &s, from, (-30. * k, 0.), free, 1);
    assert_eq!(selection(cx, &page).as_deref(), Some(ACHIEVEMENTS));
    let moved = screen_box(cx, &page, ACHIEVEMENTS);
    assert!((moved.0 - (x - 30)).abs() <= 1, "{moved:?} from {x}");
    assert_eq!(screen_box(cx, &page, FOOTER), footer, "the footer stays");
}

#[gpui_kit::test]
fn a_click_on_a_buttons_icon_selects_the_button_and_alt_the_icon(cx: &mut TestAppContext) {
    let (s, page) = footer_menu(cx);
    support::put_start_card_away(cx, s.win);
    let (x, y, _, _) = screen_box(cx, &page, ICON);
    let (at, _) = window_at(cx, &s, &page, (x + 4, y + 4));
    click_at(cx, &s, at, gpui_kit::Modifiers::default(), 1);
    assert_eq!(selection(cx, &page).as_deref(), Some(ACHIEVEMENTS));
    click_at(cx, &s, at, alt(), 1);
    assert_eq!(selection(cx, &page).as_deref(), Some(ICON), "the icon next");
}

#[gpui_kit::test]
fn alt_and_a_second_click_still_step_through_a_selected_parts_stack(cx: &mut TestAppContext) {
    let (s, page) = footer_menu(cx);
    let none = gpui_kit::Modifiers::default();
    click_row(cx, &s, ACHIEVEMENTS);
    let (x, y, w, h) = screen_box(cx, &page, ACHIEVEMENTS);
    let (at, _) = window_at(cx, &s, &page, (x + w - 4, y + h - 4));
    click_at(cx, &s, at, none, 1);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(ACHIEVEMENTS),
        "a first press keeps the selection"
    );
    click_at(cx, &s, at, none, 1);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(FOOTER),
        "a second press on the spot steps on"
    );
    click_at(cx, &s, at, alt(), 1);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(ACHIEVEMENTS),
        "Alt steps on and wraps round"
    );
    let (other, _) = window_at(cx, &s, &page, (x + w - 8, y + h - 8));
    click_at(cx, &s, other, none, 1);
    click_at(cx, &s, other, none, 2);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(ACHIEVEMENTS),
        "a double-click keeps the selection"
    );
}

#[gpui_kit::test]
fn a_click_on_a_buttons_icon_over_the_selected_footer_selects_the_button(cx: &mut TestAppContext) {
    let (s, page) = footer_menu(cx);
    click_row(cx, &s, FOOTER);
    let (x, y, _, _) = screen_box(cx, &page, ICON);
    let (at, _) = window_at(cx, &s, &page, (x + 4, y + 4));
    click_at(cx, &s, at, gpui_kit::Modifiers::default(), 1);
    assert_eq!(selection(cx, &page).as_deref(), Some(ACHIEVEMENTS));
}

#[gpui_kit::test]
fn a_drawn_child_takes_the_press_inside_its_selected_panel(cx: &mut TestAppContext) {
    let (s, page) = footer_menu(cx);
    let none = gpui_kit::Modifiers::default();
    click_row(cx, &s, FOOTER);
    let (x, y, w, h) = screen_box(cx, &page, VERSION);
    let (at, _) = window_at(cx, &s, &page, (x + w / 2, y + h / 2));
    click_at(cx, &s, at, none, 1);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(VERSION),
        "what paints over the selection takes the press"
    );
    click_at(cx, &s, at, none, 1);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(FOOTER),
        "a second click reaches the panel"
    );
    click_at(cx, &s, at, alt(), 1);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(VERSION),
        "Alt steps on"
    );
    click_at(cx, &s, at, alt(), 1);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(FOOTER),
        "and Alt again"
    );
}

/// The stock rank area: the medal panel, the level panel and the gear that picks the rank type.
const RANK_MENU: &str = r#""Resource/UI/MainMenuOverride.res"
{
  MainMenuOverride { ControlName EditablePanel xpos 0 ypos 0 wide f0 tall f0 }
  RankModelPanel { ControlName CPvPRankPanel xpos cs-0.5-256 ypos cs-0.5-120 zpos -51 wide 1000 tall 1000 visible 0 proportionaltoparent 1 show_progress 0 }
  RankPanel { ControlName CPvPRankPanel xpos c-350 ypos 72 zpos -52 wide 320 tall 100 visible 0 proportionaltoparent 1 show_model 0 show_type 1 }
}
"#;

#[gpui_kit::test]
fn the_main_menu_draws_a_sample_rank_where_the_game_draws_yours(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(
        cx,
        &env,
        &[("resource/ui/mainmenuoverride.res", Some(RANK_MENU))],
    );
    let _page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::MainMenu);
    let level = PartKey::new("MainMenuOverride/RankPanel");
    let model = PartKey::new("MainMenuOverride/RankModelPanel");
    // The level panel draws the sample rank's name and a level bar three fifths full.
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_rank_name(&level)).as_deref(),
        Some("Level 42")
    );
    let bar = bounds(cx, &s, ids::hud_maker_rank_bar(&level));
    let filled = bounds(cx, &s, ids::hud_maker_rank_fill(&level));
    assert!(f(bar.size.width) > 0. && f(bar.size.height) > 0.);
    // The fill sits a unit inside the track and is three fifths of its own width.
    assert!(f(filled.left()) > f(bar.left()) && f(filled.top()) > f(bar.top()));
    assert!(f(filled.right()) < f(bar.right()) && f(filled.bottom()) < f(bar.bottom()));
    let ratio = f(filled.size.width) / f(bar.size.width);
    assert!((0.55..0.6).contains(&ratio), "filled {ratio}");
    // Its medal is off; the medal panel draws the medal as the plain stand-in, beside the name
    // and nothing else.
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_stand_in(&level)
    ));
    let medal = bounds(cx, &s, ids::hud_maker_stand_in(&model));
    assert!((f(medal.size.width) - f(medal.size.height)).abs() < 0.5);
    assert!(medal.right() < bar.right() && medal.bottom() > bar.top());
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_rank_bar(&model)
    ));
}

fn resize(cx: &mut TestAppContext, s: &support::ShellWindow, w: f32, h: f32) {
    cx.simulate_window_resize(s.win, gpui_kit::size(gpui_kit::px(w), gpui_kit::px(h)));
    support::settle(cx, s.win);
}

fn select(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    key: &PartKey,
) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(key.clone()));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
}

/// Moves the pointer to `at` with no button held.
fn point_at(cx: &mut TestAppContext, s: &support::ShellWindow, at: gpui_kit::Point<Pixels>) {
    cx.update_window(s.win, |_, w, cx| {
        let moved = gpui_kit::MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers: Default::default(),
        };
        let _ = w.dispatch_event(gpui_kit::InputEvent::to_platform_input(moved), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
}

// Every part the scene draws, with where its shown box lies in the window.
fn drawn_parts(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
) -> Vec<(PartKey, Bounds<Pixels>)> {
    let canvas = bounds(cx, s, ids::HUD_MAKER_CANVAS);
    cx.update(|cx| {
        let editor = page.read(cx).editor().unwrap();
        let scale = f(canvas.size.width) / editor.screen().wide() as f32;
        let layout = editor.layout();
        (layout.paint_order())
            .filter(|b| b.hidden().is_none())
            .filter_map(|b| {
                let (x, y, w, h) = layout.shown(b.key())?;
                let at = point(
                    canvas.left() + gpui_kit::px(x as f32 * scale),
                    canvas.top() + gpui_kit::px(y as f32 * scale),
                );
                let size = gpui_kit::size(
                    gpui_kit::px(w as f32 * scale),
                    gpui_kit::px(h as f32 * scale),
                );
                (w > 0 && h > 0).then(|| (b.key().clone(), Bounds::new(at, size)))
            })
            .collect()
    })
}

/// A main menu packed with filled panels in a grid whose gaps are narrower than the part list,
/// so every spot for the open list covers some of the menu.
const CROWDED_MENU: &str = r#""Resource/UI/MainMenuOverride.res"
{
  MainMenuOverride { ControlName EditablePanel xpos 0 ypos 0 wide f0 tall f0 }
  Box00 { ControlName EditablePanel xpos 10 ypos 15 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box01 { ControlName EditablePanel xpos 220 ypos 15 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box02 { ControlName EditablePanel xpos 430 ypos 15 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box03 { ControlName EditablePanel xpos 640 ypos 15 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box10 { ControlName EditablePanel xpos 10 ypos 175 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box11 { ControlName EditablePanel xpos 220 ypos 175 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box12 { ControlName EditablePanel xpos 430 ypos 175 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box13 { ControlName EditablePanel xpos 640 ypos 175 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box20 { ControlName EditablePanel xpos 10 ypos 335 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box21 { ControlName EditablePanel xpos 220 ypos 335 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box22 { ControlName EditablePanel xpos 430 ypos 335 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
  Box23 { ControlName EditablePanel xpos 640 ypos 335 wide 190 tall 130 visible 1 bgcolor_override "40 40 40 200" }
}
"#;

// Selecting part after part, none of them under the open list, never moves or folds it: the
// list's spot follows what it covers, not which part is selected.
fn the_list_stays_while_the_selection_is_elsewhere_on(
    scene: Scene,
    (w, h): (f32, f32),
    cx: &mut TestAppContext,
) {
    let env = support::env();
    let s = shell_with(
        cx,
        &env,
        &[("resource/ui/mainmenuoverride.res", Some(CROWDED_MENU))],
    );
    resize(cx, &s, w, h);
    let page = open_maker(cx, &s);
    set_scene(cx, &s, &page, scene);
    show_parts(cx, &s);
    support::settle(cx, s.win);
    let list = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    let near = Bounds::new(
        list.origin - point(gpui_kit::px(16.), gpui_kit::px(16.)),
        list.size + gpui_kit::size(gpui_kit::px(32.), gpui_kit::px(32.)),
    );
    let away: Vec<PartKey> = (drawn_parts(cx, &s, &page).into_iter())
        .filter(|(_, b)| !b.intersects(&near))
        .map(|(key, _)| key)
        .collect();
    assert!(away.len() >= 3, "{scene:?}: only {away:?} lie off {list:?}");
    let picks = [&away[0], &away[away.len() / 2], &away[away.len() - 1]];
    for key in picks {
        select(cx, &s, &page, key);
        assert_eq!(selection(cx, &page).as_deref(), Some(key.as_str()));
        let now = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
        assert_eq!(now.origin, list.origin, "{scene:?}: {key:?} moved the list");
        assert!(
            support::present(cx, s.win, ids::HUD_MAKER_PARTS),
            "{scene:?}: {key:?} folded the list"
        );
    }
}

#[gpui_kit::test]
fn on_the_main_menu_a_selection_elsewhere_leaves_the_list_where_it_is(cx: &mut TestAppContext) {
    the_list_stays_while_the_selection_is_elsewhere_on(Scene::MainMenu, (1920., 1080.), cx);
}

#[gpui_kit::test]
fn on_the_hud_a_selection_elsewhere_leaves_the_list_where_it_is(cx: &mut TestAppContext) {
    the_list_stays_while_the_selection_is_elsewhere_on(Scene::Hud, (1920., 1080.), cx);
}

#[gpui_kit::test]
fn at_800x600_on_the_hud_a_selection_elsewhere_leaves_the_list_where_it_is(
    cx: &mut TestAppContext,
) {
    the_list_stays_while_the_selection_is_elsewhere_on(Scene::Hud, (800., 600.), cx);
}

#[gpui_kit::test]
fn at_1280x800_on_the_hud_a_selection_elsewhere_leaves_the_list_where_it_is(
    cx: &mut TestAppContext,
) {
    the_list_stays_while_the_selection_is_elsewhere_on(Scene::Hud, (1280., 800.), cx);
}

// The chat selected at 1280x800 sends the tall side panel off the chat lines; it settles clear
// of the open list too, and the list stays where it stood.
#[gpui_kit::test]
fn at_1280x800_the_side_panel_leaving_the_chat_keeps_off_the_list(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(
        cx,
        &env,
        &[("resource/ui/mainmenuoverride.res", Some(CROWDED_MENU))],
    );
    resize(cx, &s, 1280., 800.);
    let page = open_maker(cx, &s);
    set_scene(cx, &s, &page, Scene::Hud);
    show_parts(cx, &s);
    support::settle(cx, s.win);
    let list = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    let chat = PartKey::new("HudChat");
    // Through the list's row, as a player picks it.
    support::click_part_row(cx, s.win, &chat);
    assert_eq!(selection(cx, &page).as_deref(), Some(chat.as_str()));
    support::settle(cx, s.win);
    let side = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    // The chat's box, and its drawn lines when any draw.
    let chat = (drawn_parts(cx, &s, &page).into_iter())
        .find(|(key, _)| key.as_str() == "HudChat")
        .map(|(_, b)| b)
        .expect("the chat draws");
    let lines = cx.update(|cx| page.read(cx).chat_band());
    let now = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    assert_eq!(now.origin, list.origin, "the list moved");
    assert!(
        !side.intersects(&now),
        "the side panel {side:?} lies on the list {now:?}"
    );
    for on in std::iter::once(chat).chain(lines) {
        assert!(
            !side.intersects(&on),
            "the side panel {side:?} lies on the chat {on:?}"
        );
    }
    // Settling again changes nothing.
    support::settle(cx, s.win);
    assert_eq!(
        bounds(cx, &s, ids::HUD_MAKER_PANEL),
        side,
        "the side panel moved again"
    );
    // Cut short, it keeps its minimum height, and its body scrolls "All settings" into reach.
    assert!(
        side.size.height >= gpui_kit::px(SIDE_PANEL_MIN),
        "the side panel {side:?} is under its minimum height"
    );
    for _ in 0..80 {
        let body = bounds(cx, &s, ids::HUD_MAKER_PANEL_BODY);
        let all = bounds(cx, &s, ids::HUD_MAKER_ALL_SETTINGS);
        if all.top() >= body.top() && all.bottom() <= body.bottom() {
            break;
        }
        let turn = ScrollDelta::Lines(point(0., -1.));
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::HUD_MAKER_PANEL_BODY, turn, cx)
        })
        .unwrap();
        support::settle(cx, s.win);
    }
    let body = bounds(cx, &s, ids::HUD_MAKER_PANEL_BODY);
    let all = bounds(cx, &s, ids::HUD_MAKER_ALL_SETTINGS);
    assert!(
        all.top() >= body.top() && all.bottom() <= body.bottom(),
        "\"All settings\" {all:?} never scrolled fully into the body {body:?}"
    );
    assert!(
        body.bottom() <= side.bottom(),
        "the body runs past the panel"
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_ALL_SETTINGS);
    assert!(
        support::present(cx, s.win, ids::hud_maker_setting("xpos")),
        "\"All settings\" did not open"
    );
}

// The collapsed list keeps its spot from the HUD tab on the Scoreboard tab while nothing the
// scoreboard draws lies under it.
#[gpui_kit::test]
fn a_tab_switch_keeps_the_collapsed_list_where_it_stays_clear(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    resize(cx, &s, 1920., 1080.);
    let page = open_maker(cx, &s);
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_PARTS),
        "starts collapsed"
    );
    let bar = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    set_scene(cx, &s, &page, Scene::Scoreboard);
    let under: Vec<PartKey> = (drawn_parts(cx, &s, &page).into_iter())
        .filter(|(_, b)| b.intersects(&bar))
        .map(|(key, _)| key)
        .collect();
    assert!(
        under.is_empty(),
        "setup: the scoreboard draws {under:?} under {bar:?}"
    );
    let now = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    assert_eq!(now.origin, bar.origin, "the bar moved");
}

// How much of the drawn screen the collapsed list covers in a window resized to `w`x`h`, or in
// a fresh window that size when `from` is `None`.
fn collapsed_list_over_screen(
    cx: &mut TestAppContext,
    from: Option<(f32, f32)>,
    (w, h): (f32, f32),
) -> f32 {
    let env = support::env();
    let s = shell_with(cx, &env, &[]);
    if let Some((w, h)) = from {
        resize(cx, &s, w, h);
    }
    let _page = open_maker(cx, &s);
    resize(cx, &s, w, h);
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_PARTS),
        "collapsed"
    );
    let over = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL).intersect(&bounds(
        cx,
        &s,
        ids::HUD_MAKER_CANVAS,
    ));
    f(over.size.width).max(0.) * f(over.size.height).max(0.)
}

// A resize weighs the collapsed list afresh against the new screen, so it covers no more of it
// than in a fresh window that size.
#[gpui_kit::test]
fn a_resize_keeps_the_collapsed_list_off_the_screen_as_a_fresh_window_does(
    cx: &mut TestAppContext,
) {
    let mut over = Vec::new();
    for (from, to) in [
        ((1024., 768.), (1920., 1080.)),
        ((800., 600.), (1600., 900.)),
    ] {
        let resized = collapsed_list_over_screen(cx, Some(from), to);
        let fresh = collapsed_list_over_screen(cx, None, to);
        if resized > fresh + 1. {
            over.push((from, to, resized, fresh));
        }
    }
    assert!(over.is_empty(), "{over:?}");
}

// Editing the selection elsewhere changes what the canvas draws, but the open list covers
// neither the selection nor the killfeed, so it stays.
#[gpui_kit::test]
fn shrinking_a_selection_elsewhere_leaves_the_list_where_it_is(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with(
        cx,
        &env,
        &[("resource/ui/mainmenuoverride.res", Some(CROWDED_MENU))],
    );
    resize(cx, &s, 1920., 1080.);
    let page = open_maker(cx, &s);
    support::maker_page::float_panels(cx, &s, &page);
    set_scene(cx, &s, &page, Scene::MainMenu);
    show_parts(cx, &s);
    support::settle(cx, s.win);
    let list = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    let drawn = drawn_parts(cx, &s, &page);
    assert!(
        drawn
            .iter()
            .any(|(k, b)| k.as_str() != "MainMenuOverride" && b.intersects(&list)),
        "setup: the list covers some of the menu"
    );
    // The box nearest the list that it does not cover, which frees room beside it once shrunk.
    let gap = |b: &Bounds<Pixels>| {
        let dx = f((b.left() - list.right()).max(list.left() - b.right()));
        let dy = f((b.top() - list.bottom()).max(list.top() - b.bottom()));
        dx.max(0.) + dy.max(0.)
    };
    let (key, _) = (drawn.iter())
        .filter(|(k, b)| k.as_str() != "MainMenuOverride" && !b.intersects(&list))
        .min_by(|a, b| gap(&a.1).total_cmp(&gap(&b.1)))
        .expect("a box off the list")
        .clone();
    select(cx, &s, &page, &key);
    commit(cx, &s, &page, Field::Wide, "4");
    commit(cx, &s, &page, Field::Tall, "4");
    // A hover redraws the page, which settles against the new drawing; the selection's row is
    // the one the list keeps in view.
    hover(cx, &s, ids::hud_maker_row(&key));
    let now = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
    assert_eq!(now.origin, list.origin, "shrinking {key:?} moved the list");
}

fn menu_vtf(wide: u32, tall: u32) -> Vec<u8> {
    let pixels = vec![200; (wide * tall * 4) as usize];
    let image = tf2tools_core::RgbaImage::from_pixels(wide, tall, pixels).unwrap();
    tf2tools_core::rgba_to_vtf(&image).unwrap()
}

/// Draws one Main menu frame, which starts the background read, then leaves for the HUD tab
/// before it finishes, so `change` begins no read of its own and only the stale one lands.
fn change_during_menu_read(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    change: impl FnOnce(&mut HudMakerPage, &mut gpui_kit::Window, &mut gpui_kit::Context<HudMakerPage>),
) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.set_scene(Scene::MainMenu, window, cx));
        window.render_frame(cx);
        page.update(cx, |p, cx| {
            p.set_scene(Scene::Hud, window, cx);
            change(p, window, cx);
        });
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_menu_background_read_from_before_a_pick_is_dropped(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_menu_art(cx, &env, "");
    let page = open_maker(cx, &s);
    change_during_menu_read(cx, &s, &page, |p, window, cx| {
        p.apply_menu_background(menu_vtf(1024, 1024), menu_vtf(2048, 1024), window, cx)
    });
    to_scene(cx, &s, Scene::MainMenu);
    support::settle(cx, s.win);
    assert_eq!(
        size_of(cx.update(|cx| page.read(cx).menu_background())),
        Some((2048, 1024)),
        "the HUD's own picture from the older read landed over the pick"
    );
}

#[gpui_kit::test]
fn a_menu_background_read_from_before_an_undo_is_dropped(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_menu_art(cx, &env, "");
    let page = open_maker(cx, &s);
    to_scene(cx, &s, Scene::MainMenu);
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            p.apply_menu_background(menu_vtf(1024, 1024), menu_vtf(2048, 1024), window, cx);
            p.set_scene(Scene::Hud, window, cx);
            p.undo(window, cx);
        })
    })
    .unwrap();
    support::settle(cx, s.win);
    to_scene(cx, &s, Scene::MainMenu);
    support::settle(cx, s.win);
    assert_eq!(
        size_of(cx.update(|cx| page.read(cx).menu_background())),
        Some((4, 2)),
        "the undone pick's picture from the older read landed"
    );
}
