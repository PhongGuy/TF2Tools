//! Selecting several parts on the HUD Maker page: Shift-clicks, a box drawn on empty canvas and
//! Shift-clicked list rows, then moving, hiding, aligning them, and snapping a resize and the
//! layer buttons, through the pilot ids.

use crate::support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, InputEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, TestAppContext, point, px,
};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{PartKey, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const HEALTH: &str = "HudPlayerStatus/HudPlayerHealth";
const AMMO: &str = "HudWeaponAmmo/AmmoInClip";
const AMMO_PANEL: &str = "HudWeaponAmmo";

fn opened(cx: &mut TestAppContext) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    support::seed_stock_game(&env.game);
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found);
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    support::put_start_card_away(cx, s.win);
    (env, s, page)
}

/// The window point of screen pixel (x, y) on the canvas.
fn at(cx: &mut TestAppContext, s: &support::ShellWindow, x: f32, y: f32) -> Point<Pixels> {
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    let k = f32::from(canvas.size.width) / 1920.;
    canvas.origin + point(px(x * k), px(y * k))
}

fn send(cx: &mut TestAppContext, s: &support::ShellWindow, event: impl InputEvent) {
    cx.update_window(s.win, |_, w, cx| {
        let _ = w.dispatch_event(event.to_platform_input(), cx);
        w.render_frame(cx);
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn down(cx: &mut TestAppContext, s: &support::ShellWindow, p: Point<Pixels>, m: Modifiers) {
    let event = MouseDownEvent {
        button: MouseButton::Left,
        position: p,
        modifiers: m,
        click_count: 1,
        first_mouse: false,
    };
    send(cx, s, event);
}

fn move_to(cx: &mut TestAppContext, s: &support::ShellWindow, p: Point<Pixels>, m: Modifiers) {
    let event = MouseMoveEvent {
        position: p,
        pressed_button: Some(MouseButton::Left),
        modifiers: m,
    };
    send(cx, s, event);
}

fn up(cx: &mut TestAppContext, s: &support::ShellWindow, p: Point<Pixels>, m: Modifiers) {
    let event = MouseUpEvent {
        button: MouseButton::Left,
        position: p,
        modifiers: m,
        click_count: 1,
    };
    send(cx, s, event);
}

/// Clicks screen pixel (x, y) with `m` held.
fn click(cx: &mut TestAppContext, s: &support::ShellWindow, x: f32, y: f32, m: Modifiers) {
    let p = at(cx, s, x, y);
    down(cx, s, p, m);
    up(cx, s, p, m);
}

fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}

fn selection(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Vec<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.selection()
            .iter()
            .map(|k| k.as_str().to_owned())
            .collect()
    })
}

fn rect(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, k: &str) -> (i32, i32, i32, i32) {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        let b = e.layout().get(&PartKey::new(k)).unwrap();
        (b.x(), b.y(), b.wide(), b.tall())
    })
}

fn undo_tip(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> String {
    cx.update(|cx| page.read(cx).undo_tip())
}

/// Health's group by a click on its number, then the clip count by a Shift-click.
fn two_selected(cx: &mut TestAppContext, s: &support::ShellWindow) {
    click(cx, s, 220., 947., Modifiers::default());
    click(cx, s, 1812., 1007., shift());
}

#[gpui_kit::test]
fn shift_click_adds_a_part_and_a_second_one_takes_it_out(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    two_selected(cx, &s);
    assert_eq!(selection(cx, &page), [HEALTH, AMMO]);
    for key in [HEALTH, AMMO] {
        let outline = ids::hud_maker_selected(&PartKey::new(key));
        assert!(support::present(cx, s.win, outline), "{key} is outlined");
    }
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SEVERAL));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SELECTION));
    click(cx, &s, 1812., 1007., shift());
    assert_eq!(selection(cx, &page), [HEALTH]);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SELECTION));
}

#[gpui_kit::test]
fn dragging_one_of_several_moves_them_all_as_one_step(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    two_selected(cx, &s);
    let (hv, ammo) = (rect(cx, &page, HV), rect(cx, &page, AMMO));
    // Ctrl keeps the drag free of snapping, so the delta is the pointer's.
    let free = Modifiers::secondary_key();
    let from = at(cx, &s, 220., 947.);
    let to = at(cx, &s, 220., 917.);
    down(cx, &s, from, free);
    move_to(cx, &s, to, free);
    up(cx, &s, to, free);
    assert_eq!(selection(cx, &page), [HEALTH, AMMO], "the drag kept both");
    let (hv2, ammo2) = (rect(cx, &page, HV), rect(cx, &page, AMMO));
    assert!(
        hv2.1 < hv.1 && hv2.1 - hv.1 == ammo2.1 - ammo.1,
        "{hv2:?} {ammo2:?}"
    );
    assert_eq!(undo_tip(cx, &page), "Undo move 2 parts (Ctrl+Z)");
    support::press(cx, s.win, "ctrl-z");
    assert_eq!((rect(cx, &page, HV), rect(cx, &page, AMMO)), (hv, ammo));
}

