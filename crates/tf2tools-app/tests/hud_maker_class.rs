//! The Class select tab: what it draws, its stub note, its hint and its edits in the shared list.

use crate::support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, Tf2Install};

const SOLDIER: &str = "class/soldier";
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

fn present(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> bool {
    support::present(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
}

#[gpui_kit::test]
fn the_tab_is_last_and_switches_to_the_class_menu(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    let tab = ids::hud_maker_scene(Scene::ClassSelect);
    assert!(format!("{tab:?}").contains("class-select"));
    assert!(support::present(cx, s.win, tab.clone()));
    support::click_scene(cx, s.win, Scene::ClassSelect);
    assert_eq!(
        cx.update(|cx| page.read(cx).scene()),
        Some(Scene::ClassSelect)
    );
    // Its own parts, none of the HUD's, and no crosshair.
    assert!(
        present(cx, &s, SOLDIER)
            && present(cx, &s, "class/numSoldier")
            && present(cx, &s, "class/CancelButton")
    );
    assert!(!present(cx, &s, HV) && !present(cx, &s, "WinPanel/WinningTeamLabel"));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_CROSSHAIR));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
    // What the game hides stays off the canvas: the select label (a class is chosen) and the MvM panels.
    assert!(!present(cx, &s, "class/ClassMenuSelect") && !present(cx, &s, "class/ResetButton"));
    assert!(!present(cx, &s, "class/StartExplanation"));
    support::click_scene(cx, s.win, Scene::Hud);
    assert!(present(cx, &s, HV) && !present(cx, &s, SOLDIER));
}

#[gpui_kit::test]
fn a_hud_with_a_stub_class_file_shows_it_read_only_with_the_include_note(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(
        &env.game,
        &[
            (
                "resource/ui/classselection_variant.res",
                Some(&support::hud_stock::class_select()),
            ),
            (
                "resource/ui/classselection.res",
                Some("#base \"classselection_variant.res\"\n"),
            ),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    assert!(present(cx, &s, SOLDIER));
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::click_part_row(cx, s.win, &PartKey::new(SOLDIER));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_FROM_BASE));
}

#[gpui_kit::test]
fn nothing_picked_the_panel_says_how_to_start(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_EMPTY).as_deref(),
        Some("Click a part of the class select menu to change it.")
    );
}

#[gpui_kit::test]
fn a_hud_that_moves_a_button_moves_it_on_the_canvas(cx: &mut TestAppContext) {
    let env = support::env();
    let moved = support::hud_stock::class_select().replace(
        "soldier { ControlName CExImageButton xpos c-250 ypos -5",
        "soldier { ControlName CExImageButton xpos c-100 ypos 40",
    );
    assert_ne!(moved, support::hud_stock::class_select());
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/classselection.res", Some(&moved))],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&PartKey::new(SOLDIER))).unwrap();
    // The canvas is 16:9 at scale 2.25 (853.3 x 480 game units): centre 426.7 less 100, and y 40.
    let fx = f32::from(part.left() - canvas.left()) / f32::from(canvas.size.width);
    let fy = f32::from(part.top() - canvas.top()) / f32::from(canvas.size.height);
    assert!(
        (fx - (1920. / 2.25 / 2. - 100.) / (1920. / 2.25)).abs() < 0.005,
        "{fx}"
    );
    assert!((fy - 40. / 480.).abs() < 0.005, "{fy}");
}

#[gpui_kit::test]
fn edits_in_the_class_menu_and_the_hud_share_one_list_and_undo_switches_tabs(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    click_row(cx, &s, SOLDIER);
    // The X field is a screen pixel at scale 2.25: 737 px lands 100 game units left of the centre (the game-unit centre is the integer 426).
    commit(cx, &s, &page, Field::X, "737");
    assert_eq!(xpos(cx, &page, SOLDIER).as_deref(), Some("c-100"));
    support::click_scene(cx, s.win, Scene::Hud);
    // Undo walks back to where the edit was made, switching tabs.
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.undo(w, cx)),
        Some(Scene::ClassSelect)
    );
    assert_eq!(xpos(cx, &page, SOLDIER).as_deref(), Some("c-250"));
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.redo(w, cx)),
        Some(Scene::ClassSelect)
    );
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
