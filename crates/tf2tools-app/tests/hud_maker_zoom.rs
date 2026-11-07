//! The HUD Maker's canvas zooms and pans: the toolbar's zoom menu, Ctrl+Plus, Minus and 0,
//! Ctrl+wheel around the pointer, middle-drag and Space+drag, and parts picked and moved at zoom.

mod support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, point,
    px,
};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Screen, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn open(cx: &mut TestAppContext) -> (support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let game = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, game);
    cx.simulate_window_resize(s.win, gpui_kit::size(px(1280.), px(800.)));
    support::settle(cx, s.win);
    support::click_nav(cx, s.win, tf2tools_app::shell::route::Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (s, page)
}

fn canvas(cx: &mut TestAppContext, s: &support::ShellWindow) -> Bounds<Pixels> {
    support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).expect("no canvas")
}

fn send(cx: &mut TestAppContext, s: &support::ShellWindow, event: impl gpui_kit::InputEvent) {
    cx.update_window(s.win, |_, w, cx| {
        let _ = w.dispatch_event(event.to_platform_input(), cx);
        w.render_frame(cx);
    })
    .unwrap();
    support::settle(cx, s.win);
}

// Picks `item` from the toolbar's zoom menu, then shuts the menu if the item left it open.
fn menu(cx: &mut TestAppContext, s: &support::ShellWindow, item: &'static str) {
    support::click_id(cx, s.win, ids::HUD_MAKER_ZOOM);
    support::click_id(cx, s.win, item);
    if support::present(cx, s.win, item) {
        support::click_id(cx, s.win, ids::HUD_MAKER_ZOOM);
    }
    assert!(
        !support::present(cx, s.win, item),
        "the zoom menu stayed open"
    );
}

fn zoom_label(cx: &mut TestAppContext, s: &support::ShellWindow) -> Option<String> {
    support::label_of(cx, s.win, ids::HUD_MAKER_ZOOM)
}

// The screen pixel under window point `at`, unrounded, read from the canvas as drawn.
fn under(cx: &mut TestAppContext, s: &support::ShellWindow, at: Point<Pixels>) -> (f32, f32) {
    let c = canvas(cx, s);
    let scale = f32::from(c.size.width) / 1920.;
    (
        f32::from(at.x - c.origin.x) / scale,
        f32::from(at.y - c.origin.y) / scale,
    )
}

fn hv_box(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> (i32, i32) {
    cx.update(|cx| {
        let editor = page.read(cx).editor().unwrap();
        let b = editor.layout().get(&PartKey::new(HV)).unwrap();
        (b.x(), b.y())
    })
}

fn selection(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| (page.read(cx).editor()?.selected()).map(|k| k.as_str().to_owned()))
}

fn wheel(at: Point<Pixels>, lines: f32, modifiers: Modifiers) -> gpui_kit::ScrollWheelEvent {
    gpui_kit::ScrollWheelEvent {
        position: at,
        delta: gpui_kit::ScrollDelta::Lines(point(0., lines)),
        modifiers,
        touch_phase: gpui_kit::TouchPhase::Moved,
    }
}

fn down(button: MouseButton, at: Point<Pixels>, modifiers: Modifiers) -> gpui_kit::MouseDownEvent {
    gpui_kit::MouseDownEvent {
        button,
        position: at,
        modifiers,
        click_count: 1,
        first_mouse: false,
    }
}

fn moved(button: MouseButton, at: Point<Pixels>, modifiers: Modifiers) -> gpui_kit::MouseMoveEvent {
    gpui_kit::MouseMoveEvent {
        position: at,
        pressed_button: Some(button),
        modifiers,
    }
}

fn up(button: MouseButton, at: Point<Pixels>) -> gpui_kit::MouseUpEvent {
    gpui_kit::MouseUpEvent {
        button,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
    }
}

// Drags with `button` from `from` by `by` and lets go there.
fn drag(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    button: MouseButton,
    from: Point<Pixels>,
    by: Point<Pixels>,
    modifiers: Modifiers,
) {
    send(cx, s, down(button, from, modifiers));
    send(cx, s, moved(button, from + by, modifiers));
    send(cx, s, up(button, from + by));
}

