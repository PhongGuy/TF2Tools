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

/// Chinese UI text, regular and bold, shapes through the theme's fallbacks: no glyph is .notdef
/// and every Han glyph comes from the first listed family installed. Skips where none is (CI).
#[test]
fn chinese_ui_text_draws_through_the_first_installed_fallback() {
    use std::borrow::Cow;
    use std::sync::Arc;

    use gpui_kit::{
        Font, FontId, FontWeight, PlatformTextSystem as _, SharedString, TextRun, TextSystem,
        WindowTextSystem, font, px,
    };
    use gpui_wgpu::CosmicTextSystem;
    use tf2tools_app::theme::{FONTS, UI_FONT_FALLBACKS, ui_font};

    // A separate probe finds the family, so the shaping system loads no CJK face early.
    let probe = CosmicTextSystem::new("Inter");
    let Some(expected) = UI_FONT_FALLBACKS
        .iter()
        .copied()
        .find(|family| probe.font_id(&font(*family)).is_ok())
    else {
        eprintln!("skipped: none of {UI_FONT_FALLBACKS:?} is installed");
        return;
    };
    let text = Arc::new(TextSystem::new(Arc::new(CosmicTextSystem::new("Inter"))));
    let fonts = FONTS.iter().map(|f| Cow::Borrowed(*f)).collect();
    text.add_fonts(fonts).expect("the bundled fonts load");
    let shaper = WindowTextSystem::new(Arc::clone(&text));
    let lines = [
        "设置",
        "安装 HUD",
        "命中音效和击杀音效",
        "侦察兵的主武器准星",
        "下载完成：1.3万 次（已安装）。",
        "“预设”已保存…",
    ];
    let weights = [FontWeight::NORMAL, FontWeight::BOLD];
    let mut han = Vec::new();
    for weight in weights {
        for line in lines {
            let run = TextRun {
                len: line.len(),
                font: Font {
                    weight,
                    ..ui_font("Inter")
                },
                ..Default::default()
            };
            let shaped = shaper.shape_line(SharedString::from(line), px(14.), &[run], None);
            for r in &shaped.runs {
                for g in &r.glyphs {
                    let ch = line[g.index..].chars().next().unwrap();
                    assert_ne!(g.id.0, 0, "{line:?} {weight:?}: {ch:?} is .notdef");
                    if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
                        han.push((line, weight, ch, r.font_id));
                    }
                }
            }
        }
    }
    assert!(!han.is_empty());
    // Resolved only now: the chain's faces share these ids, a last-resort pick would not.
    let listed: Vec<FontId> = weights
        .iter()
        .map(|w| {
            text.resolve_font(&Font {
                weight: *w,
                ..font(expected)
            })
        })
        .collect();
    for (line, weight, ch, id) in han {
        assert!(
            listed.contains(&id),
            "{line:?} {weight:?}: {ch:?} drew from {id:?}, not {expected:?} {listed:?}"
        );
    }
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

// GPUI hides an element's hover style, so this reads the store card builders' source: each must
// take its hover fill from `card_hover`, not the translucent accent that matches the resting card.
#[test]
fn every_store_card_lights_up_with_the_card_hover_fill() {
    let builders = [
        ("gb_card.rs", include_str!("../src/gb_card.rs")),
        (
            "sound_store/card.rs",
            include_str!("../src/pages/sound_store/card.rs"),
        ),
        (
            "hud_store/curated_tab.rs",
            include_str!("../src/pages/hud_store/curated_tab.rs"),
        ),
    ];
    for (name, source) in builders {
        assert!(
            source.contains("card_hover(theme)") && !source.contains("accent.opacity(0.5)"),
            "{name} must hover with theme::card_hover"
        );
    }
}

// A checked segment must read at a glance against the page and its unchecked neighbours, with
// its own text clearly legible on it.
#[gpui_kit::test]
fn a_selected_segment_stands_out_from_the_page(cx: &mut TestAppContext) {
    support::init(cx);
    cx.update(|cx| {
        let t = Theme::global(cx);
        let (fill, text) = tf2tools_app::theme::selected_segment(t);
        assert!(
            (fill.l - t.background.l).abs() >= 0.5,
            "a selected segment fills {} on a {} page",
            hex(fill),
            hex(t.background)
        );
        assert!(
            (text.l - fill.l).abs() >= 0.5,
            "{} text on a {} segment",
            hex(text),
            hex(fill)
        );
    });
}

// The Graphics rows' segments take their checked look from the pill the preset shares.
#[test]
fn graphics_segments_use_the_shared_pill_look() {
    let source = include_str!("../src/pages/graphics/settings.rs");
    assert!(source.contains("pill::segment_look(") && source.contains("pill::row_track("));
}
