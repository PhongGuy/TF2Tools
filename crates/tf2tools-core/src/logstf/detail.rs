//! A stored logs.tf `/json/<id>` body read into the expanded log view's shape.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::{SteamId, logstf_class, logstf_class_name};
use crate::Team;
use crate::error::LogParseError;
use crate::weapons::TfClass;

/// One log as the expanded view shows it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LogDetail {
    length: u32,
    title: String,
    map: String,
    date: i64,
    red: TeamTotals,
    blue: TeamTotals,
    players: Vec<PlayerLine>,
    columns: LogColumns,
    medics: Vec<MedicLine>,
    class_kills: Vec<MatrixRow>,
    class_kill_assists: Vec<MatrixRow>,
    class_deaths: Vec<MatrixRow>,
    rounds: Vec<RoundLine>,
    killstreaks: Vec<Killstreak>,
    chat: Vec<LogChatLine>,
}

impl LogDetail {
    /// The log's length in seconds.
    pub fn length(&self) -> u32 {
        self.length
    }
    /// The uploader's title, empty when missing.
    pub fn title(&self) -> &str {
        &self.title
    }
    /// The map name, empty when missing.
    pub fn map(&self) -> &str {
        &self.map
    }
    /// The upload time in Unix seconds, 0 when missing.
    pub fn date(&self) -> i64 {
        self.date
    }
    /// Red's totals.
    pub fn red(&self) -> &TeamTotals {
        &self.red
    }
    /// Blue's totals.
    pub fn blue(&self) -> &TeamTotals {
        &self.blue
    }
    /// Red first, then by longest-played class in TF2 order, then by name.
    pub fn players(&self) -> &[PlayerLine] {
        &self.players
    }
    /// Which optional columns this log recorded.
    pub fn columns(&self) -> &LogColumns {
        &self.columns
    }
    /// Every player logs.tf kept Medic stats for, in the players' order.
    pub fn medics(&self) -> &[MedicLine] {
        &self.medics
    }
    /// One row per player in the players' order, counting `kind` by the other player's
    /// class; a player logs.tf has no entry for reads as zeros.
    pub fn class_matrix(&self, kind: MatrixKind) -> Vec<MatrixRow> {
        match kind {
            MatrixKind::Kills => &self.class_kills,
            MatrixKind::KillsAssists => &self.class_kill_assists,
            MatrixKind::Deaths => &self.class_deaths,
        }
        .clone()
    }
    /// The rounds in play order; empty when the log has none or they do not read.
    pub fn rounds(&self) -> &[RoundLine] {
        &self.rounds
    }
    /// Killstreaks by the time they happened; tied times keep logs.tf's order.
    pub fn killstreaks(&self) -> &[Killstreak] {
        &self.killstreaks
    }
    /// The chat in the order it was said.
    pub fn chat(&self) -> &[LogChatLine] {
        &self.chat
    }
}

/// One player's killstreak.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Killstreak {
    steam_id: SteamId,
    streak: u32,
    at: u32,
}

impl Killstreak {
    /// The player on the streak.
    pub fn steam_id(&self) -> SteamId {
        self.steam_id
    }
    /// Kills in the streak.
    pub fn streak(&self) -> u32 {
        self.streak
    }
    /// Seconds since the first round's start.
    pub fn at(&self) -> u32 {
        self.at
    }
}

/// One chat message.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LogChatLine {
    steam_id: Option<SteamId>,
    name: String,
    msg: String,
}

impl LogChatLine {
    /// The sender, `None` for a non-player such as the console.
    pub fn steam_id(&self) -> Option<SteamId> {
        self.steam_id
    }
    /// The sender's name as logs.tf shows it.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The message text.
    pub fn msg(&self) -> &str {
        &self.msg
    }
}

/// One round: its result, each team's share and the events on the log's clock.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RoundLine {
    length: u32,
    winner: Option<Team>,
    midfight: Option<Team>,
    blue: RoundTeam,
    red: RoundTeam,
    players: Vec<RoundPlayer>,
    events: Vec<RoundEvent>,
}

impl RoundLine {
    /// The round's length in seconds.
    pub fn length(&self) -> u32 {
        self.length
    }
    /// The team that won the round, `None` for a stalemate or an unfinished round.
    pub fn winner(&self) -> Option<Team> {
        self.winner
    }
    /// The team that took the first point, `None` when no point was taken.
    pub fn midfight(&self) -> Option<Team> {
        self.midfight
    }
    /// Blue's share of the round.
    pub fn blue(&self) -> &RoundTeam {
        &self.blue
    }
    /// Red's share of the round.
    pub fn red(&self) -> &RoundTeam {
        &self.red
    }
    /// Players by kills, then damage (both high to low), then SteamID64.
    pub fn players(&self) -> &[RoundPlayer] {
        &self.players
    }
    /// The round's events in the log's order.
    pub fn events(&self) -> &[RoundEvent] {
        &self.events
    }
}

/// One team's share of one round; `score` is the running score after it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RoundTeam {
    score: u32,
    kills: u32,
    dmg: u32,
    ubers: u32,
}

