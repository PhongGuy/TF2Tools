use crate::support;

use std::time::Duration;

// `AppContext as _` is used by `cx.update_window` in `set_slider` and the opacity setup.
use gpui_kit::point;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshair_maker::CrosshairMakerPage;
use tf2tools_app::pages::crosshair_maker::model::{CanvasSize, Tool};
use tf2tools_app::pages::crosshair_maker::starter::{Param, Starter, starter_drawing};
use tf2tools_app::pages::crosshair_maker::starter_dialog::StarterDialog;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::RgbaImage;

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::shell_window(cx, env, Tf2State::Missing { saved: None })
}

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairMakerPage> {
    support::click_id(cx, s.win, ids::nav(Page::CrosshairMaker));
    cx.update(|cx| s.shell.read(cx).crosshair_maker_page())
        .expect("a Crosshair Maker page")
}

/// Clicks Starters and returns the dialog it opened.
fn open_starters(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
) -> Entity<StarterDialog> {
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_STARTERS);
    support::settle(cx, s.win);
    cx.update(|cx| page.read(cx).starter_dialog())
        .expect("a starter dialog")
}

fn set_slider(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    dialog: &Entity<StarterDialog>,
    param: Param,
    value: f32,
) {
    let slider = cx.update(|cx| dialog.read(cx).slider(param).clone());
    cx.update_window(s.win, |_, window, cx| {
        slider.update(cx, |state, cx| state.set_value(value, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn preview(cx: &mut TestAppContext, dialog: &Entity<StarterDialog>) -> RgbaImage {
    cx.update(|cx| dialog.read(cx).drawing(cx))
}

fn value_label(cx: &mut TestAppContext, s: &support::ShellWindow, param: Param) -> Option<String> {
    support::label_of(cx, s.win, ids::crosshair_maker_starter_value(param.slug()))
}

/// The sliders the dialog shows, in order.
fn shown(cx: &mut TestAppContext, s: &support::ShellWindow) -> Vec<Param> {
    Param::ALL
        .into_iter()
        .filter(|p| support::present(cx, s.win, ids::crosshair_maker_starter_slider(p.slug())))
        .collect()
}

#[gpui_kit::test]
fn starter_shapes_open_in_a_dialog_with_a_live_preview(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let dialog = open_starters(cx, &s, &page);
    assert!(support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_STARTER_DIALOG
    ));
    for shape in Starter::ALL {
        assert_eq!(
            support::label_of(cx, s.win, ids::crosshair_maker_starter(shape.slug())).as_deref(),
            Some(shape.label())
        );
    }
    let (large, white) = (CanvasSize::Large, [255, 255, 255, 255]);
    assert_eq!(cx.update(|cx| dialog.read(cx).shape()), Starter::Cross);
    assert_eq!(
        preview(cx, &dialog),
        starter_drawing(Starter::Cross, Starter::Cross.defaults(large), large, white)
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_STARTER_PREVIEW).as_deref(),
        Some("Cross preview")
    );
    assert_eq!(shown(cx, &s), [Param::Length, Param::Thickness]);
    assert_eq!(
        value_label(cx, &s, Param::Length).as_deref(),
        Some("Length: 8 px")
    );
    assert_eq!(
        value_label(cx, &s, Param::Thickness).as_deref(),
        Some("Thickness: 2 px")
    );

    support::click_id(
        cx,
        s.win,
        ids::crosshair_maker_starter(Starter::CrossWithGap.slug()),
    );
    assert_eq!(
        cx.update(|cx| dialog.read(cx).shape()),
        Starter::CrossWithGap
    );
    assert_eq!(shown(cx, &s), [Param::Length, Param::Thickness, Param::Gap]);
    set_slider(cx, &s, &dialog, Param::Gap, 6.);
    assert_eq!(
        value_label(cx, &s, Param::Gap).as_deref(),
        Some("Gap: 6 px")
    );
    let mut want = Starter::CrossWithGap.defaults(large);
    want.gap = 6;
    assert_eq!(cx.update(|cx| dialog.read(cx).params(cx)), want);
    assert_eq!(
        preview(cx, &dialog),
        starter_drawing(Starter::CrossWithGap, want, large, white)
    );
    // Choosing a shape starts it from its own defaults.
    support::click_id(cx, s.win, ids::crosshair_maker_starter(Starter::Dot.slug()));
    assert_eq!(
        cx.update(|cx| dialog.read(cx).params(cx)),
        Starter::Dot.defaults(large)
    );
    assert_eq!(shown(cx, &s), [Param::Size]);

    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_STARTER_CANCEL);
    assert!(!support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_STARTER_DIALOG
    ));
    let e = cx.update(|cx| {
        let e = page.read(cx).editor();
        (e.undo_depth(), e.is_dirty())
    });
    assert_eq!(e, (0, false));
}

#[gpui_kit::test]
fn the_starters_follow_the_canvas_size_and_the_paint(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SIZE);
    support::click_id(cx, s.win, ids::crosshair_maker_swatch(1));
    let opacity = cx.update(|cx| page.read(cx).opacity_state().clone());
    cx.update_window(s.win, |_, window, cx| {
        opacity.update(cx, |state, cx| state.set_value(50., window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    let dialog = open_starters(cx, &s, &page);
    let small = CanvasSize::Small;
    assert_eq!(
        preview(cx, &dialog),
        starter_drawing(
            Starter::Cross,
            Starter::Cross.defaults(small),
            small,
            [192, 192, 192, 128]
        )
    );
    assert_eq!(
        cx.update(|cx| dialog.read(cx).slider(Param::Length).read(cx).max_value()),
        16.
    );
    assert_eq!(
        value_label(cx, &s, Param::Length).as_deref(),
        Some("Length: 4 px")
    );
}

#[gpui_kit::test]
fn maker_keys_wait_while_the_starters_are_open(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let tool = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).editor().tool());
    let _dialog = open_starters(cx, &s, &page);
    support::click_id(
        cx,
        s.win,
        ids::crosshair_maker_starter(Starter::Circle.slug()),
    );
    support::press(cx, s.win, "l");
    assert_eq!(tool(cx), Tool::Pencil);
    support::press(cx, s.win, "escape");
    assert!(!support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_STARTER_DIALOG
    ));
    // Closing hands the keys back to the page; after Escape the kit restores focus on a
    // 0.25 s timer of the executor's fake clock, so tick past it first.
    cx.executor().advance_clock(Duration::from_millis(300));
    support::settle(cx, s.win);
    support::press(cx, s.win, "l");
    assert_eq!(tool(cx), Tool::Line);
}

/// Clicks cell (x, y) of the grid.
fn paint_cell(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    x: u32,
    y: u32,
) {
    support::settle(cx, s.win);
    let cell = cx.update(|cx| page.read(cx).cell_size());
    let offset = point(cell * (x as f32 + 0.5), cell * (y as f32 + 0.5));
    cx.update_window(s.win, |_, w, cx| {
        w.click_at(ids::CROSSHAIR_MAKER_GRID, offset, cx)
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn set_name(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn save(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<CrosshairMakerPage>) {
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SAVE);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    support::settle(cx, s.win);
}

fn drawing(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>) -> RgbaImage {
    cx.update(|cx| page.read(cx).editor().drawing())
}

fn undo_depth(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>) -> usize {
    cx.update(|cx| page.read(cx).editor().undo_depth())
}

#[gpui_kit::test]
fn use_puts_the_starter_on_a_clean_canvas_as_one_undo_step(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let dialog = open_starters(cx, &s, &page);
    support::click_id(cx, s.win, ids::crosshair_maker_starter(Starter::Dot.slug()));
    let want = preview(cx, &dialog);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_STARTER_USE);
    assert!(!support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_STARTER_DIALOG
    ));
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(drawing(cx, &page), want);
    assert_eq!(undo_depth(cx, &page), 1);
    assert!(!cx.update(|cx| page.read(cx).can_leave()));
    support::press(cx, s.win, "ctrl-z");
    assert!(drawing(cx, &page).pixels().iter().all(|&b| b == 0));
    support::press(cx, s.win, "ctrl-y");
    assert_eq!(drawing(cx, &page), want);
}

#[gpui_kit::test]
fn use_on_an_unsaved_drawing_asks_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    let before = drawing(cx, &page);
    let dialog = open_starters(cx, &s, &page);
    let want = preview(cx, &dialog);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_STARTER_USE);
    assert!(!support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_STARTER_DIALOG
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("You have unsaved changes. Discard them?")
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(drawing(cx, &page), before);
    let _dialog = open_starters(cx, &s, &page);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_STARTER_USE);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    assert_eq!(drawing(cx, &page), want);
    assert_eq!(undo_depth(cx, &page), 1);
}

