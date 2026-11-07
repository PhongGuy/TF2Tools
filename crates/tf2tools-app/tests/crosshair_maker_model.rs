use tf2tools_app::pages::crosshair_maker::model::{
    CLEAR, CanvasSize, Editor, FIT_CENTRED, FIT_SHRUNK, REDO_TIP, Tool, UNDO_LIMIT, UNDO_TIP,
    fit_note, position_text,
};
use tf2tools_core::{CrosshairId, FitChange, RgbaImage};

const RED: [u8; 3] = [255, 0, 0];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];

fn opaque([r, g, b]: [u8; 3]) -> [u8; 4] {
    [r, g, b, 255]
}

/// One press and release: a one-cell stroke, or a fill with the Fill tool.
fn dab(e: &mut Editor, x: u32, y: u32) {
    e.press(x, y);
    e.release();
}

/// One stroke from `from` through each point of `path`.
fn stroke(e: &mut Editor, from: (u32, u32), path: &[(u32, u32)]) {
    e.press(from.0, from.1);
    for &(x, y) in path {
        e.drag_to(x, y);
    }
    e.release();
}

/// Every cell equal to `colour` (or, with None, not clear), row by row from the top.
fn cells_where(e: &Editor, colour: Option<[u8; 4]>) -> Vec<(u32, u32)> {
    let n = e.size().side();
    let mut out = Vec::new();
    for y in 0..n {
        for x in 0..n {
            let p = e.pixel(x, y);
            if colour.map_or(p != CLEAR, |c| p == c) {
                out.push((x, y));
            }
        }
    }
    out
}

fn cells(e: &Editor) -> Vec<(u32, u32)> {
    cells_where(e, None)
}

#[test]
fn a_new_editor_is_a_clean_empty_64_canvas() {
    let e = Editor::new();
    assert_eq!(e.size(), CanvasSize::Large);
    assert_eq!(
        (CanvasSize::Large.side(), CanvasSize::Small.side()),
        (64, 32)
    );
    assert_eq!(CanvasSize::Large.other(), CanvasSize::Small);
    assert!(cells(&e).is_empty());
    assert_eq!(e.tool(), Tool::Pencil);
    assert_eq!(e.colour(), [255, 255, 255]);
    assert!(!e.mirror_x() && !e.mirror_y() && !e.in_stroke());
    assert!(!e.is_dirty() && !e.can_undo() && !e.can_redo());
    assert_eq!((e.open_id(), e.name()), (None, ""));
    let drawing = e.drawing();
    assert_eq!((drawing.width(), drawing.height()), (64, 64));
    assert!(drawing.pixels().iter().all(|&b| b == 0));
}

#[test]
fn the_pencil_paints_opaque_and_the_eraser_clears() {
    let mut e = Editor::new();
    e.set_colour(RED);
    dab(&mut e, 10, 12);
    assert_eq!(e.pixel(10, 12), opaque(RED));
    assert_eq!(cells(&e), [(10, 12)]);
    assert!(e.is_dirty());
    e.set_tool(Tool::Eraser);
    dab(&mut e, 10, 12);
    assert_eq!(e.pixel(10, 12), CLEAR);
    assert_eq!(e.undo_depth(), 2);
}

#[test]
fn a_drag_paints_every_cell_between_move_events() {
    let mut e = Editor::new();
    stroke(&mut e, (2, 3), &[(40, 3)]);
    let row: Vec<_> = (2..=40).map(|x| (x, 3)).collect();
    assert_eq!(cells(&e), row);

    let mut e = Editor::new();
    stroke(&mut e, (0, 10), &[(20, 30)]);
    let diagonal: Vec<_> = (0..=20).map(|i| (i, 10 + i)).collect();
    assert_eq!(cells(&e), diagonal);

    // A shallow slope: one cell per column, each a king's step from the last.
    let mut e = Editor::new();
    stroke(&mut e, (5, 40), &[(25, 48)]);
    let mut by_x = cells(&e);
    by_x.sort();
    for x in 5..=25 {
        assert_eq!(by_x.iter().filter(|c| c.0 == x).count(), 1, "column {x}");
    }
    assert_eq!((by_x[0], by_x[by_x.len() - 1]), ((5, 40), (25, 48)));
    for pair in by_x.windows(2) {
        assert!(pair[0].1.abs_diff(pair[1].1) <= 1, "{pair:?}");
    }
}

#[test]
fn one_stroke_is_one_undo_step_and_redo_brings_it_back() {
    let mut e = Editor::new();
    stroke(&mut e, (1, 1), &[(10, 1), (30, 1)]);
    assert_eq!(e.undo_depth(), 1);
    assert_eq!(cells(&e).len(), 30);
    assert!(e.undo());
    assert!(cells(&e).is_empty());
    assert!(!e.undo());
    assert!(e.redo());
    assert_eq!(cells(&e).len(), 30);
    assert!(!e.redo());
}

#[test]
fn mirrors_paint_the_reflected_cells() {
    let mut e = Editor::new();
    e.set_mirror_x(true);
    dab(&mut e, 3, 5);
    assert_eq!(cells(&e), [(3, 5), (60, 5)]);

    let mut e = Editor::new();
    e.set_mirror_y(true);
    dab(&mut e, 3, 5);
    assert_eq!(cells(&e), [(3, 5), (3, 58)]);

    let mut e = Editor::new();
    e.set_mirror_x(true);
    e.set_mirror_y(true);
    dab(&mut e, 3, 5);
    assert_eq!(cells(&e), [(3, 5), (60, 5), (3, 58), (60, 58)]);
    assert_eq!(e.undo_depth(), 1);

    let mut e = Editor::new();
    e.set_size(CanvasSize::Small);
    e.set_mirror_x(true);
    e.set_mirror_y(true);
    stroke(&mut e, (0, 0), &[(1, 0)]);
    assert_eq!(
        cells(&e),
        [
            (0, 0),
            (1, 0),
            (30, 0),
            (31, 0),
            (0, 31),
            (1, 31),
            (30, 31),
            (31, 31)
        ]
    );
}