#[gpui_kit::test]
fn the_toolbar_zooms_in_out_and_back_to_the_fit(cx: &mut TestAppContext) {
    let (s, _page) = open(cx);
    let fit = canvas(cx, &s);
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("100%"));
    menu(cx, &s, ids::HUD_MAKER_ZOOM_IN);
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("125%"));
    let zoomed = canvas(cx, &s);
    let grew = f32::from(zoomed.size.width) / f32::from(fit.size.width);
    assert!((grew - 1.25).abs() < 0.01, "{fit:?} became {zoomed:?}");
    menu(cx, &s, ids::HUD_MAKER_ZOOM_IN);
    menu(cx, &s, ids::HUD_MAKER_ZOOM_OUT);
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("125%"));
    menu(cx, &s, ids::HUD_MAKER_ZOOM_FIT);
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("100%"));
    assert_eq!(canvas(cx, &s), fit);
}

#[gpui_kit::test]
fn ctrl_plus_minus_and_zero_zoom_the_canvas(cx: &mut TestAppContext) {
    let (s, _page) = open(cx);
    let fit = canvas(cx, &s);
    // A press on the canvas hands the keys to the page.
    let corner = fit.origin + point(px(4.), px(4.));
    send(
        cx,
        &s,
        down(MouseButton::Left, corner, Modifiers::default()),
    );
    send(cx, &s, up(MouseButton::Left, corner));
    support::press(cx, s.win, "ctrl-=");
    support::press(cx, s.win, "ctrl-=");
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("150%"));
    support::press(cx, s.win, "ctrl--");
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("125%"));
    support::press(cx, s.win, "ctrl-0");
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("100%"));
    assert_eq!(canvas(cx, &s), fit);
}

#[gpui_kit::test]
fn ctrl_wheel_zooms_around_the_pointer(cx: &mut TestAppContext) {
    let (s, _page) = open(cx);
    support::put_start_card_away(cx, s.win);
    // At twice the fit the screen overflows the view both ways, so nothing centres it.
    for _ in 0..3 {
        menu(cx, &s, ids::HUD_MAKER_ZOOM_IN);
    }
    let view = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS_VIEW).unwrap();
    let at = view.origin + point(view.size.width * 0.3, view.size.height * 0.7);
    let before = under(cx, &s, at);
    send(cx, &s, wheel(at, 1., Modifiers::secondary_key()));
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("250%"));
    send(cx, &s, wheel(at, 1., Modifiers::secondary_key()));
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("300%"));
    let after = under(cx, &s, at);
    assert!(
        (before.0 - after.0).abs() < 1. && (before.1 - after.1).abs() < 1.,
        "the pointer was over {before:?} and is now over {after:?}"
    );
    send(cx, &s, wheel(at, -1., Modifiers::secondary_key()));
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("250%"));
}

#[gpui_kit::test]
fn middle_drag_space_drag_and_the_wheel_pan_a_zoomed_canvas(cx: &mut TestAppContext) {
    let (s, page) = open(cx);
    support::put_start_card_away(cx, s.win);
    support::maker_page::float_panels(cx, &s, &page);
    for _ in 0..3 {
        menu(cx, &s, ids::HUD_MAKER_ZOOM_IN);
    }
    let start = canvas(cx, &s);
    let mid = start.center();
    let by = point(px(-40.), px(-30.));
    drag(cx, &s, MouseButton::Middle, mid, by, Modifiers::default());
    let panned = canvas(cx, &s);
    assert_eq!(panned.origin - start.origin, by, "middle-drag");

    // Space held turns a left drag into a pan, which selects and moves nothing.
    let hv = hv_box(cx, &page);
    let space = || gpui_kit::Keystroke::parse("space").unwrap();
    send(
        cx,
        &s,
        gpui_kit::KeyDownEvent {
            keystroke: space(),
            is_held: false,
            prefer_character_input: false,
        },
    );
    drag(cx, &s, MouseButton::Left, mid, by, Modifiers::default());
    send(cx, &s, gpui_kit::KeyUpEvent { keystroke: space() });
    let again = canvas(cx, &s);
    assert_eq!(again.origin - panned.origin, by, "Space+drag");
    assert_eq!(hv_box(cx, &page), hv);

    // A plain wheel scrolls the view down.
    send(cx, &s, wheel(mid, -2., Modifiers::default()));
    assert!(canvas(cx, &s).top() < again.top(), "the wheel");
}

