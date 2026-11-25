//! The Demos.tf page's one scrolling list: the toolbar and column titles as its first item,
//! one row per demo, then a footer that says whether more is loading, nothing was found or
//! the page failed.

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, IntoElement, ListState, ParentElement as _, Pixels,
    SharedString, Styled as _, WeakEntity, Window, div, list, px,
};

use super::text::{demo_title, format_label};
use super::{DemoRow, DemosTfPage};
use crate::i18n::tr;
use crate::ids;
use crate::pages::logs_tf::list::{
    DATE, GAP, LENGTH, MAP, MAX_WIDTH, centred, date_cell, map_cell,
};
use crate::pages::logs_tf::text::duration_text;
use crate::store_grid::feed::FeedItem as _;
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_beside;
use crate::widgets::store_error::error_line;

// Column widths beyond the ones shared with logs.tf's rows; WATCH keeps room for the row's
// Watch button.
const FORMAT: Pixels = px(88.);
const WATCH: Pixels = px(96.);
const TITLE_MIN: Pixels = px(120.);
// The row's horizontal padding, both sides.
const PADDING: Pixels = px(24.);

/// How the rows fit the list's width: a narrow list moves format and map under the title,
/// then the date too; title, length and Watch always keep their columns.
#[derive(Clone, Copy)]
struct Fit {
    side_columns: bool,
    date_column: bool,
}

fn fit(list: &ListState) -> Fit {
    let width = list.viewport_bounds().size.width.min(MAX_WIDTH);
    // Before the first frame the list has no width yet, so it lays out in full.
    let unknown = width <= px(0.);
    let dated = DATE + TITLE_MIN + LENGTH + WATCH + GAP * 3. + PADDING;
    Fit {
        side_columns: unknown || width >= dated + FORMAT + MAP + GAP * 2.,
        date_column: unknown || width >= dated,
    }
}

/// The whole list over `page`'s list state: item 0 is the toolbar and column titles, item
/// `n` the demo at `rows[n - 1]`, and the last item the footer. The caller sizes it.
pub(super) fn page_list(page: &DemosTfPage, cx: &mut Context<DemosTfPage>) -> Div {
    let this = cx.entity().downgrade();
    let fit = fit(&page.list);
    let rows = div().size_full().child(
        list(page.list.clone(), move |ix, _window, cx| {
            let Some(page) = this.upgrade() else {
                return div().into_any_element();
            };
            let item = if ix == 0 {
                // The Open demo button takes a listener, so the top builds inside the page.
                page.update(cx, |_, cx| top(fit, cx))
            } else {
                let page = page.read(cx);
                match page.feed.items().get(ix - 1) {
                    Some(row) => demo_row(row, fit, cx),
                    None => {
                        // Building the footer means it is near the view, so ask for the next
                        // page after this layout; a notify during layout would be dropped.
                        let wants = page.wants_page();
                        let footer = footer(page, this.clone(), cx);
                        if wants {
                            let this = this.clone();
                            cx.defer(move |cx| {
                                let _ = this.update(cx, |page, cx| page.next_page(cx));
                            });
                        }
                        footer
                    }
                }
            };
            centred(item)
        })
        .size_full(),
    );
    scroll_beside(ids::DEMOS_TF_LIST_SCROLLBAR, &page.list, rows)
}

// Open demo at the top right, then the column titles in the rows' widths.
fn top(fit: Fit, cx: &mut Context<DemosTfPage>) -> AnyElement {
    let open = Button::new(ids::DEMOS_TF_OPEN)
        .outline()
        .small()
        .label(tr!("demos_tf.open"))
        .on_click(cx.listener(|page, _, window, cx| page.open_demo(window, cx)));
    v_flex()
        .w_full()
        .gap_2()
        .child(h_flex().w_full().justify_end().child(open))
        .child(column_header(fit, cx))
        .into_any_element()
}

