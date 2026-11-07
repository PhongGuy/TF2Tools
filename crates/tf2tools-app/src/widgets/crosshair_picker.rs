//! v5's crosshair picker: a search row with the selection's swatch over one virtualized list
//! of the builtin group then the library group. The profile wizard hosts the list; the
//! Crosshairs page hosts its gallery mode, a virtualized grid of large tiles under source chips,
//! which leaves the swatch to the page's large preview.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, ImageSource,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _,
    UniformListScrollHandle, WeakEntity, Window, canvas, div, img, px, rgb, uniform_list,
};

use tf2tools_core::{BuiltinCrosshairs, CrosshairEntry, CrosshairFormat, CrosshairRef};

use crate::ids;
use crate::previews::{CrosshairPreviews, Preview};
use crate::services::Services;
use crate::widgets::labelled;
use crate::widgets::scroll::{BAR_WIDTH, scroll_area};

/// Every picker row, header or option, is this tall; `uniform_list` sizes all rows by the first.
const ROW_HEIGHT: Pixels = px(28.);

/// A gallery tile's width and height, the gap between tiles, and its drawing's black square.
const TILE_WIDTH: Pixels = px(88.);
const TILE_HEIGHT: Pixels = px(106.);
const TILE_GAP: Pixels = px(8.);
const TILE_SQUARE: Pixels = px(80.);

/// Which group the gallery shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    All,
    Builtin,
    Library,
}

/// What a gallery tile carries while dragged: the crosshair, and the ghost drawn under the
/// pointer.
#[derive(Clone)]
pub struct DraggedCrosshair {
    pub crosshair: CrosshairRef,
    image: Option<ImageSource>,
}

