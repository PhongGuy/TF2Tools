//! The preset card: Low to Ultra in one pill over a frames-to-looks scale, then Custom apart.

use gpui_kit::component::button::Toggle;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, TestSupportExt as _, div, linear_color_stop, linear_gradient, px,
};
use tf2tools_core::Preset;

use super::GraphicsPage;
use super::model::{preset_label, preset_line};
use super::pill;
use crate::i18n::tr;
use crate::ids;
use crate::pages::common::card_frame;
use crate::widgets::labelled;

// The ranked presets in the order the scale reads, fewest effects first.
const RANKED: [Preset; 4] = [Preset::Low, Preset::Medium, Preset::High, Preset::Ultra];

impl GraphicsPage {
    // A card holding the heading and the current preset's line, then the ranked four as one
    // pill over the frames-to-looks scale, with Custom in its own pill beside it; on a narrow
    // page Custom wraps under the four instead.
    pub(super) fn presets(
        &self,
        current: Preset,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (border, muted, accent) = (theme.border, theme.muted_foreground, theme.sidebar_primary);
        let head = h_flex()
            .flex_wrap()
            .items_baseline()
            .gap_x_3()
            .gap_y_1()
            .child(labelled(
                div().text_sm().font_weight(FontWeight::SEMIBOLD),
                ids::GRAPHICS_PRESET_HEADING,
                tr!("graphics.preset.heading"),
            ))
            .child(labelled(
                div().text_sm().text_color(muted),
                ids::GRAPHICS_PRESET_LINE,
                preset_line(current),
            ));
        let ranked = pill::track(cx).w_full().children(
            RANKED
                .into_iter()
                .map(|preset| self.segment(preset, current, cx).flex_1().min_w_0()),
        );
        // The rule warms from the border grey to the accent as the presets grow richer.
        let scale = h_flex()
            .id(ids::GRAPHICS_PRESET_SCALE)
            .test_support()
            .gap(px(10.))
            .text_xs()
            .text_color(muted)
            .child(tr!("graphics.preset.more_frames"))
            .child(div().flex_1().h(px(1.)).bg(linear_gradient(
                90.,
                linear_color_stop(border, 0.),
                linear_color_stop(accent, 1.),
            )))
            .child(tr!("graphics.preset.better_looking"));
        let column = v_flex()
            .flex_1()
            .min_w_0()
            .when(narrow, |c| c.w_full())
            .gap(px(10.))
            .child(ranked)
            .child(scale);
        let custom = pill::track(cx)
            .flex_none()
            .child(self.segment(Preset::Custom, current, cx).px_3());
        let row = div()
            .flex()
            .when(narrow, |f| f.flex_col())
            .items_start()
            .gap_3()
            .child(column)
            .child(custom);
        card_frame(cx)
            .id(ids::GRAPHICS_PRESETS)
            .test_support()
            .flex()
            .flex_col()
            .p_4()
            .gap(px(10.))
            .child(head)
            .child(row)
            .into_any_element()
    }

    // One preset segment, checked when it is the current preset. Picking the checked one
    // again does nothing; any other runs the preset pick, a tab stop for Enter and Space.
    fn segment(&self, preset: Preset, current: Preset, cx: &mut Context<Self>) -> Toggle {
        let checked = preset == current;
        let entity = cx.entity().downgrade();
        let toggle = Toggle::new(ids::graphics_preset(preset))
            .label(preset_label(preset))
            .tooltip(preset_label(preset))
            .checked(checked)
            .disabled(self.busy)
            .h(px(32.))
            .flex_shrink_0()
            .on_click(move |&next, window, cx| {
                if !next {
                    return;
                }
                let _ = entity.update(cx, |page, cx| page.pick(preset, window, cx));
            });
        pill::segment_look(toggle, checked, cx)
    }
}
