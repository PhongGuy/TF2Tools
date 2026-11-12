//! The class averages: how many logs they cover beside the history range slider, then one
//! compact table row per class in TF2 order, the open class's weapons right under its row.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Entity, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, Role, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, WeakEntity, Window, div, px, transparent_black,
};
use tf2tools_core::{ClassAverage, LogsRange, TfClass};

use super::LogsTfPage;
use super::text::{one_decimal, range_label, summary_text, time_played_text, whole};
use crate::ids;
use crate::services::Services;
use crate::widgets::class_icon::class_icon;
use crate::widgets::labelled;
use crate::widgets::scroll::sideways;

const ICON_SIZE: f32 = 20.;
const ROW_PAD: Pixels = px(12.);
const GAP: Pixels = px(10.);
// The class column grows to fill the box but keeps room for "Demoman" beside its icon; each
// number column fits its title and widest value on one line, so the table fits a 1200 px window.
const CLASS_MIN: Pixels = px(120.);
const COLUMNS: [(&str, Pixels); 9] = [
    ("Played", px(72.)),
    ("Logs", px(44.)),
    ("Kills/min", px(64.)),
    ("Assists/min", px(76.)),
    ("Deaths/min", px(76.)),
    ("Dmg/min", px(60.)),
    ("KA/D", px(44.)),
    ("Heals/min", px(68.)),
    ("Ubers/10 min", px(84.)),
];

// The stop labels sit one cell apart, each centred on its thumb stop. The slider's track is
// inset half a cell on both sides, so the end labels fit inside the picker too.
const STOP_CELL: f32 = 68.;
const RANGE_WIDTH: f32 = STOP_CELL * LogsRange::STOPS.len() as f32;
const LABEL_HEIGHT: f32 = 24.;

/// The summary line for `logs` in `range` back from `newest` and the range slider, so the
/// range is picked before the stats are read, then the class table. A played row opens its
/// weapons; `selected` is highlighted with `weapons`, the open block, drawn under it.
#[allow(clippy::too_many_arguments)]
pub(super) fn class_grid(
    averages: &[ClassAverage; 9],
    logs: usize,
    newest: Option<i64>,
    range: LogsRange,
    slider: &Entity<SliderState>,
    page: WeakEntity<LogsTfPage>,
    selected: Option<TfClass>,
    weapons: Option<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    // The table comes after the picker, which takes `page`, so it is built first.
    let table = class_table(averages, page.clone(), selected, weapons, window, cx);
    v_flex()
        .gap_2()
        .child(
            h_flex()
                .flex_wrap()
                .items_start()
                .justify_between()
                .gap_4()
                .children((logs > 0).then(|| {
                    labelled(
                        div().text_sm().text_color(muted),
                        ids::LOGSTF_SUMMARY,
                        summary_text(range, logs, newest, chrono::Utc::now().timestamp()),
                    )
                }))
                .child(range_picker(slider, page)),
        )
        .child(table)
}

