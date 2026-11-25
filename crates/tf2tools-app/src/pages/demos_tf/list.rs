//! The Demos.tf page's one scrolling list: the toolbar and column titles as its first item,
//! one row per demo, then a footer that says whether more is loading, nothing was found or
//! the page failed.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::select::Select;
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ListState, ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, WeakEntity, Window, div, list, px,
};
use tf2tools_core::DemosRange;

use super::text::{demo_title, format_label, range_label, watch_label};
use super::{DemoRow, DemosTfPage};
use crate::i18n::tr;
use crate::ids;
use crate::pages::common::filter_badge;
use crate::pages::logs_tf::list::{
    DATE, GAP, LENGTH, MAP, MAX_WIDTH, centred, date_cell, map_cell,
};
use crate::pages::logs_tf::text::{demo_button_label, duration_text};
use crate::store_grid::feed::FeedItem as _;
use crate::widgets::scroll::scroll_beside;
use crate::widgets::store_error::error_line;
use crate::widgets::{Named as _, labelled};

// Column widths beyond the ones shared with logs.tf's rows; ACTIONS keeps room for the row's
// Download and Watch buttons.
const FORMAT: Pixels = px(88.);
const ACTIONS: Pixels = px(132.);
const TITLE_MIN: Pixels = px(120.);
// The row's horizontal padding, both sides.
const PADDING: Pixels = px(24.);
// The stop labels sit one cell apart, each centred on its thumb stop. The slider's track is
// inset half a cell on both sides, so the end labels fit inside the picker too.
const STOP_CELL: f32 = 68.;
const RANGE_WIDTH: f32 = STOP_CELL * DemosRange::STOPS.len() as f32;
const LABEL_HEIGHT: f32 = 24.;
// Fits the longest format label whole beside the kit's caret and padding.
const FORMAT_SELECT: Pixels = px(176.);
// Fits a long map name such as koth_product_final.
const MAP_INPUT: Pixels = px(176.);

/// How the rows fit the list's width: a narrow list moves format and map under the title,
/// then the date too; title, length and the actions always keep their columns.
#[derive(Clone, Copy)]
struct Fit {
    side_columns: bool,
    date_column: bool,
}