impl RoundTeam {
    /// Rounds won so far, this one included.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Kills in the round.
    pub fn kills(&self) -> u32 {
        self.kills
    }
    /// Damage dealt in the round.
    pub fn dmg(&self) -> u32 {
        self.dmg
    }
    /// Ubercharges used in the round.
    pub fn ubers(&self) -> u32 {
        self.ubers
    }
}

/// One player's line in one round.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RoundPlayer {
    steam_id: SteamId,
    team: Team,
    kills: u32,
    dmg: u32,
}

impl RoundPlayer {
    /// The player's Steam ID.
    pub fn steam_id(&self) -> SteamId {
        self.steam_id
    }
    /// The player's team.
    pub fn team(&self) -> Team {
        self.team
    }
    /// Kills in the round.
    pub fn kills(&self) -> u32 {
        self.kills
    }
    /// Damage dealt in the round.
    pub fn dmg(&self) -> u32 {
        self.dmg
    }
}

/// One event in a round.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RoundEvent {
    at: u32,
    kind: EventKind,
}

impl RoundEvent {
    /// Seconds since the first round's start.
    pub fn at(&self) -> u32 {
        self.at
    }
    /// What happened.
    pub fn kind(&self) -> &EventKind {
        &self.kind
    }
}

/// What a round event records; a type this app does not know, or a known one missing its
/// team or medic, keeps its raw name in `Other`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventKind {
    /// A team captured a control point.
    PointCap {
        /// The capturing team.
        team: Team,
        /// The point's number.
        point: u32,
    },
    /// A Medic used a charge.
    Charge {
        /// The Medic's team.
        team: Team,
        /// The Medic.
        medic: SteamId,
        /// The medigun charged.
        medigun: Medigun,
    },
    /// A Medic died.
    MedicDeath {
        /// The Medic's team.
        team: Team,
        /// The Medic.
        medic: SteamId,
        /// The killer, `None` when the log does not name one.
        killer: Option<SteamId>,
    },
    /// A Medic died holding a full charge.
    Drop {
        /// The Medic's team.
        team: Team,
        /// The Medic.
        medic: SteamId,
    },
    /// A team won the round.
    RoundWin {
        /// The winning team.
        team: Team,
    },
    /// Any other event, by its logs.tf type.
    Other {
        /// The raw type.
        name: String,
    },
}

/// The medigun behind a charge.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Medigun {
    /// The stock Medi Gun.
    MediGun,
    /// The Kritzkrieg.
    Kritzkrieg,
    /// The Quick-Fix.
    QuickFix,
    /// The Vaccinator.
    Vaccinator,
    /// A medigun logs.tf names otherwise, by that name.
    Other(String),
}

impl Medigun {
    /// The medigun logs.tf names `key`; an unknown name is kept in `Other`.
    pub fn from_key(key: &str) -> Medigun {
        match key {
            "medigun" => Medigun::MediGun,
            "kritzkrieg" => Medigun::Kritzkrieg,
            "quickfix" => Medigun::QuickFix,
            "vaccinator" => Medigun::Vaccinator,
            other => Medigun::Other(other.to_owned()),
        }
    }
    /// The name logs.tf uses for this medigun.
    pub fn key(&self) -> &str {
        match self {
            Medigun::MediGun => "medigun",
            Medigun::Kritzkrieg => "kritzkrieg",
            Medigun::QuickFix => "quickfix",
            Medigun::Vaccinator => "vaccinator",
            Medigun::Other(name) => name,
        }
    }
}

/// One team's totals for the whole log.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TeamTotals {
    score: u32,
    kills: u32,
    dmg: u32,
    charges: u32,
    drops: u32,
    caps: u32,
    midfights: u32,
}

impl TeamTotals {
    /// Rounds won.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Kills.
    pub fn kills(&self) -> u32 {
        self.kills
    }
    /// Damage dealt.
    pub fn dmg(&self) -> u32 {
        self.dmg
    }
    /// Ubercharges used.
    pub fn charges(&self) -> u32 {
        self.charges
    }
    /// Ubercharges dropped.
    pub fn drops(&self) -> u32 {
        self.drops
    }
    /// Points captured.
    pub fn caps(&self) -> u32 {
        self.caps
    }
    /// Midfights won (first captures of a round).
    pub fn midfights(&self) -> u32 {
        self.midfights
    }
}

/// One player's row in the players table.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PlayerLine {
    steam_id: SteamId,
    name: String,
    team: Team,
    classes: Vec<ClassLine>,
    kills: u32,
    assists: u32,
    deaths: u32,
    dmg: u32,
    da_per_min: u32,
    kapd: f64,
    kpd: f64,
    dt: u32,
    dt_per_min: f64,
    medkits: u32,
    backstabs: u32,
    headshots: u32,
    airshots: u32,
    caps: u32,
    heal: u32,
    ubers: u32,
    drops: u32,
}

