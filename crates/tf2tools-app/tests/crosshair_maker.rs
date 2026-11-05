mod support;

use std::path::PathBuf;

use gpui_kit::component::color_picker::ColorPickerEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Bounds, ElementId, Entity, InputEvent, Modifiers, MouseButton,
    MouseDownEvent, MouseExitEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta,
    ScrollWheelEvent, TestAppContext, TouchPhase, Window, point, px,
};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshair_maker::CrosshairMakerPage;
use tf2tools_app::pages::crosshair_maker::model::{
    CLEAR, CanvasSize, FOLLOWS, MIXED, Tool, hsla_of, position_text, rgb_of,
};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::leave;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    CrosshairEntry, CrosshairFormat, CrosshairRef, LibraryKind, PaintTarget, RgbaImage,
};

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::shell_window(cx, env, Tf2State::Missing { saved: None })
}

/// Opens the Crosshair Maker from the sidebar.
fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairMakerPage> {
    support::click_id(cx, s.win, ids::nav(Page::CrosshairMaker));
    cx.update(|cx| s.shell.read(cx).crosshair_maker_page())
        .expect("a Crosshair Maker page")
}

/// The boxes of `wanted`, all read from one settled frame.
fn boxes(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    wanted: &[ElementId],
) -> Vec<Bounds<Pixels>> {
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, w, _| {
        wanted
            .iter()
            .map(|id| w.find(id.clone()).bounds())
            .collect()
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_maker_has_its_own_group_under_the_stores_and_is_dropped_on_leaving(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let order = [
        ids::nav(Page::HudStore),
        ids::nav(Page::SoundStore),
        ids::nav(Page::AddonStore),
        ids::nav(Page::CrosshairMaker),
        ids::nav(Page::Settings),
    ];
    let gap = |a: Bounds<Pixels>, b: Bounds<Pixels>| b.top() - a.bottom();
    for collapsed in [false, true] {
        if collapsed {
            support::click_id(cx, s.win, ids::SIDEBAR_TOGGLE);
            support::wait_real(cx, "the setting", |_| {
                s.services.db.settings().unwrap().sidebar_collapsed()
            });
        }
        let nav = boxes(cx, &s, &order);
        // Items in one group sit close; a separator between groups adds clear space.
        assert!(gap(nav[0], nav[1]) < px(16.), "{collapsed} {nav:?}");
        assert!(gap(nav[2], nav[3]) > px(16.), "{collapsed} {nav:?}");
        assert!(gap(nav[3], nav[4]) > px(0.), "{collapsed} {nav:?}");
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::nav(Page::CrosshairMaker)).as_deref(),
        Some("Crosshair Maker")
    );
    let _page = open_maker(cx, &s);
    assert!(support::present(cx, s.win, ids::page(Page::CrosshairMaker)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Crosshair Maker")
    );
    support::assert_starts_under_top_bar(
        cx,
        s.win,
        ids::page(Page::CrosshairMaker),
        ids::CROSSHAIR_MAKER_PENCIL,
    );
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(
        cx.update(|cx| s.shell.read(cx).crosshair_maker_page())
            .is_none()
    );
}

/// The grid's box and cell size, read from one settled frame.
fn grid(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
) -> (Bounds<Pixels>, Pixels) {
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, w, cx| {
        (
            w.find(ids::CROSSHAIR_MAKER_GRID).bounds(),
            page.read(cx).cell_size(),
        )
    })
    .unwrap()
}

/// The window position of cell (x, y)'s centre.
fn centre((grid, cell): (Bounds<Pixels>, Pixels), x: u32, y: u32) -> Point<Pixels> {
    point(
        grid.origin.x + cell * (x as f32 + 0.5),
        grid.origin.y + cell * (y as f32 + 0.5),
    )
}

/// Clicks cell (x, y): the pointer moves there, presses and releases.
fn paint_cell(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    x: u32,
    y: u32,
) {
    let (_, cell) = grid(cx, s, page);
    let offset = point(cell * (x as f32 + 0.5), cell * (y as f32 + 0.5));
    cx.update_window(s.win, |_, w, cx| {
        w.click_at(ids::CROSSHAIR_MAKER_GRID, offset, cx)
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Every cell the editor holds that is not clear, row by row from the top.
fn painted(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>) -> Vec<(u32, u32)> {
    cx.update(|cx| {
        let editor = page.read(cx).editor();
        let n = editor.size().side();
        let mut out = Vec::new();
        for y in 0..n {
            for x in 0..n {
                if editor.pixel(x, y) != CLEAR {
                    out.push((x, y));
                }
            }
        }
        out
    })
}

fn undo_depth(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>) -> usize {
    cx.update(|cx| page.read(cx).editor().undo_depth())
}

/// Dispatches one pointer event to the shell window, then draws a frame.
fn send(cx: &mut TestAppContext, s: &support::ShellWindow, event: impl InputEvent) {
    cx.update_window(s.win, |_, w, cx| {
        let _ = w.dispatch_event(event.to_platform_input(), cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn mouse_move(at: Point<Pixels>, held: bool) -> MouseMoveEvent {
    MouseMoveEvent {
        position: at,
        pressed_button: held.then_some(MouseButton::Left),
        modifiers: Modifiers::default(),
    }
}

fn mouse_down(at: Point<Pixels>) -> MouseDownEvent {
    MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    }
}

fn mouse_up(at: Point<Pixels>) -> MouseUpEvent {
    MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
    }
}

#[gpui_kit::test]
fn the_grid_is_a_square_of_whole_pixel_cells(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let (bounds, cell) = grid(cx, &s, &page);
    assert!(cell >= px(4.), "{cell:?}");
    assert_eq!(cell, cell.floor());
    assert_eq!(
        (bounds.size.width, bounds.size.height),
        (cell * 64., cell * 64.)
    );
}

#[gpui_kit::test]
fn a_fast_drag_paints_a_gap_free_line_as_one_step(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let at = grid(cx, &s, &page);
    // The kit's drag sends eight moves, about seven cells apart here.
    let (from, to) = (centre(at, 4, 20), centre(at, 59, 20));
    cx.update_window(s.win, |_, w, cx| w.drag(from, to, cx))
        .unwrap();
    support::settle(cx, s.win);
    let row: Vec<_> = (4..=59).map(|x| (x, 20)).collect();
    assert_eq!(painted(cx, &page), row);
    assert_eq!(undo_depth(cx, &page), 1);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_UNDO);
    assert!(painted(cx, &page).is_empty());
}

#[gpui_kit::test]
fn mirror_x_and_y_paint_four_cells(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_MIRROR_X);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_MIRROR_Y);
    assert!(cx.update(|cx| {
        let e = page.read(cx).editor();
        e.mirror_x() && e.mirror_y()
    }));
    paint_cell(cx, &s, &page, 3, 5);
    assert_eq!(painted(cx, &page), [(3, 5), (60, 5), (3, 58), (60, 58)]);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_UNDO);
    assert!(painted(cx, &page).is_empty());
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_REDO);
    assert_eq!(painted(cx, &page).len(), 4);
}

#[gpui_kit::test]
fn a_release_off_the_grid_ends_the_stroke(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let at = grid(cx, &s, &page);
    let off = point(at.0.right() + px(12.), centre(at, 20, 10).y);
    send(cx, &s, mouse_move(centre(at, 10, 10), false));
    send(cx, &s, mouse_down(centre(at, 10, 10)));
    send(cx, &s, mouse_move(centre(at, 20, 10), true));
    send(cx, &s, mouse_move(off, true));
    send(cx, &s, mouse_up(off));
    assert!(!cx.update(|cx| page.read(cx).editor().in_stroke()));
    // Back over the grid with no button held paints nothing.
    send(cx, &s, mouse_move(centre(at, 30, 30), false));
    // The move off the right edge clamps to the edge cell, so the line runs to column 63.
    let row: Vec<_> = (10..=63).map(|x| (x, 10)).collect();
    assert_eq!(painted(cx, &page), row);
    assert_eq!(undo_depth(cx, &page), 1);
}

#[gpui_kit::test]
fn a_move_with_no_button_held_ends_a_stroke_whose_release_was_lost(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let at = grid(cx, &s, &page);
    send(cx, &s, mouse_move(centre(at, 10, 10), false));
    send(cx, &s, mouse_down(centre(at, 10, 10)));
    send(cx, &s, mouse_move(centre(at, 20, 10), true));
    // The release happened where the window never saw it; the next move has no button held.
    send(cx, &s, mouse_move(centre(at, 30, 10), false));
    assert!(!cx.update(|cx| page.read(cx).editor().in_stroke()));
    let row: Vec<_> = (10..=20).map(|x| (x, 10)).collect();
    assert_eq!(painted(cx, &page), row);
    assert_eq!(undo_depth(cx, &page), 1);
}

#[gpui_kit::test]
fn the_toolbar_drives_the_editor(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let tool = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).editor().tool());
    assert_eq!(
        support::disabled_ids(
            cx,
            s.win,
            &[
                ids::CROSSHAIR_MAKER_UNDO.into(),
                ids::CROSSHAIR_MAKER_REDO.into()
            ]
        )
        .len(),
        2
    );
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_FILL);
    assert_eq!(tool(cx), Tool::Fill);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_ERASER);
    assert_eq!(tool(cx), Tool::Eraser);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_PENCIL);
    assert_eq!(tool(cx), Tool::Pencil);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_MIRROR_X);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_MIRROR_X);
    assert!(!cx.update(|cx| page.read(cx).editor().mirror_x()));

    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_SIZE).as_deref(),
        Some("64x64")
    );
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SIZE);
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().size()),
        CanvasSize::Small
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_SIZE).as_deref(),
        Some("32x32")
    );
    assert!(!support::disabled_of(cx, s.win, ids::CROSSHAIR_MAKER_UNDO));
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_UNDO);
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().size()),
        CanvasSize::Large
    );

    paint_cell(cx, &s, &page, 1, 1);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_CLEAR);
    assert!(painted(cx, &page).is_empty());
    assert_eq!(undo_depth(cx, &page), 2);
}

