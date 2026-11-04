//! The Spectator tab's target ID: what it draws, where, what is editable and where undo goes.

mod support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, Tf2Install};

const ROOT: &str = "CSpectatorTargetID";
const NAME: &str = "CSpectatorTargetID/TargetNameLabel";
const DATA: &str = "CSpectatorTargetID/TargetDataLabel";
const VALUE: &str = "CSpectatorTargetID/SpectatorGUIHealth/PlayerStatusHealthValue";

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

/// The stock game with the target ID fixtures, the maker open on the Spectator tab.
fn on_spectator(cx: &mut TestAppContext) -> (support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    let texts = support::hud_stock::target_id_texts();
    let over: Vec<(&str, Option<&str>)> =
        texts.iter().map(|(p, t)| (*p, Some(t.as_str()))).collect();
    support::seed_stock_game_with(&env.game, &over);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Spectator);
    (s, page)
}

fn label(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> Option<String> {
    support::label_of(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
}

fn present(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> bool {
    support::present(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
}

fn ypos(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor()?;
        Some(
            e.model()
                .part(&PartKey::new(key))?
                .value("ypos")?
                .text()
                .to_owned(),
        )
    })
}

#[gpui_kit::test]
fn the_tab_draws_the_watched_players_name_line_and_health(cx: &mut TestAppContext) {
    let (s, _page) = on_spectator(cx);
    assert_eq!(label(cx, &s, NAME).as_deref(), Some("Dr. Pancake"));
    assert_eq!(label(cx, &s, DATA).as_deref(), Some("ÜberCharge: 42%"));
    assert_eq!(label(cx, &s, VALUE).as_deref(), Some("118"));
    // The red background shows for a RED player; the HUD's own and the blue one do not.
    assert!(present(cx, &s, "CSpectatorTargetID/TargetIDBG_Spec_Red"));
    assert!(!present(cx, &s, "CSpectatorTargetID/TargetIDBG_Spec_Blue"));
    assert!(!present(cx, &s, "CSpectatorTargetID/TargetIDBG"));
    // The bars are still there and no "Can't show" note appears.
    assert!(present(cx, &s, "Spectator/TipLabel"));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
}

#[gpui_kit::test]
fn the_name_label_sits_at_the_x_the_code_gives_it(cx: &mut TestAppContext) {
    let (s, _page) = on_spectator(cx);
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    let name = support::bounds_of(cx, s.win, ids::hud_maker_part(&PartKey::new(NAME))).unwrap();
    // 1920x1080 at scale 2.25: the root is (1920 - 567) / 2 = 676 px from the left, the health
    // panel 72 px wide, and XRES(8) = 8 * 1920 / 640 = 24 px: the label starts at 772 px.
    let fx = f32::from(name.left() - canvas.left()) / f32::from(canvas.size.width);
    let fy = f32::from(name.top() - canvas.top()) / f32::from(canvas.size.height);
    assert!((fx - 772. / 1920.).abs() < 0.005, "{fx}");
    // The root's y is 250 * 2.25 = 562 px, the label's 5 * 2.25 = 11 px below it.
    assert!((fy - 573. / 1080.).abs() < 0.005, "{fy}");
}

#[gpui_kit::test]
fn the_name_label_offers_y_and_font_but_not_x_or_colour(cx: &mut TestAppContext) {
    let (s, page) = on_spectator(cx);
    click_row(cx, &s, NAME);
    let caps = cx.update(|cx| {
        page.read(cx)
            .editor()
            .and_then(|e| e.model().capabilities(&PartKey::new(NAME)))
    });
    let caps = caps.expect("caps");
    assert!(!caps.move_x() && caps.move_y() && !caps.recolour());
}

#[gpui_kit::test]
fn undo_after_an_edit_to_hudlayout_made_here_returns_to_this_tab(cx: &mut TestAppContext) {
    let (s, page) = on_spectator(cx);
    click_row(cx, &s, ROOT);
    // The Y field is a screen pixel at scale 2.25 and the root sits at 562 px (250 units,
    // truncated), so 337 px is 225 px higher: game unit 150.
    commit(cx, &s, &page, Field::Y, "337");
    assert_eq!(ypos(cx, &page, ROOT).as_deref(), Some("150"));
    support::click_scene(cx, s.win, Scene::Hud);
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.undo(w, cx)),
        Some(Scene::Spectator)
    );
    assert_eq!(ypos(cx, &page, ROOT).as_deref(), Some("250"));
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