#[test]
fn fill_takes_the_four_connected_region_of_one_colour() {
    // A diagonal wall seals the corner for a 4-connected fill; an 8-connected one would leak.
    let mut e = Editor::new();
    stroke(&mut e, (0, 5), &[(5, 0)]);
    e.set_tool(Tool::Fill);
    e.set_colour(RED);
    dab(&mut e, 0, 0);
    let corner: Vec<_> = (0..5u32)
        .flat_map(|y| (0..5 - y).map(move |x| (x, y)))
        .collect();
    assert_eq!(cells_where(&e, Some(opaque(RED))), corner);
    assert_eq!(cells_where(&e, Some(WHITE)).len(), 6);
    assert_eq!(e.undo_depth(), 2);
    // Filling a region with its own colour changes nothing and records nothing.
    dab(&mut e, 0, 0);
    assert_eq!(e.undo_depth(), 2);
}

#[test]
fn fill_starts_from_the_mirrored_points_too() {
    let mut e = Editor::new();
    e.set_mirror_x(true);
    stroke(&mut e, (20, 0), &[(20, 63)]);
    e.set_tool(Tool::Fill);
    e.set_colour(RED);
    dab(&mut e, 0, 0);
    let red = cells_where(&e, Some(opaque(RED)));
    assert_eq!(red.len(), 2 * 20 * 64);
    assert!(red.iter().all(|&(x, _)| !(20..=43).contains(&x)));
    assert_eq!(e.pixel(30, 30), CLEAR);
    assert_eq!(e.undo_depth(), 2);
}

#[test]
fn fill_with_both_mirrors_fills_all_four_corners() {
    // Walls at row and column 20, mirrored to 43, cut the canvas into nine regions.
    let mut e = Editor::new();
    e.set_mirror_x(true);
    e.set_mirror_y(true);
    stroke(&mut e, (20, 0), &[(20, 63)]);
    stroke(&mut e, (0, 20), &[(63, 20)]);
    e.set_tool(Tool::Fill);
    e.set_colour(RED);
    dab(&mut e, 0, 0);
    let corner = |lo: u32| lo..lo + 20;
    let red = cells_where(&e, Some(opaque(RED)));
    let expected: Vec<_> = [0, 44]
        .into_iter()
        .flat_map(corner)
        .flat_map(|y| {
            [0, 44]
                .into_iter()
                .flat_map(move |x0| corner(x0).map(move |x| (x, y)))
        })
        .collect();
    assert_eq!(red, expected);
    assert_eq!((e.pixel(30, 30), e.pixel(30, 0)), (CLEAR, CLEAR));
    assert_eq!(e.undo_depth(), 3);
}

#[test]
fn the_undo_stack_keeps_the_last_hundred_steps() {
    assert_eq!(UNDO_LIMIT, 100);
    let mut e = Editor::new();
    for i in 0..105u32 {
        dab(&mut e, i % 64, i / 64);
    }
    assert_eq!(e.undo_depth(), UNDO_LIMIT);
    for _ in 0..UNDO_LIMIT {
        assert!(e.undo());
    }
    assert!(!e.undo());
    assert_eq!(cells(&e), [(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)]);
    assert_eq!(e.redo_depth(), UNDO_LIMIT);
}

#[test]
fn any_edit_clears_redo() {
    let mut e = Editor::new();
    dab(&mut e, 1, 1);
    dab(&mut e, 2, 2);
    assert!(e.undo());
    assert!(e.can_redo());
    dab(&mut e, 3, 3);
    assert!(!e.can_redo());
    assert_eq!(cells(&e), [(1, 1), (3, 3)]);
}

#[test]
fn an_edit_that_changes_nothing_records_nothing() {
    let mut e = Editor::new();
    e.set_tool(Tool::Eraser);
    dab(&mut e, 4, 4);
    e.clear();
    e.set_size(CanvasSize::Large);
    assert_eq!(e.undo_depth(), 0);
    assert!(!e.is_dirty());
}

#[test]
fn clear_is_one_undoable_step() {
    let mut e = Editor::new();
    for x in 0..3 {
        dab(&mut e, x, 0);
    }
    e.clear();
    assert!(cells(&e).is_empty());
    assert_eq!(e.undo_depth(), 4);
    assert!(e.undo());
    assert_eq!(cells(&e).len(), 3);
}

#[test]
fn a_size_change_keeps_the_centre_and_undoes() {
    let mut e = Editor::new();
    for (x, y) in [(0, 0), (16, 16), (47, 47)] {
        dab(&mut e, x, y);
    }
    e.set_size(CanvasSize::Small);
    assert_eq!(e.size(), CanvasSize::Small);
    assert_eq!(cells(&e), [(0, 0), (31, 31)]);
    assert!(e.undo());
    assert_eq!(e.size(), CanvasSize::Large);
    assert_eq!(cells(&e), [(0, 0), (16, 16), (47, 47)]);
    assert!(e.redo());
    assert_eq!(e.size(), CanvasSize::Small);

    let mut e = Editor::new();
    e.set_size(CanvasSize::Small);
    dab(&mut e, 0, 0);
    dab(&mut e, 31, 31);
    e.set_size(CanvasSize::Large);
    assert_eq!(cells(&e), [(16, 16), (47, 47)]);
    assert_eq!(e.undo_depth(), 4);
}

