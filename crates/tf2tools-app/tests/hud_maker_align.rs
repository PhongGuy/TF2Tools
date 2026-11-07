//! Alignment on the HUD Maker page: arrow-key nudges, snapping with guides, Mirror and Match
//! size, through the pilot ids.

mod support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, point, px};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{PartKey, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

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
    (env, s, page)
}

/// Window pixels per screen pixel on the canvas.
fn scale(cx: &mut TestAppContext, s: &support::ShellWindow) -> f32 {
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    f32::from(canvas.size.width) / 1920.
}

/// Clicks the canvas at screen pixel (x, y).
fn click_screen(cx: &mut TestAppContext, s: &support::ShellWindow, x: f32, y: f32) {
    let k = scale(cx, s);
    cx.update_window(s.win, |_, w, cx| {
        w.click_at(ids::HUD_MAKER_CANVAS, point(px(x * k), px(y * k)), cx)
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Alt-clicks the canvas at screen pixel (x, y), which selects the part there itself rather
/// than its group.
fn pick_screen(cx: &mut TestAppContext, s: &support::ShellWindow, x: f32, y: f32) {
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    let k = scale(cx, s);
    let position = canvas.origin + point(px(x * k), px(y * k));
    let (button, modifiers) = (MouseButton::Left, alt());
    send(
        cx,
        s,
        MouseDownEvent {
            button,
            position,
            modifiers,
            click_count: 1,
            first_mouse: false,
        },
    );
    send(
        cx,
        s,
        gpui_kit::MouseUpEvent {
            button,
            position,
            modifiers,
            click_count: 1,
        },
    );
}

fn alt() -> Modifiers {
    Modifiers {
        alt: true,
        ..Default::default()
    }
}

fn hv_box(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> (i32, i32, i32, i32) {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        let b = e.layout().get(&PartKey::new(HV)).unwrap();
        (b.x(), b.y(), b.wide(), b.tall())
    })
}

fn can_undo(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| page.read(cx).editor().unwrap().can_undo())
}

#[gpui_kit::test]
fn arrow_keys_nudge_the_selection_one_unit_and_shift_ten(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    pick_screen(cx, &s, 220., 947.);
    support::press(cx, s.win, "right");
    assert_eq!(hv_box(cx, &page).0, 173);
    support::press(cx, s.win, "shift-right");
    assert_eq!(hv_box(cx, &page).0, 195, "87 units");
    support::press(cx, s.win, "up");
    assert_eq!(hv_box(cx, &page).1, 924);
    support::press(cx, s.win, "ctrl-z");
    assert_eq!(
        hv_box(cx, &page),
        (195, 927, 99, 40),
        "each press is its own step"
    );
}

#[gpui_kit::test]
fn arrow_keys_without_a_selection_change_nothing(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    support::press(cx, s.win, "left");
    support::press(cx, s.win, "shift-down");
    assert!(!can_undo(cx, &page));
}

use gpui_kit::{
    InputEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
};

/// Dispatches one pointer event to the shell window, then draws a frame.
fn send(cx: &mut TestAppContext, s: &support::ShellWindow, event: impl InputEvent) {
    cx.update_window(s.win, |_, w, cx| {
        let _ = w.dispatch_event(event.to_platform_input(), cx);
        w.render_frame(cx);
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Presses at `from` and moves by `by` window pixels with the button held.
fn press_and_move(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    from: Point<Pixels>,
    by: (f32, f32),
    modifiers: Modifiers,
) {
    let left = MouseButton::Left;
    send(
        cx,
        s,
        MouseDownEvent {
            button: left,
            position: from,
            modifiers,
            click_count: 1,
            first_mouse: false,
        },
    );
    let position = from + point(px(by.0), px(by.1));
    send(
        cx,
        s,
        MouseMoveEvent {
            position,
            pressed_button: Some(left),
            modifiers,
        },
    );
}

fn release(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    at: Point<Pixels>,
    modifiers: Modifiers,
) {
    send(
        cx,
        s,
        MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers,
            click_count: 1,
        },
    );
}

fn hv_centre(cx: &mut TestAppContext, s: &support::ShellWindow) -> Point<Pixels> {
    support::bounds_of(cx, s.win, ids::hud_maker_part(&PartKey::new(HV)))
        .unwrap()
        .center()
}

#[gpui_kit::test]
fn a_dragged_part_snaps_to_a_nearby_centre_and_shows_a_guide(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let k = scale(cx, &s);
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    let from = hv_centre(cx, &s);
    // Alt takes the health value itself, not its group.
    press_and_move(cx, &s, from, (45. * k, 0.), alt());
    let guide =
        support::bounds_of(cx, s.win, ids::HUD_MAKER_GUIDE_X).expect("a guide while snapped");
    let at = f32::from(guide.origin.x - canvas.origin.x) / k;
    assert!(
        (at - 221.).abs() <= 1.,
        "the guide sits on the image's centre, not {at}"
    );
    assert_eq!(hv_box(cx, &page).0, 220, "50 px is 22 whole units at 1080p");
    release(
        cx,
        &s,
        from + point(px(45. * k), px(0.)),
        Modifiers::default(),
    );
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_GUIDE_X));
    assert_eq!(hv_box(cx, &page).0, 220);
    assert!(can_undo(cx, &page));
    support::press(cx, s.win, "ctrl-z");
    assert_eq!(hv_box(cx, &page).0, 171, "the snapped drag is one step");
}

#[gpui_kit::test]
fn holding_ctrl_drags_without_snapping(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let k = scale(cx, &s);
    let from = hv_centre(cx, &s);
    let free = Modifiers::secondary_key();
    press_and_move(cx, &s, from, (45. * k, 0.), free);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_GUIDE_X));
    assert_eq!(hv_box(cx, &page).0, 216);
    release(cx, &s, from + point(px(45. * k), px(0.)), free);
    assert_eq!(hv_box(cx, &page).0, 216);
}

