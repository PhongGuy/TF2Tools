//! The logs.tf page's one scrolling list: the chips and grid as its first item, then one row
//! per log in logs.tf's column order, the open row followed by its panel.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ListState,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, WeakEntity, div, list, px,
};
use tf2tools_core::{LogRow, LogState, SteamId};

use super::text::{
    LOG_GONE, LOG_PENDING, LOG_UNREADABLE, NO_LOGS_TEXT, date_time_text, duration_text, kapd_text,
    known_text, log_url, per_min_text, profile_url,
};
use super::{LogsTfPage, detail};
use crate::ids;
use crate::services::Services;
use crate::widgets::class_icon::class_icon;
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_beside;

/// The tracked player's own columns on each row, in logs.tf's order.
pub const STAT_COLUMNS: [&str; 8] = ["K", "A", "D", "DA", "DA/M", "KA/D", "DT", "DT/M"];

// The page's content stops growing here and centres, so a wide window keeps names, classes
// and stats together instead of stretching the title across the screen.
const MAX_WIDTH: Pixels = px(1400.);

// Column widths shared by the header and every row, so the columns line up like a table.
const CHEVRON: Pixels = px(16.);
const DATE: Pixels = px(124.);
const MAP: Pixels = px(160.);
const LENGTH: Pixels = px(56.);
// Four icons on one line, or two in a narrow window, where the stats need the room.
const CLASSES: Pixels = px(84.);
const CLASSES_NARROW: Pixels = px(40.);
// The stat columns' widths in STAT_COLUMNS order, and tighter ones in smaller text for a
// narrow window, so all eight stay on screen while the title gives way.
const STATS: [f32; 8] = [28., 28., 28., 48., 40., 40., 48., 40.];
const STATS_NARROW: [f32; 8] = [18., 18., 18., 36., 30., 28., 36., 30.];
const STAT_GAP: Pixels = px(8.);
const STAT_GAP_NARROW: Pixels = px(2.);
const GAP: Pixels = px(16.);
const ICON: Pixels = px(18.);
const TITLE_MIN: Pixels = px(100.);
// Below the core columns' width the rows tighten their gaps, title and stats, and the date
// moves under the title, so DT/M, the last column, still ends inside the smallest window's
// 448 px list.
const GAP_NARROW: Pixels = px(8.);
const TITLE_MIN_NARROW: Pixels = px(72.);
// The row's horizontal padding, both sides.
const PADDING: Pixels = px(24.);

/// How the rows fit the list's width. A narrow window moves Map under the title first, then
/// gives up Length, then moves Date under Map and tightens the gaps, title and stats;
/// before the first frame the list has no width yet.
#[derive(Clone, Copy)]
pub(super) struct Fit {
    map_column: bool,
    length: bool,
    date_column: bool,
    gap: Pixels,
    title_min: Pixels,
    classes: Pixels,
    narrow_stats: bool,
}

impl Fit {
    pub(super) const ALL: Self = Self {
        map_column: true,
        length: true,
        date_column: true,
        gap: GAP,
        title_min: TITLE_MIN,
        classes: CLASSES,
        narrow_stats: false,
    };

    fn stat_widths(self) -> [f32; 8] {
        if self.narrow_stats {
            STATS_NARROW
        } else {
            STATS
        }
    }

    fn stat_gap(self) -> Pixels {
        if self.narrow_stats {
            STAT_GAP_NARROW
        } else {
            STAT_GAP
        }
    }

    // The eight stat columns and the gaps between them.
    fn stats_width(self) -> Pixels {
        px(self.stat_widths().iter().sum()) + self.stat_gap() * 7.
    }
}

pub(super) fn fit(list: &ListState) -> Fit {
    let width = list.viewport_bounds().size.width.min(MAX_WIDTH);
    let core = CHEVRON + DATE + CLASSES + Fit::ALL.stats_width() + TITLE_MIN + GAP * 4. + PADDING;
    let unknown = width <= px(0.);
    let roomy = unknown || width >= core;
    Fit {
        map_column: unknown || width >= core + MAP + LENGTH + GAP * 2.,
        length: unknown || width >= core + LENGTH + GAP,
        date_column: roomy,
        gap: if roomy { GAP } else { GAP_NARROW },
        title_min: if roomy { TITLE_MIN } else { TITLE_MIN_NARROW },
        classes: if roomy { CLASSES } else { CLASSES_NARROW },
        narrow_stats: !roomy,
    }
}

