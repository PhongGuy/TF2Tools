//! The HUD Maker's bars and floating panels keep one layout: the scene bar picks tabs or a menu
//! by width alone, the toolbar's hide button stays on its first row, and panels stay put.

use crate::support;

use gpui_kit::{AppContext as _, Bounds, Entity, Pixels, TestAppContext, point};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{BoardMode, PartKey, Scene, Tf2Install};

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

// At 900x600 the hide button shares the toolbar's first row with the screen menu.
#[gpui_kit::test]
fn at_900x600_the_hide_button_stays_on_the_toolbars_first_row(cx: &mut TestAppContext) {
    let (s, page) = open_at(cx, 900., 600.);
    let mut wrapped = Vec::new();
    for scene in Scene::ALL {
        set_scene(cx, &s, &page, scene);
        let menu = bounds(cx, &s, ids::HUD_MAKER_SCREEN_MENU);
        let hide = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR_COLLAPSE);
        let middle = hide.top() + hide.size.height / 2.;
        if middle < menu.top() || middle > menu.bottom() {
            wrapped.push((scene, menu, hide));
        }
    }
    assert!(wrapped.is_empty(), "the hide button wrapped: {wrapped:?}");
}

fn show_parts(cx: &mut TestAppContext, s: &support::ShellWindow) {
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::settle(cx, s.win);
}

// The side panel's and the open part list's bounds as drawn.
fn panels(cx: &mut TestAppContext, s: &support::ShellWindow) -> [Bounds<Pixels>; 2] {
    [
        bounds(cx, s, ids::HUD_MAKER_PANEL),
        bounds(cx, s, ids::HUD_MAKER_PARTS_PANEL),
    ]
}

// Every part the canvas draws in the current scene, with its window bounds.
fn drawn_parts(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
) -> Vec<(PartKey, Bounds<Pixels>)> {
    let canvas = bounds(cx, s, ids::HUD_MAKER_CANVAS);
    cx.update(|cx| {
        let editor = page.read(cx).editor().unwrap();
        let scale = f32::from(canvas.size.width) / editor.screen().wide() as f32;
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

fn select(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    key: Option<&PartKey>,
) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(key.cloned());
            cx.notify();
        })
    });
    support::settle(cx, s.win);
}

// With nothing selected, a tour of the tabs leaves both floating panels where they stand.
#[gpui_kit::test]
fn a_tab_switch_leaves_the_floating_panels_where_they_stand(cx: &mut TestAppContext) {
    let mut moved = Vec::new();
    for (w, h) in [(900., 600.), (1280., 800.), (1920., 1080.)] {
        let (s, page) = open_at(cx, w, h);
        show_parts(cx, &s);
        let start = panels(cx, &s);
        for scene in [
            Scene::Scoreboard,
            Scene::MainMenu,
            Scene::Hud,
            Scene::FreezeCam,
            Scene::WinPanel,
            Scene::Hud,
        ] {
            set_scene(cx, &s, &page, scene);
            let now = panels(cx, &s);
            if now.map(|b| b.origin) != start.map(|b| b.origin) {
                moved.push(((w, h), scene, start, now));
            }
        }
    }
    assert!(moved.is_empty(), "the panels moved: {moved:?}");
}

// On the HUD tab, selecting a part neither panel covers leaves both where the player left them.
#[gpui_kit::test]
fn a_selection_the_panels_do_not_cover_leaves_them_where_they_stand(cx: &mut TestAppContext) {
    let mut moved = Vec::new();
    for (w, h) in [(900., 600.), (1280., 800.), (1920., 1080.)] {
        let (s, page) = open_at(cx, w, h);
        show_parts(cx, &s);
        let home = panels(cx, &s);
        let parts = drawn_parts(cx, &s, &page);
        for (key, part) in parts {
            if home.iter().any(|b| b.intersects(&part)) {
                continue;
            }
            select(cx, &s, &page, Some(&key));
            let now = panels(cx, &s);
            if now.map(|b| b.origin) != home.map(|b| b.origin) {
                moved.push(((w, h), key, home, now));
            }
            select(cx, &s, &page, None);
        }
    }
    assert!(moved.is_empty(), "the panels moved: {moved:?}");
}