fn column_header(fit: Fit, cx: &App) -> impl IntoElement {
    let cell = |w: Pixels, text: SharedString| div().w(w).flex_none().child(text);
    h_flex()
        .gap(GAP)
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().muted_foreground)
        .when(fit.date_column, |el| {
            el.child(cell(DATE, tr!("demos_tf.column.date")))
        })
        .child(
            div()
                .flex_1()
                .min_w(TITLE_MIN)
                .child(tr!("demos_tf.column.title")),
        )
        .when(fit.side_columns, |el| {
            el.child(cell(FORMAT, tr!("demos_tf.column.format")))
                .child(cell(MAP, tr!("demos_tf.column.map")))
        })
        .child(cell(LENGTH, tr!("demos_tf.column.length")).text_right())
        .child(div().w(WATCH).flex_none())
}

// One demo: date, title, format, map and length, with whatever no longer fits beside the
// title moved under it as one muted line.
fn demo_row(row: &DemoRow, fit: Fit, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let demo = &row.demo;
    let id = row.feed_id();
    let title = demo_title(demo.server(), demo.red(), demo.blue());
    let format = format_label(demo.format());
    let under = (!fit.side_columns).then(|| {
        h_flex()
            .gap_2()
            .min_w_0()
            .text_xs()
            .text_color(theme.muted_foreground)
            .when(!fit.date_column, |el| {
                el.child(date_cell(div(), ids::demos_tf_row_date(id), demo.time()))
            })
            .child(labelled(
                div().flex_none().whitespace_nowrap(),
                ids::demos_tf_row_format(id),
                format.clone(),
            ))
            .child(map_cell(
                div().min_w_0(),
                ids::demos_tf_row_map(id),
                demo.map(),
            ))
    });
    h_flex()
        .w_full()
        .gap(GAP)
        .px_3()
        .py_2()
        .text_sm()
        .border_b_1()
        .border_color(theme.border)
        .when(fit.date_column, |el| {
            el.child(date_cell(
                div().w(DATE).flex_none(),
                ids::demos_tf_row_date(id),
                demo.time(),
            ))
        })
        .child(
            v_flex()
                .flex_1()
                .min_w(TITLE_MIN)
                .child(labelled(
                    div().truncate(),
                    ids::demos_tf_row_title(id),
                    title,
                ))
                .children(under),
        )
        .when(fit.side_columns, |el| {
            el.child(labelled(
                div().w(FORMAT).flex_none().truncate(),
                ids::demos_tf_row_format(id),
                format,
            ))
            .child(map_cell(
                div().w(MAP).flex_none(),
                ids::demos_tf_row_map(id),
                demo.map(),
            ))
        })
        .child(labelled(
            div().w(LENGTH).flex_none().text_right(),
            ids::demos_tf_row_duration(id),
            duration_text(demo.duration()),
        ))
        .child(div().w(WATCH).flex_none())
        .into_any_element()
}

// What follows the rows: the failed page's error with Retry, a loading line, or the empty
// range's note once demos.tf has nothing for the query.
fn footer(page: &DemosTfPage, this: WeakEntity<DemosTfPage>, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let line = || div().py_4().px_3().text_sm().text_color(muted);
    if let Some(error) = &page.error {
        let retry = move |_: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |page, cx| page.retry(cx));
        };
        return div()
            .py_4()
            .px_3()
            .child(error_line(
                ids::DEMOS_TF_ERROR,
                ids::DEMOS_TF_RETRY,
                error.clone(),
                retry,
                cx,
            ))
            .into_any_element();
    }
    // A footer about to page already says so: a footer drawn empty measures zero height at
    // the list's bottom edge, and the list would not draw it again to show what lands.
    if page.feed.is_loading() || page.wants_page() {
        return labelled(line(), ids::DEMOS_TF_LOADING, tr!("demos_tf.loading")).into_any_element();
    }
    if page.feed.is_end() && page.feed.items().is_empty() {
        return labelled(line(), ids::DEMOS_TF_EMPTY, tr!("demos_tf.empty")).into_any_element();
    }
    div().into_any_element()
}