impl PlayerLine {
    /// The player's Steam ID.
    pub fn steam_id(&self) -> SteamId {
        self.steam_id
    }
    /// The name the log recorded, or the id64 when it has none.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The player's team.
    pub fn team(&self) -> Team {
        self.team
    }
    /// The nine classes only, longest played first.
    pub fn classes(&self) -> &[ClassLine] {
        &self.classes
    }
    /// Kills.
    pub fn kills(&self) -> u32 {
        self.kills
    }
    /// Assists.
    pub fn assists(&self) -> u32 {
        self.assists
    }
    /// Deaths.
    pub fn deaths(&self) -> u32 {
        self.deaths
    }
    /// Damage dealt.
    pub fn dmg(&self) -> u32 {
        self.dmg
    }
    /// Damage per minute, as logs.tf computed it.
    pub fn da_per_min(&self) -> u32 {
        self.da_per_min
    }
    /// (Kills + assists) per death.
    pub fn kapd(&self) -> f64 {
        self.kapd
    }
    /// Kills per death.
    pub fn kpd(&self) -> f64 {
        self.kpd
    }
    /// Damage taken.
    pub fn dt(&self) -> u32 {
        self.dt
    }
    /// Damage taken per minute of the log.
    pub fn dt_per_min(&self) -> f64 {
        self.dt_per_min
    }
    /// Medkits picked up.
    pub fn medkits(&self) -> u32 {
        self.medkits
    }
    /// Backstabs.
    pub fn backstabs(&self) -> u32 {
        self.backstabs
    }
    /// Headshots: the hits when the log counts them, else the kills.
    pub fn headshots(&self) -> u32 {
        self.headshots
    }
    /// Airshots.
    pub fn airshots(&self) -> u32 {
        self.airshots
    }
    /// Points captured.
    pub fn caps(&self) -> u32 {
        self.caps
    }
    /// Healing done (health received for non-Medics, as logs.tf records it).
    pub fn heal(&self) -> u32 {
        self.heal
    }
    /// Ubercharges used.
    pub fn ubers(&self) -> u32 {
        self.ubers
    }
    /// Ubercharges dropped.
    pub fn drops(&self) -> u32 {
        self.drops
    }
}

/// One class a player played in the log.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ClassLine {
    class: TfClass,
    seconds: u32,
    kills: u32,
    deaths: u32,
    assists: u32,
    dmg: u32,
    weapons: Vec<WeaponLine>,
}

impl ClassLine {
    /// The class.
    pub fn class(&self) -> TfClass {
        self.class
    }
    /// Seconds played on it.
    pub fn seconds(&self) -> u32 {
        self.seconds
    }
    /// Kills on it.
    pub fn kills(&self) -> u32 {
        self.kills
    }
    /// Deaths on it.
    pub fn deaths(&self) -> u32 {
        self.deaths
    }
    /// Assists on it.
    pub fn assists(&self) -> u32 {
        self.assists
    }
    /// Damage dealt on it.
    pub fn dmg(&self) -> u32 {
        self.dmg
    }
    /// The weapons used on it, most kills first, then most damage, then by name.
    pub fn weapons(&self) -> &[WeaponLine] {
        &self.weapons
    }
}

/// One weapon a player used on one class.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct WeaponLine {
    name: String,
    kills: u32,
    dmg: u32,
    shots: u32,
    hits: u32,
}

impl WeaponLine {
    /// The logs.tf weapon name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Kills with it.
    pub fn kills(&self) -> u32 {
        self.kills
    }
    /// Damage dealt with it.
    pub fn dmg(&self) -> u32 {
        self.dmg
    }
    /// Shots fired.
    pub fn shots(&self) -> u32 {
        self.shots
    }
    /// Shots that hit.
    pub fn hits(&self) -> u32 {
        self.hits
    }
}

/// The optional columns a log recorded; a missing `info` records none.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LogColumns {
    hp: bool,
    bs: bool,
    hs: bool,
    airshots: bool,
    caps: bool,
    dt: bool,
}

impl LogColumns {
    /// Medkits picked up.
    pub fn hp(&self) -> bool {
        self.hp
    }
    /// Backstabs.
    pub fn bs(&self) -> bool {
        self.bs
    }
    /// Headshots.
    pub fn hs(&self) -> bool {
        self.hs
    }
    /// Airshots.
    pub fn airshots(&self) -> bool {
        self.airshots
    }
    /// Captures.
    pub fn caps(&self) -> bool {
        self.caps
    }
    /// Damage taken.
    pub fn dt(&self) -> bool {
        self.dt
    }
}

/// One Medic's block.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct MedicLine {
    steam_id: SteamId,
    name: String,
    team: Team,
    heal: u32,
    heal_per_min: f64,
    charges: Vec<(Medigun, u32)>,
    drops: u32,
    avg_time_to_build: f64,
    avg_uber_length: f64,
    deaths_after_uber: u32,
    avg_time_before_using: f64,
    near_full_deaths: u32,
    advantages_lost: u32,
    biggest_advantage_lost: u32,
    heal_spread: Vec<HealTarget>,
}

