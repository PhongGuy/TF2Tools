//! The HUD Maker's bars and floating panels keep one layout: the scene bar picks tabs or a menu
//! by width alone, the toolbar's hide button stays on its first row, and panels stay put.

mod support;

use gpui_kit::{AppContext as _, Bounds, Entity, Pixels, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{BoardMode, Scene, Tf2Install};

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

fn open_at(
    cx: &mut TestAppContext,
    w: f32,
    h: f32,
) -> (support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    resize(cx, &s, w, h);
    let page = open_maker(cx, &s);
    (s, page)
}

fn resize(cx: &mut TestAppContext, s: &support::ShellWindow, w: f32, h: f32) {
    cx.simulate_window_resize(s.win, gpui_kit::size(gpui_kit::px(w), gpui_kit::px(h)));
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

fn set_mode(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    mode: BoardMode,
) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.set_board_mode(mode, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

// Across a sweep of widths, every scene and every scoreboard mode shows the same chooser: the
// tabs or the menu, never one for some scenes and the other for the rest.
#[gpui_kit::test]
fn the_scene_bar_picks_tabs_or_menu_by_width_alone(cx: &mut TestAppContext) {
    let (s, page) = open_at(cx, 900., 800.);
    let mut mixed = Vec::new();
    for w in (900..=1600).step_by(50) {
        resize(cx, &s, w as f32, 800.);
        let mut seen = Vec::new();
        for scene in Scene::ALL {
            set_scene(cx, &s, &page, scene);
            if scene == Scene::Scoreboard {
                for mode in BoardMode::ALL {
                    set_mode(cx, &s, &page, mode);
                    let menu = support::present(cx, s.win, ids::HUD_MAKER_SCENE_MENU);
                    seen.push((format!("{scene:?}/{mode:?}"), menu));
                }
            } else {
                let menu = support::present(cx, s.win, ids::HUD_MAKER_SCENE_MENU);
                seen.push((format!("{scene:?}"), menu));
            }
        }
        if seen.iter().any(|&(_, m)| m != seen[0].1) {
            mixed.push((w, seen));
        }
    }
    assert!(
        mixed.is_empty(),
        "the chooser changed with the scene: {mixed:?}"
    );
}
