//! The class card that opens over a class icon in the players table, as logs.tf's does.

use std::time::Duration;

use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use tf2tools_core::{ClassLine, PlayerLine, SteamId, TfClass};

use crate::ids;
use crate::pages::logs_tf::tables::{WeaponRow, weapon_rows};
use crate::pages::logs_tf::text::duration_text;

// logs.tf opens its card a tenth of a second after the pointer arrives.
const OPEN_DELAY: Duration = Duration::from_millis(100);
const CARD_WIDTH: Pixels = px(320.);
const CELL: Pixels = px(64.);

// What one card shows, worded up front so the hover card's builder owns its data.
struct Card {
    id: SteamId,
    class: TfClass,
    facts: [(&'static str, String); 5],
    weapons: Vec<WeaponRow>,
}

/// `icon` wrapped in a hover card showing `player`'s time, kills and weapons on `class`.
pub(super) fn class_hover(
    player: &PlayerLine,
    class: &ClassLine,
    icon: impl IntoElement + 'static,
) -> HoverCard {
    let card = Card {
        id: player.steam_id(),
        class: class.class(),
        facts: [
            ("Played", duration_text(class.seconds())),
            ("K", class.kills().to_string()),
            ("A", class.assists().to_string()),
            ("D", class.deaths().to_string()),
            ("DA", class.dmg().to_string()),
        ],
        weapons: weapon_rows(player, class),
    };
    HoverCard::new(ids::logstf_class_hover(card.id, card.class))
        .open_delay(OPEN_DELAY)
        .trigger(icon)
        .content(move |_, _, cx| class_card(&card, cx))
}

// The class's totals in a row, then one line per weapon with its shares and accuracy.
fn class_card(card: &Card, cx: &App) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    let cell = || div().w(CELL).flex_none().text_right();
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
        .child(cell().child("K"))
        .child(cell().child("DA"))
        .child(cell().w(px(40.)).child("Acc"));
    let rows = card.weapons.iter().map(|w| {
        let kills = format!("{} ({})", w.kills, w.kill_share);
        let dmg = format!("{} ({})", w.dmg, w.dmg_share);
        let label: SharedString = format!("{} {kills} {dmg} {}", w.name, w.accuracy).into();
        h_flex()
            .id(ids::logstf_weapon(card.id, card.class, &w.name))
            .aria_label(label)
            .test_support()
            .gap_1()
            .py_0p5()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().flex_1().truncate().child(w.name.clone()))
            .child(cell().child(kills))
            .child(cell().child(dmg))
            .child(cell().w(px(40.)).child(w.accuracy.clone()))
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
