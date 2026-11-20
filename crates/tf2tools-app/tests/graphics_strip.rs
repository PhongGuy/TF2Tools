//! The strip under an open row: every value's picture, the preset's tag, and picking by picture.

use crate::support;

use gpui_kit::{Pixels, TestAppContext, px};
use support::graphics::{
    HEADER, idle, modules_cfg, picture_page, picture_url, reveal, saved, toggle_pictures, value,
    wants,
};
use tf2tools_app::ids;
use tf2tools_app::remote_images::ImageKey;

#[gpui_kit::test]
fn the_strip_shows_every_value_with_a_picture_and_tags_the_preset_s(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "medium", "{}");
    toggle_pictures(cx, &s, "lod");
    reveal(cx, &s, ids::graphics_strip_caption("lod"));
    let lefts: Vec<Pixels> = ["low", "medium", "high", "ultra"]
        .iter()
        .map(|v| {
            support::bounds_of(cx, s.win, ids::graphics_strip_picture("lod", v))
                .unwrap()
                .left()
        })
        .collect();
    assert!(
        lefts.windows(2).all(|w| w[0] < w[1]),
        "catalogue order: {lefts:?}"
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_strip_picture("lod", "high")).as_deref(),
        Some("High")
    );
    // The Medium card under the Medium preset would read "Medium Medium", so its tag says
    // "Preset" instead of repeating the name.
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_strip_tag("lod")).as_deref(),
        Some("Preset")
    );
    assert_tag_differs_from_its_card(cx, &s, "lod", "medium");
    let tag = support::bounds_of(cx, s.win, ids::graphics_strip_tag("lod")).unwrap();
    let medium =
        support::bounds_of(cx, s.win, ids::graphics_strip_picture("lod", "medium")).unwrap();
    assert!(
        tag.left() >= medium.left() && tag.right() <= medium.right(),
        "{tag:?} {medium:?}"
    );
    let picture = support::bounds_of(cx, s.win, ids::graphics_strip_area("lod", "medium")).unwrap();
    assert!(
        tag.top() >= picture.bottom() && tag.top() - picture.bottom() <= px(10.),
        "the tag sits on the label line: {tag:?} {picture:?}"
    );
    assert!(
        tag.left() >= medium.left() + px(20.),
        "the tag follows the label inline: {tag:?} {medium:?}"
    );
    assert!(
        medium.bottom() - tag.bottom() <= px(12.),
        "one line under the picture: {tag:?} {medium:?}"
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_strip_caption("lod")).as_deref(),
        Some("Click a picture to use that setting.")
    );
    let ultra = picture_url(cx, &s, &page, "lod", "ultra.webp");
    assert!(wants(cx, &page, &ImageKey::card(&ultra)));
}

#[gpui_kit::test]
fn a_picture_picks_its_value_by_click_or_keyboard(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "medium", "{}");
    toggle_pictures(cx, &s, "lod");
    reveal(cx, &s, ids::graphics_strip_picture("lod", "high"));
    support::click_id(cx, s.win, ids::graphics_strip_picture("lod", "high"));
    support::wait_real(cx, "lod high", |cx| {
        saved(cx, &page, "lod") == Some(value("high"))
    });
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}lod=high\n").as_str())
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::graphics_segment("lod", "high")),
        Some(true)
    );
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_compare_pill("lod", "yours")
    ));
    support::tab_to(cx, s.win, ids::graphics_strip_picture("lod", "medium"), 160);
    support::press(cx, s.win, "enter");
    support::wait_real(cx, "back to the preset", |cx| {
        saved(cx, &page, "lod").is_none()
    });
    idle(cx, &page);
    assert_eq!(modules_cfg(&env).as_deref(), Some(HEADER));
}

#[gpui_kit::test]
fn under_custom_no_picture_carries_the_preset_tag(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "custom", "{}");
    toggle_pictures(cx, &s, "lod");
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_strip_picture("lod", "low")
    ));
    assert!(!support::present(cx, s.win, ids::graphics_strip_tag("lod")));
}

#[gpui_kit::test]
fn a_preset_value_named_otherwise_carries_the_preset_s_name(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = picture_page(cx, &env, "medium", "{}");
    toggle_pictures(cx, &s, "shading");
    reveal(cx, &s, ids::graphics_strip_caption("shading"));
    let tag = support::bounds_of(cx, s.win, ids::graphics_strip_tag("shading")).unwrap();
    let high =
        support::bounds_of(cx, s.win, ids::graphics_strip_picture("shading", "high")).unwrap();
    assert!(
        tag.left() >= high.left() && tag.right() <= high.right(),
        "Medium sets shading to High: {tag:?} {high:?}"
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_strip_tag("shading")).as_deref(),
        Some("Medium")
    );
    assert_tag_differs_from_its_card(cx, &s, "shading", "high");
}

// A value card never repeats its own label in its preset tag.
fn assert_tag_differs_from_its_card(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    module: &str,
    value: &str,
) {
    let tag = support::label_of(cx, s.win, ids::graphics_strip_tag(module));
    let card = support::label_of(cx, s.win, ids::graphics_strip_picture(module, value));
    assert!(tag.is_some() && card.is_some(), "{tag:?} {card:?}");
    assert_ne!(tag, card, "the {module} {value} card repeats its label");
}