/// Every button in the maker's toolbar.
const TOOLBAR: [&str; 21] = [
    ids::CROSSHAIR_MAKER_PENCIL,
    ids::CROSSHAIR_MAKER_ERASER,
    ids::CROSSHAIR_MAKER_FILL,
    ids::CROSSHAIR_MAKER_LINE,
    ids::CROSSHAIR_MAKER_RECTANGLE,
    ids::CROSSHAIR_MAKER_CIRCLE,
    ids::CROSSHAIR_MAKER_FILLED,
    ids::CROSSHAIR_MAKER_MIRROR_X,
    ids::CROSSHAIR_MAKER_MIRROR_Y,
    ids::CROSSHAIR_MAKER_GUIDES,
    ids::CROSSHAIR_MAKER_GRID_LINES,
    ids::CROSSHAIR_MAKER_ZOOM_OUT,
    ids::CROSSHAIR_MAKER_ZOOM,
    ids::CROSSHAIR_MAKER_ZOOM_IN,
    ids::CROSSHAIR_MAKER_UNDO,
    ids::CROSSHAIR_MAKER_REDO,
    ids::CROSSHAIR_MAKER_CLEAR,
    ids::CROSSHAIR_MAKER_SIZE,
    ids::CROSSHAIR_MAKER_NEW,
    ids::CROSSHAIR_MAKER_STARTERS,
    ids::CROSSHAIR_MAKER_IMPORT_PNG,
];

#[gpui_kit::test]
fn the_toolbar_stays_inside_the_canvas_column_at_small_window_sizes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let _page = open_maker(cx, &s);
    // 800x600 is the app's minimum window size.
    for (w, h) in [(800., 600.), (1280., 800.), (1200., 900.)] {
        cx.simulate_window_resize(s.win, gpui_kit::size(px(w), px(h)));
        // The view spans the canvas column; the toolbar buttons, it, the name field and the
        // side panel come from one frame.
        let mut wanted: Vec<ElementId> = TOOLBAR.into_iter().map(ElementId::from).collect();
        wanted.push(ids::CROSSHAIR_MAKER_VIEW.into());
        wanted.push(ids::CROSSHAIR_MAKER_NAME.into());
        wanted.push(ids::CROSSHAIR_MAKER_PANEL.into());
        let mut got = boxes(cx, &s, &wanted);
        let panel = got.pop().unwrap();
        let name = got.pop().unwrap();
        let column = got.pop().unwrap();
        assert!(
            column.right() <= panel.left(),
            "{w}x{h}: the column {column:?} runs under the panel {panel:?}"
        );
        for (id, b) in TOOLBAR.iter().zip(&got) {
            assert!(
                b.left() >= column.left() && b.right() <= column.right(),
                "{w}x{h}: {id} {b:?} leaves the column {column:?}"
            );
            assert!(
                b.bottom() <= column.top(),
                "{w}x{h}: {id} {b:?} overlaps the grid view {column:?}"
            );
            assert!(
                !b.intersects(&name),
                "{w}x{h}: {id} {b:?} overlaps the name field {name:?}"
            );
            assert!(
                !b.intersects(&panel),
                "{w}x{h}: {id} {b:?} overlaps the side panel {panel:?}"
            );
        }
        // The minimum window still leaves a grid worth zooming into.
        let tall = if w == 800. { px(100.) } else { px(200.) };
        assert!(column.size.height > tall, "{w}x{h}: {column:?}");
        // A narrower panel at small sizes still holds its widest row.
        let save = boxes(
            cx,
            &s,
            &[
                ids::CROSSHAIR_MAKER_SAVE.into(),
                ids::CROSSHAIR_MAKER_SAVE_AS_NEW.into(),
            ],
        );
        for b in [save[0], save[1], name] {
            assert!(
                b.left() >= panel.left() && b.right() <= panel.right(),
                "{w}x{h}: {b:?} leaves the panel {panel:?}"
            );
        }
    }
}

#[gpui_kit::test]
fn no_toolbar_row_starts_or_ends_with_a_divider(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let _page = open_maker(cx, &s);
    for (w, h) in [(800., 600.), (1200., 900.), (1280., 800.)] {
        cx.simulate_window_resize(s.win, gpui_kit::size(px(w), px(h)));
        let dividers: Vec<ElementId> = (0..5).map(ids::crosshair_maker_divider).collect();
        let mut wanted: Vec<ElementId> = TOOLBAR.into_iter().map(ElementId::from).collect();
        wanted.extend(dividers.iter().cloned());
        wanted.push(ids::CROSSHAIR_MAKER_VIEW.into());
        let mut got = boxes(cx, &s, &wanted);
        let column = got.pop().unwrap();
        // A divider left of the column is clipped away at a row's start, so only boxes inside
        // the column are drawn; none may straddle its edge.
        let mut rows: Vec<Vec<(bool, Bounds<Pixels>)>> = Vec::new();
        for (ix, b) in got.into_iter().enumerate() {
            let divider = ix >= TOOLBAR.len();
            if divider && b.right() <= column.left() {
                continue;
            }
            assert!(
                b.left() >= column.left() && b.right() <= column.right(),
                "{w}x{h}: {:?} {b:?} is cut by the column {column:?}",
                wanted[ix]
            );
            match rows
                .iter_mut()
                .find(|row| (support::mid_y(row[0].1) - support::mid_y(b)).abs() < px(2.))
            {
                Some(row) => row.push((divider, b)),
                None => rows.push(vec![(divider, b)]),
            }
        }
        if w == 1200. {
            assert!(rows.len() >= 2, "{w}x{h}: the toolbar no longer wraps");
        }
        for row in &mut rows {
            row.sort_by(|a, b| a.1.left().partial_cmp(&b.1.left()).unwrap());
            let (first, last) = (row.first().unwrap(), row.last().unwrap());
            assert!(
                !first.0,
                "{w}x{h}: a row starts with a divider at {:?}",
                first.1
            );
            assert!(
                !last.0,
                "{w}x{h}: a row ends with a divider at {:?}",
                last.1
            );
        }
    }
}

/// Scrolls the side panel down by `by` pixels with the wheel.
fn scroll_panel(cx: &mut TestAppContext, s: &support::ShellWindow, by: f32) {
    support::settle(cx, s.win);
    let delta = ScrollDelta::Pixels(point(px(0.), px(-by)));
    cx.update_window(s.win, |_, w, cx| {
        w.scroll(ids::CROSSHAIR_MAKER_PANEL, delta, cx)
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_short_window_scrolls_the_side_panel_to_its_last_control(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let _page = open_maker(cx, &s);
    let last = [ElementId::from(ids::CROSSHAIR_MAKER_PREVIEW_COLOR)];
    // 1280x800 is where QA saw the cut; 720 is short enough to cut it today.
    for h in [800., 720.] {
        cx.simulate_window_resize(s.win, gpui_kit::size(px(1280.), px(h)));
        let before = boxes(cx, &s, &last)[0];
        if h == 720. {
            assert!(before.bottom() > px(h), "the panel fits at {h}: {before:?}");
        }
        scroll_panel(cx, &s, 1000.);
        let after = boxes(cx, &s, &last)[0];
        assert!(
            after.top() >= px(0.),
            "{h}: scrolled past the top: {after:?}"
        );
        assert!(after.bottom() <= px(h), "{h}: still cut off: {after:?}");
    }
}

#[gpui_kit::test]
fn a_tall_window_leaves_the_side_panel_still(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let _page = open_maker(cx, &s);
    cx.simulate_window_resize(s.win, gpui_kit::size(px(1280.), px(1200.)));
    let wanted = [
        ElementId::from(ids::CROSSHAIR_MAKER_NAME),
        ids::CROSSHAIR_MAKER_PREVIEW_COLOR.into(),
    ];
    let before = boxes(cx, &s, &wanted);
    assert!(before[1].bottom() <= px(1200.), "{before:?}");
    scroll_panel(cx, &s, 1000.);
    assert_eq!(boxes(cx, &s, &wanted), before);
}

#[gpui_kit::test]
fn a_drag_out_and_back_follows_the_edge(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let at = grid(cx, &s, &page);
    // Above the top edge, over columns 20 and 30.
    let above = |x: u32| point(centre(at, x, 0).x, at.0.top() - px(12.));
    send(cx, &s, mouse_move(centre(at, 10, 3), false));
    send(cx, &s, mouse_down(centre(at, 10, 3)));
    send(cx, &s, mouse_move(centre(at, 10, 0), true));
    send(cx, &s, mouse_move(above(20), true));
    send(cx, &s, mouse_move(above(30), true));
    send(cx, &s, mouse_move(centre(at, 30, 3), true));
    send(cx, &s, mouse_up(centre(at, 30, 3)));
    let mut path: Vec<(u32, u32)> = (10..=30).map(|x| (x, 0)).collect();
    for y in 1..=3 {
        path.extend([(10, y), (30, y)]);
    }
    assert_eq!(painted(cx, &page), path);
    assert_eq!(undo_depth(cx, &page), 1);
}

#[gpui_kit::test]
fn a_swatch_picks_a_colour_that_follows_the_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    for ix in 0..4 {
        assert!(
            support::present(cx, s.win, ids::crosshair_maker_swatch(ix)),
            "swatch {ix}"
        );
    }
    support::click_id(cx, s.win, ids::crosshair_maker_swatch(2));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().colour()),
        [128, 128, 128]
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_COLOR_HINT).as_deref(),
        Some(FOLLOWS)
    );
    paint_cell(cx, &s, &page, 0, 0);
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().pixel(0, 0)),
        [128, 128, 128, 255]
    );
}

#[gpui_kit::test]
fn the_colour_hint_shows_once_under_the_swatches(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let _page = open_maker(cx, &s);
    let got = boxes(
        cx,
        &s,
        &[
            ids::crosshair_maker_swatch(0),
            ids::CROSSHAIR_MAKER_COLOR_HINT.into(),
            ids::CROSSHAIR_MAKER_COLOR.into(),
        ],
    );
    let (swatch, hint, picker) = (got[0], got[1], got[2]);
    // One column gap apart on each side, so no second caption fits above or below the hint.
    let gap = px(8.5);
    assert!(
        hint.top() >= swatch.bottom() && hint.top() - swatch.bottom() <= gap,
        "hint {hint:?} is not right under the swatches {swatch:?}"
    );
    assert!(
        picker.top() >= hint.bottom() && picker.top() - hint.bottom() <= gap,
        "the colour picker {picker:?} is not right under the hint {hint:?}"
    );
    let follows = cx
        .update_window(s.win, |_, w, _| {
            gpui_kit::base::test_support::snapshots(w)
                .iter()
                .filter(|e| e.label() == Some(FOLLOWS))
                .count()
        })
        .unwrap();
    assert_eq!(follows, 1);
}

