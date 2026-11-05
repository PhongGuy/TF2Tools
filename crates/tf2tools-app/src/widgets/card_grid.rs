//! v5's responsive card grids: a virtualized grid of equal-height cards for long stores, and
//! plain rows that stretch to their tallest card for short lists.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::TestSupportExt as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Pixels, SharedString, UniformListScrollHandle, Window, canvas, div, px,
    uniform_list,
};

use crate::ids;
use crate::widgets::scroll::scroll_beside;

/// v5's `gap-4` between cards and between rows.
pub const GRID_GAP: Pixels = px(16.);

/// The shell around the grid at full sidebar: `SIDEBAR_WIDTH + 2 * CONTENT_PADDING`, spelled out
/// because `Pixels` arithmetic is not const; tests/card_grid.rs pins the sum.
pub const SHELL_CHROME_WIDTH: Pixels = px(352.);

/// Columns for a grid `width` wide: v5's sm / lg viewport breakpoints (640 / 1024 px) less
/// the shell chrome, so 1 below 288 px, 2 below 672 px, else 3.
pub fn columns_for(width: Pixels) -> usize {
    let viewport = width + SHELL_CHROME_WIDTH;
    if viewport < px(640.) {
        1
    } else if viewport < px(1024.) {
        2
    } else {
        3
    }
}

/// v5's store breakpoints less the shell chrome: 1 below sm (640), 2 below lg (1024), 3 below
/// xl (1280), else 4; at most `max`.
pub fn columns_for_store(width: Pixels, max: usize) -> usize {
    let viewport = width + SHELL_CHROME_WIDTH;
    let columns = if viewport < px(640.) {
        1
    } else if viewport < px(1024.) {
        2
    } else if viewport < px(1280.) {
        3
    } else {
        4
    };
    columns.min(max.max(1))
}

// One whole-pixel cell of `columns` across `width`, gaps between them.
fn cell_width(width: Pixels, columns: usize) -> Pixels {
    ((width - GRID_GAP * (columns - 1) as f32) / columns as f32).floor()
}

/// v5's CSS grid for short lists, not virtualized: `columns` equal cells per row, each row as
/// tall as its tallest card, a short last row padded with empty cells.
pub fn stretch_rows(cards: Vec<AnyElement>, columns: usize) -> AnyElement {
    let columns = columns.max(1);
    let mut cards = cards.into_iter().peekable();
    let mut rows = Vec::new();
    while cards.peek().is_some() {
        let row: Vec<_> = cards.by_ref().take(columns).collect();
        let padding = columns - row.len();
        rows.push(
            h_flex()
                .items_stretch()
                .gap(GRID_GAP)
                .children(row.into_iter().map(|card| cell().child(card)))
                .children((0..padding).map(|_| cell())),
        );
    }
    v_flex().gap(GRID_GAP).children(rows).into_any_element()
}

// One equal column of a row. Cards sit inside cells rather than being the flex items, so a
// card's padding and border never make it wider than an empty padding cell.
fn cell() -> gpui_kit::Div {
    div().flex().flex_1().flex_basis(px(0.)).min_w_0()
}

/// A grid of equal-height cards laid into rows by its own measured width; only visible rows are built.
pub struct CardGrid {
    id: SharedString,
    scroll: UniformListScrollHandle,
    width: Rc<Cell<Pixels>>,
    /// `Some(max)` lays out on the store breakpoints, capped at `max` columns.
    store_max: Option<usize>,
    card_height: Rc<Cell<Pixels>>,
    /// The row range of uniform_list's last range call this frame, as `(start, end)`.
    rendered: Rc<Cell<(usize, usize)>>,
    hooks: Rc<Hooks>,
}

type VisibleHook = Rc<dyn Fn(Range<usize>, &mut App)>;
type NearEndHook = Rc<dyn Fn(&mut App)>;

#[derive(Default)]
struct Hooks {
    visible: RefCell<Option<VisibleHook>>,
    near_end: RefCell<Option<NearEndHook>>,
    /// The `(card range, count)` the visible hook last fired for.
    fired_visible: Cell<Option<(usize, usize, usize)>>,
    /// The count the near-end hook last fired for.
    fired_near_end: Cell<Option<usize>>,
}

