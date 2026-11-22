//! Each category is a card: a header bar on top, its rows inside, Learn more under each description.

use crate::support;

use gpui_kit::component::Theme;
use gpui_kit::{AppContext as _, Bounds, ElementId, Hsla, Pixels, TestAppContext, px, size};
use support::graphics::{found, open, picture_page, reveal, toggle_pictures, wait_for_list};
use tf2tools_app::ids;
use tf2tools_core::Preset;

fn at(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<ElementId>,
) -> Bounds<Pixels> {
    let id = id.into();
    support::bounds_of(cx, s.win, id.clone()).unwrap_or_else(|| panic!("{id:?} not drawn"))
}

fn wide_page(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    let s = support::shell_window(cx, env, found(env));
    support::seed_managed_mastercomfig(env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(env, "9.100.1");
    support::seed_mastercomfig_choices(env, "{}");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    cx.simulate_window_resize(s.win, size(px(1280.), px(900.)));
    support::settle(cx, s.win);
    s
}

/// A full-catalogue page with its pictures loaded, `width` px wide.
fn picture_page_at(
    cx: &mut TestAppContext,
    env: &support::Env,
    width: f32,
) -> support::ShellWindow {
    let (s, _page) = picture_page(cx, env, "medium", "{}");
    cx.simulate_window_resize(s.win, size(px(width), px(900.)));
    support::settle(cx, s.win);
    s
}

#[gpui_kit::test]
fn a_category_is_a_card_with_a_header_bar_and_rows_inside(cx: &mut TestAppContext) {
    let env = support::env();
    let s = wide_page(cx, &env);
    reveal(cx, &s, ids::graphics_row("lod"));
    let card = at(cx, &s, ids::graphics_section("graphics"));
    let head = at(cx, &s, ids::graphics_category("graphics"));
    let row = at(cx, &s, ids::graphics_row("lod"));
    assert!(head.top() >= card.top(), "{head:?} {card:?}");
    assert!(
        row.top() >= head.bottom(),
        "the header bar sits above the rows: {head:?} {row:?}"
    );
    assert!(
        row.left() - card.left() <= px(2.) && card.right() - row.right() <= px(2.),
        "rows run edge to edge inside the card: {row:?} {card:?}"
    );
    let name = at(cx, &s, ids::graphics_module("lod"));
    assert!(
        name.left() >= row.left() + px(12.),
        "the row pads its content: {name:?} {row:?}"
    );
    let hud = at(cx, &s, ids::graphics_section("hud"));
    assert!(
        hud.top() >= card.bottom() + px(8.),
        "cards stand apart: {card:?} {hud:?}"
    );
}

#[gpui_kit::test]
fn learn_more_sits_under_the_description_and_opens_the_docs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = wide_page(cx, &env);
    reveal(cx, &s, ids::graphics_learn_more("lod"));
    let desc = at(cx, &s, ids::graphics_module_description("lod"));
    let more = at(cx, &s, ids::graphics_learn_more("lod"));
    assert!(more.top() >= desc.bottom() - px(1.), "{desc:?} {more:?}");
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_learn_more("lod")).as_deref(),
        Some("Learn more")
    );
    support::click_id(cx, s.win, ids::graphics_learn_more("lod"));
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://docs.mastercomfig.com/")
    );
}

// The solid fills the last frame painted exactly over `id`'s box, read from its quads.
fn fills_at(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<ElementId>,
) -> Vec<Hsla> {
    let b = at(cx, s, id);
    cx.update_window(s.win, |_, w, _| {
        let scale = w.scale_factor();
        let near = |a: f32, b: Pixels| (a - f32::from(b) * scale).abs() <= 0.5;
        w.painted_quads()
            .iter()
            .filter(|q| {
                near(q.bounds.origin.x.0, b.origin.x)
                    && near(q.bounds.origin.y.0, b.origin.y)
                    && near(q.bounds.size.width.0, b.size.width)
                    && near(q.bounds.size.height.0, b.size.height)
            })
            .filter_map(|q| q.background.as_solid())
            .filter(|c| c.a > 0.)
            .collect()
    })
    .unwrap()
}

// A row's checked segment is painted like the checked preset: the page background on the
// secondary track, while the other segments paint no fill of their own.
#[gpui_kit::test]
fn the_checked_segment_is_painted_like_the_checked_preset(cx: &mut TestAppContext) {
    let env = support::env();
    let s = wide_page(cx, &env);
    let (background, secondary) = cx.update(|cx| {
        let t = Theme::global(cx);
        (t.background, t.secondary)
    });
    let preset = fills_at(cx, &s, ids::graphics_preset(Preset::Medium));
    assert_eq!(preset, [background], "the checked preset");
    reveal(cx, &s, ids::graphics_segments("lod"));
    assert_eq!(
        fills_at(cx, &s, ids::graphics_segment("lod", "medium")),
        preset,
        "the checked segment"
    );
    for v in ["low", "high", "ultra"] {
        assert_eq!(fills_at(cx, &s, ids::graphics_segment("lod", v)), [], "{v}");
    }
    assert_eq!(
        fills_at(cx, &s, ids::graphics_segments("lod")),
        [secondary],
        "the track"
    );
}

