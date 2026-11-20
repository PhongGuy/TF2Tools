//! A Graphics settings row and its control, drawn by `GraphicsChoicesEditor` under its id
//! scope; every control only proposes its edit.

use gpui_kit::component::button::{Button, ButtonVariants as _, Toggle};
use gpui_kit::component::select::Select;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, TestSupportExt as _, div, px, rgb,
};
use tf2tools_core::ModuleChoice;

use super::GraphicsChoicesEditor;
use super::view::{ControlKind, RowView};
use super::{picture_panel, pictures, pill};
use crate::i18n::tr;
use crate::ids;
use crate::links::MASTERCOMFIG_DOCS_URL;
use crate::widgets::labelled;

/// The left bar and tint that mark a row the player changed from the preset.
pub(super) const CHANGED: u32 = 0x1447e6;

impl GraphicsChoicesEditor {
    /// One settings row, with its thumbnail, Show pictures and open panel when the module has
    /// pictures; None without a catalog or for an unknown module. Every row but the card's
    /// `first` rules itself off from the one above.
    pub fn settings_row(
        &self,
        module: &str,
        first: bool,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let row = self.row(module)?;
        let scope = self.scope;
        let name = row.module.as_str();
        let muted = cx.theme().muted_foreground;
        let description = row.description.clone().map(|text| {
            labelled(
                div().text_xs().line_height(px(16.)).text_color(muted),
                scope.id(ids::graphics_module_description(name)),
                text,
            )
        });
        // The docs link reads as part of the description: its size and muted colour, underlined.
        let learn_more = h_flex().child(
            Button::new(scope.id(ids::graphics_learn_more(name)))
                .link()
                .xsmall()
                .text_color(muted)
                .label(tr!("graphics.row.learn_more"))
                .on_click(|_, _, cx| cx.open_url(MASTERCOMFIG_DOCS_URL)),
        );
        let preset = row.changed.then(|| {
            labelled(
                div().text_xs().text_color(muted),
                scope.id(ids::graphics_module_preset(name)),
                row.preset_text.clone(),
            )
        });
        let reset =
            row.changed.then(|| {
                let module = name.to_owned();
                Button::new(scope.id(ids::graphics_module_reset(name)))
                    .outline()
                    .small()
                    .label(tr!("graphics.row.reset"))
                    .disabled(self.disabled)
                    .on_click(cx.listener(move |editor, _, _, cx| {
                        editor.propose(|c, _| c.reset(&module), cx)
                    }))
            });
        let control = match row.kind {
            ControlKind::Switch => self.switch(&row, cx),
            ControlKind::Segmented => self.segments(&row, cx),
            ControlKind::Select => self.select(&row),
        };
        // The panel compares the crops while zoom is on, the same pictures the editor wants;
        // the thumbnail and strip stay on the full pictures.
        let index = self.pictures.as_deref();
        let pictures = index.and_then(|index| pictures::row_pictures(index, &row, false));
        let thumb = pictures
            .as_ref()
            .map(|p| picture_panel::thumbnail(self, name, &row.label, p, cx));
        let toggle = pictures.as_ref().map(|_| {
            h_flex()
                .ml(px(-8.))
                .child(picture_panel::toggle(self, name, cx))
        });
        let panel = index
            .filter(|_| self.open_row() == Some(name))
            .and_then(|index| pictures::row_pictures(index, &row, self.zoom()))
            .map(|p| picture_panel::panel(self, name, &p, cx));
        let border = cx.theme().border;
        let tint = Hsla::from(rgb(CHANGED));
        let text = v_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            // The name and description keep shadcn's 20 px and 16 px lines, so a row stays tight.
            .child(labelled(
                div().text_sm().line_height(px(20.)),
                scope.id(ids::graphics_module(name)),
                row.label.clone(),
            ))
            .children(description)
            .child(learn_more)
            .children(preset)
            .children(toggle);
        let controls = h_flex().flex_none().gap_2().children(reset).child(control);
        // The thumbnail always sits beside the text. On a narrow page (the one without the rail)
        // the control drops under them, since a segmented control beside a wrapped description
        // would not fit; otherwise it sits on the right.
        let lead = h_flex()
            .min_w_0()
            .items_start()
            .gap_3()
            .children(thumb)
            .child(text);
        let line = if narrow {
            v_flex()
                .items_start()
                .gap_2()
                .child(lead.w_full())
                .child(controls)
        } else {
            h_flex()
                .justify_between()
                .gap_4()
                .child(lead.flex_1())
                .child(controls)
        };
        // A changed row's line is tinted with a 3 px accent bar down its left edge, drawn as an
        // overlay so the row's single border colour stays free for the rule above it. The open
        // panel runs the card's full width under the line, outside the tint.
        let bar = row.changed.then(|| {
            div()
                .id(scope.id(ids::graphics_row_bar(name)))
                .test_support()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(3.))
                .bg(tint)
        });
        let line = div()
            .relative()
            .px_4()
            .py_3()
            .when(row.changed, |this| this.bg(tint.opacity(0.07)))
            .children(bar)
            .child(line);
        Some(
            v_flex()
                .id(scope.id(ids::graphics_row(name)))
                .test_support()
                .when(!first, |this| this.border_t_1().border_color(border))
                .child(line)
                .children(panel)
                .into_any_element(),
        )
    }

    // An off/on module's Switch; a flip proposes on or off.
    fn switch(&self, row: &RowView, cx: &mut Context<Self>) -> AnyElement {
        let module = row.module.clone();
        let entity = cx.entity().downgrade();
        Switch::new(self.scope.id(ids::graphics_switch(&row.module)))
            .small()
            .checked(row.shown == ModuleChoice::Value("on".to_owned()))
            .disabled(self.disabled)
            .accessibility_label(row.label.clone())
            .on_click(move |&on, _, cx| {
                let value = if on { "on" } else { "off" };
                let choice = ModuleChoice::Value(value.to_owned());
                let _ = entity.update(cx, |editor, cx| {
                    editor.propose(|c, catalog| c.set(catalog, &module, choice), cx)
                });
            })
            .into_any_element()
    }

    // One standalone Toggle per value in a single dark pill, so each is a tab stop that Enter
    // and Space click. A click on the checked segment reports false and proposes nothing.
    fn segments(&self, row: &RowView, cx: &mut Context<Self>) -> AnyElement {
        let toggles = row.options.iter().filter_map(|option| {
            // Segments offer only values; anything else has no segment to draw.
            let ModuleChoice::Value(value) = &option.choice else {
                return None;
            };
            let module = row.module.clone();
            let choice = option.choice.clone();
            let entity = cx.entity().downgrade();
            let checked = row.shown == option.choice;
            let toggle = Toggle::new(self.scope.id(ids::graphics_segment(&row.module, value)))
                .small()
                .label(option.label.clone())
                .tooltip(option.label.clone())
                .checked(checked)
                .disabled(self.disabled)
                .h(px(26.))
                .px(px(10.))
                .text_size(px(13.))
                .on_click(move |&next, _, cx| {
                    if !next {
                        return;
                    }
                    let choice = choice.clone();
                    let _ = entity.update(cx, |editor, cx| {
                        editor.propose(|c, catalog| c.set(catalog, &module, choice), cx)
                    });
                });
            Some(pill::segment_look(toggle, checked, cx))
        });
        pill::row_track(cx)
            .id(self.scope.id(ids::graphics_segments(&row.module)))
            .test_support()
            .children(toggles)
            .into_any_element()
    }

    // The row's kit Select, built with the catalog; empty until it exists.
    fn select(&self, row: &RowView) -> AnyElement {
        let name = row.module.as_str();
        let Some(select) = self.selects.get(name) else {
            return div().into_any_element();
        };
        div()
            .id(self.scope.id(ids::graphics_module_select(name)))
            .test_support()
            .w(px(200.))
            .child(
                Select::new(select)
                    .small()
                    .disabled(self.disabled)
                    .accessibility_label(row.label.clone()),
            )
            .into_any_element()
    }
}
