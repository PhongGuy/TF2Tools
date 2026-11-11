//! The open log's table rows shaped from core's lines, free of UI so tests read them without a
//! window.

use tf2tools_core::{ClassLine, PlayerLine};

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