fn fit(list: &ListState) -> Fit {
    let width = list.viewport_bounds().size.width.min(MAX_WIDTH);
    // Before the first frame the list has no width yet, so it lays out in full.
    let unknown = width <= px(0.);
    let dated = DATE + TITLE_MIN + LENGTH + ACTIONS + GAP * 3. + PADDING;
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
        list(page.list.clone(), move |ix, window, cx| {
            let Some(page) = this.upgrade() else {
                return div().into_any_element();
            };
            let item = if ix == 0 {
                // The Open demo button takes a listener, so the top builds inside the page.
                page.update(cx, |page, cx| top(page, fit, window, cx))
            } else {
                let page = page.read(cx);
                match page.feed.items().get(ix - 1) {
                    Some(row) => {
                        let id = row.feed_id();
                        let (downloading, saving) =
                            (page.download_progress(id), page.save_progress(id));
                        demo_row(row, downloading, saving, fit, this.clone(), cx)
                    }
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

// The format Select, map Input and range picker, Open demo at the right, the tracked
// players' chips, then the column titles in the rows' widths. A narrow list wraps the toolbar
// instead of squeezing it.
fn top(
    page: &DemosTfPage,
    fit: Fit,
    window: &mut Window,
    cx: &mut Context<DemosTfPage>,
) -> AnyElement {
    let open = Button::new(ids::DEMOS_TF_OPEN)
        .outline()
        .small()
        .label(tr!("demos_tf.open"))
        .on_click(cx.listener(|page, _, window, cx| page.open_demo(window, cx)));
    let format = div()
        .id(ids::DEMOS_TF_FORMAT)
        .test_support()
        .w(FORMAT_SELECT)
        .child(Select::new(&page.format_select).small());
    let map = div()
        .id(ids::DEMOS_TF_MAP)
        .test_support()
        .w(MAP_INPUT)
        .child(Input::new(&page.map_input).small());
    let tools = h_flex()
        .w_full()
        .flex_wrap()
        .items_start()
        .gap_4()
        .child(format)
        .child(map)
        .child(range_picker(&page.range_slider, cx.entity().downgrade()))
        .child(h_flex().ml_auto().child(open));
    v_flex()
        .w_full()
        .gap_2()
        .child(tools)
        .when(!page.players.is_empty(), |el| {
            el.child(chips(page, window, cx))
        })
        .child(column_header(fit, cx))
        .into_any_element()
}

// All, then one chip per tracked player, the one the list keeps to selected.
fn chips(page: &DemosTfPage, window: &mut Window, cx: &mut Context<DemosTfPage>) -> Div {
    let active = page.player();
    let all = filter_badge(
        ids::DEMOS_TF_ALL,
        active.is_none(),
        tr!("demos_tf.all"),
        window,
        cx,
    )
    .on_click(cx.listener(|page, _, _, cx| page.select_player(None, cx)));
    let mut row = h_flex().flex_wrap().gap_2().child(all);
    for player in &page.players {
        let id = player.steam_id();
        let chip = filter_badge(
            ids::demos_tf_chip(id),
            active == Some(id),
            player.display_name().to_owned(),
            window,
            cx,
        )
        .on_click(cx.listener(move |page, _, _, cx| page.select_player(Some(id), cx)));
        row = row.child(chip);
    }
    row
}

// The slider over its six stop labels, as logs.tf's; a label click picks its stop like a
// release there, and being buttons the labels give the keyboard a way to pick.
fn range_picker(slider: &Entity<SliderState>, page: WeakEntity<DemosTfPage>) -> impl IntoElement {
    let cells = DemosRange::STOPS.iter().enumerate().map(|(i, &range)| {
        let page = page.clone();
        let label = Button::new(ids::demos_tf_range_stop(range))
            .ghost()
            .xsmall()
            .label(range_label(range))
            .on_click(move |_, window, cx| {
                let _ = page.update(cx, |page, cx| page.set_range(range, window, cx));
            });
        // Stop `i` lies `i` cells in from the first label's centre, under the kit's thumb.
        h_flex()
            .absolute()
            .top_0()
            .left(px(STOP_CELL * i as f32))
            .w(px(STOP_CELL))
            .h_full()
            .justify_center()
            .child(label)
    });
    v_flex()
        .w(px(RANGE_WIDTH))
        .gap_1()
        .child(
            div()
                .id(ids::DEMOS_TF_RANGE)
                .aria_label(tr!("demos_tf.range_name"))
                .test_support()
                .px(px(STOP_CELL / 2.))
                .child(Slider::new(slider)),
        )
        .child(
            div()
                .relative()
                .w_full()
                .h(px(LABEL_HEIGHT))
                .text_xs()
                .children(cells),
        )
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
        .child(div().w(ACTIONS).flex_none())
}

// One demo: date, title, format, map, length, Download and Watch, with whatever no longer
// fits beside the title moved under it as one muted line.
fn demo_row(
    row: &DemoRow,
    downloading: Option<Option<u8>>,
    saving: Option<Option<u8>>,
    fit: Fit,
    this: WeakEntity<DemosTfPage>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let demo = &row.demo;
    let id = row.feed_id();
    let title = demo_title(demo.server(), demo.red(), demo.blue());
    let format = format_label(demo.format());
    let public = !demo.url().is_empty();
    let save_page = this.clone();
    let download = Button::new(ids::demos_tf_download(id))
        .outline()
        .small()
        .icon(IconName::Download)
        .named(demo_button_label(saving))
        .loading(saving.is_some())
        .disabled(!public || saving.is_some())
        .on_click(move |_, window, cx| {
            let _ = save_page.update(cx, |page, cx| page.save(id, window, cx));
        });
    let watch = Button::new(ids::demos_tf_watch(id))
        .outline()
        .small()
        .when_some(watch_label(public, downloading), |button, label| {
            button.label(label)
        })
        // A running download spins and gives its full wording to the tooltip and screen reader.
        .when(public && downloading.is_some(), |button| {
            button.loading(true).named(demo_button_label(downloading))
        })
        .disabled(!public || downloading.is_some())
        .on_click(move |_, window, cx| {
            let _ = this.update(cx, |page, cx| page.watch(id, window, cx));
        });
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
        .child(
            div()
                .w(ACTIONS)
                .flex_none()
                .flex()
                .justify_end()
                .gap_1()
                .child(download)
                .child(watch),
        )
        .into_any_element()
}

// What follows the rows: the failed page's error with Retry, Keep looking past the skip
// limit, a loading line, or the empty range's note with a way to look further back.
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
                error.text(),
                retry,
                cx,
            ))
            .into_any_element();
    }
    if page.stalled {
        let keep = Button::new(ids::DEMOS_TF_KEEP_LOOKING)
            .outline()
            .small()
            .label(tr!("demos_tf.keep_looking"))
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |page, cx| page.keep_looking(cx));
            });
        return div().py_4().px_3().child(keep).into_any_element();
    }
    // A footer about to page already says so: a footer drawn empty measures zero height at
    // the list's bottom edge, and the list would not draw it again to show what lands.
    if page.feed.is_loading() || page.wants_page() {
        return labelled(line(), ids::DEMOS_TF_LOADING, tr!("demos_tf.loading")).into_any_element();
    }
    if page.feed.is_end() && page.feed.items().is_empty() {
        let range = page.range();
        let wider = DemosRange::STOPS
            .iter()
            .skip_while(|stop| **stop != range)
            .nth(1)
            .copied();
        let widen = wider.map(|wider| {
            Button::new(ids::DEMOS_TF_WIDEN)
                .ghost()
                .small()
                .label(tr!("demos_tf.widen"))
                .on_click(move |_, window, cx| {
                    let _ = this.update(cx, |page, cx| page.set_range(wider, window, cx));
                })
        });
        return h_flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(labelled(line(), ids::DEMOS_TF_EMPTY, tr!("demos_tf.empty")))
            .children(widen)
            .into_any_element();
    }
    div().into_any_element()
}