#[gpui_kit::test]
fn a_used_starter_saves_as_a_new_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Mine");
    save(cx, &s, &page);
    assert!(cx.update(|cx| page.read(cx).editor().open_id().is_some()));
    let _dialog = open_starters(cx, &s, &page);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_STARTER_USE);
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert!(cx.update(|cx| page.read(cx).editor().open_id().is_none()));
    assert_eq!(
        cx.update(|cx| page.read(cx).name_input().read(cx).value().to_string()),
        ""
    );
    set_name(cx, &s, &page, "Starter");
    save(cx, &s, &page);
    let mut names: Vec<String> = s
        .services
        .library
        .list_crosshairs()
        .unwrap()
        .iter()
        .map(|c| c.name().to_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["Mine", "Starter"]);
}

/// Whether the page still sees an open starter dialog, after the kit's 0.25 s focus timer.
fn dialog_kept(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
) -> bool {
    cx.executor().advance_clock(Duration::from_millis(300));
    support::settle(cx, s.win);
    cx.update(|cx| page.read(cx).starter_dialog().is_some())
}

#[gpui_kit::test]
fn a_closed_starter_dialog_is_released(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    // Once closed, only the page's weak handle is left, so the dialog is gone.
    drop(open_starters(cx, &s, &page));
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_STARTER_CANCEL);
    assert!(!dialog_kept(cx, &s, &page), "kept after Cancel");
    drop(open_starters(cx, &s, &page));
    support::press(cx, s.win, "escape");
    assert!(!support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_STARTER_DIALOG
    ));
    assert!(!dialog_kept(cx, &s, &page), "kept after Escape");
}