#[gpui_kit::test]
fn a_picked_colour_is_mixed_in_game(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    assert!(support::present(cx, s.win, ids::CROSSHAIR_MAKER_COLOR));
    let state = cx.update(|cx| page.read(cx).colour_state().clone());
    cx.update(|cx| {
        state.update(cx, |_, cx| {
            cx.emit(ColorPickerEvent::Change(Some(hsla_of([200, 40, 40]))))
        })
    });
    support::settle(cx, s.win);
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().colour()),
        [200, 40, 40]
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_COLOR_HINT).as_deref(),
        Some(MIXED)
    );
}

#[gpui_kit::test]
fn the_preview_shows_the_drawing_over_a_map_tinted_by_the_preview_colour(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_PREVIEW).as_deref(),
        Some("Preview on 2fort")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_BACKGROUND).as_deref(),
        Some("2fort")
    );
    assert!(support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_PREVIEW_COLOR
    ));
    let preview = boxes(cx, &s, &[ids::CROSSHAIR_MAKER_PREVIEW.into()])[0];
    assert!(
        preview.size.width >= px(64.) && preview.size.height >= px(64.),
        "{preview:?}"
    );
    assert_eq!(
        cx.update(|cx| page.read(cx).preview_colour()),
        [255, 255, 255]
    );
    let state = cx.update(|cx| page.read(cx).preview_colour_state().clone());
    cx.update(|cx| {
        state.update(cx, |_, cx| {
            cx.emit(ColorPickerEvent::Change(Some(hsla_of([255, 0, 0]))))
        })
    });
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| page.read(cx).preview_colour()), [255, 0, 0]);
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

/// Clicks Save and waits for the background save to finish.
fn save(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<CrosshairMakerPage>) {
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SAVE);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    support::settle(cx, s.win);
}

fn open_id(
    cx: &mut TestAppContext,
    page: &Entity<CrosshairMakerPage>,
) -> Option<tf2tools_core::CrosshairId> {
    cx.update(|cx| page.read(cx).editor().open_id().cloned())
}

#[gpui_kit::test]
fn save_waits_for_a_name(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    assert!(support::disabled_of(cx, s.win, ids::CROSSHAIR_MAKER_SAVE));
    set_name(cx, &s, &page, "   ");
    assert!(support::disabled_of(cx, s.win, ids::CROSSHAIR_MAKER_SAVE));
    set_name(cx, &s, &page, "Dot");
    assert!(!support::disabled_of(cx, s.win, ids::CROSSHAIR_MAKER_SAVE));
}

#[gpui_kit::test]
fn saving_a_new_drawing_adds_a_painted_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (changes, _sub) = support::library_changes(cx);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Dot");
    save(cx, &s, &page);
    let library = s.services.library.list_crosshairs().unwrap();
    assert_eq!(library.len(), 1);
    let entry = &library[0];
    assert_eq!(
        (entry.name(), entry.format()),
        ("Dot", CrosshairFormat::Painted)
    );
    let source = s.services.library.painted_source(entry.id()).unwrap();
    assert_eq!(
        source.drawing(),
        &cx.update(|cx| page.read(cx).editor().drawing())
    );
    assert!(!cx.update(|cx| page.read(cx).editor().is_dirty()));
    assert_eq!(open_id(cx, &page).as_ref(), Some(entry.id()));
    assert_eq!(support::toast_labels(cx, s.win), ["Saved"]);
    let rows = support::log_rows(&s.services);
    assert_eq!(
        (rows[0].category(), rows[0].action(), rows[0].message()),
        ("crosshair", "paint", "Saved painted crosshair: Dot")
    );
    assert_eq!(changes.get(), 1);
}

#[gpui_kit::test]
fn resaving_an_assigned_crosshair_says_to_install(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Dot");
    save(cx, &s, &page);
    let id = open_id(cx, &page).expect("a saved crosshair");
    s.services
        .library
        .assign("tf_weapon_scattergun", &CrosshairRef::Library(id.clone()))
        .unwrap();
    paint_cell(cx, &s, &page, 2, 2);
    set_name(cx, &s, &page, "Big Dot");
    save(cx, &s, &page);
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts
            .iter()
            .any(|t| t == "Saved. Press Install on the Crosshairs page to update it in TF2."),
        "{toasts:?}"
    );
    let library = s.services.library.list_crosshairs().unwrap();
    assert_eq!(library.len(), 1);
    assert_eq!((library[0].id(), library[0].name()), (&id, "Big Dot"));
    let kept: Vec<_> = s
        .services
        .library
        .assignments()
        .unwrap()
        .into_iter()
        .map(|a| (a.weapon_script().to_owned(), a.crosshair().clone()))
        .collect();
    assert_eq!(
        kept,
        [("tf_weapon_scattergun".to_owned(), CrosshairRef::Library(id))]
    );
}

#[gpui_kit::test]
fn a_taken_name_shows_under_the_field(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    support::seed_crosshair(&s.services, &env, "Dot");
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "dot");
    save(cx, &s, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_NAME_ERROR).as_deref(),
        Some("You already have a crosshair called dot. Pick another name.")
    );
    assert!(support::toast_labels(cx, s.win).is_empty());
    assert!(cx.update(|cx| page.read(cx).editor().is_dirty()));
    assert_eq!(open_id(cx, &page), None);
    set_name(cx, &s, &page, "Ring");
    assert!(!support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_NAME_ERROR
    ));
    save(cx, &s, &page);
    let names: Vec<String> = s
        .services
        .library
        .list_crosshairs()
        .unwrap()
        .iter()
        .map(|c| c.name().to_owned())
        .collect();
    assert!(names.contains(&"Ring".to_owned()), "{names:?}");
}

#[gpui_kit::test]
fn a_crosshair_deleted_while_open_saves_as_new(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Dot");
    save(cx, &s, &page);
    let first = open_id(cx, &page).expect("a saved crosshair");
    s.services.library.delete_crosshair(&first).unwrap();
    paint_cell(cx, &s, &page, 2, 2);
    save(cx, &s, &page);
    let deleted = "This crosshair was deleted. Save it under a name to keep it as a new one.";
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts
            .iter()
            .any(|t| *t == format!("Failed to save crosshair. {deleted}")),
        "{toasts:?}"
    );
    support::assert_plain(deleted);
    assert_eq!(open_id(cx, &page), None);
    assert!(cx.update(|cx| page.read(cx).editor().is_dirty()));
    save(cx, &s, &page);
    let library = s.services.library.list_crosshairs().unwrap();
    assert_eq!(library.len(), 1);
    assert_eq!(library[0].name(), "Dot");
    assert_ne!(library[0].id(), &first);
    assert_eq!(open_id(cx, &page).as_ref(), Some(library[0].id()));
}

/// A 64x64 drawing with one red texel at (5, 6), saved as painted crosshair `name`.
fn seed_painted(s: &support::ShellWindow, name: &str) -> (CrosshairEntry, RgbaImage) {
    let mut pixels = vec![0u8; 64 * 64 * 4];
    let at = (6 * 64 + 5) * 4;
    pixels[at..at + 4].copy_from_slice(&[255, 0, 0, 255]);
    let drawing = RgbaImage::from_pixels(64, 64, pixels).unwrap();
    let entry = s
        .services
        .library
        .save_painted_crosshair(&PaintTarget::New, name, &drawing)
        .unwrap();
    (entry, drawing)
}

fn crosshair_dir(env: &support::Env, entry: &CrosshairEntry) -> PathBuf {
    env.data
        .library_dir(LibraryKind::Crosshairs)
        .join(entry.folder_name())
}

/// Shows the Crosshairs page, searches for `entry` by name and selects it.
fn select_on_crosshairs(cx: &mut TestAppContext, s: &support::ShellWindow, entry: &CrosshairEntry) {
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshairs_page())
        .expect("a Crosshairs page");
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    let input = cx.update(|cx| page.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(entry.name(), window, cx))
    })
    .unwrap();
    support::click_id(cx, s.win, ids::crosshair_library(entry.folder_name()));
}

/// Clicks Edit and waits for the maker to load the drawing.
fn edit(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairMakerPage> {
    support::click_id(cx, s.win, ids::CROSSHAIRS_EDIT);
    let page = cx
        .update(|cx| s.shell.read(cx).crosshair_maker_page())
        .expect("the maker");
    support::wait_real(cx, "the drawing to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

#[gpui_kit::test]
fn edit_shows_only_for_painted_crosshairs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, _) = seed_painted(&s, "Zebra");
    let yak = support::seed_crosshair(&s.services, &env, "Yak");
    select_on_crosshairs(cx, &s, &zebra);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_EDIT).as_deref(),
        Some("Edit")
    );
    select_on_crosshairs(cx, &s, &yak);
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_EDIT));
}

// Leaving the maker drops it, so a crosshair can only be deleted once the maker has let go.
#[gpui_kit::test]
fn deleting_what_the_maker_had_open_leaves_a_fresh_maker(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, _) = seed_painted(&s, "Zebra");
    select_on_crosshairs(cx, &s, &zebra);
    let maker = edit(cx, &s);
    assert_eq!(open_id(cx, &maker).as_ref(), Some(zebra.id()));
    drop(maker);

    select_on_crosshairs(cx, &s, &zebra);
    support::click_id(cx, s.win, ids::CROSSHAIRS_DELETE);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the delete", |_| {
        s.services.library.list_crosshairs().unwrap().is_empty()
    });

    let fresh = open_maker(cx, &s);
    assert_eq!(open_id(cx, &fresh), None);
    paint_cell(cx, &s, &fresh, 1, 1);
    set_name(cx, &s, &fresh, "Zebra");
    save(cx, &s, &fresh);
    let library = s.services.library.list_crosshairs().unwrap();
    assert_eq!(library.len(), 1);
    assert_ne!(library[0].id(), zebra.id());
}

#[gpui_kit::test]
fn edit_opens_the_drawing_in_the_maker(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, drawing) = seed_painted(&s, "Zebra");
    select_on_crosshairs(cx, &s, &zebra);
    let page = edit(cx, &s);
    assert_eq!(
        cx.update(|cx| s.shell.read(cx).page()),
        Page::CrosshairMaker
    );
    let (got, id, dirty) = cx.update(|cx| {
        let e = page.read(cx).editor();
        (e.drawing(), e.open_id().cloned(), e.is_dirty())
    });
    assert_eq!(got, drawing);
    assert_eq!(id.as_ref(), Some(zebra.id()));
    assert!(!dirty);
    assert_eq!(
        cx.update(|cx| page.read(cx).name_input().read(cx).value().to_string()),
        "Zebra"
    );
    assert!(support::toast_labels(cx, s.win).is_empty());
}

