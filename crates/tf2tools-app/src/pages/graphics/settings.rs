//! The Graphics page's settings list: one row per mastercomfig module, grouped by category,
//! or a note in its place while there is no catalog.

use gpui_kit::component::button::Button;
use gpui_kit::component::select::Select;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, TestSupportExt as _, div, px,
};
use tf2tools_core::{ComfigModule, ModuleCatalog};

use super::GraphicsPage;
use super::model::{SettingsNote, settings_note_text};
use crate::ids;
use crate::widgets::labelled;

/// The section: its heading with Reset all, then the list or the note.
pub(super) fn section(page: &GraphicsPage, cx: &mut Context<GraphicsPage>) -> AnyElement {
    let nothing_saved = page.choices.as_ref().is_none_or(|c| c.modules().is_empty());
    let header = h_flex()
        .justify_between()
        .gap_4()
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Settings"),
        )
        .child(
            Button::new(ids::GRAPHICS_RESET_ALL)
                .outline()
                .label("Reset all to preset")
                .disabled(nothing_saved || page.busy)
                .on_click(cx.listener(|page, _, _, cx| page.reset_all(cx))),
        );
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
        .gap_3()
        .child(header)
        .child(body)
        .into_any_element()
}

// Each category's heading over its rows, in the catalog's order.
fn list(
    page: &GraphicsPage,
    catalog: &ModuleCatalog,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
    let groups: Vec<AnyElement> = catalog
        .categories()
        .iter()
        .map(|category| {
            let rows: Vec<AnyElement> = category
                .modules()
                .iter()
                .map(|module| row(page, module, cx))
                .collect();
            v_flex()
                .gap_2()
                .child(labelled(
                    div().font_weight(FontWeight::SEMIBOLD),
                    ids::graphics_category(category.key()),
                    category.label().to_owned(),
                ))
                .children(rows)
                .into_any_element()
        })
        .collect();
    v_flex().gap_6().children(groups).into_any_element()
}

// The module's title and description on the left; its Select, and Reset when saved, on the right.
fn row(page: &GraphicsPage, module: &ComfigModule, cx: &mut Context<GraphicsPage>) -> AnyElement {
    let name = module.name();
    let muted = cx.theme().muted_foreground;
    let description = module.description().map(|text| {
        labelled(
            div().text_xs().text_color(muted),
            ids::graphics_module_description(name),
            text.to_owned(),
        )
    });
    let select = page.selects.get(name).map(|select| {
        div()
            .id(ids::graphics_module_select(name))
            .test_support()
            .w(px(200.))
            .child(
                Select::new(select)
                    .small()
                    .disabled(page.busy)
                    .accessibility_label(module.label().to_owned()),
            )
    });
    let saved = page.choices.as_ref().is_some_and(|c| c.get(name).is_some());
    let reset = saved.then(|| {
        let module = name.to_owned();
        Button::new(ids::graphics_module_reset(name))
            .outline()
            .small()
            .label("Reset")
            .disabled(page.busy)
            .on_click(cx.listener(move |page, _, _, cx| page.reset_module(&module, cx)))
    });
    h_flex()
        .justify_between()
        .gap_4()
        .child(
            v_flex()
                .flex_1()
                .gap_0p5()
                .child(labelled(
                    div().text_sm(),
                    ids::graphics_module(name),
                    module.label().to_owned(),
                ))
                .children(description),
        )
        .child(h_flex().gap_2().children(reset).children(select))
        .into_any_element()
}
