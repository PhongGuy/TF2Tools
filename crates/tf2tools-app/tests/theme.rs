use gpui_kit::component::Theme;
use gpui_kit::{Anchor, Hsla, TestAppContext, px};

use crate::support;

/// `#rrggbbaa` of a resolved colour, so assertions read like the token map.
fn hex(c: Hsla) -> String {
    let c = c.to_rgb();
    let ch = |v: f32| (v * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        ch(c.r),
        ch(c.g),
        ch(c.b),
        ch(c.a)
    )
}

#[gpui_kit::test]
fn the_theme_is_v5_zinc_dark(cx: &mut TestAppContext) {
    support::init(cx);
    cx.update(|cx| {
        let t = Theme::global(cx);
        assert!(t.mode.is_dark());
        assert_eq!(hex(t.background), "#09090bff");
        assert_eq!(hex(t.foreground), "#fafafaff");
        assert_eq!(hex(t.sidebar), "#18181bff");
        assert_eq!(hex(t.group_box), "#18181bff");
        assert_eq!(hex(t.popover), "#18181bff");
        assert_eq!(hex(t.primary), "#e4e4e7ff");
        assert_eq!(hex(t.secondary), "#27272aff");
        assert_eq!(hex(t.muted_foreground), "#9f9fa9ff");
        assert_eq!(hex(t.danger), "#ff6467ff");
        // v5 sonner's dark warning toast text, hsl(46 87% 65%).
        assert_eq!(hex(t.warning), "#f3cf58ff");
        assert_eq!(hex(t.border), "#ffffff1a");
        assert_eq!(hex(t.input), "#ffffff26");
        assert_eq!(hex(t.ring), "#71717bff");
        assert_eq!(hex(t.sidebar_primary), "#1447e6ff");
    });
}

#[gpui_kit::test]
fn shape_and_toast_placement_match_v5(cx: &mut TestAppContext) {
    support::init(cx);
    cx.update(|cx| {
        let t = Theme::global(cx);
        assert_eq!(t.radius, px(10.));
        assert_eq!(t.radius_lg, px(14.));
        assert_eq!(t.notification.placement, Anchor::BottomRight);
    });
}

// v5's dialog overlay is `bg-black/50` and its destructive button `dark:bg-destructive/60`
// with white text; the kit defaults are a 20% overlay and a red-on-tint danger button.
#[gpui_kit::test]
fn dialog_overlay_and_destructive_button_match_v5(cx: &mut TestAppContext) {
    support::init(cx);
    cx.update(|cx| {
        let t = Theme::global(cx);
        assert_eq!(hex(t.overlay), "#00000080");
        assert_eq!(hex(t.button_danger), "#ff646799");
        assert_eq!(hex(t.button_danger_foreground), "#ffffffff");
    });
}

// v5's later `dark:bg-destructive/60` rule outranks `hover:bg-destructive/90` at equal
// specificity, and the variant has no active class: the fill never changes.
#[gpui_kit::test]
fn the_destructive_button_keeps_its_fill_on_hover_and_press(cx: &mut TestAppContext) {
    support::init(cx);
    cx.update(|cx| {
        let t = Theme::global(cx);
        assert_eq!(hex(t.button_danger_hover), "#ff646799");
        assert_eq!(hex(t.button_danger_active), "#ff646799");
    });
}

#[gpui_kit::test]
fn the_active_list_fill_is_opaque(cx: &mut TestAppContext) {
    support::init(cx);
    cx.update(|cx| assert_eq!(hex(Theme::global(cx).list_active), "#27272aff"));
}

#[gpui_kit::test]
fn the_ui_font_is_bundled_inter(cx: &mut TestAppContext) {
    support::init(cx);
    cx.update(|cx| assert_eq!(Theme::global(cx).font_family.as_ref(), "Inter"));
}

/// The headless platform's text system is a no-op, so resolve weights against the real
/// cosmic-text system holding only the bundled fonts: each weight must be its own face.
#[test]
fn bundled_inter_resolves_a_distinct_face_per_weight() {
    use std::borrow::Cow;
    use std::sync::Arc;

    use gpui_kit::{FontWeight, TextSystem, font};
    use gpui_wgpu::CosmicTextSystem;

    let text = TextSystem::new(Arc::new(CosmicTextSystem::new_without_system_fonts(
        "Inter",
    )));
    let fonts = tf2tools_app::theme::FONTS
        .iter()
        .map(|f| Cow::Borrowed(*f))
        .collect();
    text.add_fonts(fonts).expect("the bundled fonts load");
    assert_eq!(text.all_font_names(), ["Inter"]);

    let face = |weight| {
        let mut inter = font("Inter");
        inter.weight = weight;
        text.resolve_font(&inter)
    };
    let (regular, medium, semibold) = (
        face(FontWeight::NORMAL),
        face(FontWeight::MEDIUM),
        face(FontWeight::SEMIBOLD),
    );
    assert_ne!(regular, medium);
    assert_ne!(regular, semibold);
    assert_ne!(medium, semibold);
}

/// `usWeightClass` from a TrueType font's OS/2 table.
fn weight_class(ttf: &[u8]) -> u16 {
    let be16 = |at: usize| u16::from_be_bytes([ttf[at], ttf[at + 1]]);
    let be32 =
        |at: usize| u32::from_be_bytes([ttf[at], ttf[at + 1], ttf[at + 2], ttf[at + 3]]) as usize;
    let tables = be16(4) as usize;
    (0..tables)
        .map(|i| 12 + 16 * i)
        .find(|&record| &ttf[record..record + 4] == b"OS/2")
        .map(|record| be16(be32(record + 8) + 4))
        .expect("an OS/2 table")
}

#[test]
fn the_bundled_inter_faces_run_from_regular_to_bold() {
    let weights: Vec<u16> = tf2tools_app::theme::FONTS
        .iter()
        .map(|f| weight_class(f))
        .collect();
    assert_eq!(weights, vec![400, 500, 600, 700]);
}

// The hover fill is painted on the page background in place of the card's group-box fill, so
// what shows must be clearly lighter than the resting card.
#[gpui_kit::test]
fn a_hovered_card_is_visibly_lighter(cx: &mut TestAppContext) {
    support::init(cx);
    cx.update(|cx| {
        let t = Theme::global(cx);
        let shown = t.background.blend(tf2tools_app::theme::card_hover(t));
        assert!(
            shown.l >= t.group_box.l + 0.04,
            "hover shows {} over a {} card",
            hex(shown),
            hex(t.group_box)
        );
    });
}