#[gpui_kit::test]
fn a_damaged_drawing_is_rebuilt_from_the_crosshair_file(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, drawing) = seed_painted(&s, "Zebra");
    std::fs::write(crosshair_dir(&env, &zebra).join("source.png"), b"not a png").unwrap();
    select_on_crosshairs(cx, &s, &zebra);
    let page = edit(cx, &s);
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts
            .iter()
            .any(|t| t
                == "The saved drawing was damaged, so it was rebuilt from the crosshair file."),
        "{toasts:?}"
    );
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), drawing);
}

#[gpui_kit::test]
fn an_unreadable_crosshair_says_so_and_leaves_a_new_canvas(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, _) = seed_painted(&s, "Zebra");
    let dir = crosshair_dir(&env, &zebra);
    std::fs::write(dir.join("source.png"), b"not a png").unwrap();
    std::fs::write(
        dir.join(format!("{}.vtf", zebra.folder_name())),
        b"not a vtf",
    )
    .unwrap();
    select_on_crosshairs(cx, &s, &zebra);
    let page = edit(cx, &s);
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts.iter().any(|t| t
            == "Failed to open crosshair. This crosshair can't be opened for editing. You can still use it, or delete it and draw a new one."),
        "{toasts:?}"
    );
    assert_eq!(open_id(cx, &page), None);
    assert!(painted(cx, &page).is_empty());
    assert!(!cx.update(|cx| page.read(cx).editor().is_dirty()));
}

/// The controls that change the drawing, which a running save or open locks.
const LOCKED: [&str; 9] = [
    ids::CROSSHAIR_MAKER_UNDO,
    ids::CROSSHAIR_MAKER_REDO,
    ids::CROSSHAIR_MAKER_CLEAR,
    ids::CROSSHAIR_MAKER_SIZE,
    ids::CROSSHAIR_MAKER_NEW,
    ids::CROSSHAIR_MAKER_STARTERS,
    ids::CROSSHAIR_MAKER_IMPORT_PNG,
    ids::CROSSHAIR_MAKER_SAVE,
    ids::CROSSHAIR_MAKER_SAVE_AS_NEW,
];

/// A drawing with undo and redo both available and a name, so every LOCKED control is live.
fn ready_to_save(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
) {
    paint_cell(cx, s, page, 1, 1);
    paint_cell(cx, s, page, 2, 2);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_UNDO);
    set_name(cx, s, page, "Dot");
    assert!(support::disabled_ids(cx, s.win, &LOCKED.map(ElementId::from)).is_empty());
}

/// Runs `start` and, in the frame after it with no background work run yet, checks the page
/// is busy, every LOCKED control is disabled and a press on cell (5, 5) paints nothing.
fn assert_locked_during(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    start: impl FnOnce(&mut Window, &mut App),
) {
    let (_, cell) = grid(cx, s, page);
    let offset = point(cell * 5.5, cell * 5.5);
    let (busy, disabled, pixel) = cx
        .update_window(s.win, |_, w, cx| {
            start(w, cx);
            w.render_frame(cx);
            let busy = page.read(cx).is_saving() || !page.read(cx).is_loaded();
            let disabled: Vec<&str> = LOCKED
                .into_iter()
                .filter(|id| support::button_disabled(w, *id))
                .collect();
            w.click_at(ids::CROSSHAIR_MAKER_GRID, offset, cx);
            (busy, disabled, page.read(cx).editor().pixel(5, 5))
        })
        .unwrap();
    assert!(busy, "the save or open is still running");
    assert_eq!(disabled, LOCKED);
    assert_eq!(pixel, CLEAR);
}

#[gpui_kit::test]
fn a_running_save_locks_the_drawing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    ready_to_save(cx, &s, &page);
    assert_locked_during(cx, &s, &page, |w, cx| {
        w.click(ids::CROSSHAIR_MAKER_SAVE, cx)
    });
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    assert!(!cx.update(|cx| page.read(cx).editor().is_dirty()));
    assert!(support::disabled_ids(cx, s.win, &LOCKED.map(ElementId::from)).is_empty());
}

#[gpui_kit::test]
fn an_opening_drawing_locks_the_canvas(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, drawing) = seed_painted(&s, "Zebra");
    let page = open_maker(cx, &s);
    ready_to_save(cx, &s, &page);
    let id = zebra.id().clone();
    let opener = page.clone();
    assert_locked_during(cx, &s, &page, move |_, cx| {
        opener.update(cx, |page, cx| page.open(id, cx))
    });
    support::wait_real(cx, "the drawing to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), drawing);
    assert!(
        support::disabled_ids(
            cx,
            s.win,
            &[
                ids::CROSSHAIR_MAKER_NEW.into(),
                ids::CROSSHAIR_MAKER_SAVE.into()
            ]
        )
        .is_empty()
    );
}

/// Runs `start` and, in the frame after it with no background work run yet, presses Ctrl+Z
/// and Ctrl+Y and checks neither changed the drawing.
fn assert_keys_locked_during(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    start: impl FnOnce(&mut Window, &mut App),
) {
    let before = cx.update(|cx| page.read(cx).editor().drawing());
    let (busy, undone, redone) = cx
        .update_window(s.win, |_, w, cx| {
            start(w, cx);
            w.render_frame(cx);
            let busy = page.read(cx).is_saving() || !page.read(cx).is_loaded();
            w.press("ctrl-z", cx);
            let undone = page.read(cx).editor().drawing();
            w.press("ctrl-y", cx);
            (busy, undone, page.read(cx).editor().drawing())
        })
        .unwrap();
    assert!(busy, "the save or open is still running");
    assert!(undone == before, "Ctrl+Z changed the drawing");
    assert!(redone == before, "Ctrl+Y changed the drawing");
}

#[gpui_kit::test]
fn undo_and_redo_keys_wait_for_a_running_save(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    ready_to_save(cx, &s, &page);
    assert_keys_locked_during(cx, &s, &page, |w, cx| {
        w.click(ids::CROSSHAIR_MAKER_SAVE, cx)
    });
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    // Once the save lands the keys work again.
    support::press(cx, s.win, "ctrl-z");
    assert!(painted(cx, &page).is_empty());
}

#[gpui_kit::test]
fn undo_and_redo_keys_wait_for_an_opening_drawing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, drawing) = seed_painted(&s, "Zebra");
    let page = open_maker(cx, &s);
    ready_to_save(cx, &s, &page);
    let id = zebra.id().clone();
    let opener = page.clone();
    assert_keys_locked_during(cx, &s, &page, move |_, cx| {
        opener.update(cx, |page, cx| page.open(id, cx))
    });
    support::wait_real(cx, "the drawing to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), drawing);
}

/// Runs `start` and, in the frame after it with no background work run yet, hands the page a
/// starter; true when the page was busy and the drawing did not change.
fn use_waits_during(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    start: impl FnOnce(&mut Window, &mut App),
) -> bool {
    use tf2tools_app::pages::crosshair_maker::starter::{Starter, starter_drawing};
    let large = CanvasSize::Large;
    let starter = starter_drawing(Starter::Dot, Starter::Dot.defaults(large), large, [255; 4]);
    let before = cx.update(|cx| page.read(cx).editor().drawing());
    cx.update_window(s.win, |_, w, cx| {
        start(w, cx);
        w.render_frame(cx);
        let busy = page.read(cx).is_saving() || !page.read(cx).is_loaded();
        page.update(cx, |page, cx| page.use_starter(starter, w, cx));
        busy && page.read(cx).editor().drawing() == before
    })
    .unwrap()
}

#[gpui_kit::test]
fn use_waits_for_a_running_save_or_open(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, drawing) = seed_painted(&s, "Zebra");
    let page = open_maker(cx, &s);
    ready_to_save(cx, &s, &page);
    let before = cx.update(|cx| page.read(cx).editor().drawing());
    assert!(use_waits_during(cx, &s, &page, |w, cx| {
        w.click(ids::CROSSHAIR_MAKER_SAVE, cx)
    }));
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    // Nothing was queued behind the save: no discard question and the same drawing.
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), before);

    let id = zebra.id().clone();
    let opener = page.clone();
    assert!(use_waits_during(cx, &s, &page, move |_, cx| {
        opener.update(cx, |page, cx| page.open(id, cx))
    }));
    support::wait_real(cx, "the drawing to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), drawing);
}

fn can_leave(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>) -> bool {
    cx.update(|cx| page.read(cx).can_leave())
}

#[gpui_kit::test]
fn new_on_a_saved_drawing_starts_over_without_asking(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SIZE);
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Dot");
    save(cx, &s, &page);
    assert!(can_leave(cx, &page));
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_NEW);
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(open_id(cx, &page), None);
    assert!(painted(cx, &page).is_empty());
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().size()),
        CanvasSize::Large
    );
    assert!(can_leave(cx, &page));
    assert_eq!(
        cx.update(|cx| page.read(cx).name_input().read(cx).value().to_string()),
        ""
    );
}

#[gpui_kit::test]
fn new_with_unsaved_changes_asks_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    assert!(!can_leave(cx, &page));
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_NEW);
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("You have unsaved changes. Discard them?")
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(painted(cx, &page), [(1, 1)]);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_NEW);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    assert!(painted(cx, &page).is_empty());
    assert!(can_leave(cx, &page));
}

#[gpui_kit::test]
fn the_sidebar_asks_before_dropping_a_drawing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    assert!(!cx.update(|cx| leave::can_leave(cx)));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(
        cx.update(|cx| s.shell.read(cx).page()),
        Page::CrosshairMaker
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert_eq!(
        cx.update(|cx| s.shell.read(cx).page()),
        Page::CrosshairMaker
    );
    assert_eq!(painted(cx, &page), [(1, 1)]);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Huds);
    assert!(
        cx.update(|cx| s.shell.read(cx).crosshair_maker_page())
            .is_none()
    );
    assert!(cx.update(|cx| leave::can_leave(cx)));
}

// Keep editing, by button or Esc, hands the keys back to the maker without a click on it.
#[gpui_kit::test]
fn keep_editing_gives_the_keys_back_to_the_maker(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    for dismiss in ["button", "escape"] {
        paint_cell(cx, &s, &page, 1, 1);
        support::click_id(cx, s.win, ids::nav(Page::Huds));
        assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
        match dismiss {
            "button" => support::click_id(cx, s.win, ids::CONFIRM_CANCEL),
            _ => support::press(cx, s.win, "escape"),
        }
        support::settle(cx, s.win);
        assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
        support::press(cx, s.win, "ctrl-z");
        assert!(painted(cx, &page).is_empty(), "{dismiss}: Ctrl+Z was lost");
    }
}

