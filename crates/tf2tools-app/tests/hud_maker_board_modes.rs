//! The scoreboard's mode dropdown: what each mode draws, undo across modes and a narrow window.

use crate::support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_app::services::Tf2State;
use tf2tools_core::{BoardMode, PartKey, Scene, Tf2Install};

const LIST: &str = "scores/MvMScoreboard/MvMPlayerList";
const LABEL: &str = "scores/ServerLabel";

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

fn present(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> bool {
    support::present(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
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

fn mode(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<BoardMode> {
    cx.update(|cx| page.read(cx).board_mode())
}

// The dropdown's items exist only while it is open.
fn pick_mode(cx: &mut TestAppContext, s: &support::ShellWindow, m: BoardMode) {
    if !support::present(cx, s.win, ids::hud_maker_board_mode(m)) {
        support::click_id(cx, s.win, ids::HUD_MAKER_BOARD_MODE);
    }
    support::click_id(cx, s.win, ids::hud_maker_board_mode(m));
}

#[gpui_kit::test]
fn the_dropdown_shows_on_the_scoreboard_tab_only(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_BOARD_MODE));
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_BOARD_MODE));
    assert_eq!(mode(cx, &page), Some(BoardMode::Casual));
    support::click_scene(cx, s.win, Scene::Spectator);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_BOARD_MODE));
}

#[gpui_kit::test]
fn each_mode_draws_its_own_parts(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert!(present(cx, &s, "scores/BluePlayerList") && !present(cx, &s, LIST));
    pick_mode(cx, &s, BoardMode::Mvm);
    assert_eq!(mode(cx, &page), Some(BoardMode::Mvm));
    assert!(present(cx, &s, LIST) && present(cx, &s, "scores/MvMScoreboard/PopFileLabel"));
    assert!(
        !present(cx, &s, "scores/BluePlayerList"),
        "the game hides the casual lists"
    );
    pick_mode(cx, &s, BoardMode::Competitive);
    assert!(present(cx, &s, "scores/BlueTeamLabel") && !present(cx, &s, LIST));
    pick_mode(cx, &s, BoardMode::Casual);
    assert!(!present(cx, &s, "scores/BlueTeamLabel") && present(cx, &s, "scores/BluePlayerList"));
}

#[gpui_kit::test]
fn undo_of_an_mvm_edit_to_the_shared_file_returns_to_mvm(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    pick_mode(cx, &s, BoardMode::Mvm);
    click_row(cx, &s, LABEL);
    // The X field is a screen pixel at scale 2.25, and the scoreboard's panel is centred
    // (240 px in): 465 px is xpos 100 inside it.
    commit(cx, &s, &page, Field::X, "465");
    assert_eq!(xpos(cx, &page, LABEL).as_deref(), Some("100"));
    pick_mode(cx, &s, BoardMode::Casual);
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.undo(w, cx)),
        Some(Scene::Scoreboard)
    );
    assert_eq!(
        mode(cx, &page),
        Some(BoardMode::Mvm),
        "same scene, other mode: followed"
    );
    assert_eq!(xpos(cx, &page, LABEL).as_deref(), Some("8"));
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.redo(w, cx)),
        Some(Scene::Scoreboard)
    );
    assert_eq!(xpos(cx, &page, LABEL).as_deref(), Some("100"));
}

#[gpui_kit::test]
fn undo_of_a_casual_edit_from_mvm_returns_to_casual(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    click_row(cx, &s, LABEL);
    commit(cx, &s, &page, Field::X, "465");
    assert_eq!(xpos(cx, &page, LABEL).as_deref(), Some("100"));
    pick_mode(cx, &s, BoardMode::Mvm);
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.undo(w, cx)),
        Some(Scene::Scoreboard)
    );
    assert_eq!(mode(cx, &page), Some(BoardMode::Casual));
    assert_eq!(xpos(cx, &page, LABEL).as_deref(), Some("8"));
}

#[gpui_kit::test]
fn a_narrow_window_collapses_the_tabs_and_keeps_the_dropdown(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(560.), gpui_kit::px(600.)),
    );
    support::settle(cx, s.win);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert!(
        support::present(cx, s.win, ids::HUD_MAKER_SCENE_MENU),
        "tabs collapsed"
    );
    assert!(support::present(cx, s.win, ids::HUD_MAKER_BOARD_MODE));
    pick_mode(cx, &s, BoardMode::Mvm);
    assert_eq!(mode(cx, &page), Some(BoardMode::Mvm));
}
