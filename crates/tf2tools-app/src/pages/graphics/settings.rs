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
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    div, px, rgb,
};
use tf2tools_core::ModuleChoice;

use super::model::{SettingsNote, preset_label, settings_note_text};
use super::view::{self, ControlKind, EmptyState, RowView, SectionView};
use super::{GraphicsPage, picture_panel, pictures, pill};
use crate::i18n::tr;
use crate::ids;
use crate::links::MASTERCOMFIG_DOCS_URL;
use crate::pages::common::card_frame;
use crate::widgets::labelled;

/// Pushes the settings area onto the scroll column's `children`: the narrow rail's chips, the
/// search toolbar (or the note while there is no catalog), one section per category with shown
/// rows, then the empty state. On a wide page the list is indented and the rail hangs off the
/// toolbar into that column, `pin` below its top. Records each section's child index in
/// `items` for the rail, and returns the toolbar's.
pub(super) fn push(
    page: &GraphicsPage,
    sections: Option<&[SectionView]>,
    narrow: bool,
    pin: Pixels,
    children: &mut Vec<AnyElement>,
    items: &mut BTreeMap<String, usize>,
    cx: &mut Context<GraphicsPage>,
) -> Option<usize> {
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
        return None;
    };
    let indent = if narrow {
        children.push(rail(sections, true, cx));
        px(0.)
    } else {
        px(RAIL_WIDTH + RAIL_GUTTER)
    };
    let toolbar_item = children.len();
    let side = (!narrow).then(|| {
        div()
            .absolute()
            .left(-indent)
            .top(pin)
            .child(rail(sections, false, cx))
    });
    children.push(toolbar(page, indent, side, cx));
    for section in sections.iter().filter(|s| !s.rows.is_empty()) {
        items.insert(section.key.clone(), children.len());
        children.push(section_view(page, section, narrow, indent, cx));
    }
    if let Some(empty) = empty(page, sections, indent, cx) {
        children.push(empty);
    }
    Some(toolbar_item)
}

// The search box and the "Changed only" switch over the list, carrying the wide rail.
fn toolbar(
    page: &GraphicsPage,
    indent: Pixels,
    rail: Option<impl IntoElement>,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .id(ids::GRAPHICS_SETTINGS)
        .test_support()
        .relative()
        .ml(indent)
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
        .children(rail)
        .into_any_element()
}

// A category's card: a muted header bar over its shown rows, which run edge to edge.
fn section_view(
    page: &GraphicsPage,
    section: &SectionView,
    narrow: bool,
    indent: Pixels,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
    let rows: Vec<AnyElement> = section
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| row(page, r.clone(), i == 0, narrow, cx))
        .collect();
    let theme = cx.theme();
    let (border, muted) = (theme.border, theme.muted_foreground);
    card_frame(cx)
        .id(ids::graphics_section(&section.key))
        .test_support()
        .ml(indent)
        .flex_none()
        .flex()
        .flex_col()
        // Clipping drops a flex item's content height as its floor, so the card keeps it
        // explicitly rather than shrink to fit the scroll column.
        .overflow_hidden()
        .child(labelled(
            div()
                .px_4()
                .py_3()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(muted)
                .border_b_1()
                .border_color(border),
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
    indent: Pixels,
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
            .ml(indent)
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
            .gap_1()
            .children(entries)
            .into_any_element()
    }
}

/// The wide rail's entry width, enough for the longest category name beside its count.
const RAIL_WIDTH: f32 = 184.;

/// The space between the wide rail and the list it sits beside.
const RAIL_GUTTER: f32 = 24.;

/// The left bar and tint that mark a row the player changed from the preset.
const CHANGED: u32 = 0x1447e6;

// The title, description and Learn more, plus the preset's value when changed, on the left;
// Reset when changed and the row's control on the right. A module with pictures adds its
// thumbnail before the text and Show pictures under it, and the open row's panel goes below
// the line. Every row but the card's `first` rules itself off from the one above.
fn row(
    page: &GraphicsPage,
    row: RowView,
    first: bool,
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
    let learn_more = h_flex().child(
        Button::new(ids::graphics_learn_more(name))
            .link()
            .small()
            .label(tr!("graphics.row.learn_more"))
            .on_click(|_, _, cx| cx.open_url(MASTERCOMFIG_DOCS_URL)),
    );
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
    let pictures = page
        .pictures
        .as_deref()
        .and_then(|index| pictures::row_pictures(index, &row, false));
    let thumb = pictures
        .as_ref()
        .map(|p| picture_panel::thumbnail(page, name, &row.label, p, cx));
    let toggle = pictures.as_ref().map(|_| {
        h_flex()
            .ml(px(-8.))
            .child(picture_panel::toggle(page, name, cx))
    });
    let border = cx.theme().border;
    // The open panel compares the crops while zoom is on, the same pictures the page wants;
    // the thumbnail and strip stay on the full pictures.
    let panel = page
        .pictures
        .as_deref()
        .filter(|_| page.open_row() == Some(name))
        .and_then(|index| pictures::row_pictures(index, &row, page.zoom()))
        .map(|p| picture_panel::panel(page, name, &p, cx));
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
        .child(learn_more)
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
    // A changed row is tinted with a 3 px accent bar down its left edge, drawn as an overlay
    // so the row's single border colour stays free for the rule above it.
    let bar = row.changed.then(|| {
        div()
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .w(px(3.))
            .bg(tint)
    });
    v_flex()
        .id(ids::graphics_row(name))
        .test_support()
        .relative()
        .px_4()
        .py_3()
        .when(!first, |this| this.border_t_1().border_color(border))
        .when(row.changed, |this| this.bg(tint.opacity(0.07)))
        .children(bar)
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

// One standalone Toggle per value in a single dark pill, so each is a tab stop that Enter and
// Space click. A click on the checked segment reports false and changes nothing.
fn segments(page: &GraphicsPage, row: &RowView, cx: &mut Context<GraphicsPage>) -> AnyElement {
    let toggles = row.options.iter().filter_map(|option| {
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
            .h(px(26.))
            .px(px(10.))
            .text_size(px(13.))
            .on_click(move |&next, _, cx| {
                if !next {
                    return;
                }
                let choice = choice.clone();
                let _ = entity.update(cx, |page, cx| {
                    page.change(|c, catalog| c.set(catalog, &module, choice), cx)
                });
            });
        Some(pill::segment_look(toggle, checked, cx))
    });
    pill::row_track(cx)
        .id(ids::graphics_segments(&row.module))
        .test_support()
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