#[gpui_kit::test]
fn a_clean_maker_leaves_without_asking(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let _page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Huds);
}

// Headless windows never get a platform close request, so this calls the handler the shell
// registers directly.
#[gpui_kit::test]
fn closing_the_window_asks_first_and_discard_closes_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    // A second window keeps the app from quitting when the shell's window goes.
    let _other = support::blank_window(cx);
    let page = open_maker(cx, &s);
    let should_close = |cx: &mut TestAppContext| {
        cx.update_window(s.win, |_, w, cx| leave::should_close(w, cx))
            .unwrap()
    };
    assert!(should_close(cx));
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    paint_cell(cx, &s, &page, 1, 1);
    assert!(!should_close(cx));
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert!(cx.update(|cx| cx.windows().contains(&s.win)));
    assert!(!should_close(cx));
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, w, cx| w.click(ids::CONFIRM_OK, cx))
        .unwrap();
    cx.run_until_parked();
    assert!(!cx.update(|cx| cx.windows().contains(&s.win)));
}

#[gpui_kit::test]
fn a_second_close_request_does_not_stack_another_dialog(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let should_close = |cx: &mut TestAppContext| {
        let close = cx
            .update_window(s.win, |_, w, cx| leave::should_close(w, cx))
            .unwrap();
        support::settle(cx, s.win);
        close
    };
    paint_cell(cx, &s, &page, 1, 1);
    assert!(!should_close(cx));
    assert!(!should_close(cx));
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    // Once answered, the next request asks again.
    assert!(!should_close(cx));
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
}

#[gpui_kit::test]
fn letter_keys_pick_the_tools_as_soon_as_the_page_opens(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let tool = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).editor().tool());
    support::press(cx, s.win, "e");
    assert_eq!(tool(cx), Tool::Eraser);
    support::press(cx, s.win, "f");
    assert_eq!(tool(cx), Tool::Fill);
    support::press(cx, s.win, "p");
    assert_eq!(tool(cx), Tool::Pencil);
}

#[gpui_kit::test]
fn ctrl_z_undoes_and_ctrl_y_or_ctrl_shift_z_redoes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 4, 4);
    support::press(cx, s.win, "ctrl-z");
    assert!(painted(cx, &page).is_empty());
    support::press(cx, s.win, "ctrl-y");
    assert_eq!(painted(cx, &page), [(4, 4)]);
    support::press(cx, s.win, "ctrl-z");
    assert!(painted(cx, &page).is_empty());
    support::press(cx, s.win, "ctrl-shift-z");
    assert_eq!(painted(cx, &page), [(4, 4)]);
}

#[gpui_kit::test]
fn typing_in_the_name_field_never_triggers_a_shortcut(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    // A click on the grid gives the page focus back, so keys work after painting.
    support::press(cx, s.win, "e");
    assert_eq!(cx.update(|cx| page.read(cx).editor().tool()), Tool::Eraser);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_NAME);
    support::press(cx, s.win, "p");
    support::press(cx, s.win, "f");
    assert_eq!(cx.update(|cx| page.read(cx).editor().tool()), Tool::Eraser);
    assert_eq!(
        cx.update(|cx| page.read(cx).name_input().read(cx).value().to_string()),
        "pf"
    );
    support::press(cx, s.win, "ctrl-z");
    assert_eq!(painted(cx, &page), [(1, 1)]);
    assert_eq!(undo_depth(cx, &page), 1);
}

#[gpui_kit::test]
fn clicking_a_toolbar_button_takes_the_keys_back_from_the_name_field(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_NAME);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_ERASER);
    support::press(cx, s.win, "f");
    assert_eq!(cx.update(|cx| page.read(cx).editor().tool()), Tool::Fill);
    assert_eq!(
        cx.update(|cx| page.read(cx).name_input().read(cx).value().to_string()),
        ""
    );
}

/// An Alt-held press and release at `at`, after moving the pointer there.
fn alt_click(cx: &mut TestAppContext, s: &support::ShellWindow, at: Point<Pixels>) {
    send(cx, s, mouse_move(at, false));
    send(
        cx,
        s,
        MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::alt(),
            click_count: 1,
            first_mouse: false,
        },
    );
    send(
        cx,
        s,
        MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::alt(),
            click_count: 1,
        },
    );
}

#[gpui_kit::test]
fn alt_click_takes_a_pixels_colour_into_the_picker(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let state = cx.update(|cx| page.read(cx).colour_state().clone());
    let colour = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).editor().colour());
    cx.update(|cx| {
        state.update(cx, |_, cx| {
            cx.emit(ColorPickerEvent::Change(Some(hsla_of([200, 40, 40]))))
        })
    });
    support::settle(cx, s.win);
    paint_cell(cx, &s, &page, 2, 2);
    support::click_id(cx, s.win, ids::crosshair_maker_swatch(0));
    assert_eq!(colour(cx), [255, 255, 255]);
    let at = grid(cx, &s, &page);
    alt_click(cx, &s, centre(at, 2, 2));
    assert_eq!(colour(cx), [200, 40, 40]);
    assert_eq!(
        cx.update(|cx| state.read(cx).value()).map(rgb_of),
        Some([200, 40, 40])
    );
    assert_eq!(painted(cx, &page), [(2, 2)]);
    assert_eq!(undo_depth(cx, &page), 1);
    // A clear pixel leaves the colour alone and paints nothing.
    alt_click(cx, &s, centre(at, 9, 9));
    assert_eq!(colour(cx), [200, 40, 40]);
    assert_eq!(painted(cx, &page), [(2, 2)]);
    assert!(!cx.update(|cx| page.read(cx).editor().in_stroke()));
}

/// Asserts both guides cross at the grid's middle and span it, in one settled frame.
fn assert_guides_centred(cx: &mut TestAppContext, s: &support::ShellWindow) {
    let b = boxes(
        cx,
        s,
        &[
            ids::CROSSHAIR_MAKER_GRID.into(),
            ids::CROSSHAIR_MAKER_GUIDE_V.into(),
            ids::CROSSHAIR_MAKER_GUIDE_H.into(),
        ],
    );
    let (grid, v, h) = (b[0], b[1], b[2]);
    assert_eq!(v.center().x, grid.center().x, "{b:?}");
    assert_eq!((v.top(), v.bottom()), (grid.top(), grid.bottom()), "{b:?}");
    assert_eq!(h.center().y, grid.center().y, "{b:?}");
    assert_eq!((h.left(), h.right()), (grid.left(), grid.right()), "{b:?}");
    assert!(v.size.width > px(0.) && h.size.height > px(0.), "{b:?}");
}

#[gpui_kit::test]
fn centre_guides_cross_at_the_middle_and_toggle(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    assert!(cx.update(|cx| page.read(cx).guides()));
    assert_guides_centred(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SIZE);
    assert_guides_centred(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_GUIDES);
    assert!(!cx.update(|cx| page.read(cx).guides()));
    assert!(!support::present(cx, s.win, ids::CROSSHAIR_MAKER_GUIDE_V));
    assert!(!support::present(cx, s.win, ids::CROSSHAIR_MAKER_GUIDE_H));
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_GUIDES);
    assert_guides_centred(cx, &s);
}

#[gpui_kit::test]
fn hovering_a_cell_shows_its_distance_from_the_centre(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    assert!(!support::present(cx, s.win, ids::CROSSHAIR_MAKER_POSITION));
    let at = grid(cx, &s, &page);
    send(cx, &s, mouse_move(centre(at, 34, 29), false));
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_POSITION).as_deref(),
        Some("3 right, 3 up")
    );
    send(cx, &s, mouse_move(centre(at, 31, 32), false));
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_POSITION).as_deref(),
        Some("1 left, 1 down")
    );
    send(
        cx,
        &s,
        mouse_move(point(at.0.right() + px(12.), at.0.top()), false),
    );
    assert!(!support::present(cx, s.win, ids::CROSSHAIR_MAKER_POSITION));
    assert!(painted(cx, &page).is_empty());
}

/// Moves the pointer over cell (34, 29) and checks the readout shows it.
fn hover_a_cell(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
) {
    let at = grid(cx, s, page);
    send(cx, s, mouse_move(centre(at, 34, 29), false));
    assert!(support::present(cx, s.win, ids::CROSSHAIR_MAKER_POSITION));
}

#[gpui_kit::test]
fn the_position_readout_clears_when_the_pointer_leaves_the_window(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    hover_a_cell(cx, &s, &page);
    send(
        cx,
        &s,
        MouseExitEvent {
            position: point(px(-5.), px(-5.)),
            pressed_button: None,
            modifiers: Modifiers::default(),
        },
    );
    assert!(!support::present(cx, s.win, ids::CROSSHAIR_MAKER_POSITION));
}

/// Clicks `id` with a press and release and no move, as under a still pointer.
fn still_click(cx: &mut TestAppContext, s: &support::ShellWindow, id: &'static str) {
    let at = boxes(cx, s, &[id.into()])[0].center();
    send(cx, s, mouse_down(at));
    send(cx, s, mouse_up(at));
}

#[gpui_kit::test]
fn the_position_readout_clears_and_waits_while_a_dialog_is_open(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let at = grid(cx, &s, &page);
    // The Starters dialog, then the discard question New asks over unsaved changes.
    for (opener, dialog) in [
        (
            ids::CROSSHAIR_MAKER_STARTERS,
            ids::CROSSHAIR_MAKER_STARTER_DIALOG,
        ),
        (ids::CROSSHAIR_MAKER_NEW, ids::CONFIRM_DIALOG),
    ] {
        if opener == ids::CROSSHAIR_MAKER_NEW {
            paint_cell(cx, &s, &page, 0, 0);
        }
        hover_a_cell(cx, &s, &page);
        still_click(cx, &s, opener);
        assert!(support::present(cx, s.win, dialog), "{dialog}");
        assert!(
            !support::present(cx, s.win, ids::CROSSHAIR_MAKER_POSITION),
            "{dialog} opened"
        );
        send(cx, &s, mouse_move(centre(at, 31, 32), false));
        assert!(
            !support::present(cx, s.win, ids::CROSSHAIR_MAKER_POSITION),
            "a move under {dialog}"
        );
        support::press(cx, s.win, "escape");
        assert!(!support::present(cx, s.win, dialog), "{dialog} closed");
    }
}

#[gpui_kit::test]
fn the_position_readout_clears_when_the_size_changes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    hover_a_cell(cx, &s, &page);
    // A press and release with no move, as a click on a button under a still pointer.
    let size = boxes(cx, &s, &[ids::CROSSHAIR_MAKER_SIZE.into()])[0].center();
    send(cx, &s, mouse_down(size));
    send(cx, &s, mouse_up(size));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().size()),
        CanvasSize::Small
    );
    assert!(!support::present(cx, s.win, ids::CROSSHAIR_MAKER_POSITION));
}

