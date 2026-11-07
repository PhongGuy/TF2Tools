//! The Spectator tab: what it draws, its stub note, its hint and its edits in the shared list.

use crate::support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, Tf2Install};

const TIP: &str = "Spectator/TipLabel";
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
fn the_tab_is_last_and_switches_to_the_spectator_bar(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    assert!(format!("{:?}", ids::hud_maker_scene(Scene::Spectator)).contains("spectator"));
    support::click_scene(cx, s.win, Scene::Spectator);
    assert_eq!(
        cx.update(|cx| page.read(cx).scene()),
        Some(Scene::Spectator)
    );
    // Its own parts, none of the HUD's or the class menu's, and no crosshair.
    for key in [
        TIP,
        "Spectator/topbar",
        "Spectator/bottombarblank",
        "Spectator/MapLabel",
    ] {
        assert!(present(cx, &s, key), "{key}");
    }
    assert!(!present(cx, &s, HV) && !present(cx, &s, "class/soldier"));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_CROSSHAIR));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
    // What the game hides stays off the canvas: the buy-back line and the item panel.
    assert!(!present(cx, &s, "Spectator/BuyBackLabel") && !present(cx, &s, "Spectator/itempanel"));
    support::click_scene(cx, s.win, Scene::Hud);
    assert!(present(cx, &s, HV) && !present(cx, &s, TIP));
}

#[gpui_kit::test]
fn a_hud_with_a_stub_spectator_file_shows_it_read_only_with_the_include_note(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    support::seed_stock_game_with(
        &env.game,
        &[
            (
                "resource/ui/spectator_variant.res",
                Some(support::hud_stock::SPECTATOR),
            ),
            (
                "resource/ui/spectator.res",
                Some("#base \"spectator_variant.res\"\n"),
            ),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Spectator);
    assert!(present(cx, &s, TIP));
    click_row(cx, &s, TIP);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_FROM_BASE));
}

#[gpui_kit::test]
fn nothing_picked_the_panel_says_how_to_start(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Spectator);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_EMPTY).as_deref(),
        Some("Click a part of the spectator bar to change it.")
    );
}

#[gpui_kit::test]
fn a_hud_that_moves_the_tip_moves_it_on_the_canvas(cx: &mut TestAppContext) {
    let env = support::env();
    let moved = support::hud_stock::SPECTATOR.replace(
        "TipLabel { ControlName CExLabel xpos 15 ypos r67",
        "TipLabel { ControlName CExLabel xpos 200 ypos 300",
    );
    assert_ne!(moved, support::hud_stock::SPECTATOR);
    support::seed_stock_game_with(&env.game, &[("resource/ui/spectator.res", Some(&moved))]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Spectator);
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&PartKey::new(TIP))).unwrap();
    // The canvas is 16:9 at scale 2.25 (853.3 x 480 game units): x 200 and y 300.
    let fx = f32::from(part.left() - canvas.left()) / f32::from(canvas.size.width);
    let fy = f32::from(part.top() - canvas.top()) / f32::from(canvas.size.height);
    assert!((fx - 200. / (1920. / 2.25)).abs() < 0.005, "{fx}");
    assert!((fy - 300. / 480.).abs() < 0.005, "{fy}");
}

#[gpui_kit::test]
fn edits_in_the_spectator_bar_and_the_hud_share_one_list_and_undo_switches_tabs(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Spectator);
    click_row(cx, &s, TIP);
    // The X field is a screen pixel at scale 2.25: 225 px is game unit 100.
    commit(cx, &s, &page, Field::X, "225");
    assert_eq!(xpos(cx, &page, TIP).as_deref(), Some("100"));
    support::click_scene(cx, s.win, Scene::Hud);
    // Undo walks back to where the edit was made, switching tabs.
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.undo(w, cx)),
        Some(Scene::Spectator)
    );
    assert_eq!(xpos(cx, &page, TIP).as_deref(), Some("15"));
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.redo(w, cx)),
        Some(Scene::Spectator)
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