impl MedicLine {
    /// The Medic's Steam ID.
    pub fn steam_id(&self) -> SteamId {
        self.steam_id
    }
    /// The Medic's name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The Medic's team.
    pub fn team(&self) -> Team {
        self.team
    }
    /// Healing done.
    pub fn heal(&self) -> u32 {
        self.heal
    }
    /// Healing per minute of the log.
    pub fn heal_per_min(&self) -> f64 {
        self.heal_per_min
    }
    /// Charges used per medigun, in alphabetical order of the logs.tf key.
    pub fn charges(&self) -> &[(Medigun, u32)] {
        &self.charges
    }
    /// Ubercharges dropped.
    pub fn drops(&self) -> u32 {
        self.drops
    }
    /// Average seconds to build a charge.
    pub fn avg_time_to_build(&self) -> f64 {
        self.avg_time_to_build
    }
    /// Average seconds an uber lasted.
    pub fn avg_uber_length(&self) -> f64 {
        self.avg_uber_length
    }
    /// Deaths within 20 seconds after using an uber.
    pub fn deaths_after_uber(&self) -> u32 {
        self.deaths_after_uber
    }
    /// Average seconds a full charge was held before it was used.
    pub fn avg_time_before_using(&self) -> f64 {
        self.avg_time_before_using
    }
    /// Deaths with a 95 to 99 percent charge.
    pub fn near_full_deaths(&self) -> u32 {
        self.near_full_deaths
    }
    /// Times the Medic lost an uber advantage.
    pub fn advantages_lost(&self) -> u32 {
        self.advantages_lost
    }
    /// The longest uber advantage lost, in seconds.
    pub fn biggest_advantage_lost(&self) -> u32 {
        self.biggest_advantage_lost
    }
    /// The players this Medic healed, most healing first.
    pub fn heal_spread(&self) -> &[HealTarget] {
        &self.heal_spread
    }
}

/// One player a Medic healed.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct HealTarget {
    steam_id: SteamId,
    name: String,
    healing: u32,
    class: Option<TfClass>,
}

impl HealTarget {
    /// The healed player's Steam ID.
    pub fn steam_id(&self) -> SteamId {
        self.steam_id
    }
    /// The healed player's name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Health given to them.
    pub fn healing(&self) -> u32 {
        self.healing
    }
    /// The healed player's longest-played class; `None` when they are not a player in
    /// this log or played none of the nine.
    pub fn class(&self) -> Option<TfClass> {
        self.class
    }
}

/// Which of logs.tf's three class matrices to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MatrixKind {
    /// Kills by victim class (`classkills`).
    Kills,
    /// Kills plus assists by victim class (`classkillassists`).
    KillsAssists,
    /// Deaths by killer class (`classdeaths`).
    Deaths,
}

/// One player's row of a class matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MatrixRow {
    steam_id: SteamId,
    counts: [u32; 9],
    total: u32,
}

impl MatrixRow {
    /// The player's Steam ID.
    pub fn steam_id(&self) -> SteamId {
        self.steam_id
    }
    /// The count against each class, in `TfClass::ALL` order.
    pub fn counts(&self) -> [u32; 9] {
        self.counts
    }
    /// The sum of `counts`.
    pub fn total(&self) -> u32 {
        self.total
    }
}

/// Reads a logs.tf `/json/<id>` body. Only `length`, `teams` and `players` are required;
/// players with an unreadable Steam3 key or team, and classes outside the nine, are skipped.
pub fn parse_log(body: &str) -> Result<LogDetail, LogParseError> {
    let wire: WireLog = serde_json::from_str(body).map_err(|e| LogParseError::Format {
        message: e.to_string(),
    })?;
    Ok(build(wire))
}

