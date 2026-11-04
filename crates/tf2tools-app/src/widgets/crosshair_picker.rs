//! v5's crosshair picker: a search row with the selection's swatch over one virtualized list
//! of the builtin group then the library group. The Crosshairs page and the profile wizard
//! both host it.

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _,
    UniformListScrollHandle, WeakEntity, Window, div, img, px, rgb, uniform_list,
};

use tf2tools_core::{BuiltinCrosshairs, CrosshairEntry, CrosshairFormat, CrosshairRef};

use crate::ids;
use crate::previews::{CrosshairPreviews, Preview};
use crate::services::Services;
use crate::widgets::labelled;
use crate::widgets::scroll::{BAR_WIDTH, scroll_area};

/// Every picker row, header or option, is this tall; `uniform_list` sizes all rows by the first.
const ROW_HEIGHT: Pixels = px(28.);

/// One row of the picker list, indexing into the catalog or the library.
#[derive(Clone, Copy)]
enum Row {
    BuiltinHeader(usize),
    LibraryHeader(usize),
    Builtin(usize),
    Library(usize),
}

/// v5's crosshair picker box: a search row with the selection's swatch over one virtualized
/// list of the builtin group then the library group.
pub struct CrosshairPicker {
    builtins: Arc<BuiltinCrosshairs>,
    /// Each builtin's display name lowercased, once per picker since the catalog never changes.
    builtins_lowered: Vec<String>,
    library: Rc<[CrosshairEntry]>,
    /// Each library crosshair's name lowercased, cached per `set_library`.
    library_lowered: Vec<String>,
    selected: Option<CrosshairRef>,
    compact: bool,
    /// A host operation runs: option clicks and the search box are inert.
    disabled: bool,
    previews: Entity<CrosshairPreviews>,
    search: Entity<InputState>,
    list: UniformListScrollHandle,
    _previews: Subscription,
    _search: Subscription,
}

impl CrosshairPicker {
    /// Needs the `Services` global. Starts with an empty library and no selection. Compact is
    /// v5's dialog variant: a 128 px list (`h-32`) and a 28 px swatch (`size-7`); otherwise the
    /// page's 160 px list and 36 px swatch.
    pub fn new(compact: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Rows draw whatever the cache holds, so each landed decode batch redraws them.
        let previews = CrosshairPreviews::global(cx);
        let observe_previews = cx.observe(&previews, |_, _, cx| cx.notify());
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search crosshairs..."));
        let search_changed = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let builtins = cx.global::<Services>().builtins.clone();
        let builtins_lowered = builtins
            .all()
            .iter()
            .map(|b| b.display_name().to_lowercase())
            .collect();
        Self {
            builtins,
            builtins_lowered,
            library: Rc::from([]),
            library_lowered: Vec::new(),
            selected: None,
            compact,
            disabled: false,
            previews,
            search,
            list: UniformListScrollHandle::new(),
            _previews: observe_previews,
            _search: search_changed,
        }
    }

    /// Replaces the library group and its lowered names, keeps the selection, and requests a
    /// preview for every library crosshair.
    pub fn set_library(&mut self, library: Rc<[CrosshairEntry]>, cx: &mut Context<Self>) {
        self.library_lowered = library.iter().map(|c| c.name().to_lowercase()).collect();
        let refs: Vec<CrosshairRef> = library
            .iter()
            .map(|c| CrosshairRef::Library(c.id().clone()))
            .collect();
        self.library = library;
        self.previews
            .update(cx, |previews, cx| previews.request(refs, cx));
        cx.notify();
    }

    /// The crosshair picked in the list, if any.
    pub fn selected(&self) -> Option<&CrosshairRef> {
        self.selected.as_ref()
    }

    /// Sets or clears the selection and notifies, as clicking an option does.
    pub fn select(&mut self, crosshair: Option<CrosshairRef>, cx: &mut Context<Self>) {
        self.selected = crosshair;
        cx.notify();
    }

