//! The open log's table rows shaped from core's lines, free of UI so tests read them without a
//! window.

use tf2tools_core::{ClassLine, MedicLine, PlayerLine, SteamId, TfClass};

use super::text::{accuracy_text, share_text};

/// One weapon's line in a class card, its shares already worded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponRow {
    pub name: String,
    pub kills: u32,
    pub kill_share: String,
    pub dmg: u32,
    pub dmg_share: String,
    pub accuracy: String,
}

/// `class`'s weapons by kills high to low (ties keep core's order); shares are of the player's
/// whole-log kills and damage.
pub fn weapon_rows(player: &PlayerLine, class: &ClassLine) -> Vec<WeaponRow> {
    let mut weapons: Vec<_> = class.weapons().iter().collect();
    weapons.sort_by_key(|w| std::cmp::Reverse(w.kills()));
    weapons
        .into_iter()
        .map(|w| WeaponRow {
            name: w.name().to_owned(),
            kills: w.kills(),
            kill_share: share_text(w.kills(), player.kills()),
            dmg: w.dmg(),
            dmg_share: share_text(w.dmg(), player.dmg()),
            accuracy: accuracy_text(w.hits(), w.shots()),
        })
        .collect()
}

/// One player a Medic healed, with their share of the Medic's healing worded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealRow {
    pub steam_id: SteamId,
    pub name: String,
    pub class: Option<TfClass>,
    pub heals: u32,
    pub share: String,
}

/// `medic`'s heal targets by heals high to low, each share of the Medic's whole healing.
pub fn heal_rows(medic: &MedicLine) -> Vec<HealRow> {
    // Core already lists the spread most healing first, so the order carries over.
    medic
        .heal_spread()
        .iter()
        .map(|t| HealRow {
            steam_id: t.steam_id(),
            name: t.name().to_owned(),
            class: t.class(),
            heals: t.healing(),
            share: share_text(t.healing(), medic.heal()),
        })
        .collect()
}