impl Render for DraggedCrosshair {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size(px(56.))
            .rounded(cx.theme().radius)
            .border_2()
            .border_color(cx.theme().primary)
            .bg(rgb(0x000000))
            .flex()
            .items_center()
            .justify_center()
            .children(self.image.clone().map(|i| img(i).size(px(48.))))
    }
}

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
    /// The Crosshairs page's grid of tiles in place of the list.
    gallery: bool,
    source: Source,
    /// The gallery grid's painted width, which sets its columns.
    width: Rc<Cell<Pixels>>,
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
        let builtins = Arc::clone(&cx.global::<Services>().builtins);
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
            gallery: false,
            source: Source::All,
            width: Rc::new(Cell::new(px(300.))),
            disabled: false,
            previews,
            search,
            list: UniformListScrollHandle::new(),
            _previews: observe_previews,
            _search: search_changed,
        }
    }

    /// The Crosshairs page's gallery: source chips over a grid of large tiles that can be
    /// dragged onto the loadout board.
    pub fn gallery(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            gallery: true,
            ..Self::new(false, window, cx)
        }
    }

    /// Shows only one group, or both.
    pub fn set_source(&mut self, source: Source, cx: &mut Context<Self>) {
        self.source = source;
        cx.notify();
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
        let builtins = Arc::clone(&self.builtins);
        let library = Rc::clone(&self.library);
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
                        option(
                            id,
                            name,
                            crosshair,
                            selected.as_ref(),
                            &previews,
                            &picker,
                            cx,
                        )
                    }
                    Row::Library(i) => {
                        let c = &library[i];
                        let crosshair = CrosshairRef::Library(c.id().clone());
                        let id = ids::crosshair_library(c.folder_name());
                        let name = c.name().to_owned();
                        option(
                            id,
                            name,
                            crosshair,
                            selected.as_ref(),
                            &previews,
                            &picker,
                            cx,
                        )
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

impl CrosshairPicker {
    // The gallery's matching tiles for `query`, the library group first since those are the
    // player's own, then the counts of each group before the source chip narrows them.
    fn tiles(&self, query: &str) -> (Vec<Row>, usize, usize) {
        let library: Vec<Row> = (0..self.library_lowered.len())
            .filter(|&ix| self.library_lowered[ix].contains(query))
            .map(Row::Library)
            .collect();
        let builtins: Vec<Row> = (0..self.builtins_lowered.len())
            .filter(|&ix| self.builtins_lowered[ix].contains(query))
            .map(Row::Builtin)
            .collect();
        let counts = (builtins.len(), library.len());
        let tiles = match self.source {
            Source::All => library.into_iter().chain(builtins).collect(),
            Source::Builtin => builtins,
            Source::Library => library,
        };
        (tiles, counts.0, counts.1)
    }

    // A source chip: the active one filled, hidden when its group has no match.
    fn chip(
        &self,
        id: &'static str,
        label: String,
        source: Source,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let active = self.source == source;
        let (bg, fg) = if active {
            (theme.secondary, theme.foreground)
        } else {
            (gpui_kit::transparent_black(), theme.muted_foreground)
        };
        div()
            .id(id)
            .aria_label(SharedString::from(label.clone()))
            .test_support()
            .px_2()
            .h(px(24.))
            .flex()
            .items_center()
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .text_xs()
            .bg(bg)
            .text_color(fg)
            .cursor_pointer()
            .hover(|chip| chip.text_color(theme.foreground))
            .on_click(cx.listener(move |picker, _, _, cx| picker.set_source(source, cx)))
            .child(label)
    }

    // The gallery: the search row, the source chips, then the virtualized grid filling the
    // rest, each list item one row of as many tiles as the measured width holds.
    fn render_gallery(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (border, muted, radius) = (theme.border, theme.muted_foreground, theme.radius);
        let query = self.search.read(cx).value().to_lowercase();
        let (tiles, builtins, library) = self.tiles(&query);
        let mut chips = vec![
            self.chip(ids::CROSSHAIRS_GROUP_ALL, "All".into(), Source::All, cx)
                .into_any_element(),
        ];
        if builtins > 0 {
            let label = format!("Built-in ({builtins})");
            chips.push(
                self.chip(ids::CROSSHAIRS_GROUP_BUILTIN, label, Source::Builtin, cx)
                    .into_any_element(),
            );
        }
        if library > 0 {
            let label = format!("My Crosshairs ({library})");
            chips.push(
                self.chip(ids::CROSSHAIRS_GROUP_LIBRARY, label, Source::Library, cx)
                    .into_any_element(),
            );
        }
        let body = if tiles.is_empty() {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(labelled(
                    div().text_sm().text_color(muted),
                    ids::CROSSHAIRS_NO_MATCH,
                    "No crosshairs match your search",
                ))
                .into_any_element()
        } else {
            self.grid(tiles.into(), cx)
        };
        v_flex()
            .size_full()
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
                    .child(self.search_box(muted)),
            )
            .child(h_flex().flex_wrap().gap_1().px_2().pt_2().children(chips))
            .child(body)
            .into_any_element()
    }

    // The tile grid. The canvas records the painted width; a change asks for another frame,
    // since a refresh requested while drawing is dropped.
    fn grid(&self, tiles: Rc<[Row]>, cx: &mut Context<Self>) -> AnyElement {
        let measured = Rc::clone(&self.width);
        let watch = canvas(
            move |bounds, window, _| {
                if measured.get() != bounds.size.width {
                    measured.set(bounds.size.width);
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let usable = self.width.get() - BAR_WIDTH - TILE_GAP;
        let columns = (((usable + TILE_GAP) / (TILE_WIDTH + TILE_GAP)).floor() as usize).max(1);
        let rows = tiles.len().div_ceil(columns);
        let picker = cx.entity().downgrade();
        let builtins = Arc::clone(&self.builtins);
        let library = Rc::clone(&self.library);
        let selected = self.selected.clone();
        let previews = self.previews.clone();
        let list = uniform_list(ids::CROSSHAIRS_LIST, rows, move |range, _, cx| {
            range
                .map(|row| {
                    let start = row * columns;
                    let end = (start + columns).min(tiles.len());
                    let cells = tiles[start..end].iter().map(|tile| {
                        let (id, name, crosshair) = match *tile {
                            Row::Library(i) => {
                                let c = &library[i];
                                (
                                    ids::crosshair_library(c.folder_name()),
                                    c.name().to_owned(),
                                    CrosshairRef::Library(c.id().clone()),
                                )
                            }
                            Row::Builtin(i) => {
                                let b = &builtins.all()[i];
                                (
                                    ids::crosshair_builtin(b.name()),
                                    b.display_name().to_owned(),
                                    CrosshairRef::Builtin(b.name().to_owned()),
                                )
                            }
                            _ => unreachable!("the gallery holds no headers"),
                        };
                        tile_element(
                            id,
                            name,
                            crosshair,
                            selected.as_ref(),
                            &previews,
                            &picker,
                            cx,
                        )
                    });
                    h_flex()
                        .id(("crosshair-tile-row", row))
                        .gap(TILE_GAP)
                        .pb(TILE_GAP)
                        .children(cells)
                        .into_any_element()
                })
                .collect()
        })
        .track_scroll(&self.list)
        .test_support()
        .size_full()
        .p(TILE_GAP)
        .pr(BAR_WIDTH);
        let grid = div().relative().size_full().child(watch).child(list);
        div()
            .flex_1()
            .min_h_0()
            .child(scroll_area(ids::CROSSHAIRS_LIST_SCROLLBAR, &self.list, grid).size_full())
            .into_any_element()
    }

    // The search input, inert while disabled.
    fn search_box(&self, muted: gpui_kit::Hsla) -> impl IntoElement {
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
            )
    }

    // The selection's swatch: its preview on black, labelled with its name.
    fn swatch(&self, size: Pixels, cx: &App) -> Option<impl IntoElement + use<>> {
        let selected = self.selected.as_ref()?;
        let name = self.name_of(selected)?;
        let preview = self.previews.read(cx).preview(selected, cx);
        Some(
            div()
                .id(ids::CROSSHAIRS_SWATCH)
                .aria_label(name)
                .test_support()
                .flex_none()
                .size(size)
                .rounded(cx.theme().radius)
                .bg(rgb(0x000000))
                .flex()
                .items_center()
                .justify_center()
                .children(preview_image(preview, size - px(8.))),
        )
    }
}

impl Render for CrosshairPicker {
    // v5's picker box: the search row with the selection's swatch, then the grouped list, or
    // the muted no-match line when the search hides every option.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.gallery {
            return self.render_gallery(cx);
        }
        let theme = cx.theme();
        let (border, muted, radius) = (theme.border, theme.muted_foreground, theme.radius);
        let query = self.search.read(cx).value().to_lowercase();
        let rows = self.rows(&query);
        let swatch_size = if self.compact { px(28.) } else { px(36.) };
        let swatch = self.swatch(swatch_size, cx);
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
                    .child(self.search_box(muted))
                    .children(swatch),
            )
            .child(list)
            .into_any_element()
    }
}