#[test]
fn an_opened_crosshair_starts_clean_and_saving_renames_it() {
    let mut pixels = vec![0u8; 32 * 32 * 4];
    let at = (2 * 32 + 1) * 4;
    pixels[at..at + 4].copy_from_slice(&opaque(RED));
    let drawing = RgbaImage::from_pixels(32, 32, pixels).unwrap();
    let mut e = Editor::open(CrosshairId::new("abc"), "Dot".to_owned(), &drawing).unwrap();
    assert_eq!(e.size(), CanvasSize::Small);
    assert_eq!(e.pixel(1, 2), opaque(RED));
    assert_eq!(e.drawing(), drawing);
    assert!(!e.is_dirty() && !e.can_undo());
    assert_eq!(
        (e.open_id().map(CrosshairId::as_str), e.name()),
        (Some("abc"), "Dot")
    );
    let odd = RgbaImage::from_pixels(16, 16, vec![0; 16 * 16 * 4]).unwrap();
    assert!(Editor::open(CrosshairId::new("abc"), "Dot".to_owned(), &odd).is_none());

    dab(&mut e, 5, 5);
    assert!(e.is_dirty());
    let sent = e.drawing();
    e.mark_saved(CrosshairId::new("abc"), "Ring".to_owned(), &sent);
    assert!(!e.is_dirty());
    assert_eq!(e.name(), "Ring");
    assert!(e.can_undo(), "saving keeps the history");
    e.forget_open();
    assert_eq!((e.open_id(), e.name()), (None, "Ring"));
}

#[test]
fn an_edit_made_while_a_save_ran_stays_unsaved() {
    let mut e = Editor::new();
    dab(&mut e, 1, 1);
    let sent = e.drawing();
    dab(&mut e, 2, 2);
    e.mark_saved(CrosshairId::new("abc"), "Dot".to_owned(), &sent);
    assert!(e.is_dirty(), "the second dab was not in the save");
    assert_eq!(
        (e.open_id().map(CrosshairId::as_str), e.name()),
        (Some("abc"), "Dot"),
        "the next save updates the same crosshair"
    );
    e.undo();
    assert!(!e.is_dirty(), "undoing the second dab returns to the save");
}

#[test]
fn undoing_back_to_the_opened_drawing_is_clean() {
    let drawing = RgbaImage::from_pixels(32, 32, vec![0; 32 * 32 * 4]).unwrap();
    let mut e = Editor::open(CrosshairId::new("abc"), "Dot".to_owned(), &drawing).unwrap();
    dab(&mut e, 5, 5);
    assert!(e.is_dirty());
    e.undo();
    assert!(!e.is_dirty(), "the drawing is the one opened");
    e.redo();
    assert!(
        e.is_dirty(),
        "redo past the opened drawing is unsaved again"
    );

    let mut fresh = Editor::new();
    dab(&mut fresh, 1, 1);
    fresh.undo();
    assert!(!fresh.is_dirty(), "a new canvas undone to blank is clean");
}

#[test]
fn undoing_back_to_the_saved_drawing_is_clean() {
    let mut e = Editor::new();
    dab(&mut e, 1, 1);
    let sent = e.drawing();
    e.mark_saved(CrosshairId::new("abc"), "Dot".to_owned(), &sent);
    dab(&mut e, 2, 2);
    assert!(e.is_dirty());
    e.undo();
    assert!(!e.is_dirty(), "the drawing is the one saved");
    e.undo();
    assert!(e.is_dirty(), "undoing past the save is unsaved");
    e.redo();
    assert!(!e.is_dirty());
}

#[test]
fn cells_take_the_largest_whole_pixel_size_that_fits() {
    use gpui_kit::{px, size};
    use tf2tools_app::pages::crosshair_maker::model::cell_px;
    assert_eq!(cell_px(size(px(600.), px(500.)), 64), px(7.));
    assert_eq!(cell_px(size(px(600.), px(500.)), 32), px(15.));
    assert_eq!(cell_px(size(px(512.), px(900.)), 64), px(8.));
    assert_eq!(cell_px(size(px(20.), px(20.)), 64), px(1.));
}

#[test]
fn a_position_maps_to_the_cell_under_it() {
    use gpui_kit::{point, px};
    use tf2tools_app::pages::crosshair_maker::model::cell_at;
    let origin = point(px(100.), px(50.));
    let at = |x: f32, y: f32| cell_at(origin, px(8.), 64, point(px(x), px(y)));
    assert_eq!(at(100., 50.), Some((0, 0)));
    assert_eq!(at(107.9, 57.9), Some((0, 0)));
    assert_eq!(at(108., 50.), Some((1, 0)));
    assert_eq!(at(611.9, 561.9), Some((63, 63)));
    assert_eq!(at(612., 60.), None);
    assert_eq!(at(99.9, 60.), None);
    assert_eq!(at(150., 49.9), None);
}

#[test]
fn a_position_off_the_grid_clamps_to_the_nearest_edge_cell() {
    use gpui_kit::{point, px};
    use tf2tools_app::pages::crosshair_maker::model::nearest_cell;
    let origin = point(px(100.), px(50.));
    let at = |x: f32, y: f32| nearest_cell(origin, px(8.), 64, point(px(x), px(y)));
    assert_eq!(at(108., 58.), (1, 1));
    assert_eq!(at(20., 60.), (0, 1));
    assert_eq!(at(900., 10.), (63, 0));
    assert_eq!(at(150., 900.), (6, 63));
}

#[test]
fn grey_follows_the_crosshair_colour_and_anything_else_is_mixed() {
    use tf2tools_app::pages::crosshair_maker::model::{FOLLOWS, MIXED, SWATCHES, colour_hint};
    assert_eq!(FOLLOWS, "Follows your crosshair colour");
    assert_eq!(
        MIXED,
        "In game this colour is mixed with your crosshair colour"
    );
    assert_eq!(
        SWATCHES,
        [
            [255, 255, 255],
            [192, 192, 192],
            [128, 128, 128],
            [64, 64, 64]
        ]
    );
    for swatch in SWATCHES {
        assert_eq!(colour_hint(swatch), FOLLOWS);
    }
    assert_eq!(colour_hint([0, 0, 0]), FOLLOWS);
    assert_eq!(colour_hint([255, 0, 0]), MIXED);
    assert_eq!(colour_hint([128, 128, 129]), MIXED);
}