#[gpui_kit::test]
fn a_row_s_segments_sit_inside_one_track(cx: &mut TestAppContext) {
    let env = support::env();
    let s = wide_page(cx, &env);
    reveal(cx, &s, ids::graphics_segments("lod"));
    let track = at(cx, &s, ids::graphics_segments("lod"));
    let segs: Vec<Bounds<Pixels>> = ["low", "medium", "high", "ultra"]
        .iter()
        .map(|v| at(cx, &s, ids::graphics_segment("lod", v)))
        .collect();
    for b in &segs {
        assert!(
            b.left() >= track.left() + px(2.)
                && b.right() <= track.right() - px(2.)
                && b.top() >= track.top() + px(2.)
                && b.bottom() <= track.bottom() - px(2.),
            "a segment sits inset in the track: {b:?} {track:?}"
        );
    }
    for w in segs.windows(2) {
        assert!(
            w[0].right() <= w[1].left() && w[1].left() - w[0].right() <= px(3.),
            "{w:?}"
        );
    }
}

#[gpui_kit::test]
fn learn_more_stays_small_and_a_picture_row_tight(cx: &mut TestAppContext) {
    let env = support::env();
    let s = picture_page_at(cx, &env, 1280.);
    reveal(cx, &s, ids::graphics_learn_more("lod"));
    let desc = at(cx, &s, ids::graphics_module_description("lod"));
    let more = at(cx, &s, ids::graphics_learn_more("lod"));
    let row = at(cx, &s, ids::graphics_row("lod"));
    assert!(
        more.top() - desc.bottom() <= px(4.),
        "Learn more hugs the description: {desc:?} {more:?}"
    );
    assert!(more.size.height <= px(20.), "{more:?}");
    // The row stays compact even though the description wraps in the app's narrower column.
    assert!(
        row.size.height <= px(100.),
        "a row with pictures stays compact: {row:?}"
    );
}

#[gpui_kit::test]
fn at_800_px_the_thumbnail_stays_beside_the_text_and_only_the_control_drops(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    let s = picture_page_at(cx, &env, 800.);
    reveal(cx, &s, ids::graphics_segments("lod"));
    let thumb = at(cx, &s, ids::graphics_thumb("lod"));
    let name = at(cx, &s, ids::graphics_module("lod"));
    let desc = at(cx, &s, ids::graphics_module_description("lod"));
    let control = at(cx, &s, ids::graphics_segments("lod"));
    let row = at(cx, &s, ids::graphics_row("lod"));
    assert!(
        thumb.right() <= name.left() && (thumb.top() - name.top()).abs() <= px(8.),
        "{thumb:?} {name:?}"
    );
    assert!(
        control.top() >= desc.bottom(),
        "the control drops under the text: {control:?} {desc:?}"
    );
    let more = at(cx, &s, ids::graphics_learn_more("lod"));
    assert!(
        control.top() >= more.bottom(),
        "the text column keeps its full height above the control: {more:?} {control:?}"
    );
    assert!(
        desc.right() <= row.right(),
        "the description wraps inside the row: {desc:?} {row:?}"
    );
    // Keeping the thumbnail beside the text keeps the narrow row short.
    assert!(row.size.height <= px(140.), "{row:?}");
}

#[gpui_kit::test]
fn the_thumbnail_has_a_thin_frame_around_its_picture(cx: &mut TestAppContext) {
    let env = support::env();
    let s = picture_page_at(cx, &env, 1280.);
    reveal(cx, &s, ids::graphics_thumb("lod"));
    let frame = at(cx, &s, ids::graphics_thumb("lod"));
    let picture = at(cx, &s, ids::graphics_thumb_area("lod"));
    assert!(
        picture.left() >= frame.left() + px(1.)
            && picture.right() <= frame.right() - px(1.)
            && picture.top() >= frame.top() + px(1.)
            && picture.bottom() <= frame.bottom() - px(1.)
            && frame.size.width <= px(100.),
        "a 1 px frame around the 96x54 picture: {frame:?} {picture:?}"
    );
}

#[gpui_kit::test]
fn the_page_opens_on_the_status_card_with_no_subtitle(cx: &mut TestAppContext) {
    let env = support::env();
    let s = wide_page(cx, &env);
    support::graphics::scroll_to_top(cx, &s);
    let view = at(
        cx,
        &s,
        ids::page(tf2tools_app::shell::route::Page::Graphics),
    );
    let status = at(cx, &s, ids::GRAPHICS_STATUS);
    assert!(
        status.top() - view.top() <= tf2tools_app::shell::CONTENT_PADDING + px(2.),
        "{status:?} {view:?}"
    );
}

