//! The Graphics page's settings list: the search toolbar, one section per mastercomfig
//! category with a row per module, the empty state and the category rail; a note in their
//! place while there is no catalog.

use std::collections::BTreeMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::button::{Button, ButtonVariants as _, Toggle};
use gpui_kit::component::input::Input;
use gpui_kit::component::select::Select;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
    rgb, transparent_black,
};
use tf2tools_core::ModuleChoice;

use super::model::{SettingsNote, preset_label, settings_note_text};
use super::view::{self, ControlKind, EmptyState, RowView, SectionView};
use super::{GraphicsPage, picture_panel, pictures};
use crate::i18n::tr;
use crate::ids;
use crate::shell::CONTENT_PADDING;
use crate::theme::selected_segment;
use crate::widgets::labelled;

/// Pushes the settings area onto the scroll column's `children`: the narrow rail's chips, the
/// search toolbar (or the note while there is no catalog), one section per category with shown
/// rows, then the empty state. Records each section's child index in `items` for the rail.
pub(super) fn push(
    page: &GraphicsPage,
    sections: Option<&[SectionView]>,
    narrow: bool,
    children: &mut Vec<AnyElement>,
    items: &mut BTreeMap<String, usize>,
    cx: &mut Context<GraphicsPage>,
) {
    let Some(sections) = sections else {
        let note = settings_note_text(page.settings_note().unwrap_or(SettingsNote::Loading));
        children.push(
            v_flex()
                .id(ids::GRAPHICS_SETTINGS)
                .test_support()
                .child(labelled(
                    div().text_sm().text_color(cx.theme().muted_foreground),
                    ids::GRAPHICS_SETTINGS_NOTE,
                    note,
                ))
                .into_any_element(),
        );
        return;
    };
    if narrow {
        children.push(rail(sections, true, cx));
    }
    children.push(toolbar(page, cx));
    for section in sections.iter().filter(|s| !s.rows.is_empty()) {
        items.insert(section.key.clone(), children.len());
        children.push(section_view(page, section, narrow, cx));
    }
    if let Some(empty) = empty(page, sections, cx) {
        children.push(empty);
    }
}

// The search box and the "Changed only" switch over the list.
fn toolbar(page: &GraphicsPage, cx: &mut Context<GraphicsPage>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .id(ids::GRAPHICS_SETTINGS)
        .test_support()
        .gap_4()
        .child(
            div()
                .id(ids::GRAPHICS_SEARCH)
                .test_support()
                .flex_1()
                .min_w_0()
                .max_w(px(320.))
                .child(
                    Input::new(&page.search)
                        .h_9()
                        .prefix(Icon::new(IconName::Search).text_color(muted)),
                ),
        )
        .child(
            Switch::new(ids::GRAPHICS_CHANGED_ONLY)
                .checked(page.changed_only)
                .label(tr!("graphics.settings.changed_only"))
                .on_click(cx.listener(|page, on: &bool, _, cx| page.set_changed_only(*on, cx))),
        )
        .into_any_element()
}

// A category's heading over its shown rows.
fn section_view(
    page: &GraphicsPage,
    section: &SectionView,
    narrow: bool,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
    let rows: Vec<AnyElement> = section
        .rows
        .iter()
        .map(|r| row(page, r.clone(), narrow, cx))
        .collect();
    v_flex()
        .id(ids::graphics_section(&section.key))
        .test_support()
        .gap_2()
        .child(labelled(
            div().font_weight(FontWeight::SEMIBOLD),
            ids::graphics_category(&section.key),
            section.label.clone(),
        ))
        .children(rows)
        .into_any_element()
}

// Why no row shows, with "Show all settings". An empty catalogue with no query has nothing to
// blame, so it says nothing rather than quote an empty query.
fn empty(
    page: &GraphicsPage,
    sections: &[SectionView],
    cx: &mut Context<GraphicsPage>,
) -> Option<AnyElement> {
    let text = match view::empty_state(sections, &page.filter(cx))? {
        EmptyState::NoMatch(query) if query.is_empty() => return None,
        EmptyState::NoMatch(query) => tr!("graphics.settings.no_match", query = query),
        EmptyState::NothingChanged => tr!(
            "graphics.settings.nothing_changed",
            preset = preset_label(page.shown_against().preset())
        ),
    };
    let muted = cx.theme().muted_foreground;
    Some(
        v_flex()
            .items_start()
            .gap_3()
            .child(labelled(
                div().text_sm().text_color(muted),
                ids::GRAPHICS_EMPTY,
                text,
            ))
            .child(
                Button::new(ids::GRAPHICS_SHOW_ALL)
                    .outline()
                    .small()
                    .label(tr!("graphics.settings.show_all"))
                    .on_click(cx.listener(|page, _, window, cx| page.show_all(window, cx))),
            )
            .into_any_element(),
    )
}