const IMAGE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage";

fn picking(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| page.read(cx).matching_size())
}

fn selection(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| {
        page.read(cx)
            .editor()?
            .selected()
            .map(|k| k.as_str().to_owned())
    })
}

const MEDIC: &str = "HudMedicCharge";

/// Selects `key` straight on the editor and draws, as a click on its row would.
fn select_part(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    key: &str,
) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(PartKey::new(key)));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
}

fn part_x(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) -> i32 {
    cx.update(|cx| {
        page.read(cx)
            .editor()
            .unwrap()
            .layout()
            .get(&PartKey::new(key))
            .unwrap()
            .x()
    })
}

fn part_text(
    cx: &mut TestAppContext,
    page: &Entity<HudMakerPage>,
    key: &str,
    setting: &str,
) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        Some(
            e.model()
                .part(&PartKey::new(key))?
                .value(setting)?
                .text()
                .to_owned(),
        )
    })
}

#[gpui_kit::test]
fn mirror_moves_the_part_to_the_other_side_as_one_step(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    select_part(cx, &s, &page, MEDIC);
    support::click_id(cx, s.win, ids::HUD_MAKER_MIRROR);
    assert_eq!(part_x(cx, &page, MEDIC), 821);
    assert_eq!(part_text(cx, &page, MEDIC, "xpos").as_deref(), Some("c-62"));
    support::press(cx, s.win, "ctrl-z");
    assert_eq!(part_x(cx, &page, MEDIC), 875);
}

#[gpui_kit::test]
fn match_size_copies_the_next_clicked_parts_size(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    pick_screen(cx, &s, 220., 947.);
    support::click_id(cx, s.win, ids::HUD_MAKER_MATCH_SIZE);
    assert!(picking(cx, &page));
    // Inside the health image, outside HV over its middle and right of the class portrait.
    click_screen(cx, &s, 240., 900.);
    assert!(!picking(cx, &page));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    assert_eq!(hv_box(cx, &page), (171, 927, 114, 114));
    support::press(cx, s.win, "ctrl-z");
    assert_eq!(hv_box(cx, &page), (171, 927, 99, 40));
}

#[gpui_kit::test]
fn escape_stops_picking_and_keeps_the_selection(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    pick_screen(cx, &s, 220., 947.);
    support::click_id(cx, s.win, ids::HUD_MAKER_MATCH_SIZE);
    support::press(cx, s.win, "escape");
    assert!(!picking(cx, &page));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    click_screen(cx, &s, 240., 900.);
    assert_eq!(
        selection(cx, &page).as_deref(),
        Some(IMAGE),
        "an ordinary click again"
    );
    assert_eq!(hv_box(cx, &page), (171, 927, 99, 40));
    assert!(!can_undo(cx, &page));
}