#[gpui_kit::test]
fn the_pictures_open_inside_the_card_with_zoom_on_the_picture(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "medium", "{}");
    cx.simulate_window_resize(s.win, size(px(1280.), px(900.)));
    support::settle(cx, s.win);
    toggle_pictures(cx, &s, "lod");
    reveal(cx, &s, ids::graphics_zoom("lod"));
    let card = at(cx, &s, ids::graphics_section("graphics"));
    let panel = at(cx, &s, ids::graphics_pictures("lod"));
    assert!(
        panel.left() - card.left() <= px(2.) && card.right() - panel.right() <= px(2.),
        "the panel runs the card's width: {panel:?} {card:?}"
    );
    let picture = at(cx, &s, ids::graphics_compare("lod"));
    let zoom = at(cx, &s, ids::graphics_zoom("lod"));
    assert!(
        zoom.left() >= picture.left()
            && zoom.right() <= picture.right()
            && zoom.top() >= picture.top()
            && zoom.bottom() <= picture.bottom(),
        "Zoom sits on the picture: {zoom:?} {picture:?}"
    );
    assert!(
        picture.right() - zoom.right() <= px(16.) && picture.bottom() - zoom.bottom() <= px(16.),
        "{zoom:?}"
    );
}

#[gpui_kit::test]
fn a_changed_row_s_bar_marks_its_line_and_not_its_open_panel(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "medium", r#"{"lod":"high"}"#);
    cx.simulate_window_resize(s.win, size(px(1280.), px(900.)));
    support::settle(cx, s.win);
    toggle_pictures(cx, &s, "lod");
    reveal(cx, &s, ids::graphics_pictures("lod"));
    let bar = at(cx, &s, ids::graphics_row_bar("lod"));
    let panel = at(cx, &s, ids::graphics_pictures("lod"));
    assert!(
        bar.bottom() <= panel.top(),
        "the bar and tint stop at the row line: {bar:?} {panel:?}"
    );
}

/// From the compare picture's bottom to the strip's top.
const PICTURE_TO_STRIP: f32 = 12.;

// The open lod panel's seams: the stage is no taller than its picture, the panel ends 16 px
// under its caption, and the next row's rule follows at once.
fn assert_no_gap_under_lod(cx: &mut TestAppContext, s: &support::ShellWindow, when: &str) {
    reveal(cx, s, ids::graphics_strip_caption("lod"));
    let frame = at(cx, s, ids::graphics_compare("lod"));
    let strip = at(cx, s, ids::graphics_strip("lod"));
    let caption = at(cx, s, ids::graphics_strip_caption("lod"));
    let panel = at(cx, s, ids::graphics_pictures("lod"));
    let next = at(cx, s, ids::graphics_row("lighting"));
    let over_strip = strip.top() - frame.bottom();
    assert!(
        (over_strip - px(PICTURE_TO_STRIP)).abs() <= px(0.5),
        "{when}: {over_strip:?} between the picture {frame:?} and the strip {strip:?}"
    );
    let under_caption = panel.bottom() - caption.bottom();
    assert!(
        under_caption <= px(17.5),
        "{when}: the panel runs {under_caption:?} past its caption"
    );
    // The panel's 1 px bottom rule overlaps the next row's top rule.
    let seam = next.top() - panel.bottom();
    assert!(
        seam >= px(-1.5) && seam <= px(0.5),
        "{when}: {seam:?} between the panel {panel:?} and the next row {next:?}"
    );
}

#[gpui_kit::test]
fn an_open_panel_leaves_no_gap_before_the_next_row(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "medium", "{}");
    toggle_pictures(cx, &s, "lod");
    for width in [1800., 1280., 800.] {
        cx.simulate_window_resize(s.win, size(px(width), px(900.)));
        support::settle(cx, s.win);
        assert_no_gap_under_lod(cx, &s, &format!("{width} px"));
    }
}

#[gpui_kit::test]
fn a_zoomed_wipe_leaves_no_gap_before_the_next_row(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "low", r#"{"lod":"medium"}"#);
    cx.simulate_window_resize(s.win, size(px(1800.), px(1000.)));
    support::settle(cx, s.win);
    toggle_pictures(cx, &s, "lod");
    reveal(cx, &s, ids::graphics_zoom("lod"));
    support::click_id(cx, s.win, ids::graphics_zoom("lod"));
    assert!(cx.update(|cx| page.read(cx).zoom()));
    assert_no_gap_under_lod(cx, &s, "1800 px, zoomed");
}
