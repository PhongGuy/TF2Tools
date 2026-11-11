//! The class averages grid: one card per class in TF2 order, then how many logs it covers
//! beside the history range slider.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, div, px,
};
use tf2tools_core::{ClassAverage, LogsRange};

use super::LogsTfPage;
use super::text::{one_decimal, range_label, summary_text, time_played_text, whole};
use crate::ids;
use crate::services::Services;
use crate::widgets::class_icon::class_icon;
use crate::widgets::labelled;

const CARD_WIDTH: f32 = 168.;
const ICON_SIZE: f32 = 28.;

// The stop labels sit one cell apart, each centred on its thumb stop. The slider's track is
// inset half a cell on both sides, so the end labels fit inside the picker too.
const STOP_CELL: f32 = 68.;
const RANGE_WIDTH: f32 = STOP_CELL * LogsRange::STOPS.len() as f32;
const LABEL_HEIGHT: f32 = 24.;

/// The nine cards, wrapping, with the summary line for `logs` in `range` back from `newest`
/// and the range slider under them.
pub(super) fn class_grid(
    averages: &[ClassAverage; 9],
    logs: usize,
    newest: Option<i64>,
    range: LogsRange,
    slider: &Entity<SliderState>,
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .gap_2()
        .child(
            h_flex()
                .flex_wrap()
                .items_start()
                .gap_3()
                .children(averages.iter().map(|average| class_card(average, cx))),
        )
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

// The icon and name, then the rates as label and value rows; Medic's two extra rates only
// exist when there is time on Medic. A class with no time says so in the muted colour.
fn class_card(average: &ClassAverage, cx: &App) -> impl IntoElement {
    let class = average.class();
    let muted = cx.theme().muted_foreground;
    let icon = cx.global::<Services>().assets.class_icon(class);
    let header = h_flex()
        .gap_2()
        .child(class_icon(class, icon.as_deref(), px(ICON_SIZE)))
        .child(div().font_weight(FontWeight::SEMIBOLD).child(class.name()));
    let body = if average.played() {
        let mut stats = vec![
            ("Time played", time_played_text(average.seconds())),
            ("Logs", average.logs().to_string()),
            ("Kills/min", one_decimal(average.kills_per_min())),
            ("Deaths/min", one_decimal(average.deaths_per_min())),
            ("Dmg/min", whole(average.dmg_per_min())),
            ("KA/D", one_decimal(average.kad())),
        ];
        stats.extend(average.heal_per_min().map(|h| ("Heals/min", whole(h))));
        stats.extend(
            average
                .ubers_per_10min()
                .map(|u| ("Ubers/10 min", one_decimal(u))),
        );
        v_flex()
            .gap_0p5()
            .text_sm()
            .children(stats.into_iter().map(|(label, value)| {
                h_flex()
                    .justify_between()
                    .gap_2()
                    .child(div().text_color(muted).child(label))
                    .child(div().child(value))
            }))
            .into_any_element()
    } else {
        labelled(
            div().text_sm().text_color(muted),
            ids::logstf_class_unplayed(class),
            "Not played",
        )
        .into_any_element()
    };
    v_flex()
        .id(ids::logstf_class(class))
        .aria_label(class.name())
        .test_support()
        .w(px(CARD_WIDTH))
        .gap_2()
        .p_3()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .child(header)
        .child(body)
}
