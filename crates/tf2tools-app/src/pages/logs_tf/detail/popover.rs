//! The class card that opens over a class icon in the players table, as logs.tf's does.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderImage,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity,
    div, px,
};
use tf2tools_core::{PlayerLine, SteamId, TfClass, WeaponNames};

use crate::ids;
use crate::kill_icons::{ICON_SLOT, KillIconImages, SLOT_GAP, icon_slot, weapon_icon};
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::logs_tf::tables::{WeaponRow, weapon_rows};
use crate::pages::logs_tf::text::duration_text;

// logs.tf opens its card a tenth of a second after the pointer arrives.
const OPEN_DELAY: Duration = Duration::from_millis(100);
// The kills and damage columns fit the fixtures' widest, "46 (100%)" and "14029 (100%)"; the
// weapon name takes what is left, and the card widens by the icon slot when it has one.
const CARD_WIDTH: Pixels = px(360.);
const KILLS: Pixels = px(80.);
const DAMAGE: Pixels = px(104.);
const ACCURACY: Pixels = px(40.);

// What one card shows, worded once the card opens; each weapon carries its kill icon if any.
struct Card {
    id: SteamId,
    class: TfClass,
    facts: [(&'static str, String); 5],
    weapons: Vec<(WeaponRow, Option<Arc<RenderImage>>)>,
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
                    page.kill_icons(cx),
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
    icons: &KillIconImages,
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
        weapons: weapon_rows(line, on, names)
            .into_iter()
            .map(|row| {
                let icon = icons.get(&row.raw).cloned();
                (row, icon)
            })
            .collect(),
    })
}

// The class's totals in a row, then one line per weapon with its shares and accuracy.
fn class_card(card: &Card, cx: &App) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    let cell = |width: Pixels| div().w(width).flex_none().text_right();
    // Once any weapon has an icon, every row keeps a slot for one so the names line up.
    let slotted = card.weapons.iter().any(|(_, icon)| icon.is_some());
    let width = if slotted {
        CARD_WIDTH + ICON_SLOT + SLOT_GAP
    } else {
        CARD_WIDTH
    };
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
    let rows = card.weapons.iter().map(|(w, icon)| {
        let icon = icon
            .as_ref()
            .map(|image| weapon_icon(ids::logstf_weapon_icon(card.id, card.class, &w.raw), image));
        let name = h_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .items_center()
            .children(slotted.then(|| icon_slot(icon)))
            .child(
                div()
                    .id(ids::logstf_weapon_name(card.id, card.class, &w.raw))
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(w.name.clone()),
            );
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
            .child(name)
            .child(cell(KILLS).child(kills))
            .child(cell(DAMAGE).child(dmg))
            .child(cell(ACCURACY).child(w.accuracy.clone()))
    });
    v_flex()
        .id(ids::logstf_class_popover(card.id, card.class))
        .test_support()
        .w(width)
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
