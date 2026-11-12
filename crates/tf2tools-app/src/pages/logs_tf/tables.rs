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

/// `class`'s weapons it can equip by kills high to low (ties keep core's order), rows of one
/// shown name merged; shares are of the player's whole-log kills and damage.
pub fn weapon_rows(player: &PlayerLine, class: &ClassLine, names: &WeaponNames) -> Vec<WeaponRow> {
    let mut weapons: Vec<_> = class.weapons().iter().collect();
    weapons.sort_by_key(|w| std::cmp::Reverse(w.kills()));
    let mut tallies = class_tallies(
        class.class(),
        names,
        weapons
            .into_iter()
            .map(|w| (w.name(), w.kills(), w.dmg(), w.hits(), w.shots())),
    );
    // Merged rows can outgrow the ones above them, so they take their place by kills again.
    tallies.sort_by_key(|t| std::cmp::Reverse(t.kills));
    tallies
        .into_iter()
        .map(|t| t.row(player.kills(), player.dmg()))
        .collect()
}

/// The class weapons block's rows for `class` in core's order (damage high to low), leaving out
/// weapons the game says `class` cannot equip, which logs.tf files there after a class switch,
/// and merging rows of one shown name.
pub fn class_weapon_rows(
    class: TfClass,
    weapons: &ClassWeapons,
    names: &WeaponNames,
) -> Vec<WeaponRow> {
    let narrow = |n: i64| u32::try_from(n).unwrap_or(u32::MAX);
    let mut tallies = class_tallies(
        class,
        names,
        weapons.weapons().iter().map(|w: &WeaponSum| {
            let (kills, dmg) = (narrow(w.kills()), narrow(w.dmg()));
            (w.name(), kills, dmg, narrow(w.hits()), narrow(w.shots()))
        }),
    );
    tallies.sort_by_key(|t| std::cmp::Reverse(t.dmg));
    // Shares are of the shown rows' summed kills and damage, so they still add to about 100%.
    let kills = tallies
        .iter()
        .fold(0u32, |sum, t| sum.saturating_add(t.kills));
    let dmg = tallies
        .iter()
        .fold(0u32, |sum, t| sum.saturating_add(t.dmg));
    tallies.into_iter().map(|t| t.row(kills, dmg)).collect()
}

// One shown weapon line before wording: the first log name's id and the summed counts, hits and
// shots summed over the sides that fired shots.
struct Tally {
    raw: String,
    name: String,
    kills: u32,
    dmg: u32,
    hits: u32,
    shots: u32,
}

impl Tally {
    fn row(self, all_kills: u32, all_dmg: u32) -> WeaponRow {
        WeaponRow {
            raw: self.raw,
            name: self.name,
            kills: self.kills,
            kill_share: share_text(self.kills, all_kills),
            dmg: self.dmg,
            dmg_share: share_text(self.dmg, all_dmg),
            accuracy: accuracy_text(self.hits, self.shots),
        }
    }
}

// `weapons` (log name, kills, damage, hits, shots) that `class` can equip, named for `class`,
// in order with each later row of an already shown name folded into the first.
fn class_tallies<'a>(
    class: TfClass,
    names: &WeaponNames,
    weapons: impl Iterator<Item = (&'a str, u32, u32, u32, u32)>,
) -> Vec<Tally> {
    let mut tallies: Vec<Tally> = Vec::new();
    for (raw, kills, dmg, hits, shots) in weapons.filter(|w| names.usable_by(w.0, class)) {
        let name = names.class_name(raw, class).unwrap_or(raw);
        // A side without shots adds no hits, so a stray hit count cannot skew the accuracy.
        let hits = if shots > 0 { hits } else { 0 };
        if let Some(t) = tallies.iter_mut().find(|t| t.name == name) {
            t.kills = t.kills.saturating_add(kills);
            t.dmg = t.dmg.saturating_add(dmg);
            t.hits = t.hits.saturating_add(hits);
            t.shots = t.shots.saturating_add(shots);
        } else {
            tallies.push(Tally {
                raw: raw.to_owned(),
                name: name.to_owned(),
                kills,
                dmg,
                hits,
                shots,
            });
        }
    }
    tallies
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
    pub steam_id: Option<SteamId>,
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
            steam_id: line.steam_id(),
            team: line.steam_id().and_then(team_of),
            name: line.name().to_owned(),
            msg: line.msg().to_owned(),
        })
        .collect()
}
