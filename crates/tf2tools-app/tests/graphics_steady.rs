//! Picking a value in an open pictures panel leaves the panel and the rows under it still.

use crate::support;

use gpui_kit::{Bounds, Pixels, TestAppContext, px, size};
use support::graphics::{
    found, idle, no_network, open, picture_url, reveal, toggle_pictures, wait_for_list,
    wait_for_pictures,
};
use tf2tools_app::ids;
use tf2tools_app::remote_images::{ImageKey, RemoteImage, RemoteImages};

const VALUES: [&str; 4] = ["low", "medium", "high", "ultra"];

// The strip cards, the caption and the next row, each measured from the lod row's top, since
// the changes summary above may move the whole row.
fn layout(cx: &mut TestAppContext, s: &support::ShellWindow) -> Vec<(String, Bounds<Pixels>)> {
    let top = support::bounds_of(cx, s.win, ids::graphics_row("lod"))
        .unwrap()
        .origin
        .y;
    let mut ids: Vec<(String, gpui_kit::ElementId)> = VALUES
        .iter()
        .map(|v| (format!("card {v}"), ids::graphics_strip_picture("lod", v)))
        .collect();
    ids.push(("caption".into(), ids::graphics_strip_caption("lod")));
    ids.push(("next row".into(), ids::graphics_row("lighting")));
    ids.into_iter()
        .map(|(name, id)| {
            let mut b = support::bounds_of(cx, s.win, id).unwrap_or_else(|| panic!("{name}"));
            b.origin.y -= top;
            (name, b)
        })
        .collect()
}

fn assert_same(before: &[(String, Bounds<Pixels>)], after: &[(String, Bounds<Pixels>)], at: &str) {
    for ((name, a), (_, b)) in before.iter().zip(after) {
        let moved = [
            a.origin.x - b.origin.x,
            a.origin.y - b.origin.y,
            a.size.width - b.size.width,
            a.size.height - b.size.height,
        ];
        assert!(
            moved.iter().all(|d| d.abs() <= px(0.5)),
            "{at}: {name} moved from {a:?} to {b:?}"
        );
    }
}

#[gpui_kit::test]
fn picking_a_value_keeps_the_pictures_panel_steady(cx: &mut TestAppContext) {
    for width in [1800., 1280., 800.] {
        let env = support::env();
        let s = support::shell_window(cx, &env, found(&env));
        cx.executor().allow_parking();
        let images = cx.update(|cx| RemoteImages::install(cx, no_network()));
        support::seed_managed_mastercomfig(&env, "9.100.1", "low");
        support::seed_full_mastercomfig_catalog(&env, "9.100.1");
        let page = open(cx, &s);
        wait_for_list(cx, &s, &page);
        wait_for_pictures(cx, &s, &page);
        // The lod pictures the page may want: every card's, and the two a Low against Medium
        // wipe compares. Each wanted one must be decoded before a measure.
        let mut keys: Vec<ImageKey> = VALUES
            .iter()
            .map(|v| ImageKey::card(&picture_url(cx, &s, &page, "lod", &format!("{v}.webp"))))
            .collect();
        for v in ["low", "medium"] {
            keys.push(ImageKey::carousel(&picture_url(
                cx,
                &s,
                &page,
                "lod",
                &format!("{v}.webp"),
            )));
        }
        cx.simulate_window_resize(s.win, size(px(width), px(900.)));
        support::settle(cx, s.win);
        toggle_pictures(cx, &s, "lod");
        reveal(cx, &s, ids::graphics_strip_caption("lod"));
        let ready = |cx: &mut TestAppContext| {
            support::wait_real(cx, "the lod pictures", |cx| {
                support::settle(cx, s.win);
                cx.update(|cx| {
                    let wanted = page.read(cx).wanted_images();
                    keys.iter()
                        .filter(|k| wanted.contains(k))
                        .all(|k| matches!(images.read(cx).image(k), RemoteImage::Ready(_)))
                })
            });
            support::settle(cx, s.win);
        };
        let pick = |cx: &mut TestAppContext, value: &str| {
            support::click_id(cx, s.win, ids::graphics_strip_picture("lod", value));
            idle(cx, &page);
            support::settle(cx, s.win);
            ready(cx);
        };
        ready(cx);
        let unchanged = layout(cx, &s);

        pick(cx, "medium");
        assert_same(&unchanged, &layout(cx, &s), &format!("{width} px, Medium"));
        pick(cx, "low");
        assert_same(
            &unchanged,
            &layout(cx, &s),
            &format!("{width} px, Low again"),
        );

        let b = |cx: &mut TestAppContext, id| support::bounds_of(cx, s.win, id).unwrap();
        let panel = b(cx, ids::graphics_pictures("lod"));
        let caption = b(cx, ids::graphics_strip_caption("lod"));
        let frame = b(cx, ids::graphics_compare("lod"));
        let strip = b(cx, ids::graphics_strip("lod"));
        let under_caption = panel.bottom() - caption.bottom();
        let over_strip = strip.top() - frame.bottom();
        // The panel's 16 px padding, then its 1 px bottom rule.
        assert!(
            (under_caption - px(17.)).abs() <= px(0.5),
            "{width} px: {under_caption:?} under the caption"
        );
        assert!(
            (over_strip - px(12.)).abs() <= px(0.5),
            "{width} px: {over_strip:?} between the picture and the strip"
        );
    }
}