#[test]
fn colours_round_trip_through_the_picker() {
    use tf2tools_app::pages::crosshair_maker::model::{hsla_of, rgb_of};
    for rgb in [
        [255, 255, 255],
        [192, 192, 192],
        [128, 128, 128],
        [64, 64, 64],
        [0, 0, 0],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [200, 40, 40],
        [12, 200, 99],
    ] {
        assert_eq!(rgb_of(hsla_of(rgb)), rgb, "{rgb:?}");
    }
}

#[test]
fn the_tint_multiplies_each_channel_and_keeps_alpha() {
    use tf2tools_app::pages::crosshair_maker::model::tint;
    assert_eq!(tint([255, 255, 255, 255], [255, 0, 0]), [255, 0, 0, 255]);
    assert_eq!(
        tint([128, 128, 128, 255], [255, 255, 255]),
        [128, 128, 128, 255]
    );
    assert_eq!(
        tint([200, 100, 50, 255], [128, 128, 128]),
        [100, 50, 25, 255]
    );
    assert_eq!(tint(CLEAR, [255, 0, 0]), CLEAR);
}

#[test]
fn the_preview_is_always_64_with_a_small_drawing_doubled() {
    use tf2tools_app::pages::crosshair_maker::model::preview_texels;
    let mut e = Editor::new();
    e.set_size(CanvasSize::Small);
    dab(&mut e, 1, 2);
    let preview = preview_texels(&e.drawing(), [255, 0, 0]);
    assert_eq!((preview.width(), preview.height()), (64, 64));
    let red: Vec<(u32, u32)> = (0..64u32)
        .flat_map(|y| (0..64u32).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let at = ((y * 64 + x) * 4) as usize;
            preview.pixels()[at..at + 4] == [255, 0, 0, 255]
        })
        .collect();
    assert_eq!(red, [(2, 4), (3, 4), (2, 5), (3, 5)]);
    assert_eq!(preview.pixels().chunks(4).filter(|p| p[3] != 0).count(), 4);

    let mut e = Editor::new();
    dab(&mut e, 63, 0);
    let preview = preview_texels(&e.drawing(), [255, 255, 255]);
    assert_eq!(&preview.pixels()[63 * 4..64 * 4], &[255, 255, 255, 255]);
}

#[test]
fn each_tool_has_a_one_letter_key_shown_in_its_tooltip() {
    let tips: Vec<_> = Tool::ALL.iter().map(|t| (t.key(), t.tooltip())).collect();
    assert_eq!(
        tips,
        [
            ("p", "Pencil (P)".to_owned()),
            ("e", "Eraser (E)".to_owned()),
            ("f", "Fill (F)".to_owned()),
            ("l", "Line (L)".to_owned()),
            ("r", "Rectangle (R)".to_owned()),
            ("c", "Circle (C)".to_owned()),
        ]
    );
    assert_eq!(UNDO_TIP, "Undo (Ctrl+Z)");
    assert_eq!(REDO_TIP, "Redo (Ctrl+Y)");
}

#[test]
fn picking_takes_a_painted_cells_colour_and_ignores_clear_cells() {
    let mut e = Editor::new();
    e.set_colour(RED);
    dab(&mut e, 3, 3);
    e.set_colour([255, 255, 255]);
    assert!(!e.pick(4, 4));
    assert!(!e.pick(64, 0));
    assert_eq!(e.colour(), [255, 255, 255]);
    assert!(e.pick(3, 3));
    assert_eq!(e.colour(), RED);
    assert_eq!(cells_where(&e, None), [(3, 3)]);
    assert_eq!(e.undo_depth(), 1);
}

#[test]
fn a_cells_position_counts_from_the_true_centre() {
    let cases = [
        (CanvasSize::Large, 32, 31, "1 right, 1 up"),
        (CanvasSize::Large, 31, 32, "1 left, 1 down"),
        (CanvasSize::Large, 34, 29, "3 right, 3 up"),
        (CanvasSize::Large, 0, 0, "32 left, 32 up"),
        (CanvasSize::Large, 63, 63, "32 right, 32 down"),
        (CanvasSize::Small, 16, 15, "1 right, 1 up"),
        (CanvasSize::Small, 15, 16, "1 left, 1 down"),
        (CanvasSize::Small, 0, 31, "16 left, 16 down"),
    ];
    for (size, x, y, want) in cases {
        assert_eq!(position_text(size, x, y), want, "{size:?} ({x}, {y})");
    }
}

#[test]
fn an_outline_rings_a_single_pixel_with_its_eight_neighbours() {
    let mut e = Editor::new();
    dab(&mut e, 10, 10);
    let before = e.drawing();
    e.outline(BLACK);
    let ring: Vec<_> = (9..=11)
        .flat_map(|y| (9..=11).map(move |x| (x, y)))
        .filter(|&c| c != (10, 10))
        .collect();
    assert_eq!(cells_where(&e, Some(opaque(BLACK))), ring);
    assert_eq!(e.pixel(10, 10), WHITE);
    assert_eq!(e.undo_depth(), 2);
    assert!(e.undo());
    assert_eq!(e.drawing(), before);
}

