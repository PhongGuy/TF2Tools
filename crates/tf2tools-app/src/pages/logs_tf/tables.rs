//! The open log's table rows shaped from core's lines, free of UI so tests read them without a
//! window.

use tf2tools_core::{
    ClassLine, LogDetail, MatrixKind, MatrixRow, MedicLine, PlayerLine, SteamId, Team, TfClass,
};

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

/// Every player's row of `kind`, K high to low, ties in the players' order.
pub fn matrix_rows(log: &LogDetail, kind: MatrixKind) -> Vec<MatrixRow> {
    // Core lists every player in the players' order, so a stable sort keeps that under ties.
    let mut rows = log.class_matrix(kind);
    rows.sort_by_key(|r| std::cmp::Reverse(r.total()));
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
