//! The compare view: the preset's picture and yours, a wipe between them, and zoom.

use crate::support;

use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::{AppContext as _, Bounds, ElementId, Entity, Pixels, TestAppContext, px, size};
use support::graphics::{
    found, no_network, open, picture_page, picture_url, reveal, toggle_pictures, wait_for_list,
    wait_for_pictures, wants,
};
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::GraphicsPage;
use tf2tools_app::remote_images::{ImageKey, RemoteImages};

fn near(a: Pixels, b: Pixels) -> bool {
    (a - b).abs() <= px(1.)
}

fn at(cx: &mut TestAppContext, s: &support::ShellWindow, id: ElementId) -> Bounds<Pixels> {
    support::bounds_of(cx, s.win, id.clone()).unwrap_or_else(|| panic!("{id:?} not drawn"))
}

fn label(cx: &mut TestAppContext, s: &support::ShellWindow, id: ElementId) -> Option<String> {
    support::label_of(cx, s.win, id)
}

/// Whether the page wants `<module>/<file>` decoded for the compare view.
fn wants_compared(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<GraphicsPage>,
    module: &str,
    file: &str,
) -> bool {
    let url = picture_url(cx, s, page, module, file);
    wants(cx, page, &ImageKey::carousel(&url))
}

/// Moves the wipe as a drag would: the slider takes the value, then reports it.
fn slide(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<GraphicsPage>, to: f32) {
    let state = cx.update(|cx| page.read(cx).wipe_state().clone());
    cx.update_window(s.win, |_, window, cx| {
        state.update(cx, |st, cx| {
            st.set_value(to, window, cx);
            cx.emit(SliderEvent::Change(SliderValue::Single(to)));
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn the_wipe_shows_yours_right_of_the_line_and_restarts_at_the_middle(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "medium", r#"{"lod":"high"}"#);
    cx.simulate_window_resize(s.win, size(px(1800.), px(1000.)));
    support::settle(cx, s.win);
    toggle_pictures(cx, &s, "lod");
    reveal(cx, &s, ids::graphics_compare_slider("lod"));
    let frame = at(cx, &s, ids::graphics_compare("lod"));
    assert!(
        near(frame.size.width, px(720.)) && near(frame.size.height, px(405.)),
        "{frame:?}"
    );
    assert_eq!(
        label(cx, &s, ids::graphics_compare_pill("lod", "preset")).as_deref(),
        Some("Medium: Medium")
    );
    assert_eq!(
        label(cx, &s, ids::graphics_compare_pill("lod", "yours")).as_deref(),
        Some("Yours: High")
    );
    assert_eq!(cx.update(|cx| page.read(cx).wipe(cx)), 50.);
    let clip = at(cx, &s, ids::graphics_compare_clip("lod"));
    assert!(
        near(clip.size.width, px(360.)) && near(clip.right(), frame.right()),
        "{clip:?}"
    );
    slide(cx, &s, &page, 20.);
    let clip = at(cx, &s, ids::graphics_compare_clip("lod"));
    let yours = at(cx, &s, ids::graphics_compare_yours("lod"));
    assert!(near(clip.size.width, px(576.)), "{clip:?}");
    assert!(
        near(yours.size.width, frame.size.width) && near(yours.right(), frame.right()),
        "{yours:?}"
    );
    slide(cx, &s, &page, 0.);
    assert!(near(
        at(cx, &s, ids::graphics_compare_clip("lod")).size.width,
        frame.size.width
    ));
    slide(cx, &s, &page, 100.);
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_compare_clip("lod")
    ));
    toggle_pictures(cx, &s, "lod");
    toggle_pictures(cx, &s, "lod");
    assert_eq!(cx.update(|cx| page.read(cx).wipe(cx)), 50.);
    for file in ["medium.webp", "high.webp"] {
        assert!(wants_compared(cx, &s, &page, "lod", file), "{file}");
    }
}

#[gpui_kit::test]
fn the_preset_s_value_shows_one_picture_without_a_slider(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "medium", "{}");
    toggle_pictures(cx, &s, "lod");
    assert_eq!(
        label(cx, &s, ids::graphics_compare_pill("lod", "same")).as_deref(),
        Some("Medium, as Medium: Medium")
    );
    for gone in [
        ids::graphics_compare_slider("lod"),
        ids::graphics_compare_clip("lod"),
        ids::graphics_compare_pill("lod", "yours"),
    ] {
        assert!(!support::present(cx, s.win, gone.clone()), "{gone:?}");
    }
}

#[gpui_kit::test]
fn under_custom_only_your_side_shows_and_nothing_picked_shows_no_compare(cx: &mut TestAppContext) {
    let env = support::env();
    // Custom leaves every module to TF2, so the preset's side never has a picture.
    let (s, _page) = picture_page(cx, &env, "custom", r#"{"lod":"high"}"#);
    toggle_pictures(cx, &s, "lod");
    assert!(support::present(cx, s.win, ids::graphics_compare("lod")));
    assert_eq!(
        label(cx, &s, ids::graphics_compare_pill("lod", "yours")).as_deref(),
        Some("Yours: High")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_compare_pill("lod", "preset")
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_compare_slider("lod")
    ));
    toggle_pictures(cx, &s, "shadows");
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_pictures("shadows")
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_compare("shadows")
    ));
}

