//! Every element plans B and C add carries its id on a seeded page.

use crate::support;

use gpui_kit::{ElementId, TestAppContext};
use support::graphics::{picture_page, toggle_pictures};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_core::Preset;

#[gpui_kit::test]
fn every_plan_b_and_c_id_is_on_a_seeded_page(cx: &mut TestAppContext) {
    let env = support::env();
    cx.update(|cx| Tf2Probe::install(cx, || true));
    let (s, page) = picture_page(cx, &env, "medium", r#"{"lod":"high"}"#);
    support::wait_real(cx, "the running answer", |cx| {
        cx.update(|cx| page.read(cx).tf2_running())
    });
    toggle_pictures(cx, &s, "lod");
    let mut drawn: Vec<ElementId> = [
        ids::GRAPHICS_STATUS,
        ids::GRAPHICS_HEADLINE,
        ids::GRAPHICS_DETAIL,
        ids::GRAPHICS_CHECK,
        ids::GRAPHICS_REMOVE,
        ids::GRAPHICS_DOCS,
        ids::GRAPHICS_RUNNING,
        ids::GRAPHICS_PRESETS,
        ids::GRAPHICS_PRESET_HEADING,
        ids::GRAPHICS_PRESET_LINE,
        ids::GRAPHICS_PRESET_SCALE,
    ]
    .map(ElementId::from)
    .to_vec();
    drawn.extend(Preset::PICKER.map(ids::graphics_preset));
    drawn.extend([
        ids::graphics_thumb("lod"),
        ids::graphics_pictures_toggle("lod"),
        ids::graphics_pictures("lod"),
        ids::graphics_compare("lod"),
        ids::graphics_compare_clip("lod"),
        ids::graphics_compare_yours("lod"),
        ids::graphics_compare_slider("lod"),
        ids::graphics_compare_pill("lod", "preset"),
        ids::graphics_compare_pill("lod", "yours"),
        ids::graphics_zoom("lod"),
        ids::graphics_strip("lod"),
        ids::graphics_strip_tag("lod"),
        ids::graphics_strip_caption("lod"),
    ]);
    drawn.extend(["low", "medium", "high", "ultra"].map(|v| ids::graphics_strip_picture("lod", v)));
    let missing: Vec<&ElementId> = drawn
        .iter()
        .filter(|id| !support::present(cx, s.win, (*id).clone()))
        .collect();
    assert!(missing.is_empty(), "not drawn: {missing:?}");
}