#[gpui_kit::test]
fn mirror_is_not_offered_where_the_game_places_the_part(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut()
                .unwrap()
                .select(Some(PartKey::new("CMainTargetID")));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MIRROR));
    assert!(
        support::present(cx, s.win, ids::HUD_MAKER_MATCH_SIZE),
        "its height still resizes"
    );
}

use support::hud_stock::{HEALTH, block, with_blocks};
use tf2tools_app::pages::hud_maker::editor::Handle;

#[gpui_kit::test]
fn mirror_is_disabled_where_it_cannot_hold_on_every_screen(cx: &mut TestAppContext) {
    let env = support::env();
    let keys = [
        ("xpos", "10"),
        ("ypos", "0"),
        ("wide", "p0.5"),
        ("tall", "10"),
        ("proportionaltoparent", "1"),
    ];
    let half = block("Half", "CExLabel", &keys);
    let health = with_blocks(HEALTH, &half);
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/hudplayerhealth.res", Some(health.as_str()))],
    );
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found);
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    // Half's width follows its parent (NoMirror::Width); HV measures against the screen inside
    // the narrower health panel (NoMirror::Place). Both show Mirror disabled.
    for key in ["HudPlayerStatus/HudPlayerHealth/Half", HV] {
        select_part(cx, &s, &page, key);
        assert!(support::present(cx, s.win, ids::HUD_MAKER_MIRROR), "{key}");
        assert!(
            support::disabled_of(cx, s.win, ids::HUD_MAKER_MIRROR),
            "{key}"
        );
    }
}

#[gpui_kit::test]
fn a_handle_or_killfeed_press_while_picking_only_stops_picking(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    pick_screen(cx, &s, 220., 947.);
    for target in [
        ids::hud_maker_handle(Handle::BottomRight),
        ids::hud_maker_killfeed_line(0),
    ] {
        support::click_id(cx, s.win, ids::HUD_MAKER_MATCH_SIZE);
        assert!(picking(cx, &page));
        support::click_id(cx, s.win, target);
        assert!(!picking(cx, &page));
        assert_eq!(selection(cx, &page).as_deref(), Some(HV));
        assert_eq!(hv_box(cx, &page), (171, 927, 99, 40));
        assert!(!can_undo(cx, &page));
        assert!(!cx.update(|cx| page.read(cx).editor().unwrap().is_dragging()));
    }
}

use tf2tools_app::pages::hud_maker::Field;