/// Turns the wire shape into the view: players first, since medics and class kills follow
/// their order.
fn build(wire: WireLog) -> LogDetail {
    let info = wire.info.unwrap_or_default();
    let minutes = f64::from(wire.length) / 60.0;
    let per_min = |x: u32| {
        if minutes > 0.0 {
            f64::from(x) / minutes
        } else {
            0.0
        }
    };
    let name_of = |key: &str, id: SteamId| {
        wire.names
            .get(key)
            .cloned()
            .unwrap_or_else(|| id.id64().to_string())
    };

    let mut players: Vec<(String, PlayerLine, WirePlayer)> = Vec::new();
    for (key, p) in wire.players {
        let Some(steam_id) = SteamId::from_steam3(&key) else {
            continue;
        };
        let Some(team) = team(&p.team) else {
            continue;
        };
        let line = player_line(&p, steam_id, name_of(&key, steam_id), team, &info, per_min);
        players.push((key, line, p));
    }
    players.sort_by(|a, b| sort_key(&a.1).cmp(&sort_key(&b.1)));
    let class_of: BTreeMap<&str, TfClass> = players
        .iter()
        .filter_map(|(key, line, _)| Some((key.as_str(), line.classes.first()?.class)))
        .collect();

    let medics = players
        .iter()
        .filter_map(|(key, line, p)| Some((key, line, p, p.medicstats.clone()?)))
        .map(|(key, line, p, stats)| MedicLine {
            steam_id: line.steam_id,
            name: line.name.clone(),
            team: line.team,
            heal: p.heal,
            heal_per_min: per_min(p.heal),
            charges: p
                .ubertypes
                .iter()
                .map(|(k, v)| (Medigun::from_key(k), *v))
                .collect(),
            drops: p.drops,
            avg_time_to_build: stats.avg_time_to_build,
            avg_uber_length: stats.avg_uber_length,
            deaths_after_uber: stats.deaths_within_20s_after_uber,
            avg_time_before_using: stats.avg_time_before_using,
            near_full_deaths: stats.deaths_with_95_99_uber,
            advantages_lost: stats.advantages_lost,
            biggest_advantage_lost: stats.biggest_advantage_lost,
            heal_spread: heal_spread(wire.healspread.get(key), &name_of, &class_of),
        })
        .collect();

    // Every player gets a row: a missing entry or class reads as 0, and names fold
    // through logstf_class so `heavyweapons` lands on Heavy.
    let matrix = |wire_map: &BTreeMap<String, BTreeMap<String, u32>>| -> Vec<MatrixRow> {
        players
            .iter()
            .map(|(key, line, _)| {
                let by_class = wire_map.get(key);
                let counts = TfClass::ALL.map(|class| {
                    by_class.map_or(0, |m| {
                        m.iter()
                            .filter(|(name, _)| logstf_class(name) == Some(class))
                            .fold(0u32, |sum, (_, n)| sum.saturating_add(*n))
                    })
                });
                MatrixRow {
                    steam_id: line.steam_id,
                    counts,
                    total: counts.iter().fold(0u32, |sum, n| sum.saturating_add(*n)),
                }
            })
            .collect()
    };
    let class_kills = matrix(&wire.classkills);
    let class_kill_assists = matrix(&wire.classkillassists);
    let class_deaths = matrix(&wire.classdeaths);

    LogDetail {
        length: wire.length,
        title: info.title.clone(),
        map: info.map.clone(),
        date: info.date,
        red: wire.teams.red.totals(),
        blue: wire.teams.blue.totals(),
        players: players.into_iter().map(|(_, line, _)| line).collect(),
        columns: LogColumns {
            hp: info.has_hp,
            bs: info.has_bs,
            hs: info.has_hs,
            airshots: info.has_as,
            caps: info.has_cp,
            dt: info.has_dt,
        },
        medics,
        class_kills,
        class_kill_assists,
        class_deaths,
        rounds: wire.rounds.into_iter().map(round_line).collect(),
        killstreaks: killstreaks(wire.killstreaks),
        chat: wire
            .chat
            .into_iter()
            .map(|c| LogChatLine {
                steam_id: SteamId::from_steam3(&c.steamid),
                name: c.name,
                msg: c.msg,
            })
            .collect(),
    }
}

/// Streaks whose player reads, by time; the stable sort keeps tied times in JSON order.
fn killstreaks(wire: Vec<WireKillstreak>) -> Vec<Killstreak> {
    let mut streaks: Vec<Killstreak> = wire
        .into_iter()
        .filter_map(|k| {
            Some(Killstreak {
                steam_id: SteamId::from_steam3(&k.steamid)?,
                streak: k.streak,
                at: k.time,
            })
        })
        .collect();
    streaks.sort_by_key(Killstreak::at);
    streaks
}

fn round_line(round: WireRound) -> RoundLine {
    let side = |t: &WireRoundTeam| RoundTeam {
        score: t.score,
        kills: t.kills,
        dmg: t.dmg,
        ubers: t.ubers,
    };
    let mut players: Vec<RoundPlayer> = round
        .players
        .iter()
        .filter_map(|(key, p)| {
            Some(RoundPlayer {
                steam_id: SteamId::from_steam3(key)?,
                team: team(&p.team)?,
                kills: p.kills,
                dmg: p.dmg,
            })
        })
        .collect();
    players.sort_by(|a, b| {
        b.kills
            .cmp(&a.kills)
            .then(b.dmg.cmp(&a.dmg))
            .then(a.steam_id.id64().cmp(&b.steam_id.id64()))
    });
    RoundLine {
        length: round.length,
        winner: team(&round.winner),
        midfight: team(&round.firstcap),
        blue: side(&round.team.blue),
        red: side(&round.team.red),
        players,
        events: round
            .events
            .into_iter()
            .map(|e| RoundEvent {
                at: e.time,
                kind: event_kind(e),
            })
            .collect(),
    }
}

