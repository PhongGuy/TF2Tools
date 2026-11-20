//! Each category is a card: a header bar on top, its rows inside, Learn more under each description.

use crate::support;

use gpui_kit::{Bounds, ElementId, Pixels, TestAppContext, px, size};
use support::graphics::{found, open, picture_page, reveal, wait_for_list};
use tf2tools_app::ids;

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
    reveal(cx, &s, ids::graphics_pictures_toggle("lod"));
    let desc = at(cx, &s, ids::graphics_module_description("lod"));
    let more = at(cx, &s, ids::graphics_learn_more("lod"));
    let toggle = at(cx, &s, ids::graphics_pictures_toggle("lod"));
    assert!(more.top() >= desc.bottom() - px(1.), "{desc:?} {more:?}");
    assert!(
        toggle.top() >= more.bottom() - px(1.),
        "{more:?} {toggle:?}"
    );
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
fn learn_more_and_the_pictures_toggle_stay_small_and_tight(cx: &mut TestAppContext) {
    let env = support::env();
    let s = picture_page_at(cx, &env, 1280.);
    reveal(cx, &s, ids::graphics_pictures_toggle("lod"));
    let desc = at(cx, &s, ids::graphics_module_description("lod"));
    let more = at(cx, &s, ids::graphics_learn_more("lod"));
    let toggle = at(cx, &s, ids::graphics_pictures_toggle("lod"));
    let row = at(cx, &s, ids::graphics_row("lod"));
    assert!(
        more.top() - desc.bottom() <= px(4.),
        "Learn more hugs the description: {desc:?} {more:?}"
    );
    assert!(more.size.height <= px(20.), "{more:?}");
    assert!(
        toggle.top() - more.bottom() <= px(4.),
        "{more:?} {toggle:?}"
    );
    assert!(toggle.size.height <= px(26.), "{toggle:?}");
    // At 1280 px the app's list column is narrower than the mock's, so the description takes
    // two 16 px lines; the old loose layout measured 133 px.
    assert!(
        row.size.height <= px(124.),
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
    let toggle = at(cx, &s, ids::graphics_pictures_toggle("lod"));
    assert!(
        control.top() >= toggle.bottom(),
        "the text column keeps its full height above the control: {toggle:?} {control:?}"
    );
    // Padding, the 97 px text column, the 8 px gap and the 32 px control; the old stacked
    // layout measured 200 px.
    assert!(row.size.height <= px(164.), "{row:?}");
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
