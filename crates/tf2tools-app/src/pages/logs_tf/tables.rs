//! The open log's table rows shaped from core's lines, free of UI so tests read them without a
//! window.

use tf2tools_core::{
    ClassLine, ClassWeapons, LogDetail, MatrixKind, MatrixRow, MedicLine, PlayerLine, SteamId,
    Team, TfClass, WeaponNames, WeaponSum,
};

use super::text::{accuracy_text, floored_share_text, share_text};

/// One weapon's line in a class card, its shares already worded. `raw` is the logs.tf name the
/// row's id uses; `name` is what it shows, the game's name when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponRow {
    pub raw: String,
    pub name: String,
    pub kills: u32,
    pub kill_share: String,
    pub dmg: u32,
    pub dmg_share: String,
    pub accuracy: String,
}

/// `class`'s weapons by kills high to low (ties keep core's order); shares are of the player's
/// whole-log kills and damage.
pub fn weapon_rows(player: &PlayerLine, class: &ClassLine, names: &WeaponNames) -> Vec<WeaponRow> {
    let mut weapons: Vec<_> = class.weapons().iter().collect();
    weapons.sort_by_key(|w| std::cmp::Reverse(w.kills()));
    weapons
        .into_iter()
        .map(|w| WeaponRow {
            raw: w.name().to_owned(),
            name: shown_name(w.name(), names),
            kills: w.kills(),
            kill_share: share_text(w.kills(), player.kills()),
            dmg: w.dmg(),
            dmg_share: share_text(w.dmg(), player.dmg()),
            accuracy: accuracy_text(w.hits(), w.shots()),
        })
        .collect()
}

/// The class weapons panel's rows in core's order (damage high to low); shares are of the
/// class's summed weapon kills and damage, so they add to about 100%.
pub fn class_weapon_rows(weapons: &ClassWeapons, names: &WeaponNames) -> Vec<WeaponRow> {
    let narrow = |n: i64| u32::try_from(n).unwrap_or(u32::MAX);
    let kills = narrow(weapons.weapons().iter().map(WeaponSum::kills).sum());
    let dmg = narrow(weapons.weapons().iter().map(WeaponSum::dmg).sum());
    weapons
        .weapons()
        .iter()
        .map(|w| WeaponRow {
            raw: w.name().to_owned(),
            name: shown_name(w.name(), names),
            kills: narrow(w.kills()),
            kill_share: share_text(narrow(w.kills()), kills),
            dmg: narrow(w.dmg()),
            dmg_share: share_text(narrow(w.dmg()), dmg),
            accuracy: accuracy_text(narrow(w.hits()), narrow(w.shots())),
        })
        .collect()
}

// The game's name for a logs.tf weapon, or the log name when the game has none.
fn shown_name(raw: &str, names: &WeaponNames) -> String {
    names.get(raw).unwrap_or(raw).to_owned()
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
            share: floored_share_text(t.healing(), medic.heal()),
        })
        .collect()
}

/// Each player with a count in `kind`, with their row, K high to low, ties in the players'
/// order. logs.tf leaves out a player with none, such as a medic without kills.
pub fn matrix_rows(log: &LogDetail, kind: MatrixKind) -> Vec<(&PlayerLine, &MatrixRow)> {
    // Core lists one row per player in the players' order, so a row pairs with its player by
    // position and a stable sort keeps that order under ties.
    let mut rows: Vec<_> = log
        .players()
        .iter()
        .zip(log.class_matrix(kind))
        .filter(|(_, row)| row.total() > 0)
        .collect();
    rows.sort_by_key(|(_, r)| std::cmp::Reverse(r.total()));
    rows
}

/// One chat message with its sender's team; `None` for a sender who is not a player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRow {
    pub team: Option<Team>,
    pub name: String,
    pub msg: String,
}

/// The log's chat in the order it was said, each line's team read from its sender's player
/// line, so the console and anyone missing from the players carry none.
pub fn chat_rows(log: &LogDetail) -> Vec<ChatRow> {
    let team_of = |id: SteamId| {
        log.players()
            .iter()
            .find(|p| p.steam_id() == id)
            .map(PlayerLine::team)
    };
    log.chat()
        .iter()
        .map(|line| ChatRow {
            team: line.steam_id().and_then(team_of),
            name: line.name().to_owned(),
            msg: line.msg().to_owned(),
        })
        .collect()
}