#[gpui_kit::test]
fn a_box_on_empty_canvas_selects_the_parts_inside_it(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let ammo = rect(cx, &page, AMMO_PANEL);
    let (x0, y0) = (ammo.0 as f32 - 10., ammo.1 as f32 - 10.);
    let (x1, y1) = (
        (ammo.0 + ammo.2) as f32 + 10.,
        (ammo.1 + ammo.3) as f32 + 10.,
    );
    let (from, to) = (at(cx, &s, x0, y0), at(cx, &s, x1, y1));
    down(cx, &s, from, Modifiers::default());
    move_to(cx, &s, to, Modifiers::default());
    assert!(support::present(cx, s.win, ids::HUD_MAKER_MARQUEE));
    up(cx, &s, to, Modifiers::default());
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MARQUEE));
    assert_eq!(
        selection(cx, &page),
        [AMMO_PANEL],
        "the group, not its pieces"
    );
    // A Shift-click then adds the health group.
    click(cx, &s, 220., 947., shift());
    assert_eq!(selection(cx, &page), [AMMO_PANEL, HEALTH]);
}

#[gpui_kit::test]
fn h_hides_several_parts_as_one_step_and_escape_clears_them(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    two_selected(cx, &s);
    support::press(cx, s.win, "h");
    let hidden = |cx: &mut TestAppContext, k: &str| {
        cx.update(|cx| {
            let e = page.read(cx).editor().unwrap();
            e.part_hidden_by_player(&PartKey::new(k))
        })
    };
    assert!(hidden(cx, HEALTH) && hidden(cx, AMMO));
    assert_eq!(undo_tip(cx, &page), "Undo hide 2 parts (Ctrl+Z)");
    support::press(cx, s.win, "h");
    assert!(
        !hidden(cx, HEALTH) && !hidden(cx, AMMO),
        "H shows them again"
    );
    support::press(cx, s.win, "escape");
    assert!(selection(cx, &page).is_empty());
}

#[gpui_kit::test]
fn the_align_buttons_line_several_parts_up(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    two_selected(cx, &s);
    let (health, ammo) = (rect(cx, &page, HEALTH), rect(cx, &page, AMMO));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_line_up("across")
    ));
    support::click_id(cx, s.win, ids::hud_maker_line_up("top"));
    let top = health.1.min(ammo.1);
    // Each lands within a file unit (2.25 px at 1080p) of the top line.
    assert!((rect(cx, &page, HEALTH).1 - top).abs() <= 2);
    assert!((rect(cx, &page, AMMO).1 - top).abs() <= 2);
    assert_eq!(undo_tip(cx, &page), "Undo align 2 parts (Ctrl+Z)");
}

#[gpui_kit::test]
fn shift_clicking_a_list_row_adds_its_part(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click(cx, &s, 220., 947., Modifiers::default());
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::reveal_part_row(cx, s.win, &PartKey::new(AMMO));
    let row = ids::hud_maker_row(&PartKey::new(AMMO));
    let centre = support::bounds_of(cx, s.win, row).unwrap().center();
    down(cx, &s, centre, shift());
    up(cx, &s, centre, shift());
    assert_eq!(selection(cx, &page), [HEALTH, AMMO]);
}

#[gpui_kit::test]
fn a_resize_handle_snaps_and_shows_a_guide(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click(cx, &s, 220., 947., Modifiers::default());
    assert_eq!(selection(cx, &page), [HEALTH]);
    let health = rect(cx, &page, HEALTH);
    let handle = support::bounds_of(cx, s.win, ids::hud_maker_handle(Handle::Right))
        .unwrap()
        .center();
    let k = {
        let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
        f32::from(canvas.size.width) / 1920.
    };
    // The right edge comes 3 px short of the screen centre's line at 960 and snaps onto it.
    let by = (960 - (health.0 + health.2) - 3) as f32 * k;
    let to = handle + point(px(by), px(0.));
    down(cx, &s, handle, Modifiers::default());
    move_to(cx, &s, to, Modifiers::default());
    assert!(support::present(cx, s.win, ids::HUD_MAKER_GUIDE_X));
    up(cx, &s, to, Modifiers::default());
    let after = rect(cx, &page, HEALTH);
    let right = after.0 + after.2;
    assert!(
        (right - 960).abs() <= 2,
        "the right edge sits on the centre: {after:?}"
    );
    assert_eq!(after.0, health.0, "the left edge stays");
}

#[gpui_kit::test]
fn the_layer_row_has_bring_forward_and_send_back(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    click(
        cx,
        &s,
        220.,
        947.,
        Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    assert_eq!(selection(cx, &page), [HV]);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_FORWARD));
    let z = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            let e = page.read(cx).editor().unwrap();
            e.layout().get(&PartKey::new(HV)).unwrap().z()
        })
    };
    let before = z(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_BACKWARD);
    assert!(z(cx) < before, "Send back lowers the layer");
}

use tf2tools_app::pages::hud_maker::editor::Handle;