/// How big the game draws a crosshair at the default crosshair scale: the weapon script's 64
/// texels, in screen pixels at any resolution. Both previews show crosshairs at this size.
pub const IN_GAME_SIZE: Pixels = px(64.);

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
    selected: Option<&CrosshairRef>,
    previews: &Entity<CrosshairPreviews>,
    picker: &WeakEntity<CrosshairPicker>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let is_selected = selected == Some(&crosshair);
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

// A gallery tile: the crosshair at its in-game size on a black square, ringed in the theme's
// primary when selected, and its name under it. A click selects it; a drag carries it to the
// loadout board.
fn tile_element(
    id: gpui_kit::ElementId,
    name: String,
    crosshair: CrosshairRef,
    selected: Option<&CrosshairRef>,
    previews: &Entity<CrosshairPreviews>,
    picker: &WeakEntity<CrosshairPicker>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let is_selected = selected == Some(&crosshair);
    let preview = previews.read(cx).preview(&crosshair, cx);
    let image = match &preview {
        Preview::Ready(source) => Some(source.clone()),
        _ => None,
    };
    let ring = if is_selected {
        theme.primary
    } else {
        theme.border
    };
    let (hover, fg, muted) = (theme.ring, theme.foreground, theme.muted_foreground);
    let picker = picker.clone();
    let name: SharedString = name.into();
    let dragged = DraggedCrosshair {
        crosshair: crosshair.clone(),
        image,
    };
    v_flex()
        .id(id)
        .aria_label(name.clone())
        .test_support()
        .flex_none()
        .w(TILE_WIDTH)
        .h(TILE_HEIGHT)
        .items_center()
        .gap_1()
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            let _ = picker.update(cx, |picker, cx| {
                if !picker.disabled {
                    picker.select(Some(crosshair.clone()), cx)
                }
            });
        })
        .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
        .child(
            div()
                .flex_none()
                .size(TILE_SQUARE)
                .rounded(theme.radius)
                .border_2()
                .border_color(ring)
                .bg(rgb(0x000000))
                .flex()
                .items_center()
                .justify_center()
                .when(!is_selected, |square| {
                    square.hover(|square| square.border_color(hover))
                })
                .children(preview_image(preview, IN_GAME_SIZE)),
        )
        .child(
            div()
                .w_full()
                .text_center()
                .truncate()
                .text_xs()
                .text_color(if is_selected { fg } else { muted })
                .child(name),
        )
        .into_any_element()
}