/// The whole page as a virtualized list over `page`'s list state: item 0 is everything above
/// the logs, item `n` is the log at `rows[n - 1]`. The caller sizes it to fill the page.
pub(super) fn page_list(page: &LogsTfPage, cx: &mut Context<LogsTfPage>) -> Div {
    let this = cx.entity().downgrade();
    let rows = div().id(ids::LOGSTF_LIST).test_support().size_full().child(
        list(page.list.clone(), move |ix, window, cx| {
            let Some(page) = this.upgrade() else {
                return div().into_any_element();
            };
            let item = if ix == 0 {
                // The chips and buttons take listeners, so the top builds inside the page.
                page.update(cx, |page, cx| page.top(window, cx).into_any_element())
            } else {
                let page = page.read(cx);
                match page.rows.get(ix - 1) {
                    Some(row) => log_item(row, page, page.fit, this.clone(), cx),
                    None => div().into_any_element(),
                }
            };
            centred(item)
        })
        .size_full(),
    );
    scroll_beside(ids::LOGSTF_LIST_SCROLLBAR, &page.list, rows)
}

// One list item at the page's width, up to MAX_WIDTH and centred beyond it.
fn centred(item: AnyElement) -> AnyElement {
    h_flex()
        .w_full()
        .justify_center()
        .child(div().w_full().max_w(MAX_WIDTH).child(item))
        .into_any_element()
}

// The eight stat cells as one block, so they keep their own tighter spacing; `cell` fills
// column `i`'s base, already sized and right-aligned.
fn stat_block(fit: Fit, cell: impl Fn(usize, &'static str, Div) -> AnyElement) -> impl IntoElement {
    let widths = fit.stat_widths();
    h_flex()
        .w(fit.stats_width())
        .flex_none()
        .gap(fit.stat_gap())
        .when(fit.narrow_stats, |el| el.text_xs())
        .children(STAT_COLUMNS.iter().enumerate().map(|(i, &title)| {
            let base = div()
                .w(px(widths[i]))
                .flex_none()
                .text_right()
                .whitespace_nowrap();
            cell(i, title, base)
        }))
}

/// The column titles over the rows, in the rows' widths.
pub(super) fn column_header(fit: Fit, cx: &App) -> impl IntoElement {
    let cell = |w: Pixels, text: &'static str| div().w(w).flex_none().child(text);
    h_flex()
        .gap(fit.gap)
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().muted_foreground)
        .child(div().w(CHEVRON).flex_none())
        .when(fit.date_column, |el| el.child(cell(DATE, "Date")))
        .child(div().flex_1().min_w(fit.title_min).child("Title"))
        .when(fit.map_column, |el| el.child(cell(MAP, "Map")))
        .when(fit.length, |el| {
            el.child(cell(LENGTH, "Length").text_right())
        })
        .child(cell(fit.classes, "Class"))
        .child(stat_block(fit, |_, title, base| {
            base.id(ids::logstf_head(title))
                .test_support()
                .child(title)
                .into_any_element()
        }))
}

/// The line for a player with no logs in the window, with a way to their logs.tf profile.
pub(super) fn no_logs(id: SteamId, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_2()
        .py_4()
        .child(labelled(
            div().text_sm().text_color(cx.theme().muted_foreground),
            ids::LOGSTF_NO_LOGS,
            NO_LOGS_TEXT,
        ))
        .child(
            Button::new(ids::LOGSTF_NO_LOGS_PROFILE)
                .ghost()
                .small()
                .icon(IconName::ExternalLink)
                .label("Open profile on logs.tf")
                .on_click(move |_, _, cx| cx.open_url(&profile_url(id))),
        )
}

// One log: its row, then its panel while open. A downloaded or still pending log opens; the
// rest carry a note in place of the stats, and an unreadable one links to logs.tf instead.
fn log_item(
    row: &LogRow,
    state: &LogsTfPage,
    fit: Fit,
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let id = row.id();
    let open = state.expanded() == Some(id);
    let opens = matches!(row.state(), LogState::Ready | LogState::Pending);
    let chevron = div().w(CHEVRON).flex_none().children(opens.then(|| {
        Icon::new(if open {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        })
        .size_4()
        .text_color(theme.muted_foreground)
    }));
    let mut line = h_flex()
        .id(ids::logstf_log(id))
        .aria_label(row.title().to_owned())
        .test_support()
        .gap(fit.gap)
        .px_3()
        .py_2()
        .text_sm()
        .when(open, |el| el.bg(theme.muted))
        .child(chevron)
        .when(fit.date_column, |el| {
            el.child(date_cell(div().w(DATE).flex_none(), id, row.date()))
        })
        .child(
            v_flex()
                .flex_1()
                .min_w(fit.title_min)
                .child(
                    div()
                        .id(ids::logstf_log_title(id))
                        .test_support()
                        .truncate()
                        .child(row.title().to_owned()),
                )
                .when(!fit.map_column, |el| {
                    let line = div().text_xs().text_color(theme.muted_foreground);
                    el.child(map_cell(line, id, row.map()))
                })
                .when(!fit.date_column, |el| {
                    let line = div().text_xs().text_color(theme.muted_foreground);
                    el.child(date_cell(line, id, row.date()))
                }),
        )
        .when(fit.map_column, |el| {
            el.child(map_cell(div().w(MAP).flex_none(), id, row.map()))
        });
    line = match row.state() {
        LogState::Ready => line.children(stats(row, fit, cx)),
        state @ (LogState::Pending | LogState::Gone | LogState::Unreadable | _) => {
            line.child(note(id, state, fit, cx))
        }
    };
    if opens {
        let page = page.clone();
        line = line
            .cursor_pointer()
            .hover(|s| s.bg(theme.muted))
            .on_click(move |_, _, cx| {
                let _ = page.update(cx, |page, cx| page.toggle(id, cx));
            });
    }
    v_flex()
        .w_full()
        .border_b_1()
        .border_color(theme.border)
        .child(line)
        .children(
            state
                .open
                .as_ref()
                .filter(|open| open.id == id)
                .map(|open| {
                    let (sort, matrix) = (state.player_sort, state.matrix);
                    detail::panel(row, open, &state.tracked(cx), sort, matrix, page, cx)
                }),
        )
        .into_any_element()
}