/// The event's kind; a known type whose team or medic does not read falls back to `Other`.
fn event_kind(e: WireEvent) -> EventKind {
    let team = team(&e.team);
    let medic = SteamId::from_steam3(&e.steamid);
    let known = match (e.kind.as_str(), team, medic) {
        ("pointcap", Some(team), _) => Some(EventKind::PointCap {
            team,
            point: e.point,
        }),
        ("charge", Some(team), Some(medic)) => Some(EventKind::Charge {
            team,
            medic,
            medigun: Medigun::from_key(&e.medigun),
        }),
        ("medic_death", Some(team), Some(medic)) => Some(EventKind::MedicDeath {
            team,
            medic,
            killer: SteamId::from_steam3(&e.killer),
        }),
        ("drop", Some(team), Some(medic)) => Some(EventKind::Drop { team, medic }),
        ("round_win", Some(team), _) => Some(EventKind::RoundWin { team }),
        _ => None,
    };
    known.unwrap_or(EventKind::Other { name: e.kind })
}

fn team(name: &str) -> Option<Team> {
    match name {
        "Red" => Some(Team::Red),
        "Blue" => Some(Team::Blue),
        _ => None,
    }
}

/// Red before Blue, then the longest class's place in TF2 order (no class sorts last).
fn sort_key(line: &PlayerLine) -> (u8, usize, &str) {
    let team = u8::from(line.team != Team::Red);
    let class = line.classes.first().map_or(TfClass::ALL.len(), |c| {
        TfClass::ALL.iter().position(|&k| k == c.class).unwrap_or(0)
    });
    (team, class, line.name.as_str())
}

fn player_line(
    p: &WirePlayer,
    steam_id: SteamId,
    name: String,
    team: Team,
    info: &WireInfo,
    per_min: impl Fn(u32) -> f64,
) -> PlayerLine {
    let mut classes: Vec<ClassLine> = p
        .class_stats
        .iter()
        .filter_map(|c| {
            let mut weapons = c.weapon.clone();
            weapons.sort_by(|a, b| {
                b.kills
                    .cmp(&a.kills)
                    .then_with(|| b.dmg.cmp(&a.dmg))
                    .then_with(|| a.name.cmp(&b.name))
            });
            Some(ClassLine {
                class: logstf_class(&c.kind)?,
                seconds: c.total_time,
                kills: c.kills,
                deaths: c.deaths,
                assists: c.assists,
                dmg: c.dmg,
                weapons,
            })
        })
        .collect();
    // Ties break by logs.tf class name, as the cached log row orders them.
    classes.sort_by(|a, b| {
        b.seconds
            .cmp(&a.seconds)
            .then_with(|| logstf_class_name(a.class).cmp(logstf_class_name(b.class)))
    });
    // logs.tf sends the ratios as text; recompute them when that text is missing or garbled.
    let deaths = f64::from(p.deaths.max(1));
    PlayerLine {
        steam_id,
        name,
        team,
        classes,
        kills: p.kills,
        assists: p.assists,
        deaths: p.deaths,
        dmg: p.dmg,
        da_per_min: p.dapm,
        kapd: p
            .kapd
            .unwrap_or_else(|| (f64::from(p.kills) + f64::from(p.assists)) / deaths),
        kpd: p.kpd.unwrap_or_else(|| f64::from(p.kills) / deaths),
        dt: p.dt,
        dt_per_min: per_min(p.dt),
        medkits: p.medkits,
        backstabs: p.backstabs,
        headshots: if info.has_hs_hit {
            p.headshots_hit
        } else {
            p.headshots
        },
        airshots: p.airshots,
        caps: p.cpc,
        heal: p.heal,
        ubers: p.ubers,
        drops: p.drops,
    }
}

fn heal_spread(
    spread: Option<&BTreeMap<String, u32>>,
    name_of: &impl Fn(&str, SteamId) -> String,
    class_of: &BTreeMap<&str, TfClass>,
) -> Vec<HealTarget> {
    let mut targets: Vec<HealTarget> = spread
        .into_iter()
        .flatten()
        .filter_map(|(key, &healing)| {
            let steam_id = SteamId::from_steam3(key)?;
            Some(HealTarget {
                steam_id,
                name: name_of(key, steam_id),
                healing,
                class: class_of.get(key.as_str()).copied(),
            })
        })
        .collect();
    targets.sort_by(|a, b| b.healing.cmp(&a.healing).then(a.name.cmp(&b.name)));
    targets
}

// The wire shape: everything but `length`, `teams` and `players` may be missing, and every
// number is read leniently so one odd field never makes a whole log unreadable.

