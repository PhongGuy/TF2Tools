//! The top of an open log, as logs.tf heads its page: the score banner, the title with
//! its facts line, and the team table.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    App, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use tf2tools_core::{LogDetail, LogRow, Team, TeamTotals};

use super::super::list::open_link;
use super::super::text::{date_text, duration_text};
use super::team_cell;
use crate::ids;
use crate::widgets::labelled;

// One team table column: its logs.tf title and how a team's cell reads.
type Column = (&'static str, fn(&TeamTotals) -> u32);

const COLUMNS: [Column; 6] = [
    ("Kills", TeamTotals::kills),
    ("Damage", TeamTotals::dmg),
    ("Charges", TeamTotals::charges),
    ("Drops", TeamTotals::drops),
    ("Caps", TeamTotals::caps),
    ("Midfights", TeamTotals::midfights),
];

/// The score banner of log `id`: BLU's half on the left, RED's on the right, each score
/// beside the centre line, labelled "BLU 2, RED 3".
pub(super) fn banner(id: u64, log: &LogDetail, cx: &App) -> impl IntoElement {
    let (blue, red) = (log.blue().score(), log.red().score());
    let half = |color: Hsla| {
        h_flex()
            .flex_1()
            .min_w_0()
            .justify_between()
            .items_center()
            .px_4()
            .py_2()
            .bg(color.opacity(0.18))
            .text_color(color)
            .font_weight(FontWeight::BOLD)
    };
    let name = |text: &'static str| div().text_xl().child(text);
    let score = |n: u32| div().text_3xl().child(n.to_string());
    let label = format!("BLU {blue}, RED {red}");
    h_flex()
        .id(ids::logstf_banner(id))
        .aria_label(label)
        .test_support()
        .w_full()
        .rounded_md()
        .overflow_hidden()
        .child(half(cx.theme().blue).child(name("BLU")).child(score(blue)))
        .child(half(cx.theme().red).child(score(red)).child(name("RED")))
}

/// The row's title over its map, date and length, with the way to the same log on logs.tf.
pub(super) fn title(row: &LogRow, cx: &App) -> impl IntoElement {
    let mut facts = vec![row.map().to_owned(), date_text(row.date())];
    facts.extend(row.length().map(duration_text));
    h_flex()
        .w_full()
        .items_start()
        .gap_4()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(row.title().to_owned()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(facts.join("  ·  ")),
                ),
        )
        .child(open_link(row.id()))
}

/// The team table in logs.tf's columns and order, RED's row first.
pub(super) fn team_table(log: &LogDetail, cx: &App) -> Div {
    let cell = || div().w(px(64.)).flex_none().text_right();
    let head = h_flex()
        .gap_1()
        .py_1()
        .text_color(cx.theme().muted_foreground)
        .font_weight(FontWeight::MEDIUM)
        .child(div().w(px(40.)).flex_none().child("Team"))
        .children(COLUMNS.map(|(title, _)| cell().child(title)));
    let line = |team: Team, totals: &TeamTotals| {
        h_flex()
            .gap_1()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(team_cell(team, cx).w(px(40.)))
            .children(COLUMNS.map(|(title, read)| {
                let value = read(totals).to_string();
                labelled(cell(), ids::logstf_team_cell(team, title), value)
            }))
    };
    v_flex()
        .child(head)
        .child(line(Team::Red, log.red()))
        .child(line(Team::Blue, log.blue()))
}