// The log's start date and minute in `base`, its own column or a muted line under the title.
fn date_cell(base: Div, id: u64, date: i64) -> impl IntoElement {
    labelled(
        base.whitespace_nowrap(),
        ids::logstf_log_date(id),
        date_time_text(date),
    )
}

// The log's map in `base`, truncated with the full name in a tooltip; `base` sizes it as its
// own column or styles it as a muted line under the title.
fn map_cell(base: Div, id: u64, map: &str) -> impl IntoElement {
    let map = SharedString::from(map.to_owned());
    let tip = map.clone();
    base.id(ids::logstf_log_map(id))
        .aria_label(map.clone())
        .truncate()
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .test_support()
        .child(map)
}

// A downloaded log's numbers for the player: length, classes, then the eight stat columns.
fn stats(row: &LogRow, fit: Fit, cx: &App) -> impl Iterator<Item = AnyElement> {
    let id = row.id();
    let assets = &cx.global::<Services>().assets;
    // Icons past the class column's line wrap onto further lines and the row grows.
    let icons = h_flex()
        .w(fit.classes)
        .flex_none()
        .flex_wrap()
        .gap_1()
        .children(row.classes().iter().map(|&class| {
            div()
                .id(ids::logstf_log_class(id, class))
                .test_support()
                .flex_none()
                .child(class_icon(class, assets.class_icon(class).as_deref(), ICON))
        }));
    let length = row.length().map(duration_text).unwrap_or_default();
    let (kills, assists, deaths) = (row.kills(), row.assists(), row.deaths());
    let values: [String; 8] = [
        kills.to_string(),
        assists.to_string(),
        deaths.to_string(),
        row.dmg().to_string(),
        per_min_text(Some(row.dmg()), row.length()),
        kapd_text(kills, assists, deaths),
        known_text(row.dt()),
        per_min_text(row.dt(), row.length()),
    ];
    let cells = stat_block(fit, |i, title, base| {
        labelled(base, ids::logstf_log_stat(id, title), values[i].clone()).into_any_element()
    });
    let length = fit.length.then(|| {
        div()
            .w(LENGTH)
            .flex_none()
            .text_right()
            .child(length)
            .into_any_element()
    });
    length
        .into_iter()
        .chain([icons.into_any_element(), cells.into_any_element()])
}

// Why a row has no stats, muted and spanning the stats columns.
fn note(id: u64, state: LogState, fit: Fit, cx: &App) -> impl IntoElement {
    let text = match state {
        LogState::Gone => LOG_GONE,
        LogState::Unreadable => LOG_UNREADABLE,
        LogState::Pending | LogState::Ready | _ => LOG_PENDING,
    };
    let length = if fit.length { LENGTH + fit.gap } else { px(0.) };
    h_flex()
        .w(length + fit.classes + fit.stats_width() + fit.gap)
        .flex_none()
        .gap_2()
        .child(labelled(
            div().truncate().text_color(cx.theme().muted_foreground),
            ids::logstf_log_note(id),
            text,
        ))
        .children((state == LogState::Unreadable).then(|| open_link(id)))
}

/// "Open on logs.tf" for log `id`.
pub(super) fn open_link(id: u64) -> Button {
    Button::new(ids::logstf_log_open(id))
        .ghost()
        .xsmall()
        .icon(IconName::ExternalLink)
        .label("Open on logs.tf")
        .on_click(move |_, _, cx| cx.open_url(&log_url(id)))
}