#[gpui_kit::test]
fn the_position_readout_clears_when_a_drawing_opens(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, _) = seed_painted(&s, "Zebra");
    let page = open_maker(cx, &s);
    hover_a_cell(cx, &s, &page);
    let id = zebra.id().clone();
    cx.update(|cx| page.update(cx, |page, cx| page.open(id, cx)));
    support::wait_real(cx, "the drawing to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert!(!support::present(cx, s.win, ids::CROSSHAIR_MAKER_POSITION));
}

#[gpui_kit::test]
fn the_centre_guides_never_reach_the_saved_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    assert!(cx.update(|cx| page.read(cx).guides()));
    paint_cell(cx, &s, &page, 31, 31);
    paint_cell(cx, &s, &page, 32, 32);
    set_name(cx, &s, &page, "Centre");
    save(cx, &s, &page);
    let entry = &s.services.library.list_crosshairs().unwrap()[0];
    let source = s.services.library.painted_source(entry.id()).unwrap();
    let opaque = source
        .drawing()
        .pixels()
        .chunks(4)
        .filter(|p| p[3] != 0)
        .count();
    assert_eq!(opaque, 2);
    assert_eq!(
        source.drawing(),
        &cx.update(|cx| page.read(cx).editor().drawing())
    );
}

#[gpui_kit::test]
fn the_grid_lines_toggle_is_remembered(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let on = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).grid_lines());
    assert!(!on(cx));
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_GRID_LINES);
    assert!(on(cx));
    support::wait_real(cx, "the setting", |_| {
        s.services.db.settings().unwrap().maker_grid_lines()
    });
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_GRID_LINES);
    assert!(!on(cx));
    support::wait_real(cx, "the setting", |_| {
        !s.services.db.settings().unwrap().maker_grid_lines()
    });
}

#[gpui_kit::test]
fn a_saved_grid_lines_choice_is_restored(cx: &mut TestAppContext) {
    let env = support::env();
    // These Services are dropped at the end of the statement; the shell opens its own on the
    // same data dir and reads the stored choice.
    support::services(&env, support::no_steam())
        .db
        .set_maker_grid_lines(true)
        .unwrap();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::wait_real(cx, "the saved choice", |cx| {
        cx.update(|cx| page.read(cx).grid_lines())
    });
}

#[gpui_kit::test]
fn grid_lines_never_reach_the_saved_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_GRID_LINES);
    assert!(cx.update(|cx| page.read(cx).grid_lines()));
    paint_cell(cx, &s, &page, 0, 0);
    paint_cell(cx, &s, &page, 63, 63);
    set_name(cx, &s, &page, "Corners");
    save(cx, &s, &page);
    let entry = &s.services.library.list_crosshairs().unwrap()[0];
    let source = s.services.library.painted_source(entry.id()).unwrap();
    let opaque = source
        .drawing()
        .pixels()
        .chunks(4)
        .filter(|p| p[3] != 0)
        .count();
    assert_eq!(opaque, 2);
    assert_eq!(
        source.drawing(),
        &cx.update(|cx| page.read(cx).editor().drawing())
    );
}

#[gpui_kit::test]
fn add_outline_borders_the_drawing_in_the_chosen_colour(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let state = cx.update(|cx| page.read(cx).outline_colour_state().clone());
    assert_eq!(
        cx.update(|cx| state.read(cx).value()).map(rgb_of),
        Some([0, 0, 0])
    );
    assert!(support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_OUTLINE_COLOR
    ));
    let pixel = |cx: &mut TestAppContext, x, y| cx.update(|cx| page.read(cx).editor().pixel(x, y));
    paint_cell(cx, &s, &page, 10, 10);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_OUTLINE);
    assert_eq!(painted(cx, &page).len(), 9);
    assert_eq!(pixel(cx, 9, 9), [0, 0, 0, 255]);
    assert_eq!(pixel(cx, 10, 10), [255, 255, 255, 255]);
    assert_eq!(undo_depth(cx, &page), 2);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_UNDO);
    assert_eq!(painted(cx, &page), [(10, 10)]);
    cx.update(|cx| {
        state.update(cx, |_, cx| {
            cx.emit(ColorPickerEvent::Change(Some(hsla_of([255, 0, 0]))))
        })
    });
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_OUTLINE);
    assert_eq!(pixel(cx, 11, 11), [255, 0, 0, 255]);
}

/// Runs `start` and, in the frame after it with no background work run yet, clicks Add
/// outline and checks the drawing is unchanged.
fn assert_outline_locked_during(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    start: impl FnOnce(&mut Window, &mut App),
) {
    let before = cx.update(|cx| page.read(cx).editor().drawing());
    let (busy, after) = cx
        .update_window(s.win, |_, w, cx| {
            start(w, cx);
            w.render_frame(cx);
            let busy = page.read(cx).is_saving() || !page.read(cx).is_loaded();
            w.click(ids::CROSSHAIR_MAKER_OUTLINE, cx);
            (busy, page.read(cx).editor().drawing())
        })
        .unwrap();
    assert!(busy, "the save or open is still running");
    assert!(after == before, "Add outline changed the drawing");
}

#[gpui_kit::test]
fn add_outline_waits_for_a_running_save(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    ready_to_save(cx, &s, &page);
    assert_outline_locked_during(cx, &s, &page, |w, cx| {
        w.click(ids::CROSSHAIR_MAKER_SAVE, cx)
    });
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    assert!(!cx.update(|cx| page.read(cx).editor().is_dirty()));
}

#[gpui_kit::test]
fn add_outline_waits_for_an_opening_drawing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, drawing) = seed_painted(&s, "Zebra");
    let page = open_maker(cx, &s);
    ready_to_save(cx, &s, &page);
    let id = zebra.id().clone();
    let opener = page.clone();
    assert_outline_locked_during(cx, &s, &page, move |_, cx| {
        opener.update(cx, |page, cx| page.open(id, cx))
    });
    support::wait_real(cx, "the drawing to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), drawing);
}

#[gpui_kit::test]
fn a_cleared_outline_colour_keeps_the_last_one(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let state = cx.update(|cx| page.read(cx).outline_colour_state().clone());
    let pick = |cx: &mut TestAppContext, colour| {
        cx.update(|cx| state.update(cx, |_, cx| cx.emit(ColorPickerEvent::Change(colour))));
        support::settle(cx, s.win);
    };
    pick(cx, Some(hsla_of([255, 0, 0])));
    pick(cx, None);
    paint_cell(cx, &s, &page, 10, 10);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_OUTLINE);
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().pixel(11, 11)),
        [255, 0, 0, 255]
    );
}

/// The cell whose box holds `p`, given the grid box and cell size.
fn cell_of((grid, cell): (Bounds<Pixels>, Pixels), p: Point<Pixels>) -> (u32, u32) {
    (
        ((p.x - grid.origin.x) / cell).floor() as u32,
        ((p.y - grid.origin.y) / cell).floor() as u32,
    )
}

/// The grid box and cell size, and the view box, all from one settled frame.
fn zoomed(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
) -> ((Bounds<Pixels>, Pixels), Bounds<Pixels>) {
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, w, cx| {
        (
            (
                w.find(ids::CROSSHAIR_MAKER_GRID).bounds(),
                page.read(cx).cell_size(),
            ),
            w.find(ids::CROSSHAIR_MAKER_VIEW).bounds(),
        )
    })
    .unwrap()
}

fn zoom_label(cx: &mut TestAppContext, s: &support::ShellWindow) -> Option<String> {
    support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_ZOOM)
}

#[gpui_kit::test]
fn the_zoom_buttons_grow_the_cells_and_a_click_lands_on_the_cell_under_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let (_, fit) = grid(cx, &s, &page);
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("100%"));
    assert!(support::disabled_of(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_ZOOM_OUT
    ));
    // Fitted, the view's side margin holds no cell: no readout, and a click there paints nothing.
    let (at, view) = zoomed(cx, &s, &page);
    assert!(at.0.left() > view.left() + px(4.), "{at:?} {view:?}");
    let margin = point(view.left() + px(2.), at.0.center().y);
    send(cx, &s, mouse_move(margin, false));
    assert!(!support::present(cx, s.win, ids::CROSSHAIR_MAKER_POSITION));
    send(cx, &s, mouse_down(margin));
    send(cx, &s, mouse_up(margin));
    assert!(painted(cx, &page).is_empty());
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_ZOOM_IN);
    let (at, view) = zoomed(cx, &s, &page);
    assert_eq!(at.1, fit * 2.);
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("200%"));
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_ZOOM_OUT
    ));
    assert!(at.0.size.width > view.size.width || at.0.size.height > view.size.height);
    let (x, y) = cell_of(at, view.center());
    assert!(
        (29..=34).contains(&x) && (29..=34).contains(&y),
        "({x}, {y})"
    );
    let p = centre(at, x, y);
    send(cx, &s, mouse_move(p, false));
    send(cx, &s, mouse_down(p));
    send(cx, &s, mouse_up(p));
    assert_eq!(painted(cx, &page), [(x, y)]);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_POSITION),
        Some(position_text(CanvasSize::Large, x, y))
    );
    assert_guides_centred(cx, &s);
    for want in ["300%", "400%", "600%", "800%", "1200%", "1600%"] {
        support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_ZOOM_IN);
        assert_eq!(zoom_label(cx, &s).as_deref(), Some(want));
    }
    assert!(support::disabled_of(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_ZOOM_IN
    ));
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_ZOOM);
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("100%"));
    assert_eq!(grid(cx, &s, &page).1, fit);
    assert_eq!(painted(cx, &page), [(x, y)]);
    assert_eq!(undo_depth(cx, &page), 1);
    // The zoom survives a size switch; the scroll stays inside the new grid.
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_ZOOM_IN);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SIZE);
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("200%"));
    let (_, view) = zoomed(cx, &s, &page);
    let v = cx.update(|cx| *page.read(cx).viewport());
    assert!(
        v.scroll().x <= (v.length() - view.size.width).max(px(0.)),
        "{v:?}"
    );
    assert!(
        v.scroll().y <= (v.length() - view.size.height).max(px(0.)),
        "{v:?}"
    );
}