#[test]
fn an_outline_stops_at_the_grid_edge() {
    let mut e = Editor::new();
    dab(&mut e, 0, 0);
    dab(&mut e, 63, 63);
    e.outline(BLACK);
    assert_eq!(
        cells_where(&e, Some(opaque(BLACK))),
        [(1, 0), (0, 1), (1, 1), (62, 62), (63, 62), (62, 63)]
    );
    let mut small = Editor::new();
    small.set_size(CanvasSize::Small);
    dab(&mut small, 31, 0);
    small.outline(BLACK);
    assert_eq!(
        cells_where(&small, Some(opaque(BLACK))),
        [(30, 0), (30, 1), (31, 1)]
    );
}

#[test]
fn an_outline_never_paints_over_a_pixel_or_its_own_ring() {
    let mut e = Editor::new();
    stroke(&mut e, (5, 5), &[(7, 5)]);
    e.set_colour(RED);
    dab(&mut e, 6, 6);
    e.outline(BLACK);
    assert_eq!(cells_where(&e, Some(WHITE)), [(5, 5), (6, 5), (7, 5)]);
    assert_eq!(cells_where(&e, Some(opaque(RED))), [(6, 6)]);
    assert_eq!(
        cells_where(&e, Some(opaque(BLACK))),
        [
            (4, 4),
            (5, 4),
            (6, 4),
            (7, 4),
            (8, 4),
            (4, 5),
            (8, 5),
            (4, 6),
            (5, 6),
            (7, 6),
            (8, 6),
            (5, 7),
            (6, 7),
            (7, 7),
        ]
    );
}

#[test]
fn an_outline_of_an_empty_canvas_records_nothing() {
    let mut e = Editor::new();
    e.outline(BLACK);
    assert!(cells_where(&e, None).is_empty());
    assert_eq!(e.undo_depth(), 0);
    assert!(!e.is_dirty());
}

