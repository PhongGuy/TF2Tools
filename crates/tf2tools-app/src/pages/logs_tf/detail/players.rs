//! The open log's players table, sortable by any header as on logs.tf.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, div, px,
};
use tf2tools_core::{LogColumns, LogDetail, PlayerLine, SteamId};

use super::popover::class_hover;
use super::{NUMBER, team_cell};
use crate::ids;
use crate::pages::hud_maker::tip::tip;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::sort::{PlayerColumn, PlayerSort, sort_players};
use crate::pages::logs_tf::text::{one_decimal, reads_zero, whole};
use crate::services::Services;
use crate::widgets::class_icon::class_icon;

// The table's fixed columns. The name is sized at its minimum so a long one never widens the
// table, and grows into any room the window leaves, truncating what still does not fit.
const TEAM: Pixels = px(40.);
const NAME_MIN: Pixels = px(100.);
// A class icon, a size up from the other tables' because the art leaves empty borders; and
// room for four of them with their gaps, as many as a player shows before the rest clip.
const CLASS_ICON: Pixels = px(24.);
const CLASSES: Pixels = px(102.);
// The sort arrow's row under every header, kept empty when unsorted so sorting never moves
// the table; an xsmall icon's height.
const ARROW: Pixels = px(12.);

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
    // The sorted header carries an arrow under its title, since the columns are too narrow
    // to fit one beside it.
    let header = |column: PlayerColumn| {
        let page = page.clone();
        let sorted = shown.filter(|s| s.column == column);
        let button = Button::new(ids::logstf_sort(column.title()))
            .ghost()
            .xsmall()
            .label(column.title())
            .selected(sorted.is_some())
            .w_full()
            .on_click(move |_, _, cx| {
                let _ = page.update(cx, |page, cx| page.sort_players_by(column, cx));
            });
        let arrow = div().h(ARROW).flex_none().when_some(sorted, |el, sort| {
            el.child(sort_arrow(column, sort.descending))
        });
        v_flex()
            .flex_none()
            .items_center()
            .child(button)
            .child(arrow)
    };
    let mut table = v_flex().w_full().child(
        h_flex()
            .id(ids::LOGSTF_PLAYERS_HEAD)
            .test_support()
            .items_start()
            .gap_1()
            .px_1()
            .py_1()
            .text_color(cx.theme().muted_foreground)
            .font_weight(FontWeight::MEDIUM)
            .child(header(PlayerColumn::Team).w(TEAM))
            .child(header(PlayerColumn::Name).w(NAME_MIN).flex_grow(1.))
            .child(header(PlayerColumn::Class).w(CLASSES).ml_2())
            .children(columns.iter().map(|(c, _)| header(*c).w(NUMBER))),
    );
    let assets = &cx.global::<Services>().assets;
    let players = log.players();
    for i in sort_players(players, shown.unwrap_or_default()) {
        let player = &players[i];
        // Classes come longest first; the main one is solid, the rest fade.
        // Hovering one opens its class card.
        let icons = player.classes().iter().enumerate().map(|(n, c)| {
            let class = c.class();
            let icon = class_icon(class, assets.class_icon(class).as_deref(), CLASS_ICON);
            let icon = div()
                .id(ids::logstf_class_card(player.steam_id(), class))
                .test_support()
                .flex_none()
                .when(n > 0, |el| el.opacity(0.5))
                .child(icon);
            class_hover(page.clone(), player.steam_id(), c.class(), icon)
        });
        table = table.child(
            super::player_row(
                h_flex().id(ids::logstf_player_row(player.steam_id())),
                is_tracked(player.steam_id()),
                cx,
            )
            .test_support()
            .gap_1()
            .px_1()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(team_cell(player.team(), cx).w(TEAM))
            .child(
                div()
                    .id(ids::logstf_player_name(player.steam_id()))
                    .test_support()
                    .tooltip(tip(ids::LOGSTF_NAME_TIP, player.name().to_owned()))
                    .w(NAME_MIN)
                    .flex_grow(1.)
                    .truncate()
                    .child(player.name().to_owned()),
            )
            .child(
                // Kept clear of a cut-off name before it.
                h_flex()
                    .w(CLASSES)
                    .ml_2()
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

// The sorted column's direction as a small chevron, said in words for readers.
fn sort_arrow(column: PlayerColumn, descending: bool) -> impl IntoElement {
    let (icon, words) = if descending {
        (IconName::ChevronDown, "high to low")
    } else {
        (IconName::ChevronUp, "low to high")
    };
    div()
        .id(ids::logstf_sort_arrow(column.title()))
        .aria_label(words)
        .test_support()
        .child(Icon::new(icon).xsmall())
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