#[gpui_kit::test]
fn the_hidden_part_of_a_zoomed_grid_takes_no_clicks(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    for _ in 0..7 {
        support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_ZOOM_IN);
    }
    let (at, view) = zoomed(cx, &s, &page);
    // The grid reaches far above the view, under the toolbar.
    assert!(at.0.top() < view.top() - px(200.), "{at:?} {view:?}");
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_FILL);
    assert_eq!(cx.update(|cx| page.read(cx).editor().tool()), Tool::Fill);
    let pencil = boxes(cx, &s, &[ids::CROSSHAIR_MAKER_PENCIL.into()])[0].center();
    send(cx, &s, mouse_move(pencil, false));
    assert!(!support::present(cx, s.win, ids::CROSSHAIR_MAKER_POSITION));
    send(cx, &s, mouse_down(pencil));
    send(cx, &s, mouse_up(pencil));
    assert!(painted(cx, &page).is_empty());
    assert_eq!(cx.update(|cx| page.read(cx).editor().tool()), Tool::Pencil);
}

/// One wheel event of `lines` notches (up is positive) at `at`.
fn wheel(at: Point<Pixels>, lines: f32, modifiers: Modifiers) -> ScrollWheelEvent {
    ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Lines(point(0., lines)),
        modifiers,
        touch_phase: TouchPhase::Moved,
    }
}

#[gpui_kit::test]
fn ctrl_wheel_zooms_around_the_pointer(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let fit = grid(cx, &s, &page);
    let p = centre(fit, 36, 27);
    send(cx, &s, mouse_move(p, false));
    send(cx, &s, wheel(p, 1., Modifiers::control()));
    let two = grid(cx, &s, &page);
    assert_eq!((two.1, cell_of(two, p)), (fit.1 * 2., (36, 27)));
    send(cx, &s, wheel(p, 1., Modifiers::control()));
    let three = grid(cx, &s, &page);
    assert_eq!((three.1, cell_of(three, p)), (fit.1 * 3., (36, 27)));
    send(cx, &s, wheel(p, -1., Modifiers::control()));
    send(cx, &s, wheel(p, -1., Modifiers::control()));
    assert_eq!(grid(cx, &s, &page), fit);
    assert!(painted(cx, &page).is_empty());
}

#[gpui_kit::test]
fn the_wheel_scrolls_a_zoomed_grid_and_shift_scrolls_it_sideways(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let fit = grid(cx, &s, &page);
    let p = zoomed(cx, &s, &page).1.center();
    send(cx, &s, mouse_move(p, false));
    send(cx, &s, wheel(p, -1., Modifiers::default()));
    send(cx, &s, wheel(p, -1., Modifiers::shift()));
    assert_eq!(grid(cx, &s, &page), fit);
    support::press(cx, s.win, "ctrl-=");
    let start = grid(cx, &s, &page).0.origin;
    send(cx, &s, wheel(p, -1., Modifiers::default()));
    let down = grid(cx, &s, &page).0.origin;
    assert!(down.y < start.y && down.x == start.x, "{start:?} {down:?}");
    send(cx, &s, wheel(p, -1., Modifiers::shift()));
    let right = grid(cx, &s, &page).0.origin;
    assert!(right.x < down.x && right.y == down.y, "{down:?} {right:?}");
    send(cx, &s, wheel(p, 1., Modifiers::default()));
    assert!(grid(cx, &s, &page).0.origin.y > right.y);
    assert!(painted(cx, &page).is_empty());
}

#[gpui_kit::test]
fn ctrl_plus_minus_and_zero_zoom_around_the_centre_but_never_while_typing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let fit = grid(cx, &s, &page);
    support::press(cx, s.win, "ctrl-=");
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("200%"));
    support::press(cx, s.win, "ctrl-+");
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("300%"));
    let (at, view) = zoomed(cx, &s, &page);
    let off = at.0.center() - view.center();
    assert!(off.x.abs() <= px(3.) && off.y.abs() <= px(3.), "{off:?}");
    support::press(cx, s.win, "ctrl--");
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("200%"));
    support::press(cx, s.win, "ctrl-0");
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("100%"));
    assert_eq!(grid(cx, &s, &page), fit);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_NAME);
    support::press(cx, s.win, "ctrl-=");
    support::press(cx, s.win, "ctrl-+");
    assert_eq!(zoom_label(cx, &s).as_deref(), Some("100%"));
}

fn press_with(button: MouseButton, at: Point<Pixels>) -> MouseDownEvent {
    MouseDownEvent {
        button,
        ..mouse_down(at)
    }
}

fn move_with(button: MouseButton, at: Point<Pixels>) -> MouseMoveEvent {
    MouseMoveEvent {
        pressed_button: Some(button),
        ..mouse_move(at, false)
    }
}

fn release_with(button: MouseButton, at: Point<Pixels>) -> MouseUpEvent {
    MouseUpEvent {
        button,
        ..mouse_up(at)
    }
}

fn by(p: Point<Pixels>, dx: f32, dy: f32) -> Point<Pixels> {
    point(p.x + px(dx), p.y + px(dy))
}

#[gpui_kit::test]
fn a_right_or_middle_drag_pans_a_zoomed_grid_and_never_paints(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let fit = grid(cx, &s, &page);
    let p = zoomed(cx, &s, &page).1.center();
    // Fitted, a right drag moves nothing.
    send(cx, &s, mouse_move(p, false));
    send(cx, &s, press_with(MouseButton::Right, p));
    send(cx, &s, move_with(MouseButton::Right, by(p, -40., -25.)));
    send(cx, &s, release_with(MouseButton::Right, by(p, -40., -25.)));
    assert_eq!(grid(cx, &s, &page), fit);
    support::press(cx, s.win, "ctrl-=");
    support::press(cx, s.win, "ctrl-=");
    for button in [MouseButton::Right, MouseButton::Middle] {
        let start = grid(cx, &s, &page).0.origin;
        send(cx, &s, mouse_move(p, false));
        send(cx, &s, press_with(button, p));
        assert!(cx.update(|cx| page.read(cx).is_panning()));
        send(cx, &s, move_with(button, by(p, -30., -20.)));
        send(cx, &s, move_with(button, by(p, -40., -25.)));
        send(cx, &s, release_with(button, by(p, -40., -25.)));
        let end = grid(cx, &s, &page).0.origin;
        assert_eq!(end, by(start, -40., -25.), "{button:?}");
        assert!(!cx.update(|cx| page.read(cx).is_panning()));
        send(cx, &s, mouse_move(p, false));
        assert_eq!(grid(cx, &s, &page).0.origin, end);
    }
    // A left press during a pan paints nothing.
    send(cx, &s, press_with(MouseButton::Right, p));
    send(cx, &s, mouse_down(p));
    send(cx, &s, mouse_up(p));
    send(cx, &s, release_with(MouseButton::Right, p));
    assert!(painted(cx, &page).is_empty());
    assert_eq!(undo_depth(cx, &page), 0);
    // A right press during a stroke is ignored: the grid stays and the stroke goes on.
    let at = grid(cx, &s, &page);
    let (x, y) = cell_of(at, p);
    send(cx, &s, mouse_down(centre(at, x, y)));
    send(cx, &s, press_with(MouseButton::Right, centre(at, x, y)));
    assert!(!cx.update(|cx| page.read(cx).is_panning()));
    send(cx, &s, mouse_move(centre(at, x + 1, y), true));
    send(cx, &s, mouse_up(centre(at, x + 1, y)));
    send(
        cx,
        &s,
        release_with(MouseButton::Right, centre(at, x + 1, y)),
    );
    assert_eq!(grid(cx, &s, &page), at);
    assert_eq!(painted(cx, &page), [(x, y), (x + 1, y)]);
    assert_eq!(undo_depth(cx, &page), 1);
}

#[gpui_kit::test]
fn a_pan_released_outside_the_grid_ends_cleanly(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::press(cx, s.win, "ctrl-=");
    support::press(cx, s.win, "ctrl-=");
    let view = zoomed(cx, &s, &page).1;
    let p = view.center();
    let outside = point(view.right() + px(40.), p.y);
    let panning = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).is_panning());
    send(cx, &s, mouse_move(p, false));
    let start = grid(cx, &s, &page).0.origin;
    send(cx, &s, press_with(MouseButton::Right, p));
    assert!(panning(cx));
    send(cx, &s, move_with(MouseButton::Right, outside));
    // The grid follows the pointer out of the view.
    assert_ne!(grid(cx, &s, &page).0.origin, start);
    send(cx, &s, release_with(MouseButton::Right, outside));
    assert!(!panning(cx));
    let end = grid(cx, &s, &page).0.origin;
    send(cx, &s, mouse_move(p, false));
    assert_eq!(grid(cx, &s, &page).0.origin, end);
    // A pan whose release never reached the window ends on the next move with no button held.
    send(cx, &s, press_with(MouseButton::Right, p));
    assert!(panning(cx));
    send(cx, &s, move_with(MouseButton::Right, by(p, -10., 0.)));
    let held = grid(cx, &s, &page).0.origin;
    assert_ne!(held, end);
    send(cx, &s, mouse_move(by(p, -50., 0.), false));
    assert!(!panning(cx));
    assert_eq!(grid(cx, &s, &page).0.origin, held);
    // The left button paints again afterwards.
    let at = grid(cx, &s, &page);
    let (x, y) = cell_of(at, p);
    send(cx, &s, mouse_move(centre(at, x, y), false));
    send(cx, &s, mouse_down(centre(at, x, y)));
    send(cx, &s, mouse_up(centre(at, x, y)));
    assert_eq!(painted(cx, &page), [(x, y)]);
}

#[gpui_kit::test]
fn a_move_reporting_the_other_button_keeps_a_stroke_or_a_pan_going(cx: &mut TestAppContext) {
    // Wayland reports only the most recently pressed button on a move, so a second button
    // pressed mid-drag must not end the drag the first one started.
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::press(cx, s.win, "ctrl-=");
    support::press(cx, s.win, "ctrl-=");
    let p = zoomed(cx, &s, &page).1.center();
    let stroking = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).editor().in_stroke());
    let panning = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).is_panning());
    let at = grid(cx, &s, &page);
    let (x, y) = cell_of(at, p);
    send(cx, &s, mouse_move(centre(at, x, y), false));
    send(cx, &s, mouse_down(centre(at, x, y)));
    send(cx, &s, press_with(MouseButton::Right, centre(at, x, y)));
    send(cx, &s, move_with(MouseButton::Right, centre(at, x + 1, y)));
    assert!(stroking(cx));
    send(cx, &s, mouse_move(centre(at, x + 2, y), true));
    send(cx, &s, mouse_up(centre(at, x + 2, y)));
    send(
        cx,
        &s,
        release_with(MouseButton::Right, centre(at, x + 2, y)),
    );
    assert_eq!(painted(cx, &page), [(x, y), (x + 1, y), (x + 2, y)]);
    assert_eq!(undo_depth(cx, &page), 1);
    // A left press during a pan, then a move reporting Left, still pans.
    let start = grid(cx, &s, &page).0.origin;
    send(cx, &s, mouse_move(p, false));
    send(cx, &s, press_with(MouseButton::Right, p));
    send(cx, &s, mouse_down(p));
    send(cx, &s, move_with(MouseButton::Left, by(p, -30., -20.)));
    assert!(panning(cx));
    assert_eq!(grid(cx, &s, &page).0.origin, by(start, -30., -20.));
    send(cx, &s, mouse_up(by(p, -30., -20.)));
    send(cx, &s, release_with(MouseButton::Right, by(p, -30., -20.)));
    assert!(!panning(cx));
    assert_eq!(painted(cx, &page).len(), 3);
}