    /// While disabled, option clicks leave the selection alone and the search box is inert.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        cx.notify();
    }

    /// The picker's search box.
    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    /// The list's scroll handle; its offset is what the list drew.
    pub fn scroll(&self) -> ScrollHandle {
        self.list.0.borrow().base_handle.clone()
    }

    /// The catalog's display name for a builtin, the library's name otherwise.
    pub fn name_of(&self, crosshair: &CrosshairRef) -> Option<String> {
        match crosshair {
            CrosshairRef::Builtin(name) => {
                self.builtins.get(name).map(|b| b.display_name().to_owned())
            }
            CrosshairRef::Library(id) => self
                .library
                .iter()
                .find(|c| c.id() == id)
                .map(|c| c.name().to_owned()),
            _ => None,
        }
    }

    /// The library crosshair's format; None for builtins and unknown ids.
    pub fn format_of(&self, crosshair: &CrosshairRef) -> Option<CrosshairFormat> {
        let CrosshairRef::Library(id) = crosshair else {
            return None;
        };
        self.library
            .iter()
            .find(|c| c.id() == id)
            .map(|c| c.format())
    }

    fn list_height(&self) -> Pixels {
        if self.compact { px(128.) } else { px(160.) }
    }

    // The list's rows for `query`: each non-empty group's header, then its matching options.
    fn rows(&self, query: &str) -> Rc<[Row]> {
        let builtins: Vec<usize> = (0..self.builtins_lowered.len())
            .filter(|&ix| self.builtins_lowered[ix].contains(query))
            .collect();
        let library: Vec<usize> = (0..self.library_lowered.len())
            .filter(|&ix| self.library_lowered[ix].contains(query))
            .collect();
        let mut rows = Vec::with_capacity(builtins.len() + library.len() + 2);
        if !builtins.is_empty() {
            rows.push(Row::BuiltinHeader(builtins.len()));
            rows.extend(builtins.into_iter().map(Row::Builtin));
        }
        if !library.is_empty() {
            rows.push(Row::LibraryHeader(library.len()));
            rows.extend(library.into_iter().map(Row::Library));
        }
        rows.into()
    }

    // One `uniform_list` over every row, so only the visible ones are built. Clicks reach the
    // picker through a weak handle, so a click after it is gone or while it is disabled does nothing.
    fn list(&self, rows: Rc<[Row]>, cx: &mut Context<Self>) -> AnyElement {
        let picker = cx.entity().downgrade();
        let builtins = self.builtins.clone();
        let library = self.library.clone();
        let selected = self.selected.clone();
        let previews = self.previews.clone();
        // The right padding keeps the options clear of the scrollbar's track.
        let list = uniform_list(ids::CROSSHAIRS_LIST, rows.len(), move |range, _, cx| {
            range
                .map(|ix| match rows[ix] {
                    Row::BuiltinHeader(n) => {
                        header(ids::CROSSHAIRS_GROUP_BUILTIN, format!("Built-in ({n})"), cx)
                    }
                    Row::LibraryHeader(n) => header(
                        ids::CROSSHAIRS_GROUP_LIBRARY,
                        format!("My Crosshairs ({n})"),
                        cx,
                    ),
                    Row::Builtin(i) => {
                        let b = &builtins.all()[i];
                        let crosshair = CrosshairRef::Builtin(b.name().to_owned());
                        let id = ids::crosshair_builtin(b.name());
                        let name = b.display_name().to_owned();
                        option(id, name, crosshair, &selected, &previews, &picker, cx)
                    }
                    Row::Library(i) => {
                        let c = &library[i];
                        let crosshair = CrosshairRef::Library(c.id().clone());
                        let id = ids::crosshair_library(c.folder_name());
                        let name = c.name().to_owned();
                        option(id, name, crosshair, &selected, &previews, &picker, cx)
                    }
                })
                .collect()
        })
        .track_scroll(&self.list)
        .test_support()
        .h(self.list_height())
        .p_1()
        .pr(BAR_WIDTH);
        scroll_area(ids::CROSSHAIRS_LIST_SCROLLBAR, &self.list, list).into_any_element()
    }
}

