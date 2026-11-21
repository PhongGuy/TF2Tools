//! A row's thumbnail and Show pictures button, and one open row at a time.

use crate::support;

use gpui_kit::{TestAppContext, px, size};
use support::graphics::{
    found, lod_only_assets, no_network, open, picture_page, reveal, toggle_pictures, wait_for_list,
    wait_for_pictures,
};
use tf2tools_app::ids;
use tf2tools_app::remote_images::{RemoteImage, RemoteImages};

#[gpui_kit::test]
fn a_row_with_pictures_shows_a_thumbnail_and_a_pictures_button(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "medium", "{}");
    reveal(cx, &s, ids::graphics_thumb("lod"));
    let picture = support::bounds_of(cx, s.win, ids::graphics_thumb_area("lod")).unwrap();
    assert_eq!(
        (picture.size.width, picture.size.height),
        (px(96.), px(54.))
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_thumb("lod")).as_deref(),
        Some("Pictures of Model quality")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_pictures_toggle("lod")).as_deref(),
        Some("Show pictures")
    );
    for none in ["vsync", "motion_blur", "hud_achievement"] {
        assert!(
            !support::present(cx, s.win, ids::graphics_thumb(none)),
            "{none}"
        );
        assert!(
            !support::present(cx, s.win, ids::graphics_pictures_toggle(none)),
            "{none}"
        );
    }
}

#[gpui_kit::test]
fn the_thumbnail_sits_left_of_the_name_at_every_width(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "medium", "{}");
    for width in [800., 1400.] {
        cx.simulate_window_resize(s.win, size(px(width), px(900.)));
        support::settle(cx, s.win);
        reveal(cx, &s, ids::graphics_thumb("lod"));
        let thumb = support::bounds_of(cx, s.win, ids::graphics_thumb("lod")).unwrap();
        let name = support::bounds_of(cx, s.win, ids::graphics_module("lod")).unwrap();
        assert!(
            thumb.right() <= name.left()
                && thumb.top() <= name.top()
                && name.top() < thumb.bottom(),
            "{width}: {thumb:?} {name:?}"
        );
    }
}

#[gpui_kit::test]
fn under_custom_the_slot_is_empty_but_the_pictures_still_open(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "custom", "{}");
    reveal(cx, &s, ids::graphics_thumb("lod"));
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_thumb_image("lod")
    ));
    toggle_pictures(cx, &s, "lod");
    assert!(support::present(cx, s.win, ids::graphics_pictures("lod")));
}

#[gpui_kit::test]
fn one_row_s_pictures_are_open_at_a_time(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "medium", "{}");
    let open_row =
        |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).open_row().map(str::to_owned));
    assert_eq!(open_row(cx), None);
    toggle_pictures(cx, &s, "lod");
    assert_eq!(open_row(cx).as_deref(), Some("lod"));
    assert!(support::present(cx, s.win, ids::graphics_pictures("lod")));
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_pictures_toggle("lod")).as_deref(),
        Some("Hide pictures")
    );
    reveal(cx, &s, ids::graphics_thumb("decals"));
    support::click_id(cx, s.win, ids::graphics_thumb("decals"));
    assert_eq!(open_row(cx).as_deref(), Some("decals"));
    assert!(!support::present(cx, s.win, ids::graphics_pictures("lod")));
    toggle_pictures(cx, &s, "decals");
    assert_eq!(open_row(cx), None);
    support::tab_to(cx, s.win, ids::graphics_thumb("lod"), 128);
    support::press(cx, s.win, "enter");
    assert_eq!(
        open_row(cx).as_deref(),
        Some("lod"),
        "the thumbnail opens from the keyboard"
    );
}

#[gpui_kit::test]
fn a_ready_picture_shows_and_a_failed_one_leaves_the_slot_empty(cx: &mut TestAppContext) {
    let env = support::env();
    let assets = lod_only_assets(&env, b"not a picture");
    let s =
        support::shell_window_tweaked(cx, &env, found(&env), |services| services.assets = assets);
    cx.executor().allow_parking();
    let images = cx.update(|cx| RemoteImages::install(cx, no_network()));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    wait_for_pictures(cx, &s, &page);
    let key = cx.update(|cx| page.read(cx).wanted_images()[0].clone());
    support::wait_real(cx, "the failed decode", |cx| {
        support::settle(cx, s.win);
        cx.update(|cx| matches!(images.read(cx).image(&key), RemoteImage::Failed))
    });
    reveal(cx, &s, ids::graphics_thumb("lod"));
    assert!(
        support::present(cx, s.win, ids::graphics_thumb("lod")),
        "the empty slot stays"
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_thumb_image("lod")
    ));
}

#[gpui_kit::test]
fn a_decoded_thumbnail_is_drawn(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    cx.executor().allow_parking();
    let images = cx.update(|cx| RemoteImages::install(cx, no_network()));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    wait_for_pictures(cx, &s, &page);
    let key = cx.update(|cx| page.read(cx).wanted_images()[0].clone());
    support::wait_real(cx, "a thumbnail", |cx| {
        support::settle(cx, s.win);
        cx.update(|cx| matches!(images.read(cx).image(&key), RemoteImage::Ready(_)))
    });
    reveal(cx, &s, ids::graphics_thumb("lod"));
    support::wait_real(cx, "the lod thumbnail", |cx| {
        support::settle(cx, s.win);
        support::present(cx, s.win, ids::graphics_thumb_image("lod"))
    });
}

#[gpui_kit::test]
fn a_row_without_pictures_says_why(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "medium", "{}");
    // lod sits above motion_blur, and reveal only scrolls down.
    reveal(cx, &s, ids::graphics_thumb("lod"));
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_no_picture("lod")
    ));
    reveal(cx, &s, ids::graphics_no_picture("motion_blur"));
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_no_picture("motion_blur")).as_deref(),
        Some(
            "No picture: it looks the same in a still picture. The difference only shows while you move."
        )
    );
}
