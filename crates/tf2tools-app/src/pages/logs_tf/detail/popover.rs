//! The class card that opens over a class icon in the players table, as logs.tf's does.

use std::time::Duration;

use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity,
    div, px,
};
use tf2tools_core::{PlayerLine, SteamId, TfClass, WeaponNames};

use crate::ids;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::tables::{WeaponRow, weapon_rows};
use crate::pages::logs_tf::text::duration_text;

// logs.tf opens its card a tenth of a second after the pointer arrives.
const OPEN_DELAY: Duration = Duration::from_millis(100);
// The kills and damage columns fit the fixtures' widest, "46 (100%)" and "14029 (100%)"; the
// weapon name takes what is left.
const CARD_WIDTH: Pixels = px(360.);
const KILLS: Pixels = px(80.);
const DAMAGE: Pixels = px(104.);
const ACCURACY: Pixels = px(40.);

// What one card shows, worded once the card opens.
struct Card {
    id: SteamId,
    class: TfClass,
    facts: [(&'static str, String); 5],
    weapons: Vec<WeaponRow>,
}

/// `icon` wrapped in a hover card showing `player`'s time, kills and weapons on `class`. The
/// card reads the open log from `page` only while it is open, so a closed one costs nothing.
pub(super) fn class_hover(
    page: WeakEntity<LogsTfPage>,
    player: SteamId,
    class: TfClass,
    icon: impl IntoElement + 'static,
) -> HoverCard {
    HoverCard::new(ids::logstf_class_hover(player, class))
        .open_delay(OPEN_DELAY)
        .trigger(icon)
        .content(move |_, _, cx| {
            let card = page.upgrade().and_then(|page| {
                let page = page.read(cx);
                card_for(
                    page.detail()?.players(),
                    page.weapon_names(cx),
                    player,
                    class,
                )
            });
            div().children(card.map(|card| class_card(&card, cx)))
        })
}

// `player`'s numbers on `class` in the open log, worded for the card.
fn card_for(
    players: &[PlayerLine],
    names: &WeaponNames,
    player: SteamId,
    class: TfClass,
) -> Option<Card> {
    let line = players.iter().find(|p| p.steam_id() == player)?;
    let on = line.classes().iter().find(|c| c.class() == class)?;
    Some(Card {
        id: player,
        class,
        facts: [
            ("Played", duration_text(on.seconds())),
            ("K", on.kills().to_string()),
            ("A", on.assists().to_string()),
            ("D", on.deaths().to_string()),
            ("DA", on.dmg().to_string()),
        ],
        weapons: weapon_rows(line, on, names),
    })
}

// The class's totals in a row, then one line per weapon with its shares and accuracy.
fn class_card(card: &Card, cx: &App) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    let cell = |width: Pixels| div().w(width).flex_none().text_right();
    let column = |title: &'static str, width: Pixels| {
        cell(width)
            .id(ids::logstf_card_column(card.id, card.class, title))
            .test_support()
            .child(title)
    };
    let facts = h_flex()
        .gap_3()
        .children(card.facts.iter().map(|(label, value)| {
            v_flex()
                .child(div().text_xs().text_color(muted).child(*label))
                .child(value.clone())
        }));
    let head = h_flex()
        .gap_1()
        .pt_2()
        .text_color(muted)
        .font_weight(FontWeight::MEDIUM)
        .child(div().flex_1().child("Weapon"))
        .child(column("K", KILLS))
        .child(column("DA", DAMAGE))
        .child(cell(ACCURACY).child("Acc"));
    let rows = card.weapons.iter().map(|w| {
        let kills = format!("{} ({})", w.kills, w.kill_share);
        let dmg = format!("{} ({})", w.dmg, w.dmg_share);
        let label: SharedString = format!("{} {kills} {dmg} {}", w.name, w.accuracy).into();
        h_flex()
            .id(ids::logstf_weapon(card.id, card.class, &w.raw))
            .aria_label(label)
            .test_support()
            .gap_1()
            .py_0p5()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().flex_1().min_w_0().truncate().child(w.name.clone()))
            .child(cell(KILLS).child(kills))
            .child(cell(DAMAGE).child(dmg))
            .child(cell(ACCURACY).child(w.accuracy.clone()))
    });
    v_flex()
        .id(ids::logstf_class_popover(card.id, card.class))
        .test_support()
        .w(CARD_WIDTH)
        .gap_1()
        .text_sm()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(card.class.name()),
        )
        .child(facts)
        .child(head)
        .children(rows)
}