impl Render for CrosshairPicker {
    // v5's picker box: the search row with the selection's swatch, then the grouped list, or
    // the muted no-match line when the search hides every option.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted, radius) = (theme.border, theme.muted_foreground, theme.radius);
        let query = self.search.read(cx).value().to_lowercase();
        let rows = self.rows(&query);
        let swatch_size = if self.compact { px(28.) } else { px(36.) };
        let swatch = self.selected.as_ref().and_then(|selected| {
            let name = self.name_of(selected)?;
            let preview = self.previews.read(cx).preview(selected, cx);
            Some(
                div()
                    .id(ids::CROSSHAIRS_SWATCH)
                    .aria_label(name)
                    .test_support()
                    .flex_none()
                    .size(swatch_size)
                    .rounded(radius)
                    .bg(rgb(0x000000))
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(preview_image(preview, swatch_size - px(8.))),
            )
        });
        let list = if rows.is_empty() {
            v_flex()
                .h(self.list_height())
                .items_center()
                .justify_center()
                .child(labelled(
                    div().text_sm().text_color(muted),
                    ids::CROSSHAIRS_NO_MATCH,
                    "No crosshairs match your search",
                ))
                .into_any_element()
        } else {
            self.list(rows, cx)
        };
        v_flex()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .overflow_hidden()
            .child(
                h_flex()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .id(ids::CROSSHAIRS_SEARCH)
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .child(
                                Input::new(&self.search)
                                    .appearance(false)
                                    .h_7()
                                    .disabled(self.disabled)
                                    .prefix(Icon::new(IconName::Search).text_color(muted)),
                            ),
                    )
                    .children(swatch),
            )
            .child(list)
    }
}

/// The drawn preview at `size`, or nothing while it is pending or missing.
pub(crate) fn preview_image(preview: Preview, size: Pixels) -> Option<impl IntoElement> {
    match preview {
        Preview::Ready(source) => Some(img(source).size(size)),
        _ => None,
    }
}

// A group header row, labelled with its text and count.
fn header(id: &'static str, text: String, cx: &App) -> AnyElement {
    labelled(
        div()
            .h(ROW_HEIGHT)
            .px_2()
            .flex()
            .items_center()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().muted_foreground),
        id,
        text,
    )
    .into_any_element()
}

// An option row: the preview in a small black circle, then the name; the selected one is
// drawn in the theme's primary colours.
fn option(
    id: gpui_kit::ElementId,
    name: String,
    crosshair: CrosshairRef,
    selected: &Option<CrosshairRef>,
    previews: &Entity<CrosshairPreviews>,
    picker: &WeakEntity<CrosshairPicker>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let is_selected = selected.as_ref() == Some(&crosshair);
    let preview = previews.read(cx).preview(&crosshair, cx);
    let (bg, fg) = if is_selected {
        (theme.primary, theme.primary_foreground)
    } else {
        (gpui_kit::transparent_black(), theme.foreground)
    };
    let hover = theme.accent;
    let picker = picker.clone();
    let name: SharedString = name.into();
    div()
        .id(id)
        .aria_label(name.clone())
        .test_support()
        .w_full()
        .h(ROW_HEIGHT)
        .px_2()
        .gap_2()
        .flex()
        .items_center()
        .rounded(theme.radius)
        .cursor_pointer()
        .bg(bg)
        .text_color(fg)
        .when(!is_selected, |row| row.hover(|row| row.bg(hover)))
        .on_click(move |_, _, cx| {
            let _ = picker.update(cx, |picker, cx| {
                if !picker.disabled {
                    picker.select(Some(crosshair.clone()), cx)
                }
            });
        })
        .child(
            div()
                .flex_none()
                .size(px(16.))
                .rounded_full()
                .bg(rgb(0x000000))
                .flex()
                .items_center()
                .justify_center()
                .children(preview_image(preview, px(12.))),
        )
        .child(div().min_w_0().truncate().text_sm().child(name))
        .into_any_element()
}
