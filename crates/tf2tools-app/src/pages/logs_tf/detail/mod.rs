//! An open log's panel, laid out like its page on logs.tf: the score banner, the title, the
//! team table, the players table, one block per Medic and the tracked players' kills by class.

mod banner;

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Styled as _, TestSupportExt as _, WeakEntity, Window, div, px,
};
use tf2tools_core::{
    LogColumns, LogDetail, LogRow, LogState, MatrixKind, MedicLine, PlayerLine, SteamId, Team,
    TfClass, TrackedPlayer,
};

use super::text::{LOG_LOADING, one_decimal, whole};
use super::{LogsTfPage, OpenState};
use crate::ids;
use crate::services::Services;
use crate::widgets::class_icon::class_icon;
use crate::widgets::labelled;
use crate::widgets::scroll::sideways;
use crate::widgets::store_error::error_line;

// The players table's fixed columns; the name takes what is left.
const TEAM: Pixels = px(40.);
const NAME_MIN: Pixels = px(120.);
const CLASSES: Pixels = px(76.);
const NUMBER: Pixels = px(44.);
const ICON: Pixels = px(16.);
const MEDIC_WIDTH: Pixels = px(280.);

// One players table column: its logs.tf title and how a player's cell reads.
type Column = (&'static str, fn(&PlayerLine) -> String);

/// The panel under open log `row`. The score banner heads it once `opened` is ready; the
/// title comes from the row at once; under it the parsed log, or a line saying it is
/// loading or why it failed. A
/// failed fetch of a pending log offers Retry, which asks `page` to fetch it again.
pub(super) fn panel(
    row: &LogRow,
    opened: &OpenState,
    tracked: &[TrackedPlayer],
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> AnyElement {
    let id = row.id();
    let status = |text: &'static str| {
        let line = div().text_sm().text_color(cx.theme().muted_foreground);
        labelled(line, ids::logstf_log_status(id), text).into_any_element()
    };
    let ready = match opened {
        OpenState::Ready(log) => Some(log.as_ref()),
        OpenState::Loading | OpenState::Failed(_) => None,
    };
    let body = match opened {
        OpenState::Ready(log) => log_body(log, tracked, cx).into_any_element(),
        OpenState::Loading => status(LOG_LOADING),
        OpenState::Failed(why) if row.state() == LogState::Pending => {
            let on_retry = move |_: &mut Window, cx: &mut App| {
                let _ = page.update(cx, |page, cx| page.retry(id, cx));
            };
            let retry = ids::logstf_log_retry(id);
            error_line(ids::logstf_log_status(id), retry, *why, on_retry, cx).into_any_element()
        }
        OpenState::Failed(why) => status(why),
    };
    v_flex()
        .id(ids::logstf_log_panel(row.id()))
        .test_support()
        .w_full()
        .gap_4()
        .px_3()
        .py_3()
        .bg(cx.theme().muted)
        .when_some(ready, |el, log| el.child(banner::banner(id, log, cx)))
        .child(banner::title(row, cx))
        .child(body)
        .into_any_element()
}

// Everything the parsed body adds, in logs.tf's order.
fn log_body(log: &LogDetail, tracked: &[TrackedPlayer], cx: &App) -> impl IntoElement {
    let is_tracked = |id: SteamId| tracked.iter().any(|p| p.steam_id() == id);
    let kills: Vec<_> = log
        .class_matrix(MatrixKind::Kills)
        .into_iter()
        .filter(|k| is_tracked(k.steam_id()) && k.total() > 0)
        .collect();
    let name_of = |id: SteamId| {
        log.players()
            .iter()
            .find(|p| p.steam_id() == id)
            .map_or_else(|| id.id64().to_string(), |p| p.name().to_owned())
    };
    v_flex()
        .w_full()
        .gap_6()
        .text_xs()
        .child(sideways(
            ids::LOGSTF_TEAMS_SCROLL,
            banner::team_table(log, cx),
        ))
        .child(sideways(
            ids::LOGSTF_PLAYERS_SCROLL,
            players_table(log, &is_tracked, cx),
        ))
        .when(!log.medics().is_empty(), |el| {
            el.child(section(
                "Medics",
                h_flex()
                    .flex_wrap()
                    .items_start()
                    .gap_4()
                    .children(log.medics().iter().map(|m| medic_block(m, cx))),
                cx,
            ))
        })
        .when(!kills.is_empty(), |el| {
            let mut table = v_flex().child(
                h_flex()
                    .gap_1()
                    .py_1()
                    .child(div().flex_1().min_w_0())
                    .children(TfClass::ALL.map(|class| class_cell(class, cx))),
            );
            for line in kills {
                let id = line.steam_id();
                let counts = line.counts();
                table =
                    table.child(
                        h_flex()
                            .id(ids::logstf_class_kills(id))
                            .test_support()
                            .gap_1()
                            .py_1()
                            .border_t_1()
                            .border_color(cx.theme().border)
                            .child(div().flex_1().min_w_0().truncate().child(name_of(id)))
                            .children(counts.map(|n| {
                                number(if n == 0 { String::new() } else { n.to_string() })
                            })),
                    );
            }
            el.child(section("Class kills", table.max_w(px(560.)), cx))
        })
}

// A titled part of the body.
pub(super) fn section(title: &'static str, content: impl IntoElement, cx: &App) -> Div {
    v_flex()
        .gap_2()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(title),
        )
        .child(content)
}

