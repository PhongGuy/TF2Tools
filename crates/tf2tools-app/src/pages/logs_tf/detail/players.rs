//! The open log's players table, sortable by any header as on logs.tf.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Div, FontWeight, InteractiveElement as _, ParentElement as _, Pixels, Styled as _,
    TestSupportExt as _, WeakEntity, div, px,
};
use tf2tools_core::{LogColumns, LogDetail, PlayerLine, SteamId};

use super::{ICON, NUMBER, team_cell};
use crate::ids;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::sort::{PlayerColumn, PlayerSort, sort_players};
use crate::pages::logs_tf::text::{one_decimal, whole};
use crate::services::Services;
use crate::widgets::class_icon::class_icon;

// The table's fixed columns; the name takes what is left.
const TEAM: Pixels = px(40.);
const NAME_MIN: Pixels = px(120.);
const CLASSES: Pixels = px(76.);

// One number column: which sort it is and how a player's cell reads.
type Column = (PlayerColumn, fn(&PlayerLine) -> String);

/// The players table in `sort`'s order, with the columns this log recorded. Each header is a
/// button that asks `page` to sort by its column; tracked players stay highlighted.
pub(super) fn players_table(
    log: &LogDetail,
    sort: PlayerSort,
    is_tracked: &dyn Fn(SteamId) -> bool,
    page: WeakEntity<LogsTfPage>,
    cx: &App,
) -> Div {
    let columns = player_columns(log.columns());
    let header = |column: PlayerColumn| {
        let page = page.clone();
        Button::new(ids::logstf_sort(column.title()))
            .ghost()
            .xsmall()
            .label(column.title())
            .selected(sort.column == column)
            .flex_none()
            .on_click(move |_, _, cx| {
                let _ = page.update(cx, |page, cx| page.sort_players_by(column, cx));
            })
    };
    let mut table = v_flex().w_full().child(
        h_flex()
            .gap_1()
            .px_1()
            .py_1()
            .text_color(cx.theme().muted_foreground)
            .font_weight(FontWeight::MEDIUM)
            .child(header(PlayerColumn::Team).w(TEAM))
            .child(header(PlayerColumn::Name).flex_1().min_w(NAME_MIN))
            .child(header(PlayerColumn::Class).w(CLASSES))
            .children(columns.iter().map(|(c, _)| header(*c).w(NUMBER))),
    );
    let assets = &cx.global::<Services>().assets;
    let players = log.players();
    for i in sort_players(players, sort) {
        let player = &players[i];
        // Classes come longest first; the main one is solid, the rest fade.
        let icons = player.classes().iter().enumerate().map(|(n, c)| {
            let icon = class_icon(c.class(), assets.class_icon(c.class()).as_deref(), ICON);
            div()
                .flex_none()
                .when(n > 0, |el| el.opacity(0.5))
                .child(icon)
        });
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
                .children(columns.iter().map(|(_, cell)| {
                    let text = cell(player);
                    let zero = text == "0";
                    super::number(text).when(zero, |el| el.text_color(cx.theme().muted_foreground))
                })),
        );
    }
    table
}

// logs.tf's number columns in its order, without the ones the log did not record.
fn player_columns(c: &LogColumns) -> Vec<Column> {
    use PlayerColumn as P;
    let all: [(bool, Column); 14] = [
        (true, (P::Kills, |p| p.kills().to_string())),
        (true, (P::Assists, |p| p.assists().to_string())),
        (true, (P::Deaths, |p| p.deaths().to_string())),
        (true, (P::Damage, |p| p.dmg().to_string())),
        (true, (P::DamagePerMin, |p| p.da_per_min().to_string())),
        (true, (P::KaPerD, |p| one_decimal(p.kapd()))),
        (true, (P::KPerD, |p| one_decimal(p.kpd()))),
        (c.dt(), (P::DamageTaken, |p| p.dt().to_string())),
        (c.dt(), (P::DamageTakenPerMin, |p| whole(p.dt_per_min()))),
        (c.hp(), (P::Medkits, |p| p.medkits().to_string())),
        (c.bs(), (P::Backstabs, |p| p.backstabs().to_string())),
        (c.hs(), (P::Headshots, |p| p.headshots().to_string())),
        (c.airshots(), (P::Airshots, |p| p.airshots().to_string())),
        (c.caps(), (P::Caps, |p| p.caps().to_string())),
    ];
    all.into_iter()
        .filter_map(|(shown, column)| shown.then_some(column))
        .collect()
}