#[gpui_kit::test]
fn arrow_keys_leave_a_focused_text_field_alone(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    pick_screen(cx, &s, 220., 947.);
    let input = cx.update(|cx| page.read(cx).field(Field::X).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.focus(window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    support::press(cx, s.win, "left");
    support::press(cx, s.win, "shift-right");
    assert!(!can_undo(cx, &page));
    assert_eq!(hv_box(cx, &page), (171, 927, 99, 40));
}

/// Focuses the canvas with a click on the health value, then selects `key` and presses `keys`.
fn nudge_part(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    key: &str,
    keys: &str,
) {
    pick_screen(cx, s, 220., 947.);
    select_part(cx, s, page, key);
    support::press(cx, s.win, keys);
}

#[gpui_kit::test]
fn a_nudge_the_game_refuses_says_why_once_per_press(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    nudge_part(cx, &s, &page, "CDamageAccountPanel", "right");
    assert!(!can_undo(cx, &page));
    assert_eq!(
        support::toast_labels(cx, s.win),
        ["The game places this part itself, so it can't be moved."]
    );
    // A held key repeats its press; the hold toasts once, until the key comes up.
    let right = || gpui_kit::Keystroke::parse("right").unwrap();
    for is_held in [false, true, true] {
        send(
            cx,
            &s,
            gpui_kit::KeyDownEvent {
                keystroke: right(),
                is_held,
                prefer_character_input: false,
            },
        );
    }
    send(cx, &s, gpui_kit::KeyUpEvent { keystroke: right() });
    assert_eq!(support::toast_labels(cx, s.win).len(), 2);
}

#[gpui_kit::test]
fn a_refused_part_selected_again_says_why_again(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    nudge_part(cx, &s, &page, "CDamageAccountPanel", "right");
    assert_eq!(support::toast_labels(cx, s.win).len(), 1);
    select_part(cx, &s, &page, "CMainTargetID");
    select_part(cx, &s, &page, "CDamageAccountPanel");
    support::press(cx, s.win, "right");
    assert_eq!(support::toast_labels(cx, s.win).len(), 2);
}

#[gpui_kit::test]
fn a_nudge_along_an_axis_the_game_sets_says_which_way_it_moves(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    nudge_part(cx, &s, &page, "CMainTargetID", "left");
    assert!(!can_undo(cx, &page));
    assert_eq!(
        support::toast_labels(cx, s.win),
        ["The game sets where this part sits from side to side, so it only moves up and down."]
    );
    support::press(cx, s.win, "down");
    assert!(can_undo(cx, &page), "the free axis still moves");
}

#[gpui_kit::test]
fn a_nudge_on_a_place_the_maker_cannot_rewrite_says_so(cx: &mut TestAppContext) {
    use support::hud_stock::{HEALTH, label, with_blocks};
    let env = support::env();
    let text = with_blocks(HEALTH, &label("Bad", "c1e30", "0", &[]));
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/hudplayerhealth.res", Some(text.as_str()))],
    );
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
    nudge_part(cx, &s, &page, "HudPlayerStatus/HudPlayerHealth/Bad", "up");
    assert!(!can_undo(cx, &page));
    assert_eq!(
        support::toast_labels(cx, s.win),
        ["This part's place is written in a way the HUD Maker can't change, so it can't be moved."]
    );
}

#[gpui_kit::test]
fn an_alignment_tooltip_goes_when_a_press_opens_the_font_list(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    // Tall enough that the whole panel shows, alignment grid and Font row alike.
    cx.simulate_window_resize(s.win, gpui_kit::size(px(1600.), px(1400.)));
    support::settle(cx, s.win);
    pick_screen(cx, &s, 220., 947.);
    cx.update_window(s.win, |_, w, cx| {
        w.hover(ids::hud_maker_align("north-east"), cx)
    })
    .unwrap();
    support::settle(cx, s.win);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_ALIGN_TIP).as_deref(),
        Some("Top right")
    );
    // The press lands on Fonts, whose list scrolls the panel under the pointer.
    support::click_id(cx, s.win, ids::hud_maker_font_list("font"));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_ALIGN_TIP));
}

#[gpui_kit::test]
fn a_panel_button_tooltip_goes_on_a_press_elsewhere(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx);
    cx.simulate_window_resize(s.win, gpui_kit::size(px(1600.), px(1400.)));
    support::settle(cx, s.win);
    pick_screen(cx, &s, 220., 947.);
    cx.update_window(s.win, |_, w, cx| {
        w.hover(
            ids::hud_maker_tip_for(&ids::HUD_MAKER_MATCH_SIZE.into()),
            cx,
        )
    })
    .unwrap();
    support::settle(cx, s.win);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_TIP).as_deref(),
        Some("Copy another part's width and height")
    );
    support::click_id(cx, s.win, ids::hud_maker_font_list("font"));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_TIP));
}

#[gpui_kit::test]
fn an_alignment_tooltip_stands_level_with_its_own_button_right_of_the_grid(
    cx: &mut TestAppContext,
) {
    let (_env, s, _page) = opened(cx);
    cx.simulate_window_resize(s.win, gpui_kit::size(px(1600.), px(1400.)));
    support::settle(cx, s.win);
    pick_screen(cx, &s, 220., 947.);
    for value in ["north-west", "north-east"] {
        cx.update_window(s.win, |_, w, cx| w.hover(ids::hud_maker_align(value), cx))
            .unwrap();
        support::settle(cx, s.win);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(800));
        support::settle(cx, s.win);
        let tip = support::bounds_of(cx, s.win, ids::HUD_MAKER_ALIGN_TIP).unwrap();
        let own = support::bounds_of(cx, s.win, ids::hud_maker_align(value)).unwrap();
        let east = support::bounds_of(cx, s.win, ids::hud_maker_align("north-east")).unwrap();
        assert!(tip.top() <= own.top() + px(1.), "{value}: {tip:?} {own:?}");
        assert!(tip.left() >= east.right(), "{value}: {tip:?} {east:?}");
    }
}