// The players table: RED then BLU as core orders them, with the columns this log recorded.
fn players_table(log: &LogDetail, is_tracked: &dyn Fn(SteamId) -> bool, cx: &App) -> Div {
    let columns = player_columns(log.columns());
    let title = |text: &'static str| div().id(ids::logstf_col(text)).test_support().child(text);
    let mut table = v_flex().w_full().child(
        h_flex()
            .gap_1()
            .px_1()
            .py_1()
            .text_color(cx.theme().muted_foreground)
            .font_weight(FontWeight::MEDIUM)
            .child(title("Team").w(TEAM).flex_none())
            .child(title("Name").flex_1().min_w(NAME_MIN))
            .child(title("Classes").w(CLASSES).flex_none())
            .children(
                columns
                    .iter()
                    .map(|(t, _)| title(t).w(NUMBER).flex_none().text_right()),
            ),
    );
    let assets = &cx.global::<Services>().assets;
    for player in log.players() {
        let icons = player
            .classes()
            .iter()
            .map(|c| class_icon(c.class(), assets.class_icon(c.class()).as_deref(), ICON));
        table = table.child(
            h_flex()
                .id(ids::logstf_player_row(player.steam_id()))
                .test_support()
                .gap_1()
                .px_1()
                .py_1()
                .border_t_1()
                .border_color(cx.theme().border)
                .when(is_tracked(player.steam_id()), |el| {
                    el.bg(cx.theme().primary.opacity(0.12))
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .child(team_cell(player.team(), cx).w(TEAM))
                .child(
                    div()
                        .id(ids::logstf_player_name(player.steam_id()))
                        .test_support()
                        .flex_1()
                        .min_w(NAME_MIN)
                        .truncate()
                        .child(player.name().to_owned()),
                )
                .child(
                    h_flex()
                        .w(CLASSES)
                        .flex_none()
                        .gap_0p5()
                        .overflow_hidden()
                        .children(icons),
                )
                .children(columns.iter().map(|(_, cell)| number(cell(player)))),
        );
    }
    table
}

// logs.tf's columns in its order, without the ones the log did not record.
fn player_columns(c: &LogColumns) -> Vec<Column> {
    let all: [(bool, Column); 14] = [
        (true, ("K", |p| p.kills().to_string())),
        (true, ("A", |p| p.assists().to_string())),
        (true, ("D", |p| p.deaths().to_string())),
        (true, ("DA", |p| p.dmg().to_string())),
        (true, ("DA/M", |p| p.da_per_min().to_string())),
        (true, ("KA/D", |p| one_decimal(p.kapd()))),
        (true, ("K/D", |p| one_decimal(p.kpd()))),
        (c.dt(), ("DT", |p| p.dt().to_string())),
        (c.dt(), ("DT/M", |p| whole(p.dt_per_min()))),
        (c.hp(), ("HP", |p| p.medkits().to_string())),
        (c.bs(), ("BS", |p| p.backstabs().to_string())),
        (c.hs(), ("HS", |p| p.headshots().to_string())),
        (c.airshots(), ("AS", |p| p.airshots().to_string())),
        (c.caps(), ("CAP", |p| p.caps().to_string())),
    ];
    all.into_iter()
        .filter_map(|(shown, column)| shown.then_some(column))
        .collect()
}

// One Medic's healing and uber numbers, then who they healed, most first.
fn medic_block(medic: &MedicLine, cx: &App) -> impl IntoElement {
    let fact = |label: String, value: String| {
        h_flex()
            .justify_between()
            .gap_2()
            .child(div().text_color(cx.theme().muted_foreground).child(label))
            .child(value)
    };
    let mut facts = vec![
        ("Healing".to_owned(), medic.heal().to_string()),
        ("Heals/min".to_owned(), whole(medic.heal_per_min())),
    ];
    facts.extend(medic.charges().iter().map(|(kind, n)| {
        (
            format!("Charges ({})", charge_name(kind.key())),
            n.to_string(),
        )
    }));
    facts.extend([
        ("Drops".to_owned(), medic.drops().to_string()),
        (
            "Avg time to build".to_owned(),
            format!("{}s", whole(medic.avg_time_to_build())),
        ),
        (
            "Avg uber length".to_owned(),
            format!("{}s", one_decimal(medic.avg_uber_length())),
        ),
        (
            "Deaths within 20s after uber".to_owned(),
            medic.deaths_after_uber().to_string(),
        ),
    ]);
    v_flex()
        .id(ids::logstf_medic(medic.steam_id()))
        .test_support()
        .w(MEDIC_WIDTH)
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .gap_2()
                .font_weight(FontWeight::SEMIBOLD)
                .child(team_cell(medic.team(), cx))
                .child(div().truncate().child(medic.name().to_owned())),
        )
        .children(facts.into_iter().map(|(label, value)| fact(label, value)))
        .child(
            div()
                .pt_1()
                .font_weight(FontWeight::MEDIUM)
                .child("Heal spread"),
        )
        .children(
            medic
                .heal_spread()
                .iter()
                .map(|t| fact(t.name().to_owned(), t.healing().to_string())),
        )
}

// logs.tf's medigun keys read as names: "kritzkrieg" becomes "Kritzkrieg".
fn charge_name(kind: &str) -> String {
    let mut chars = kind.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

// A team's name in its colour.
pub(super) fn team_cell(team: Team, cx: &App) -> Div {
    let (text, color): (&str, Hsla) = match team {
        Team::Red => ("RED", cx.theme().red),
        Team::Blue | _ => ("BLU", cx.theme().blue),
    };
    div()
        .flex_none()
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(text)
}

fn class_cell(class: TfClass, cx: &App) -> AnyElement {
    let assets = &cx.global::<Services>().assets;
    div()
        .w(NUMBER)
        .flex_none()
        .flex()
        .justify_end()
        .child(class_icon(class, assets.class_icon(class).as_deref(), ICON))
        .into_any_element()
}

pub(super) fn number(text: String) -> Div {
    div().w(NUMBER).flex_none().text_right().child(text)
}