// The slider over its six stop labels; a label click picks its stop like a release there.
// The labels are buttons so the keyboard can pick a range: the kit's slider takes no focus,
// and its assistive Increment and Decrement move the thumb without any event to commit on.
fn range_picker(slider: &Entity<SliderState>, page: WeakEntity<LogsTfPage>) -> impl IntoElement {
    let labels = LogsRange::STOPS.map(|range| {
        let page = page.clone();
        Button::new(ids::logstf_range_stop(range))
            .ghost()
            .xsmall()
            .label(range_label(range))
            .on_click(move |_, window, cx| {
                let _ = page.update(cx, |page, cx| page.set_range(range, window, cx));
            })
    });
    // The kit's thumb centres on its share of the track's width, which spans the picker less
    // half a cell each side, so stop `i` lies `i` cells in from the first label's centre.
    let cells = labels.into_iter().enumerate().map(|(i, label)| {
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
        .ml_auto()
        .w(px(RANGE_WIDTH))
        .gap_1()
        .child(
            div()
                .id(ids::LOGSTF_RANGE)
                .aria_label("History range")
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

// The column titles over one row per class in a bordered box. Below its natural width the
// table keeps every column and scrolls sideways inside the box instead of squashing.
fn class_table(
    averages: &[ClassAverage; 9],
    page: WeakEntity<LogsTfPage>,
    selected: Option<TfClass>,
    mut weapons: Option<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let numbers: Pixels = COLUMNS.iter().map(|(_, width)| *width).sum();
    let min_width = CLASS_MIN + numbers + GAP * COLUMNS.len() as f32 + ROW_PAD * 2. + px(2.);
    let theme = cx.theme();
    let (border, radius) = (theme.border, theme.radius_lg);
    let head = line()
        .text_xs()
        .text_color(theme.muted_foreground)
        .font_weight(FontWeight::MEDIUM)
        .child(
            div()
                .id(ids::logstf_class_head("Class"))
                .test_support()
                .flex_1()
                .min_w(CLASS_MIN)
                .child("Class"),
        )
        .children(COLUMNS.map(|(label, width)| {
            div()
                .id(ids::logstf_class_head(label))
                .test_support()
                .w(width)
                .flex_none()
                .text_right()
                .child(label)
        }));
    let mut rows = v_flex()
        .w_full()
        .min_w(min_width)
        .text_sm()
        .child(head)
        .child(div().h(px(1.)).bg(border));
    for average in averages {
        let class = average.class();
        let open = selected == Some(class);
        rows = rows
            .child(class_row(average, open, page.clone(), window, cx))
            .children(open.then(|| weapons.take()).flatten());
    }
    div()
        .rounded(radius)
        .border_1()
        .border_color(border)
        .overflow_hidden()
        .child(sideways(ids::LOGSTF_CLASS_TABLE, rows))
}

// The shared shape of the title line and every class row, so their columns line up; the
// clear border is where a focused row draws its ring.
fn line() -> gpui_kit::Div {
    h_flex()
        .w_full()
        .gap(GAP)
        .px(ROW_PAD)
        .py_1p5()
        .border_1()
        .border_color(transparent_black())
        .whitespace_nowrap()
}

// The icon and name, then each rate right-aligned in its column; Medic's two extra rates only
// exist when there is time on Medic. A class with no time is dimmed, says so under Played and,
// having no weapons to show, takes no click, focus or selected state.
fn class_row(
    average: &ClassAverage,
    open: bool,
    page: WeakEntity<LogsTfPage>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let class = average.class();
    let id = ids::logstf_class(class);
    let icon = cx.global::<Services>().assets.class_icon(class);
    let values: [Option<String>; 9] = if average.played() {
        [
            Some(time_played_text(average.seconds())),
            Some(average.logs().to_string()),
            Some(one_decimal(average.kills_per_min())),
            Some(one_decimal(average.assists_per_min())),
            Some(one_decimal(average.deaths_per_min())),
            Some(whole(average.dmg_per_min())),
            Some(one_decimal(average.kad())),
            average.heal_per_min().map(whole),
            average.ubers_per_10min().map(one_decimal),
        ]
    } else {
        Default::default()
    };
    let cells = COLUMNS
        .into_iter()
        .zip(values)
        .map(|((label, width), value)| {
            let cell = h_flex().w(width).flex_none().justify_end();
            match value {
                Some(value) => cell.child(labelled(
                    div(),
                    ids::logstf_class_value(class, label),
                    value,
                )),
                None if label == "Played" && !average.played() => cell.child(labelled(
                    div(),
                    ids::logstf_class_unplayed(class),
                    "Not played",
                )),
                None => cell,
            }
        });
    let name = h_flex()
        .flex_1()
        .min_w(CLASS_MIN)
        .gap_2()
        .child(class_icon(class, icon.as_deref(), px(ICON_SIZE)))
        .child(div().font_weight(FontWeight::SEMIBOLD).child(class.name()));
    let row = line()
        .id(id.clone())
        .aria_label(class.name())
        .test_support()
        .child(name)
        .children(cells);
    if !average.played() {
        return row.opacity(0.5).into_any_element();
    }
    // A played row is a button on the tab order, Enter or Space opening it as a click does.
    let focus = window
        .use_keyed_state(id, cx, |_, cx| cx.focus_handle().tab_stop(true))
        .read(cx)
        .clone();
    let theme = cx.theme();
    let hover = theme.muted.opacity(0.5);
    row.role(Role::Button)
        .aria_selected(open)
        .track_focus(&focus)
        .cursor_pointer()
        .when(open, |row| row.bg(theme.muted))
        .when(!open, |row| row.hover(move |style| style.bg(hover)))
        .when(focus.is_focused(window), |row| row.border_color(theme.ring))
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_click(move |_, _, cx| {
            let _ = page.update(cx, |page, cx| page.pick_class(class, cx));
        })
        .into_any_element()
}