#[derive(Deserialize)]
struct WireLog {
    #[serde(deserialize_with = "count")]
    length: u32,
    teams: WireTeams,
    players: BTreeMap<String, WirePlayer>,
    #[serde(default)]
    names: BTreeMap<String, String>,
    #[serde(default)]
    info: Option<WireInfo>,
    #[serde(default, deserialize_with = "count_maps")]
    healspread: BTreeMap<String, BTreeMap<String, u32>>,
    #[serde(default, deserialize_with = "count_maps")]
    classkills: BTreeMap<String, BTreeMap<String, u32>>,
    #[serde(default, deserialize_with = "count_maps")]
    classkillassists: BTreeMap<String, BTreeMap<String, u32>>,
    #[serde(default, deserialize_with = "count_maps")]
    classdeaths: BTreeMap<String, BTreeMap<String, u32>>,
    #[serde(default, deserialize_with = "lenient_list")]
    rounds: Vec<WireRound>,
    #[serde(default, deserialize_with = "lenient_list")]
    killstreaks: Vec<WireKillstreak>,
    #[serde(default, deserialize_with = "lenient_list")]
    chat: Vec<WireChat>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WireKillstreak {
    #[serde(deserialize_with = "text")]
    steamid: String,
    #[serde(deserialize_with = "count")]
    streak: u32,
    #[serde(deserialize_with = "count")]
    time: u32,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WireChat {
    #[serde(deserialize_with = "text")]
    steamid: String,
    #[serde(deserialize_with = "text")]
    name: String,
    #[serde(deserialize_with = "text")]
    msg: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WireRound {
    #[serde(deserialize_with = "count")]
    length: u32,
    #[serde(deserialize_with = "text")]
    winner: String,
    #[serde(deserialize_with = "text")]
    firstcap: String,
    #[serde(deserialize_with = "lenient")]
    team: WireRoundTeams,
    #[serde(deserialize_with = "lenient_map")]
    players: BTreeMap<String, WireRoundPlayer>,
    #[serde(deserialize_with = "lenient_list")]
    events: Vec<WireEvent>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WireRoundTeams {
    #[serde(rename = "Red", deserialize_with = "lenient")]
    red: WireRoundTeam,
    #[serde(rename = "Blue", deserialize_with = "lenient")]
    blue: WireRoundTeam,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WireRoundTeam {
    #[serde(deserialize_with = "count")]
    score: u32,
    #[serde(deserialize_with = "count")]
    kills: u32,
    #[serde(deserialize_with = "count")]
    dmg: u32,
    #[serde(deserialize_with = "count")]
    ubers: u32,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WireRoundPlayer {
    #[serde(deserialize_with = "text")]
    team: String,
    #[serde(deserialize_with = "count")]
    kills: u32,
    #[serde(deserialize_with = "count")]
    dmg: u32,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WireEvent {
    #[serde(rename = "type", deserialize_with = "text")]
    kind: String,
    #[serde(deserialize_with = "count")]
    time: u32,
    #[serde(deserialize_with = "text")]
    team: String,
    #[serde(deserialize_with = "count")]
    point: u32,
    #[serde(deserialize_with = "text")]
    steamid: String,
    #[serde(deserialize_with = "text")]
    killer: String,
    #[serde(deserialize_with = "text")]
    medigun: String,
}

#[derive(Deserialize)]
struct WireTeams {
    #[serde(default, rename = "Red")]
    red: WireTeam,
    #[serde(default, rename = "Blue")]
    blue: WireTeam,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WireTeam {
    #[serde(deserialize_with = "count")]
    score: u32,
    #[serde(deserialize_with = "count")]
    kills: u32,
    #[serde(deserialize_with = "count")]
    dmg: u32,
    #[serde(deserialize_with = "count")]
    charges: u32,
    #[serde(deserialize_with = "count")]
    drops: u32,
    #[serde(deserialize_with = "count")]
    caps: u32,
    #[serde(deserialize_with = "count")]
    firstcaps: u32,
}

impl WireTeam {
    fn totals(&self) -> TeamTotals {
        TeamTotals {
            score: self.score,
            kills: self.kills,
            dmg: self.dmg,
            charges: self.charges,
            drops: self.drops,
            caps: self.caps,
            midfights: self.firstcaps,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WireInfo {
    #[serde(deserialize_with = "text")]
    title: String,
    #[serde(deserialize_with = "text")]
    map: String,
    #[serde(deserialize_with = "count_i64")]
    date: i64,
    #[serde(rename = "hasHP", deserialize_with = "flag")]
    has_hp: bool,
    #[serde(rename = "hasBS", deserialize_with = "flag")]
    has_bs: bool,
    #[serde(rename = "hasHS", deserialize_with = "flag")]
    has_hs: bool,
    #[serde(rename = "hasHS_hit", deserialize_with = "flag")]
    has_hs_hit: bool,
    #[serde(rename = "hasAS", deserialize_with = "flag")]
    has_as: bool,
    #[serde(rename = "hasCP", deserialize_with = "flag")]
    has_cp: bool,
    #[serde(rename = "hasDT", deserialize_with = "flag")]
    has_dt: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WirePlayer {
    #[serde(deserialize_with = "text")]
    team: String,
    class_stats: Vec<WireClass>,
    #[serde(deserialize_with = "count")]
    kills: u32,
    #[serde(deserialize_with = "count")]
    deaths: u32,
    #[serde(deserialize_with = "count")]
    assists: u32,
    #[serde(deserialize_with = "count")]
    dmg: u32,
    #[serde(deserialize_with = "count")]
    dapm: u32,
    #[serde(deserialize_with = "ratio")]
    kapd: Option<f64>,
    #[serde(deserialize_with = "ratio")]
    kpd: Option<f64>,
    #[serde(deserialize_with = "count")]
    dt: u32,
    #[serde(deserialize_with = "count")]
    medkits: u32,
    #[serde(deserialize_with = "count")]
    backstabs: u32,
    #[serde(deserialize_with = "count")]
    headshots: u32,
    #[serde(deserialize_with = "count")]
    headshots_hit: u32,
    #[serde(rename = "as", deserialize_with = "count")]
    airshots: u32,
    #[serde(deserialize_with = "count")]
    cpc: u32,
    #[serde(deserialize_with = "count")]
    heal: u32,
    #[serde(deserialize_with = "count")]
    ubers: u32,
    #[serde(deserialize_with = "count")]
    drops: u32,
    #[serde(deserialize_with = "count_map")]
    ubertypes: BTreeMap<String, u32>,
    medicstats: Option<WireMedicStats>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct WireMedicStats {
    #[serde(deserialize_with = "real")]
    avg_time_to_build: f64,
    #[serde(deserialize_with = "real")]
    avg_uber_length: f64,
    #[serde(deserialize_with = "count")]
    deaths_within_20s_after_uber: u32,
    #[serde(deserialize_with = "real")]
    avg_time_before_using: f64,
    #[serde(deserialize_with = "count")]
    deaths_with_95_99_uber: u32,
    #[serde(deserialize_with = "count")]
    advantages_lost: u32,
    #[serde(deserialize_with = "count")]
    biggest_advantage_lost: u32,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WireClass {
    #[serde(rename = "type", deserialize_with = "text")]
    kind: String,
    #[serde(deserialize_with = "count")]
    total_time: u32,
    #[serde(deserialize_with = "count")]
    kills: u32,
    #[serde(deserialize_with = "count")]
    deaths: u32,
    #[serde(deserialize_with = "count")]
    assists: u32,
    #[serde(deserialize_with = "count")]
    dmg: u32,
    #[serde(deserialize_with = "weapon_lines")]
    weapon: Vec<WeaponLine>,
}

/// A number, or numeric text, as an `f64`; anything else is `None`.
fn number(value: &Value) -> Option<f64> {
    if let Some(text) = value.as_str() {
        return text.trim().parse::<f64>().ok().filter(|x| x.is_finite());
    }
    value.as_f64()
}

/// A count rounded and clamped into `u32`; a missing or odd value is 0.
fn to_count(value: &Value) -> u32 {
    // The `as` cast saturates, and NaN never reaches it since `number` keeps only finite text.
    number(value).map_or(0, |x| x.round().clamp(0.0, f64::from(u32::MAX)) as u32)
}

fn count<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    Ok(to_count(&Value::deserialize(d)?))
}

fn count_i64<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    Ok(number(&Value::deserialize(d)?).map_or(0, |x| x as i64))
}

fn real<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(number(&Value::deserialize(d)?).unwrap_or(0.0))
}

fn ratio<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(number(&Value::deserialize(d)?))
}

fn flag<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(matches!(Value::deserialize(d)?, Value::Bool(true)))
}

fn text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = Value::deserialize(d)?;
    Ok(value.as_str().unwrap_or_default().to_owned())
}

fn object_counts(value: Value) -> BTreeMap<String, u32> {
    let Value::Object(map) = value else {
        return BTreeMap::new();
    };
    map.into_iter().map(|(k, v)| (k, to_count(&v))).collect()
}

fn count_map<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, u32>, D::Error> {
    Ok(object_counts(Value::deserialize(d)?))
}

/// A value that reads as its default when it does not have the expected shape.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(d)?).unwrap_or_default())
}

/// A list whose unreadable entries are skipped; anything but an array reads as empty.
fn lenient_list<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    let Value::Array(items) = Value::deserialize(d)? else {
        return Ok(Vec::new());
    };
    Ok(items
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

/// A map whose unreadable entries are skipped; anything but an object reads as empty.
fn lenient_map<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    d: D,
) -> Result<BTreeMap<String, T>, D::Error> {
    let Value::Object(map) = Value::deserialize(d)? else {
        return Ok(BTreeMap::new());
    };
    Ok(map
        .into_iter()
        .filter_map(|(k, v)| Some((k, serde_json::from_value(v).ok()?)))
        .collect())
}

fn count_maps<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, BTreeMap<String, u32>>, D::Error> {
    let Value::Object(map) = Value::deserialize(d)? else {
        return Ok(BTreeMap::new());
    };
    Ok(map
        .into_iter()
        .map(|(k, v)| (k, object_counts(v)))
        .collect())
}

/// A class's weapon map as lines; entries that are not objects are skipped, and a
/// `weapon` that is not an object reads as none.
fn weapon_lines<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<WeaponLine>, D::Error> {
    let Value::Object(map) = Value::deserialize(d)? else {
        return Ok(Vec::new());
    };
    let field = |v: &Value, key: &str| v.get(key).map_or(0, to_count);
    Ok(map
        .into_iter()
        .filter(|(_, v)| v.is_object())
        .map(|(name, v)| WeaponLine {
            kills: field(&v, "kills"),
            dmg: field(&v, "dmg"),
            shots: field(&v, "shots"),
            hits: field(&v, "hits"),
            name,
        })
        .collect())
}