/// The `w` by `h` block from (x0, y0) as rows of '#' (painted) and '.' (clear).
fn art(e: &Editor, x0: u32, y0: u32, w: u32, h: u32) -> String {
    (y0..y0 + h)
        .map(|y| {
            (x0..x0 + w)
                .map(|x| if e.pixel(x, y) == CLEAR { '.' } else { '#' })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn shape(tool: Tool, filled: bool, from: (u32, u32), path: &[(u32, u32)]) -> Editor {
    let mut e = Editor::new();
    e.set_tool(tool);
    e.set_filled(filled);
    stroke(&mut e, from, path);
    e
}

#[test]
fn shape_tools_are_line_rectangle_and_circle() {
    let shapes: Vec<_> = Tool::ALL.into_iter().filter(|t| t.is_shape()).collect();
    assert_eq!(shapes, [Tool::Line, Tool::Rectangle, Tool::Circle]);
    let switch: Vec<_> = Tool::ALL
        .into_iter()
        .filter(|t| t.has_fill_switch())
        .collect();
    assert_eq!(switch, [Tool::Rectangle, Tool::Circle]);
    assert!(!Editor::new().filled());
}

/// A line's end and the six cells it covers from (10, 10).
type Octant = ((u32, u32), [(u32, u32); 6]);

#[test]
fn lines_run_end_to_end_in_every_octant() {
    let cases: [Octant; 8] = [
        (
            (15, 12),
            [(10, 10), (11, 10), (12, 11), (13, 11), (14, 12), (15, 12)],
        ),
        (
            (12, 15),
            [(10, 10), (10, 11), (11, 12), (11, 13), (12, 14), (12, 15)],
        ),
        (
            (8, 15),
            [(10, 10), (10, 11), (9, 12), (9, 13), (8, 14), (8, 15)],
        ),
        (
            (5, 12),
            [(10, 10), (9, 10), (8, 11), (7, 11), (6, 12), (5, 12)],
        ),
        ((5, 8), [(10, 10), (9, 10), (8, 9), (7, 9), (6, 8), (5, 8)]),
        ((8, 5), [(10, 10), (10, 9), (9, 8), (9, 7), (8, 6), (8, 5)]),
        (
            (12, 5),
            [(10, 10), (10, 9), (11, 8), (11, 7), (12, 6), (12, 5)],
        ),
        (
            (15, 8),
            [(10, 10), (11, 10), (12, 9), (13, 9), (14, 8), (15, 8)],
        ),
    ];
    for (to, want) in cases {
        // The drag passes (20, 20) first; only where it ends counts.
        let e = shape(Tool::Line, false, (10, 10), &[(20, 20), to]);
        let mut want = want.to_vec();
        want.sort_by_key(|&(x, y)| (y, x));
        assert_eq!(cells(&e), want, "to {to:?}");
        assert_eq!(e.undo_depth(), 1);
    }
}

#[test]
fn every_line_is_gap_free_with_one_cell_per_step() {
    for tx in 4..=16 {
        for ty in 4..=16 {
            let e = shape(Tool::Line, false, (10, 10), &[(tx, ty)]);
            let got = cells(&e);
            let steps = tx.abs_diff(10).max(ty.abs_diff(10));
            assert_eq!(got.len() as u32, steps + 1, "to ({tx}, {ty})");
            assert!(
                got.contains(&(10, 10)) && got.contains(&(tx, ty)),
                "to ({tx}, {ty})"
            );
            // Gap free: an 8-way walk from the start reaches every painted cell.
            let mut seen = vec![(10u32, 10u32)];
            let mut i = 0;
            while i < seen.len() {
                let (x, y) = seen[i];
                for &c in &got {
                    if c.0.abs_diff(x) <= 1 && c.1.abs_diff(y) <= 1 && !seen.contains(&c) {
                        seen.push(c);
                    }
                }
                i += 1;
            }
            assert_eq!(seen.len(), got.len(), "to ({tx}, {ty})");
        }
    }
}

#[test]
fn a_line_covers_the_same_cells_dragged_either_way() {
    let a = (10, 10);
    for x in 0..=20 {
        for y in 0..=20 {
            let there = cells(&shape(Tool::Line, false, a, &[(x, y)]));
            let back = cells(&shape(Tool::Line, false, (x, y), &[a]));
            assert_eq!(there, back, "between {a:?} and ({x}, {y})");
        }
    }
}

#[test]
fn rectangles_outline_or_fill_the_box_dragged_in_any_direction() {
    let e = shape(Tool::Rectangle, false, (6, 4), &[(2, 1)]);
    assert_eq!(
        art(&e, 1, 0, 7, 6),
        ".......\n.#####.\n.#...#.\n.#...#.\n.#####.\n......."
    );
    let e = shape(Tool::Rectangle, true, (2, 1), &[(6, 4)]);
    assert_eq!(
        art(&e, 1, 0, 7, 6),
        ".......\n.#####.\n.#####.\n.#####.\n.#####.\n......."
    );
    assert_eq!(e.undo_depth(), 1);
}

#[test]
fn circles_fit_the_dragged_box_for_even_and_odd_sizes() {
    let table: [(u32, &str, &str); 8] = [
        (1, "#", "#"),
        (2, "##\n##", "##\n##"),
        (3, ".#.\n#.#\n.#.", ".#.\n###\n.#."),
        (4, ".##.\n#..#\n#..#\n.##.", ".##.\n####\n####\n.##."),
        (
            5,
            ".###.\n#...#\n#...#\n#...#\n.###.",
            ".###.\n#####\n#####\n#####\n.###.",
        ),
        (
            6,
            "..##..\n.#..#.\n#....#\n#....#\n.#..#.\n..##..",
            "..##..\n.####.\n######\n######\n.####.\n..##..",
        ),
        (
            7,
            "..###..\n.#...#.\n#.....#\n#.....#\n#.....#\n.#...#.\n..###..",
            "..###..\n.#####.\n#######\n#######\n#######\n.#####.\n..###..",
        ),
        (
            8,
            "...##...\n.##..##.\n.#....#.\n#......#\n#......#\n.#....#.\n.##..##.\n...##...",
            "...##...\n.######.\n.######.\n########\n########\n.######.\n.######.\n...##...",
        ),
    ];
    for (n, outline, filled) in table {
        let far = (2 + n - 1, 2 + n - 1);
        for (fill, want) in [(false, outline), (true, filled)] {
            // Dragged either way, the box and so the circle are the same.
            for (from, to) in [((2, 2), far), (far, (2, 2))] {
                let e = shape(Tool::Circle, fill, from, &[to]);
                assert_eq!(art(&e, 2, 2, n, n), want, "size {n} filled {fill}");
                assert_eq!(cells(&e).len(), want.matches('#').count(), "size {n}");
            }
        }
    }
}

#[test]
fn every_circle_touches_all_four_sides_of_its_box() {
    for w in 1..=12 {
        for h in 1..=12 {
            for filled in [false, true] {
                let e = shape(Tool::Circle, filled, (2, 2), &[(2 + w - 1, 2 + h - 1)]);
                let got = cells(&e);
                let xs = got.iter().map(|c| c.0);
                let ys = got.iter().map(|c| c.1);
                let bounds = (xs.clone().min(), xs.max(), ys.clone().min(), ys.max());
                let want = (Some(2), Some(2 + w - 1), Some(2), Some(2 + h - 1));
                assert_eq!(bounds, want, "{w}x{h} filled {filled}");
            }
        }
    }
}

#[test]
fn a_box_that_is_not_square_makes_an_oval() {
    let e = shape(Tool::Circle, false, (0, 0), &[(6, 3)]);
    assert_eq!(art(&e, 0, 0, 7, 4), "..###..\n##...##\n##...##\n..###..");
    let e = shape(Tool::Circle, true, (0, 0), &[(6, 3)]);
    assert_eq!(art(&e, 0, 0, 7, 4), "..###..\n#######\n#######\n..###..");
}

#[test]
fn a_shape_drag_previews_only_its_latest_end_and_commits_on_release() {
    let mut e = Editor::new();
    dab(&mut e, 12, 3);
    e.set_tool(Tool::Rectangle);
    e.press(10, 2);
    e.drag_to(14, 6);
    // A 5x5 outline is 16 cells; the dot sits inside it.
    assert_eq!(cells(&e).len(), 17);
    e.drag_to(11, 3);
    assert!(e.in_stroke());
    assert_eq!(cells(&e), [(10, 2), (11, 2), (10, 3), (11, 3), (12, 3)]);
    assert_eq!(e.undo_depth(), 1);
    e.release();
    assert_eq!(e.undo_depth(), 2);
    assert!(e.undo());
    assert_eq!(cells(&e), [(12, 3)]);
    assert!(e.redo());
    assert_eq!(cells(&e).len(), 5);
}

#[test]
fn a_tool_picked_mid_drag_waits_for_the_next_press() {
    let mut e = Editor::new();
    e.set_tool(Tool::Rectangle);
    e.press(2, 2);
    e.set_tool(Tool::Eraser);
    e.drag_to(4, 3);
    e.release();
    assert_eq!(art(&e, 2, 2, 3, 2), "###\n###");
    assert_eq!(cells_where(&e, Some(WHITE)).len(), 6);
    // A Pencil stroke keeps painting after a switch to the Eraser.
    let mut e = Editor::new();
    e.press(10, 10);
    e.set_tool(Tool::Eraser);
    e.drag_to(12, 10);
    e.release();
    assert_eq!(cells(&e), [(10, 10), (11, 10), (12, 10)]);
}

#[test]
fn a_shape_click_with_no_drag_places_one_cell() {
    for tool in [Tool::Line, Tool::Rectangle, Tool::Circle] {
        let mut e = Editor::new();
        e.set_tool(tool);
        dab(&mut e, 7, 7);
        assert_eq!(cells(&e), [(7, 7)], "{tool:?}");
        assert_eq!(e.undo_depth(), 1);
    }
}

#[test]
fn shapes_follow_both_mirrors_in_the_paint_colour() {
    let mut e = Editor::new();
    e.set_colour(RED);
    e.set_mirror_x(true);
    e.set_mirror_y(true);
    e.set_tool(Tool::Rectangle);
    stroke(&mut e, (1, 1), &[(3, 2)]);
    let mut want = Vec::new();
    for y in [1, 2, 61, 62] {
        for x in [1, 2, 3, 60, 61, 62] {
            want.push((x, y));
        }
    }
    assert_eq!(cells_where(&e, Some(opaque(RED))), want);
    assert_eq!(cells(&e).len(), 24);
    assert_eq!(e.undo_depth(), 1);
}

#[test]
fn opacity_is_part_of_the_paint_and_painting_replaces_a_pixel() {
    let mut e = Editor::new();
    assert_eq!(e.alpha(), 255);
    assert_eq!(e.paint_rgba(), [255, 255, 255, 255]);
    e.set_alpha(128);
    dab(&mut e, 1, 1);
    assert_eq!(e.pixel(1, 1), [255, 255, 255, 128]);
    // A see-through dab over another pixel replaces it; nothing is blended.
    e.set_colour(RED);
    e.set_alpha(64);
    assert_eq!(e.paint_rgba(), [255, 0, 0, 64]);
    dab(&mut e, 1, 1);
    assert_eq!(e.pixel(1, 1), [255, 0, 0, 64]);
    e.set_alpha(255);
    dab(&mut e, 2, 2);
    e.set_alpha(10);
    dab(&mut e, 2, 2);
    assert_eq!(e.pixel(2, 2), [255, 0, 0, 10]);
    // The shape tools use it too.
    e.set_alpha(100);
    e.set_tool(Tool::Line);
    stroke(&mut e, (10, 10), &[(14, 10)]);
    assert_eq!(
        cells_where(&e, Some([255, 0, 0, 100])),
        [(10, 10), (11, 10), (12, 10), (13, 10), (14, 10)]
    );
    // The Eraser still clears whatever the opacity.
    e.set_tool(Tool::Eraser);
    dab(&mut e, 1, 1);
    assert_eq!(e.pixel(1, 1), CLEAR);
}

#[test]
fn fill_lays_down_the_see_through_paint() {
    let mut e = Editor::new();
    e.set_alpha(50);
    e.set_tool(Tool::Fill);
    dab(&mut e, 0, 0);
    assert_eq!(cells_where(&e, Some([255, 255, 255, 50])).len(), 64 * 64);
    // The same paint over the same region changes nothing and records nothing.
    dab(&mut e, 5, 5);
    assert_eq!(e.undo_depth(), 1);
}

#[test]
fn an_outline_stays_opaque_whatever_the_opacity() {
    let mut e = Editor::new();
    e.set_alpha(40);
    dab(&mut e, 10, 10);
    e.outline(BLACK);
    assert_eq!(e.pixel(9, 9), [0, 0, 0, 255]);
    assert_eq!(e.pixel(10, 10), [255, 255, 255, 40]);
}

#[test]
fn opacity_never_reaches_zero_so_painting_never_erases() {
    let mut e = Editor::new();
    e.set_alpha(0);
    assert_eq!(e.alpha(), 1);
    dab(&mut e, 3, 3);
    assert_eq!(e.pixel(3, 3), [255, 255, 255, 1]);
}

#[test]
fn picking_takes_the_opacity_too() {
    let mut e = Editor::new();
    e.set_colour(RED);
    e.set_alpha(77);
    dab(&mut e, 3, 3);
    e.set_colour([255, 255, 255]);
    e.set_alpha(255);
    assert!(e.pick(3, 3));
    assert_eq!((e.colour(), e.alpha()), (RED, 77));
    // A clear cell leaves both alone.
    e.set_alpha(200);
    assert!(!e.pick(4, 4));
    assert_eq!((e.colour(), e.alpha()), (RED, 200));
}

#[test]
fn every_opacity_percent_maps_to_an_alpha_and_back() {
    use tf2tools_app::pages::crosshair_maker::model::{alpha_of_percent, percent_of_alpha};
    for p in 1..=100 {
        assert_eq!(percent_of_alpha(alpha_of_percent(p)), p, "{p}%");
    }
    assert_eq!(
        [0, 1, 25, 50, 75, 100, 150].map(alpha_of_percent),
        [3, 3, 64, 128, 191, 255, 255]
    );
    assert_eq!(
        [0, 1, 64, 128, 255].map(percent_of_alpha),
        [1, 1, 25, 50, 100]
    );
}

#[test]
fn a_see_through_cell_draws_with_its_alpha() {
    use tf2tools_app::pages::crosshair_maker::model::{hsla_of, hsla_of_rgba};
    assert_eq!(hsla_of_rgba([200, 40, 40, 255]), hsla_of([200, 40, 40]));
    let half = hsla_of_rgba([200, 40, 40, 128]);
    assert!((half.a - 128. / 255.).abs() < 1e-6, "{half:?}");
    let solid = hsla_of([200, 40, 40]);
    assert_eq!((half.h, half.s, half.l), (solid.h, solid.s, solid.l));
}

#[test]
fn the_opacity_tooltip_says_painting_replaces_a_pixel() {
    use tf2tools_app::pages::crosshair_maker::model::OPACITY_TIP;
    assert_eq!(
        OPACITY_TIP,
        "See-through paint. Painting over a pixel replaces it; colours never blend."
    );
}

#[test]
fn a_starter_becomes_a_new_unsaved_drawing_with_one_undo_step() {
    use tf2tools_app::pages::crosshair_maker::starter::{Starter, starter_drawing};
    let full = RgbaImage::from_pixels(64, 64, vec![255; 64 * 64 * 4]).unwrap();
    let mut e = Editor::open(CrosshairId::new("abc"), "Old".to_owned(), &full).unwrap();
    e.set_tool(Tool::Eraser);
    dab(&mut e, 0, 0);
    e.set_colour(RED);
    e.set_alpha(77);
    e.set_mirror_x(true);
    e.set_filled(true);
    let small = CanvasSize::Small;
    let starter = starter_drawing(
        Starter::Dot,
        Starter::Dot.defaults(small),
        small,
        [1, 2, 3, 4],
    );
    assert!(e.use_starter(&starter));
    assert_eq!(e.size(), small);
    assert_eq!(e.drawing(), starter);
    assert_eq!((e.open_id(), e.name()), (None, ""));
    assert!(e.is_dirty());
    assert_eq!((e.undo_depth(), e.redo_depth()), (1, 0));
    assert_eq!((e.tool(), e.colour(), e.alpha()), (Tool::Eraser, RED, 77));
    assert!(e.mirror_x() && e.filled());
    assert!(e.undo());
    assert!(cells(&e).is_empty());
    assert_eq!(e.size(), small);
    assert!(!e.can_undo());
    assert!(e.redo());
    assert_eq!(e.drawing(), starter);
}

#[test]
fn a_starter_that_is_not_32_or_64_changes_nothing() {
    let mut e = Editor::new();
    dab(&mut e, 1, 1);
    let odd = RgbaImage::from_pixels(16, 16, vec![255; 16 * 16 * 4]).unwrap();
    assert!(!e.use_starter(&odd));
    assert_eq!(cells(&e), [(1, 1)]);
    assert_eq!(e.undo_depth(), 1);
    assert!(!e.use_starter(&RgbaImage::from_pixels(64, 32, vec![0; 64 * 32 * 4]).unwrap()));
}

#[test]
fn see_through_paint_mirrors_and_survives_undo_and_redo() {
    let mut e = Editor::new();
    e.set_colour(RED);
    e.set_mirror_x(true);
    e.set_mirror_y(true);
    e.set_alpha(37);
    dab(&mut e, 3, 5);
    let corners = [(3, 5), (60, 5), (3, 58), (60, 58)];
    assert_eq!(cells_where(&e, Some([255, 0, 0, 37])), corners);
    // A second see-through dab replaces all four reflections, and the history walks
    // back and forth between the two alphas without losing either.
    e.set_alpha(200);
    dab(&mut e, 3, 5);
    assert_eq!(cells_where(&e, Some([255, 0, 0, 200])), corners);
    assert!(e.undo());
    assert_eq!(cells_where(&e, Some([255, 0, 0, 37])), corners);
    assert_eq!(cells(&e).len(), 4);
    assert!(e.redo());
    assert_eq!(cells_where(&e, Some([255, 0, 0, 200])), corners);
    // The outline ring is opaque, sits around every reflection and undoes as one step.
    e.set_alpha(9);
    e.set_mirror_y(false);
    dab(&mut e, 10, 10);
    assert_eq!(cells_where(&e, Some([255, 0, 0, 9])), [(10, 10), (53, 10)]);
    e.outline(BLACK);
    assert_eq!(
        (e.pixel(9, 9), e.pixel(54, 11)),
        ([0, 0, 0, 255], [0, 0, 0, 255])
    );
    assert!(e.undo());
    assert_eq!(cells_where(&e, Some(opaque(BLACK))), []);
    assert_eq!(cells_where(&e, Some([255, 0, 0, 9])), [(10, 10), (53, 10)]);
    assert!(e.undo());
    assert_eq!(cells_where(&e, Some([255, 0, 0, 200])), corners);
    assert_eq!(cells(&e).len(), 4);
}

#[test]
fn a_fitted_picture_has_a_note_only_when_it_changed() {
    assert_eq!(fit_note(FitChange::AsIs), None);
    assert_eq!(fit_note(FitChange::ShrunkToFit), Some(FIT_SHRUNK));
    assert_eq!(fit_note(FitChange::Centred), Some(FIT_CENTRED));
    assert_eq!(
        FIT_SHRUNK,
        "This image is bigger than the canvas, so it was shrunk to fit."
    );
    assert_eq!(
        FIT_CENTRED,
        "This image is smaller than the canvas, so it was placed in the middle."
    );
}

#[test]
fn white_and_grey_swatches_say_they_follow_your_crosshair_colour() {
    use tf2tools_app::pages::crosshair_maker::model::{SWATCH_NAMES, SWATCHES, swatch_tooltip};
    for (&rgb, name) in SWATCHES.iter().zip(SWATCH_NAMES) {
        assert_eq!(
            swatch_tooltip(rgb, name),
            "Follows your crosshair colour in game",
            "{name}"
        );
    }
    // A swatch that is not white or grey keeps its own name as its tooltip.
    assert_eq!(swatch_tooltip(RED, "Red"), "Red");
}

#[test]
fn shrinking_knows_when_it_would_cut_off_paint() {
    let mut e = Editor::new();
    assert!(
        !e.would_crop(CanvasSize::Small),
        "an empty canvas loses nothing"
    );
    // The middle 16..48 survives a shrink, so paint on its edges is safe.
    for (x, y) in [(16, 16), (47, 47), (16, 47)] {
        dab(&mut e, x, y);
    }
    assert!(!e.would_crop(CanvasSize::Small));
    dab(&mut e, 15, 30);
    assert!(e.would_crop(CanvasSize::Small));
    assert!(
        !e.would_crop(CanvasSize::Large),
        "the current size is no change"
    );
    e.set_size(CanvasSize::Small);
    assert!(
        !e.would_crop(CanvasSize::Large),
        "growing keeps every pixel"
    );
}