#[gpui_kit::test]
fn zoom_swaps_both_pictures_for_their_crops_on_every_row(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "medium", r#"{"lod":"high"}"#);
    toggle_pictures(cx, &s, "lod");
    reveal(cx, &s, ids::graphics_zoom("lod"));
    assert_eq!(
        label(cx, &s, ids::graphics_zoom("lod")).as_deref(),
        Some("Zoom")
    );
    support::click_id(cx, s.win, ids::graphics_zoom("lod"));
    assert!(cx.update(|cx| page.read(cx).zoom()));
    assert_eq!(
        label(cx, &s, ids::graphics_zoom("lod")).as_deref(),
        Some("Full picture")
    );
    for file in ["medium.zoom.webp", "high.zoom.webp"] {
        assert!(wants_compared(cx, &s, &page, "lod", file), "{file}");
    }
    assert!(!wants_compared(cx, &s, &page, "lod", "high.webp"));
    toggle_pictures(cx, &s, "decals");
    assert!(
        !support::present(cx, s.win, ids::graphics_zoom("decals")),
        "decals has no crops"
    );
    assert!(cx.update(|cx| page.read(cx).zoom()), "zoom is page-wide");
    assert!(wants_compared(cx, &s, &page, "decals", "low.webp"));
}

/// Whether `module`'s compare picture on `side` has decoded and is drawn.
fn compare_drawn(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    module: &str,
    side: &str,
) -> bool {
    support::settle(cx, s.win);
    support::present(cx, s.win, ids::graphics_compare_image(module, side))
}

#[gpui_kit::test]
fn the_compare_view_draws_the_pictures_it_asked_for_with_zoom_on_and_off(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    cx.executor().allow_parking();
    cx.update(|cx| RemoteImages::install(cx, no_network()));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    wait_for_pictures(cx, &s, &page);
    toggle_pictures(cx, &s, "lod");
    reveal(cx, &s, ids::graphics_zoom("lod"));
    support::click_id(cx, s.win, ids::graphics_zoom("lod"));
    // texture_filter's full pictures were never asked for, so its compare view only fills
    // when it draws the crops the page wants.
    toggle_pictures(cx, &s, "texture_filter");
    reveal(cx, &s, ids::graphics_compare("texture_filter"));
    support::wait_real(cx, "the zoomed compare picture", |cx| {
        compare_drawn(cx, &s, "texture_filter", "preset")
    });
    reveal(cx, &s, ids::graphics_zoom("texture_filter"));
    support::click_id(cx, s.win, ids::graphics_zoom("texture_filter"));
    assert!(!cx.update(|cx| page.read(cx).zoom()));
    support::wait_real(cx, "the full compare picture", |cx| {
        compare_drawn(cx, &s, "texture_filter", "preset")
    });
}
