//! A stored logs.tf `/json/<id>` body read into the expanded log view's shape.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::{SteamId, logstf_class};
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
    class_kills: Vec<ClassKills>,
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
    /// Every player who played Medic, in the players' order.
    pub fn medics(&self) -> &[MedicLine] {
        &self.medics
    }
    /// Kills by victim class, one entry per player with any, in the players' order.
    pub fn class_kills(&self) -> &[ClassKills] {
        &self.class_kills
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
    charges: Vec<(String, u32)>,
    drops: u32,
    avg_time_to_build: f64,
    avg_uber_length: f64,
    deaths_after_uber: u32,
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
    /// Charges used per medigun type, by type name.
    pub fn charges(&self) -> &[(String, u32)] {
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
}

/// One player's kills by victim class.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ClassKills {
    steam_id: SteamId,
    kills: Vec<(TfClass, u32)>,
}

impl ClassKills {
    /// The killer's Steam ID.
    pub fn steam_id(&self) -> SteamId {
        self.steam_id
    }
    /// Kills per victim class in TF2 order, classes with none left out.
    pub fn kills(&self) -> &[(TfClass, u32)] {
        &self.kills
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

    let medics = players
        .iter()
        .filter(|(_, line, _)| line.classes.iter().any(|c| c.class == TfClass::Medic))
        .map(|(key, line, p)| {
            let stats = p.medicstats.clone().unwrap_or_default();
            MedicLine {
                steam_id: line.steam_id,
                name: line.name.clone(),
                team: line.team,
                heal: p.heal,
                heal_per_min: per_min(p.heal),
                charges: p.ubertypes.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                drops: p.drops,
                avg_time_to_build: stats.avg_time_to_build,
                avg_uber_length: stats.avg_uber_length,
                deaths_after_uber: stats.deaths_within_20s_after_uber,
                heal_spread: heal_spread(wire.healspread.get(key), &name_of),
            }
        })
        .collect();

    let class_kills = players
        .iter()
        .filter_map(|(key, line, _)| {
            let by_class = wire.classkills.get(key)?;
            let kills: Vec<(TfClass, u32)> = TfClass::ALL
                .iter()
                .filter_map(|&class| {
                    let n = by_class
                        .iter()
                        .filter(|(name, _)| logstf_class(name) == Some(class))
                        .map(|(_, n)| *n)
                        .sum::<u32>();
                    (n > 0).then_some((class, n))
                })
                .collect();
            (!kills.is_empty()).then_some(ClassKills {
                steam_id: line.steam_id,
                kills,
            })
        })
        .collect();

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
    }
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
            Some(ClassLine {
                class: logstf_class(&c.kind)?,
                seconds: c.total_time,
                kills: c.kills,
                deaths: c.deaths,
                assists: c.assists,
                dmg: c.dmg,
            })
        })
        .collect();
    classes.sort_by_key(|c| std::cmp::Reverse(c.seconds));
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
            .unwrap_or_else(|| f64::from(p.kills + p.assists) / deaths),
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