#[gpui_kit::test]
fn a_zoomed_press_selects_and_drags_by_hud_units(cx: &mut TestAppContext) {
    let (s, page) = open(cx);
    let hv = support::bounds_of(cx, s.win, ids::hud_maker_part(&PartKey::new(HV))).unwrap();
    // Zoom twice around the health value, so it stays in view and grows.
    send(cx, &s, wheel(hv.center(), 1., Modifiers::secondary_key()));
    send(cx, &s, wheel(hv.center(), 1., Modifiers::secondary_key()));
    let c = canvas(cx, &s);
    let scale = f32::from(c.size.width) / 1920.;
    let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&PartKey::new(HV))).unwrap();
    let start = hv_box(cx, &page);
    // Ctrl keeps snapping off, so the part moves by exactly the pointer's travel: 45 and 27
    // screen pixels are 20 and 12 whole HUD units at 1080 tall.
    drag(
        cx,
        &s,
        MouseButton::Left,
        part.center(),
        point(px(45. * scale), px(-27. * scale)),
        Modifiers {
            alt: true,
            ..Modifiers::secondary_key()
        },
    );
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(HV),
        "Alt takes the part itself"
    );
    let end = hv_box(cx, &page);
    assert_eq!(
        (end.0 - start.0, end.1 - start.1),
        (45, -27),
        "at scale {scale}"
    );
    let outline = support::bounds_of(cx, s.win, ids::HUD_MAKER_SELECTION).unwrap();
    let moved = support::bounds_of(cx, s.win, ids::hud_maker_part(&PartKey::new(HV))).unwrap();
    assert!(
        f32::from(outline.origin.x - moved.origin.x).abs() <= 1.
            && f32::from(outline.size.width - moved.size.width).abs() <= 1.,
        "the outline {outline:?} is off the part {moved:?}"
    );
}

#[gpui_kit::test]
fn a_new_screen_shape_goes_back_to_the_fit(cx: &mut TestAppContext) {
    let (s, page) = open(cx);
    menu(cx, &s, ids::HUD_MAKER_ZOOM_IN);
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("125%"));
    support::click_id(cx, s.win, ids::HUD_MAKER_SCREEN_MENU);
    support::click_id(cx, s.win, ids::hud_maker_screen(Screen::new(1440, 1080)));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().screen()),
        Screen::new(1440, 1080)
    );
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("100%"));
    let c = canvas(cx, &s);
    let area = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS_AREA).unwrap();
    assert!(c.size.width <= area.size.width && c.size.height <= area.size.height);
}

// The chip that stops a comparison belongs to the view, not the zoomed screen, so it stays
// whole wherever the screen is zoomed and panned to.
#[gpui_kit::test]
fn the_stop_comparing_chip_stays_in_the_view_at_any_zoom(cx: &mut TestAppContext) {
    let (s, page) = open(cx);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().compare_with(Vec::new());
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    for _ in 0..6 {
        menu(cx, &s, ids::HUD_MAKER_ZOOM_IN);
    }
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("400%"));
    let view = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS_VIEW).unwrap();
    // A far pan to the screen's bottom-right corner, then one to its top-left.
    for by in [point(px(-5000.), px(-5000.)), point(px(5000.), px(5000.))] {
        let mid = view.center();
        let steps = 20.;
        send(cx, &s, down(MouseButton::Middle, mid, Modifiers::default()));
        for i in 1..=steps as i32 {
            let at = mid + point(by.x * (i as f32 / steps), by.y * (i as f32 / steps));
            send(cx, &s, moved(MouseButton::Middle, at, Modifiers::default()));
        }
        send(cx, &s, up(MouseButton::Middle, mid + by));
        let chip = support::bounds_of(cx, s.win, ids::HUD_MAKER_COMPARE_STOP).unwrap();
        let inside = chip.left() >= view.left()
            && chip.top() >= view.top()
            && chip.right() <= view.right()
            && chip.bottom() <= view.bottom();
        assert!(
            inside,
            "after a pan by {by:?} the chip {chip:?} left the view {view:?}"
        );
    }
    support::click_id(cx, s.win, ids::HUD_MAKER_COMPARE_STOP);
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().compared().is_none()));
}