impl CardGrid {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            scroll: UniformListScrollHandle::new(),
            // Before the first paint, assume the narrowest three-column width.
            width: Rc::new(Cell::new(px(1024.) - SHELL_CHROME_WIDTH)),
            store_max: None,
            card_height: Rc::new(Cell::new(px(0.))),
            rendered: Rc::new(Cell::new((0, 0))),
            hooks: Rc::new(Hooks::default()),
        }
    }

    /// A grid on the store breakpoints (up to 4 columns), capped at `max_columns`.
    pub fn store(id: impl Into<SharedString>, max_columns: usize) -> Self {
        Self {
            store_max: Some(max_columns),
            ..Self::new(id)
        }
    }

    /// Columns at the last measured width (3 before the first paint).
    pub fn columns(&self) -> usize {
        match self.store_max {
            Some(max) => columns_for_store(self.width.get(), max),
            None => columns_for(self.width.get()),
        }
    }

    pub fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.scroll
    }

    /// One row's height, a card of `height(cell width)` plus the gap, at the last measured
    /// width; known before any card is drawn, so a page can floor the grid at one row.
    pub fn row_height_for(&self, height: impl Fn(Pixels) -> Pixels) -> Pixels {
        height(cell_width(self.width.get(), self.columns())) + GRID_GAP
    }

    /// The height of one row (last rendered card height plus the gap), or None before a frame
    /// has laid out a card.
    pub fn row_height(&self) -> Option<Pixels> {
        let card = self.card_height.get();
        (card > px(0.)).then(|| card + GRID_GAP)
    }

    /// Card indices of the rendered rows, when they or the count change. Replaces the hook,
    /// which fires for the current range on the next frame.
    pub fn on_visible(&mut self, f: impl Fn(Range<usize>, &mut App) + 'static) {
        *self.hooks.visible.borrow_mut() = Some(Rc::new(f));
        self.hooks.fired_visible.set(None);
    }

    /// Once per count, when the last rendered row is within two rows of the end. Replaces the
    /// hook, which fires on the next frame if the grid is near its end.
    pub fn on_near_end(&mut self, f: impl Fn(&mut App) + 'static) {
        *self.hooks.near_end.borrow_mut() = Some(Rc::new(f));
        self.hooks.fired_near_end.set(None);
    }

    /// Forgets what both hooks last fired for, so a reset that lands on the same range and
    /// count fires them again.
    pub fn rearm(&mut self) {
        self.hooks.fired_visible.set(None);
        self.hooks.fired_near_end.set(None);
    }

    /// `count` cards of `card_height`, each built by `card(index, window, cx)`. Fills its parent.
    pub fn render(
        &self,
        count: usize,
        card_height: Pixels,
        card: impl Fn(usize, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> AnyElement {
        self.render_sized(count, move |_| card_height, card)
    }

    /// Like `render`, with the card height computed from the cell width each frame.
    pub fn render_sized(
        &self,
        count: usize,
        height: impl Fn(Pixels) -> Pixels + 'static,
        card: impl Fn(usize, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> AnyElement {
        // The canvas records the grid's painted width; a change asks for another frame,
        // since a refresh requested while drawing is dropped.
        let measured = self.width.clone();
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

        // Each list item is one row of `columns` cells of one whole-pixel width, since flex
        // cells snap to unequal pixels; a short last row pads with empty cells. Rows keep the
        // column count `rows` was built from, so a breakpoint frame never reshuffles cards.
        let width = self.width.clone();
        let card_height = self.card_height.clone();
        let rendered = self.rendered.clone();
        let columns = self.columns();
        let rows = count.div_ceil(columns);
        let list = uniform_list(self.id.clone(), rows, move |range, window, cx| {
            rendered.set((range.start, range.end));
            let width = width.get();
            let cell_width = cell_width(width, columns);
            let cell_height = height(cell_width);
            card_height.set(cell_height);
            range
                .map(|row| {
                    h_flex()
                        .id(("grid-row", row))
                        .w_full()
                        .gap(GRID_GAP)
                        .pb(GRID_GAP)
                        .children((row * columns..(row + 1) * columns).map(|ix| {
                            let cell = div().flex_none().w(cell_width).h(cell_height);
                            if ix < count {
                                cell.child(card(ix, window, cx))
                            } else {
                                cell
                            }
                        }))
                })
                .collect()
        })
        .track_scroll(&self.scroll)
        .test_support()
        .size_full();

        let grid = div()
            .relative()
            .size_full()
            .child(watch)
            .child(list)
            .child(self.hook_canvas(count, columns, rows));
        scroll_beside(ids::grid_scrollbar(&self.id), &self.scroll, grid)
            .size_full()
            .into_any_element()
    }

    /// A canvas after the list: by its prepaint the list has made its measure calls (`0..1`)
    /// and then its real one, so the stored range is the rendered rows. Hooks run deferred,
    /// since a notify raised during prepaint is dropped.
    fn hook_canvas(&self, count: usize, columns: usize, rows: usize) -> impl IntoElement {
        let rendered = self.rendered.clone();
        let hooks = self.hooks.clone();
        canvas(
            move |_, _, cx| {
                let (row_start, row_end) = rendered.get();
                let row_end = row_end.min(rows);
                let row_start = row_start.min(row_end);
                let cards = (row_start * columns, (row_end * columns).min(count));
                if let Some(f) = hooks.visible.borrow().clone()
                    && hooks.fired_visible.get() != Some((cards.0, cards.1, count))
                {
                    hooks.fired_visible.set(Some((cards.0, cards.1, count)));
                    cx.defer(move |cx| f(cards.0..cards.1, cx));
                }
                let near_end = row_end > 0 && rows - row_end <= 2;
                if near_end
                    && let Some(f) = hooks.near_end.borrow().clone()
                    && hooks.fired_near_end.get() != Some(count)
                {
                    hooks.fired_near_end.set(Some(count));
                    cx.defer(move |cx| f(cx));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }
}