#[gpui_kit::test]
fn an_off_step_slider_value_snaps_and_the_label_agrees(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let dialog = open_starters(cx, &s, &page);
    set_slider(cx, &s, &dialog, Param::Thickness, 5.3);
    set_slider(cx, &s, &dialog, Param::Length, 999.);
    let params = cx.update(|cx| dialog.read(cx).params(cx));
    assert_eq!((params.thickness, params.length), (6, 32));
    assert_eq!(
        value_label(cx, &s, Param::Thickness).as_deref(),
        Some("Thickness: 6 px")
    );
    assert_eq!(
        value_label(cx, &s, Param::Length).as_deref(),
        Some("Length: 32 px")
    );
    let large = CanvasSize::Large;
    assert_eq!(
        preview(cx, &dialog),
        starter_drawing(Starter::Cross, params, large, [255, 255, 255, 255])
    );
}

#[gpui_kit::test]
fn a_starter_the_editor_refuses_leaves_the_page_alone(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    set_name(cx, &s, &page, "Keep");
    let odd = RgbaImage::from_pixels(16, 16, vec![255; 16 * 16 * 4]).unwrap();
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |page, cx| page.use_starter(odd, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert_eq!(
        cx.update(|cx| page.read(cx).name_input().read(cx).value().to_string()),
        "Keep"
    );
    assert_eq!(undo_depth(cx, &page), 0);
}

// The 64x64 Cross with gap from the bug report: arms on the canvas's edge columns and rows.
#[gpui_kit::test]
fn the_starter_preview_shows_the_whole_canvas(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let _dialog = open_starters(cx, &s, &page);
    support::click_id(
        cx,
        s.win,
        ids::crosshair_maker_starter(Starter::CrossWithGap.slug()),
    );
    let frame = support::bounds_of(cx, s.win, ids::CROSSHAIR_MAKER_STARTER_PREVIEW).unwrap();
    let picture = support::bounds_of(cx, s.win, ids::CROSSHAIR_MAKER_STARTER_PICTURE).unwrap();
    // Two pixels a cell, inside the frame's 1 px border on every side.
    assert_eq!(picture.size.width, gpui_kit::px(128.), "{picture:?}");
    assert_eq!(picture.size.height, gpui_kit::px(128.), "{picture:?}");
    let border = gpui_kit::px(1.);
    assert!(
        picture.left() >= frame.left() + border
            && picture.right() <= frame.right() - border
            && picture.top() >= frame.top() + border
            && picture.bottom() <= frame.bottom() - border,
        "the picture {picture:?} spills out of the frame {frame:?}"
    );
}

#[gpui_kit::test]
fn a_wider_gap_shortens_the_longest_arm_on_offer(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let dialog = open_starters(cx, &s, &page);
    support::click_id(
        cx,
        s.win,
        ids::crosshair_maker_starter(Starter::CrossWithGap.slug()),
    );
    let length_max = |cx: &mut TestAppContext| {
        cx.update(|cx| dialog.read(cx).slider(Param::Length).read(cx).max_value())
    };
    assert_eq!(length_max(cx), 28.);
    set_slider(cx, &s, &dialog, Param::Length, 32.);
    set_slider(cx, &s, &dialog, Param::Thickness, 16.);
    set_slider(cx, &s, &dialog, Param::Gap, 31.);
    assert_eq!(length_max(cx), 1.);
    assert_eq!(
        value_label(cx, &s, Param::Length).as_deref(),
        Some("Length: 1 px")
    );
    assert_eq!(cx.update(|cx| dialog.read(cx).params(cx)).length, 1);
    // Narrowing the gap again offers the room back.
    set_slider(cx, &s, &dialog, Param::Gap, 4.);
    assert_eq!(length_max(cx), 28.);
}
