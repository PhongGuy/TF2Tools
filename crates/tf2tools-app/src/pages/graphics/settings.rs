//! The Graphics page's settings list: one row per mastercomfig module, grouped by category,
//! or a note in its place while there is no catalog.

use gpui_kit::component::button::{Button, Toggle};
use gpui_kit::component::select::Select;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Styled as _, TestSupportExt as _, div, px, rgb, transparent_black,
};
use tf2tools_core::{ModuleCatalog, ModuleChoice};

use super::GraphicsPage;
use super::model::{SettingsNote, settings_note_text};
use super::view::{self, ControlKind, Filter, RowView};
use crate::i18n::tr;
use crate::ids;
use crate::widgets::labelled;

/// The section: the list, or the note while there is no catalog.
pub(super) fn section(page: &GraphicsPage, cx: &mut Context<GraphicsPage>) -> AnyElement {
    let body = match &page.catalog {
        Some(catalog) => list(page, catalog, cx),
        None => labelled(
            div().text_sm().text_color(cx.theme().muted_foreground),
            ids::GRAPHICS_SETTINGS_NOTE,
            settings_note_text(page.settings_note().unwrap_or(SettingsNote::Loading)),
        )
        .into_any_element(),
    };
    v_flex()
        .id(ids::GRAPHICS_SETTINGS)
        .test_support()
        .child(body)
        .into_any_element()
}

// Each category's heading over its rows, in the catalog's order, drawn from the view-model.
fn list(
    page: &GraphicsPage,
    catalog: &ModuleCatalog,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
    let sections = view::sections(catalog, &page.shown_against(), &Filter::default());
    let groups: Vec<AnyElement> = sections
        .into_iter()
        .map(|section| {
            let rows: Vec<AnyElement> =
                section.rows.into_iter().map(|r| row(page, r, cx)).collect();
            v_flex()
                .gap_2()
                .child(labelled(
                    div().font_weight(FontWeight::SEMIBOLD),
                    ids::graphics_category(&section.key),
                    section.label,
                ))
                .children(rows)
                .into_any_element()
        })
        .collect();
    v_flex().gap_6().children(groups).into_any_element()
}

/// The left bar and tint that mark a row the player changed from the preset.
const CHANGED: u32 = 0x1447e6;

// The title and description, plus the preset's value when changed, on the left; Reset when
// changed and the row's control on the right.
fn row(page: &GraphicsPage, row: RowView, cx: &mut Context<GraphicsPage>) -> AnyElement {
    let name = row.module.as_str();
    let muted = cx.theme().muted_foreground;
    let description = row.description.clone().map(|text| {
        labelled(
            div().text_xs().text_color(muted),
            ids::graphics_module_description(name),
            text,
        )
    });
    let preset = row.changed.then(|| {
        labelled(
            div().text_xs().text_color(muted),
            ids::graphics_module_preset(name),
            row.preset_text.clone(),
        )
    });
    let reset = row.changed.then(|| {
        let module = name.to_owned();
        Button::new(ids::graphics_module_reset(name))
            .outline()
            .small()
            .label(tr!("graphics.row.reset"))
            .disabled(page.busy)
            .on_click(cx.listener(move |page, _, _, cx| page.reset_module(&module, cx)))
    });
    let control = match row.kind {
        ControlKind::Switch => switch(page, &row, cx),
        ControlKind::Segmented => segments(page, &row, cx),
        ControlKind::Select => select(page, &row),
    };
    let tint = Hsla::from(rgb(CHANGED));
    h_flex()
        .id(ids::graphics_row(name))
        .test_support()
        .justify_between()
        .gap_4()
        .pl_3()
        .border_l(px(3.))
        .border_color(transparent_black())
        .when(row.changed, |this| {
            this.border_color(tint).bg(tint.opacity(0.06))
        })
        .child(
            v_flex()
                .flex_1()
                .gap_0p5()
                .child(labelled(
                    div().text_sm(),
                    ids::graphics_module(name),
                    row.label.clone(),
                ))
                .children(description)
                .children(preset),
        )
        .child(h_flex().gap_2().children(reset).child(control))
        .into_any_element()
}

// An off/on module's Switch; a flip saves on or off.
fn switch(page: &GraphicsPage, row: &RowView, cx: &mut Context<GraphicsPage>) -> AnyElement {
    let module = row.module.clone();
    let entity = cx.entity().downgrade();
    Switch::new(ids::graphics_switch(&row.module))
        .small()
        .checked(row.shown == ModuleChoice::Value("on".to_owned()))
        .disabled(page.busy)
        .accessibility_label(row.label.clone())
        .on_click(move |&on, _, cx| {
            let value = if on { "on" } else { "off" };
            let choice = ModuleChoice::Value(value.to_owned());
            let _ = entity.update(cx, |page, cx| {
                page.change(|c, catalog| c.set(catalog, &module, choice), cx)
            });
        })
        .into_any_element()
}

// One standalone Toggle per value in a single frame, so each is a tab stop that Enter and
// Space click. A click on the checked segment reports false and changes nothing.
fn segments(page: &GraphicsPage, row: &RowView, cx: &mut Context<GraphicsPage>) -> AnyElement {
    let border = cx.theme().border;
    let toggles = row.options.iter().enumerate().map(|(i, option)| {
        let ModuleChoice::Value(value) = &option.choice else {
            unreachable!("segments offer only values")
        };
        let module = row.module.clone();
        let choice = option.choice.clone();
        let entity = cx.entity().downgrade();
        Toggle::new(ids::graphics_segment(&row.module, value))
            .small()
            .label(option.label.clone())
            .tooltip(option.label.clone())
            .checked(row.shown == option.choice)
            .disabled(page.busy)
            .rounded_none()
            .px_3()
            .when(i > 0, |t| t.border_l_1().border_color(border))
            .on_click(move |&next, _, cx| {
                if !next {
                    return;
                }
                let choice = choice.clone();
                let _ = entity.update(cx, |page, cx| {
                    page.change(|c, catalog| c.set(catalog, &module, choice), cx)
                });
            })
    });
    h_flex()
        .id(ids::graphics_segments(&row.module))
        .test_support()
        .border_1()
        .border_color(border)
        .rounded(cx.theme().radius)
        .overflow_hidden()
        .children(toggles)
        .into_any_element()
}

// The row's kit Select, built with the catalog; empty until it exists.
fn select(page: &GraphicsPage, row: &RowView) -> AnyElement {
    let name = row.module.as_str();
    let Some(select) = page.selects.get(name) else {
        return div().into_any_element();
    };
    div()
        .id(ids::graphics_module_select(name))
        .test_support()
        .w(px(200.))
        .child(
            Select::new(select)
                .small()
                .disabled(page.busy)
                .accessibility_label(row.label.clone()),
        )
        .into_any_element()
}