/// The category rail: a column of entries beside the list, or a row of chips above it when
/// `narrow`. Each entry names its category, dots it while it has changes and counts its
/// modules; one whose section shows no rows is dimmed and does nothing.
pub(super) fn rail(
    sections: &[SectionView],
    narrow: bool,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let dot = Hsla::from(rgb(CHANGED));
    let entries = sections.iter().map(|section| {
        let key = section.key.clone();
        let mark = section.has_changes.then(|| {
            div()
                .id(ids::graphics_rail_dot(&section.key))
                .aria_label(tr!("graphics.settings.has_changes"))
                .test_support()
                .flex_none()
                .size(px(6.))
                .rounded_full()
                .bg(dot)
        });
        let button = Button::new(ids::graphics_rail(&section.key))
            .small()
            .accessibility_label(section.label.clone())
            .disabled(section.rows.is_empty())
            .on_click(cx.listener(move |page, _, _, cx| page.jump(&key, cx)))
            .child(
                h_flex()
                    .when(!narrow, |this| this.w_full())
                    .gap_2()
                    .child(div().flex_1().child(section.label.clone()))
                    .children(mark)
                    .child(labelled(
                        div().text_xs().text_color(muted),
                        ids::graphics_rail_count(&section.key),
                        section.count.to_string(),
                    )),
            );
        if narrow {
            button.outline()
        } else {
            button.ghost().w_full()
        }
    });
    if narrow {
        h_flex()
            .id(ids::GRAPHICS_RAIL)
            .test_support()
            .flex_wrap()
            .gap_2()
            .children(entries)
            .into_any_element()
    } else {
        v_flex()
            .id(ids::GRAPHICS_RAIL)
            .test_support()
            .flex_none()
            .w(px(RAIL_WIDTH))
            .pt(CONTENT_PADDING)
            .pl(CONTENT_PADDING)
            .gap_1()
            .children(entries)
            .into_any_element()
    }
}

/// The wide rail's entry width, enough for the longest category name beside its count.
const RAIL_WIDTH: f32 = 200.;

/// The left bar and tint that mark a row the player changed from the preset.
const CHANGED: u32 = 0x1447e6;

// The title and description, plus the preset's value when changed, on the left; Reset when
// changed and the row's control on the right. A module with pictures adds its thumbnail
// before the text and Show pictures under it, and the open row's panel goes below the line.
fn row(
    page: &GraphicsPage,
    row: RowView,
    narrow: bool,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
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
    let pictures = page.pictures.as_deref().and_then(|index| {
        let label = page.preset().map(preset_label).unwrap_or_default();
        pictures::row_pictures(index, &row, &label, false)
    });
    let thumb = pictures
        .as_ref()
        .map(|p| picture_panel::thumbnail(page, name, &row.label, p, cx));
    let toggle = pictures
        .as_ref()
        .map(|_| h_flex().child(picture_panel::toggle(page, name, cx)));
    let panel = pictures
        .as_ref()
        .filter(|_| page.open_row() == Some(name))
        .map(|p| picture_panel::panel(page, name, p, cx));
    let tint = Hsla::from(rgb(CHANGED));
    let text = v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(labelled(
            div().text_sm(),
            ids::graphics_module(name),
            row.label.clone(),
        ))
        .children(description)
        .children(preset)
        .children(toggle);
    let controls = h_flex().flex_none().gap_2().children(reset).child(control);
    // On a narrow page (the one without the rail) the thumbnail sits above the text and the
    // control under it, since a segmented control beside a wrapped description would not fit;
    // otherwise all three sit side by side.
    let line = if narrow {
        v_flex()
            .items_start()
            .gap_2()
            .children(thumb)
            .child(text)
            .child(controls)
    } else {
        h_flex()
            .justify_between()
            .gap_4()
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_start()
                    .gap_3()
                    .children(thumb)
                    .child(text),
            )
            .child(controls)
    };
    v_flex()
        .id(ids::graphics_row(name))
        .test_support()
        .pl_3()
        .border_l(px(3.))
        .border_color(transparent_black())
        .when(row.changed, |this| {
            this.border_color(tint).bg(tint.opacity(0.06))
        })
        .gap_3()
        .child(line)
        .children(panel)
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
    let muted = cx.theme().muted_foreground;
    let (fill, on_fill) = selected_segment(cx.theme());
    let toggles = row.options.iter().enumerate().filter_map(|(i, option)| {
        // Segments offer only values; anything else has no segment to draw.
        let ModuleChoice::Value(value) = &option.choice else {
            return None;
        };
        let module = row.module.clone();
        let choice = option.choice.clone();
        let entity = cx.entity().downgrade();
        let checked = row.shown == option.choice;
        let toggle = Toggle::new(ids::graphics_segment(&row.module, value))
            .small()
            .label(option.label.clone())
            .tooltip(option.label.clone())
            .checked(checked)
            .disabled(page.busy)
            .rounded_none()
            .px_3()
            .text_color(muted)
            .when(checked, |t| t.bg(fill).text_color(on_fill))
            .when(i > 0, |t| t.border_l_1().border_color(border))
            .on_click(move |&next, _, cx| {
                if !next {
                    return;
                }
                let choice = choice.clone();
                let _ = entity.update(cx, |page, cx| {
                    page.change(|c, catalog| c.set(catalog, &module, choice), cx)
                });
            });
        Some(toggle)
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
