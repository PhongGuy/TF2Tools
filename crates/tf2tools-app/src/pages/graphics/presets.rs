//! The preset segmented control: Low to Ultra over a frames-to-looks scale, then Custom apart.

use gpui_kit::component::button::Toggle;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, TestSupportExt as _, div, px,
};
use tf2tools_core::Preset;

use super::GraphicsPage;
use super::model::{preset_label, preset_line};
use crate::i18n::tr;
use crate::ids;
use crate::theme::selected_segment;
use crate::widgets::labelled;

// The ranked presets in the order the scale reads, fewest effects first.
const RANKED: [Preset; 4] = [Preset::Low, Preset::Medium, Preset::High, Preset::Ultra];

impl GraphicsPage {
    // The heading and the current preset's line, then the ranked four with the scale under
    // them and Custom past a divider; on a narrow page Custom wraps under the four instead.
    pub(super) fn presets(
        &self,
        current: Preset,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (border, muted, radius) = (theme.border, theme.muted_foreground, theme.radius);
        let head = h_flex()
            .flex_wrap()
            .items_baseline()
            .gap_x_3()
            .gap_y_1()
            .child(labelled(
                div().text_lg().font_weight(FontWeight::SEMIBOLD),
                ids::GRAPHICS_PRESET_HEADING,
                tr!("graphics.preset.heading"),
            ))
            .child(labelled(
                div().text_sm().text_color(muted),
                ids::GRAPHICS_PRESET_LINE,
                preset_line(current),
            ));
        let group = |segments: Vec<Toggle>| {
            h_flex()
                .flex_none()
                .border_1()
                .border_color(border)
                .rounded(radius)
                .overflow_hidden()
                .children(segments)
        };
        let ranked = RANKED
            .into_iter()
            .enumerate()
            .map(|(i, preset)| self.segment(preset, current, i > 0, cx))
            .collect();
        let scale = h_flex()
            .id(ids::GRAPHICS_PRESET_SCALE)
            .test_support()
            .gap_2()
            .text_xs()
            .text_color(muted)
            .child(tr!("graphics.preset.more_frames"))
            .child(div().flex_1().h(px(1.)).bg(border))
            .child(tr!("graphics.preset.better_looking"));
        let column = v_flex()
            .flex_none()
            .gap_1()
            .child(group(ranked))
            .child(scale);
        let custom = group(vec![self.segment(Preset::Custom, current, false, cx)]);
        // Wide, a vertical rule as tall as the segments sets Custom apart; narrow, the wrap does.
        let frame = div()
            .id(ids::GRAPHICS_PRESETS)
            .test_support()
            .flex()
            .when(narrow, |f| f.flex_col().items_start().gap_3())
            .when(!narrow, |f| f.flex_row().items_start())
            .child(column)
            .when(!narrow, |f| {
                f.child(div().flex_none().w(px(1.)).h_7().mx_3().bg(border))
            })
            .child(custom);
        v_flex().gap_3().child(head).child(frame).into_any_element()
    }

    // One preset segment, checked when it is the current preset. Picking the checked one
    // again does nothing; any other runs the preset pick, a tab stop for Enter and Space.
    fn segment(
        &self,
        preset: Preset,
        current: Preset,
        edge: bool,
        cx: &mut Context<Self>,
    ) -> Toggle {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let (fill, on_fill) = selected_segment(theme);
        let checked = preset == current;
        let entity = cx.entity().downgrade();
        Toggle::new(ids::graphics_preset(preset))
            .small()
            .label(preset_label(preset))
            .tooltip(preset_label(preset))
            .checked(checked)
            .disabled(self.busy)
            .rounded_none()
            .px_3()
            .flex_shrink_0()
            .text_color(muted)
            .when(checked, |t| t.bg(fill).text_color(on_fill))
            .when(edge, |t| t.border_l_1().border_color(border))
            .on_click(move |&next, window, cx| {
                if !next {
                    return;
                }
                let _ = entity.update(cx, |page, cx| page.pick(preset, window, cx));
            })
    }
}
