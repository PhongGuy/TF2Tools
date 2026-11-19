//! The preset control: Low to Ultra, then Custom apart, with the preset's line and the scale.

use crate::support;

use gpui_kit::{Bounds, ElementId, Pixels, TestAppContext, px};
use support::graphics::{found, idle, open, toasted};
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

fn near(a: Pixels, b: Pixels) -> bool {
    (a - b).abs() <= px(1.)
}

#[gpui_kit::test]
fn the_ranked_four_read_low_to_ultra_with_custom_apart(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let _page = open(cx, &s);
    // The default 1200 px window is narrow; the side-by-side layout needs a wide page.
    cx.simulate_window_resize(s.win, gpui_kit::size(px(1400.), px(900.)));
    support::settle(cx, s.win);
    let [low, medium, high, ultra, custom] = [
        Preset::Low,
        Preset::Medium,
        Preset::High,
        Preset::Ultra,
        Preset::Custom,
    ]
    .map(|p| at(cx, &s, ids::graphics_preset(p)));
    assert!(
        low.right() <= medium.left() + px(1.)
            && medium.right() <= high.left() + px(1.)
            && high.right() <= ultra.left() + px(1.),
        "{low:?} {medium:?} {high:?} {ultra:?}"
    );
    assert!(
        ultra.right() + px(4.) <= custom.left(),
        "a divider sets Custom apart: {ultra:?} {custom:?}"
    );
    assert!(near(low.top(), custom.top()));
    let scale = at(cx, &s, ids::GRAPHICS_PRESET_SCALE);
    assert!(scale.top() >= low.bottom(), "{scale:?}");
    assert!(
        near(scale.left(), low.left()) && near(scale.right(), ultra.right()),
        "{scale:?}"
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::graphics_preset(Preset::Medium)),
        Some(true)
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::graphics_preset(Preset::Custom)),
        Some(false)
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_PRESET_HEADING).as_deref(),
        Some("Preset")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_PRESET_LINE).as_deref(),
        Some("A balance of look and frame rate for most PCs")
    );
}

#[gpui_kit::test]
fn a_segment_applies_its_preset_and_the_line_follows(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let page = open(cx, &s);
    support::click_id(cx, s.win, ids::graphics_preset(Preset::Ultra));
    support::wait_real(cx, "the pick", |cx| {
        cx.update(|cx| page.read(cx).preset()) == Some(Preset::Ultra)
    });
    toasted(cx, &s, "Ultra applies the next time TF2 starts.");
    idle(cx, &page);
    let hook = std::fs::read_to_string(
        env.game
            .join("tf/custom/comfig-custom/cfg/app/setup_hook.cfg"),
    )
    .unwrap();
    assert_eq!(hook, "preset=ultra\n");
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_PRESET_LINE).as_deref(),
        Some("Everything on, for fast PCs and screenshots")
    );
    support::click_id(cx, s.win, ids::graphics_preset(Preset::Ultra));
    assert!(
        !cx.update(|cx| page.read(cx).is_busy()),
        "the checked segment changes nothing"
    );
}

#[gpui_kit::test]
fn on_a_narrow_page_custom_wraps_and_every_segment_shows_whole(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let _page = open(cx, &s);
    let ultra = at(cx, &s, ids::graphics_preset(Preset::Ultra));
    let custom = at(cx, &s, ids::graphics_preset(Preset::Custom));
    assert!(
        custom.top() >= ultra.bottom(),
        "1200 px is narrow: {ultra:?} {custom:?}"
    );
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    support::settle(cx, s.win);
    let area = at(
        cx,
        &s,
        ids::page(tf2tools_app::shell::route::Page::Graphics),
    );
    for preset in Preset::PICKER {
        let b = at(cx, &s, ids::graphics_preset(preset));
        assert!(
            b.left() >= area.left() && b.right() <= area.right() && b.size.width > px(0.),
            "{preset:?} {b:?} {area:?}"
        );
    }
    let ultra = at(cx, &s, ids::graphics_preset(Preset::Ultra));
    let custom = at(cx, &s, ids::graphics_preset(Preset::Custom));
    assert!(
        custom.top() >= ultra.bottom(),
        "Custom wraps under the ranked four: {ultra:?} {custom:?}"
    );
}
