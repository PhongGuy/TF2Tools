//! The open log's players table, sortable by any header as on logs.tf.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Div, FontWeight, InteractiveElement as _, ParentElement as _, Pixels, Styled as _,
    TestSupportExt as _, WeakEntity, div, px,
};
use tf2tools_core::{LogColumns, LogDetail, PlayerLine, SteamId};

use super::popover::class_hover;
use super::{ICON, NUMBER, team_cell};
use crate::ids;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::sort::{PlayerColumn, PlayerSort, sort_players};
use crate::pages::logs_tf::text::{one_decimal, reads_zero, whole};
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
    // A kept sort by a column this log hides lists in the default order, but stays saved.
    let shown = sort.shown_in(log.columns());
    let header = |column: PlayerColumn| {
        let page = page.clone();
        Button::new(ids::logstf_sort(column.title()))
            .ghost()
            .xsmall()
            .label(column.title())
            .selected(shown.is_some_and(|s| s.column == column))
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
    for i in sort_players(players, shown.unwrap_or_default()) {
        let player = &players[i];
        // Classes come longest first; the main one is solid, the rest fade.
        // Hovering one opens its class card.
        let icons = player.classes().iter().enumerate().map(|(n, c)| {
            let icon = class_icon(c.class(), assets.class_icon(c.class()).as_deref(), ICON);
            let icon = div()
                .id(ids::logstf_class_card(player.steam_id(), c.class()))
                .test_support()
                .flex_none()
                .when(n > 0, |el| el.opacity(0.5))
                .child(icon);
            class_hover(player, c, icon)
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
                    let zero = reads_zero(&text);
                    super::number(text).when(zero, |el| el.text_color(cx.theme().muted_foreground))
                })),
        );
    }
    table
}

// logs.tf's number columns in its order, without the ones the log did not record.
fn player_columns(c: &LogColumns) -> Vec<Column> {
    use PlayerColumn as P;
    let all: [Column; 14] = [
        (P::Kills, |p| p.kills().to_string()),
        (P::Assists, |p| p.assists().to_string()),
        (P::Deaths, |p| p.deaths().to_string()),
        (P::Damage, |p| p.dmg().to_string()),
        (P::DamagePerMin, |p| p.da_per_min().to_string()),
        (P::KaPerD, |p| one_decimal(p.kapd())),
        (P::KPerD, |p| one_decimal(p.kpd())),
        (P::DamageTaken, |p| p.dt().to_string()),
        (P::DamageTakenPerMin, |p| whole(p.dt_per_min())),
        (P::Medkits, |p| p.medkits().to_string()),
        (P::Backstabs, |p| p.backstabs().to_string()),
        (P::Headshots, |p| p.headshots().to_string()),
        (P::Airshots, |p| p.airshots().to_string()),
        (P::Caps, |p| p.caps().to_string()),
    ];
    all.into_iter()
        .filter(|(column, _)| column.recorded(c))
        .collect()
}