#[gpui_kit::test]
fn the_shape_tools_have_buttons_letter_keys_and_a_filled_switch(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let tool = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).editor().tool());
    for (id, which) in [
        (ids::CROSSHAIR_MAKER_LINE, Tool::Line),
        (ids::CROSSHAIR_MAKER_RECTANGLE, Tool::Rectangle),
        (ids::CROSSHAIR_MAKER_CIRCLE, Tool::Circle),
    ] {
        support::click_id(cx, s.win, id);
        assert_eq!(tool(cx), which);
    }
    for (key, which) in [
        ("l", Tool::Line),
        ("r", Tool::Rectangle),
        ("c", Tool::Circle),
        ("p", Tool::Pencil),
    ] {
        support::press(cx, s.win, key);
        assert_eq!(tool(cx), which);
    }
    assert!(support::disabled_of(cx, s.win, ids::CROSSHAIR_MAKER_FILLED));
    support::press(cx, s.win, "r");
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_FILLED
    ));
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_FILLED);
    assert!(cx.update(|cx| page.read(cx).editor().filled()));
    support::press(cx, s.win, "l");
    assert!(support::disabled_of(cx, s.win, ids::CROSSHAIR_MAKER_FILLED));
    // Letters typed into the name field pick no tool.
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_NAME);
    support::press(cx, s.win, "c");
    assert_eq!(tool(cx), Tool::Line);
}

#[gpui_kit::test]
fn a_line_previews_while_dragging_and_lands_as_one_step(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::press(cx, s.win, "l");
    let at = grid(cx, &s, &page);
    send(cx, &s, mouse_move(centre(at, 5, 5), false));
    send(cx, &s, mouse_down(centre(at, 5, 5)));
    send(cx, &s, mouse_move(centre(at, 12, 5), true));
    assert_eq!(
        painted(cx, &page),
        (5..=12).map(|x| (x, 5)).collect::<Vec<_>>()
    );
    assert_eq!(undo_depth(cx, &page), 0);
    send(cx, &s, mouse_move(centre(at, 5, 9), true));
    assert_eq!(
        painted(cx, &page),
        (5..=9).map(|y| (5, y)).collect::<Vec<_>>()
    );
    send(cx, &s, mouse_up(centre(at, 5, 9)));
    assert_eq!(undo_depth(cx, &page), 1);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_UNDO);
    assert!(painted(cx, &page).is_empty());
}

#[gpui_kit::test]
fn a_shape_released_off_the_grid_ends_at_the_edge(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::press(cx, s.win, "r");
    let at = grid(cx, &s, &page);
    let off = point(at.0.right() + px(12.), at.0.bottom() + px(12.));
    send(cx, &s, mouse_move(centre(at, 58, 60), false));
    send(cx, &s, mouse_down(centre(at, 58, 60)));
    send(cx, &s, mouse_move(off, true));
    send(cx, &s, mouse_up(off));
    let mut want = Vec::new();
    for y in 60..=63 {
        for x in 58..=63 {
            if x == 58 || x == 63 || y == 60 || y == 63 {
                want.push((x, y));
            }
        }
    }
    assert_eq!(painted(cx, &page), want);
    assert_eq!(undo_depth(cx, &page), 1);
    assert!(!cx.update(|cx| page.read(cx).editor().in_stroke()));
}

#[gpui_kit::test]
fn a_filled_circle_follows_mirror_x(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_MIRROR_X);
    support::press(cx, s.win, "c");
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_FILLED);
    let at = grid(cx, &s, &page);
    send(cx, &s, mouse_move(centre(at, 2, 2), false));
    send(cx, &s, mouse_down(centre(at, 2, 2)));
    send(cx, &s, mouse_move(centre(at, 4, 4), true));
    send(cx, &s, mouse_up(centre(at, 4, 4)));
    assert_eq!(
        painted(cx, &page),
        [
            (3, 2),
            (60, 2),
            (2, 3),
            (3, 3),
            (4, 3),
            (59, 3),
            (60, 3),
            (61, 3),
            (3, 4),
            (60, 4),
        ]
    );
    assert_eq!(undo_depth(cx, &page), 1);
}

/// Moves the opacity slider to `percent`, as a drag would leave it.
fn set_opacity(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    percent: f32,
) {
    let slider = cx.update(|cx| page.read(cx).opacity_state().clone());
    cx.update_window(s.win, |_, window, cx| {
        slider.update(cx, |state, cx| state.set_value(percent, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn alpha(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>) -> u8 {
    cx.update(|cx| page.read(cx).editor().alpha())
}

fn pixel(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>, x: u32, y: u32) -> [u8; 4] {
    cx.update(|cx| page.read(cx).editor().pixel(x, y))
}

#[gpui_kit::test]
fn the_opacity_slider_paints_see_through_pixels(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    assert!(support::present(cx, s.win, ids::CROSSHAIR_MAKER_OPACITY));
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_OPACITY_LABEL).as_deref(),
        Some("Opacity: 100%")
    );
    set_opacity(cx, &s, &page, 50.);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_OPACITY_LABEL).as_deref(),
        Some("Opacity: 50%")
    );
    assert_eq!(alpha(cx, &page), 128);
    paint_cell(cx, &s, &page, 1, 1);
    assert_eq!(pixel(cx, &page, 1, 1), [255, 255, 255, 128]);
    // Painting over a pixel replaces it, in either direction.
    set_opacity(cx, &s, &page, 100.);
    paint_cell(cx, &s, &page, 1, 1);
    assert_eq!(pixel(cx, &page, 1, 1), [255, 255, 255, 255]);
    set_opacity(cx, &s, &page, 25.);
    paint_cell(cx, &s, &page, 1, 1);
    assert_eq!(pixel(cx, &page, 1, 1), [255, 255, 255, 64]);
}

#[gpui_kit::test]
fn alt_click_takes_a_pixels_opacity_into_the_slider(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    set_opacity(cx, &s, &page, 40.);
    paint_cell(cx, &s, &page, 2, 2);
    assert_eq!(pixel(cx, &page, 2, 2), [255, 255, 255, 102]);
    set_opacity(cx, &s, &page, 100.);
    let at = grid(cx, &s, &page);
    alt_click(cx, &s, centre(at, 2, 2));
    assert_eq!(alpha(cx, &page), 102);
    assert_eq!(
        cx.update(|cx| page.read(cx).opacity_state().read(cx).value().start()),
        40.
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_OPACITY_LABEL).as_deref(),
        Some("Opacity: 40%")
    );
    // The picked alpha is kept exactly, not rounded through the percent.
    paint_cell(cx, &s, &page, 3, 3);
    assert_eq!(pixel(cx, &page, 3, 3), [255, 255, 255, 102]);
}

#[gpui_kit::test]
fn see_through_pixels_survive_save_and_reopen(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    set_opacity(cx, &s, &page, 25.);
    paint_cell(cx, &s, &page, 1, 1);
    set_opacity(cx, &s, &page, 75.);
    paint_cell(cx, &s, &page, 5, 5);
    set_name(cx, &s, &page, "Soft");
    save(cx, &s, &page);
    let entry = s.services.library.list_crosshairs().unwrap().remove(0);
    let drawing = cx.update(|cx| page.read(cx).editor().drawing());
    assert_eq!(
        s.services
            .library
            .painted_source(entry.id())
            .unwrap()
            .drawing(),
        &drawing
    );
    // New and opening both keep the opacity the slider shows.
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_NEW);
    assert!(painted(cx, &page).is_empty());
    assert_eq!(alpha(cx, &page), 191);
    let id = entry.id().clone();
    cx.update(|cx| page.update(cx, |page, cx| page.open(id, cx)));
    support::wait_real(cx, "the drawing to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert_eq!(pixel(cx, &page, 1, 1), [255, 255, 255, 64]);
    assert_eq!(pixel(cx, &page, 5, 5), [255, 255, 255, 191]);
    assert_eq!(alpha(cx, &page), 191);
}

#[gpui_kit::test]
fn the_preview_draws_the_drawing_at_its_in_game_size(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    // A 32x32 drawing is doubled to the same 64 px the game draws.
    for _ in 0..2 {
        let shown = boxes(cx, &s, &[ids::CROSSHAIR_MAKER_PREVIEW_CROSSHAIR.into()])[0];
        assert_eq!(shown.size, gpui_kit::size(px(64.), px(64.)));
        support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SIZE);
    }
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().size()),
        CanvasSize::Large
    );
}

#[gpui_kit::test]
fn shrinking_a_drawing_with_paint_near_the_edge_asks_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let size = |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).editor().size());
    paint_cell(cx, &s, &page, 1, 1);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SIZE);
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("Shrinking to 32x32 cuts off the edges of your drawing.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_OK).as_deref(),
        Some("Shrink")
    );
    // Cancel keeps the drawing as it was.
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(size(cx), CanvasSize::Large);
    assert_eq!(painted(cx, &page), [(1, 1)]);
    assert_eq!(undo_depth(cx, &page), 1);

    // Shrink crops, and one undo brings the edge back.
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SIZE);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    assert_eq!(size(cx), CanvasSize::Small);
    assert!(painted(cx, &page).is_empty());
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_UNDO);
    assert_eq!(size(cx), CanvasSize::Large);
    assert_eq!(painted(cx, &page), [(1, 1)]);
}

#[gpui_kit::test]
fn shrinking_a_drawing_that_fits_does_not_ask(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 20, 20);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_SIZE);
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().size()),
        CanvasSize::Small
    );
    assert_eq!(painted(cx, &page), [(4, 4)]);
}
