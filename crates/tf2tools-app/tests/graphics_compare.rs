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
        Some("Medium (same as Medium)")
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

#[gpui_kit::test]
fn the_slider_s_thumb_rides_the_wipe_line_and_zoom_keeps_clear(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "low", r#"{"lod":"medium"}"#);
    toggle_pictures(cx, &s, "lod");
    for width in [1800., 1280., 800.] {
        cx.simulate_window_resize(s.win, size(px(width), px(900.)));
        support::settle(cx, s.win);
        reveal(cx, &s, ids::graphics_compare_slider("lod"));
        for wipe in [0., 50., 100.] {
            slide(cx, &s, &page, wipe);
            let frame = at(cx, &s, ids::graphics_compare("lod"));
            // At either end the line is not drawn; it would sit on the frame's edge.
            let line = match wipe {
                0. => frame.left(),
                100. => frame.right(),
                _ => at(cx, &s, ids::graphics_compare_line("lod")).center().x,
            };
            let thumb = at(cx, &s, ElementId::from(("slider-thumb", 0u32)))
                .center()
                .x;
            assert!(
                near(thumb, line),
                "{width} px at {wipe}%: thumb {thumb:?}, line {line:?}"
            );
            let track = at(cx, &s, ids::graphics_compare_slider("lod"));
            let zoom = at(cx, &s, ids::graphics_zoom("lod"));
            assert!(
                !zoom.intersects(&track),
                "{width} px: Zoom {zoom:?} overlaps the slider {track:?}"
            );
        }
    }
}

#[gpui_kit::test]
fn the_slider_runs_under_the_picture_and_zoom_sits_in_its_corner(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "low", r#"{"lod":"medium"}"#);
    toggle_pictures(cx, &s, "lod");
    for width in [1800., 1280., 800.] {
        cx.simulate_window_resize(s.win, size(px(width), px(900.)));
        support::settle(cx, s.win);
        reveal(cx, &s, ids::graphics_compare_slider("lod"));
        let frame = at(cx, &s, ids::graphics_compare("lod"));
        let slider = at(cx, &s, ids::graphics_compare_slider("lod"));
        let zoom = at(cx, &s, ids::graphics_zoom("lod"));
        assert!(
            near(slider.top() - frame.bottom(), px(8.)) && near(slider.size.height, px(24.)),
            "{width} px: the slider {slider:?} runs 8 px under the picture {frame:?}"
        );
        assert!(
            near(slider.left(), frame.left()) && near(slider.right(), frame.right()),
            "{width} px: the slider {slider:?} is as wide as the picture {frame:?}"
        );
        assert!(
            near(frame.right() - zoom.right(), px(10.))
                && near(frame.bottom() - zoom.bottom(), px(10.)),
            "{width} px: Zoom {zoom:?} sits 10 px in from the corner of {frame:?}"
        );
    }
}

#[gpui_kit::test]
fn without_a_wipe_zoom_keeps_its_corner_and_no_slider_shows(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "medium", "{}");
    toggle_pictures(cx, &s, "lod");
    reveal(cx, &s, ids::graphics_zoom("lod"));
    let frame = at(cx, &s, ids::graphics_compare("lod"));
    let zoom = at(cx, &s, ids::graphics_zoom("lod"));
    assert!(
        near(frame.right() - zoom.right(), px(10.))
            && near(frame.bottom() - zoom.bottom(), px(10.)),
        "Zoom {zoom:?} sits 10 px in from the corner of {frame:?}"
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_compare_slider("lod")
    ));
}

// The solid fill and the top-left radius of the quad painted exactly over `id`'s box, in
// window pixels.
fn pill_look(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: ElementId,
) -> (gpui_kit::Hsla, f32, f32) {
    let b = at(cx, s, id.clone());
    cx.update_window(s.win, |_, w, _| {
        let scale = w.scale_factor();
        let close = |a: f32, b: Pixels| (a - f32::from(b) * scale).abs() <= 0.5;
        w.painted_quads()
            .iter()
            .filter(|q| {
                close(q.bounds.origin.x.0, b.origin.x)
                    && close(q.bounds.origin.y.0, b.origin.y)
                    && close(q.bounds.size.height.0, b.size.height)
            })
            .find_map(|q| {
                q.background.as_solid().filter(|c| c.a > 0.).map(|c| {
                    (
                        c,
                        q.corner_radii.top_left.0,
                        f32::from(b.size.height) * scale,
                    )
                })
            })
            .unwrap_or_else(|| panic!("no fill painted for {id:?}"))
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_corner_pills_are_round_dark_and_10_px_in(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "medium", r#"{"lod":"high"}"#);
    cx.simulate_window_resize(s.win, size(px(1800.), px(1000.)));
    support::settle(cx, s.win);
    toggle_pictures(cx, &s, "lod");
    reveal(cx, &s, ids::graphics_compare("lod"));
    let frame = at(cx, &s, ids::graphics_compare("lod"));
    let left = at(cx, &s, ids::graphics_compare_pill("lod", "preset"));
    let right = at(cx, &s, ids::graphics_compare_pill("lod", "yours"));
    assert!(
        near(left.top() - frame.top(), px(10.)) && near(left.left() - frame.left(), px(10.)),
        "{left:?}"
    );
    assert!(
        near(right.top() - frame.top(), px(10.)) && near(frame.right() - right.right(), px(10.)),
        "{right:?}"
    );
    for side in ["preset", "yours"] {
        let (fill, radius, height) = pill_look(cx, &s, ids::graphics_compare_pill("lod", side));
        assert_eq!(fill, gpui_kit::hsla(0., 0., 0., 0.8), "{side}");
        assert!(
            radius >= height / 2. - 0.5,
            "{side}: fully rounded, {radius} for {height}"
        );
    }
}
