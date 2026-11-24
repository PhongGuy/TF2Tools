//! demos.tf's demo shapes and the rule that matches one to a log.

use serde::Deserialize;

use super::list::DemoFormat;
use crate::error::DemosError;
use crate::logstf::{LogDetail, SteamId};

/// How far apart a log's and its demo's upload times may be, in seconds.
const TIME_SLACK: u64 = 300;

/// The least length difference always allowed, in seconds; longer logs allow a tenth.
const LENGTH_SLACK: i64 = 90;

/// What a demos.tf search needs from one log.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DemoQuery {
    pub(super) map: String,
    pub(super) date: i64,
    pub(super) length: u32,
    pub(super) players: Vec<SteamId>,
}

impl DemoQuery {
    /// The query for `log`, or `None` when it lacks a map, date, length or two players.
    pub fn from_log(log: &LogDetail) -> Option<DemoQuery> {
        let players: Vec<SteamId> = log.players().iter().take(2).map(|p| p.steam_id()).collect();
        let usable = !log.map().is_empty() && log.date() != 0 && log.length() != 0;
        (usable && players.len() == 2).then(|| DemoQuery {
            map: log.map().to_owned(),
            date: log.date(),
            length: log.length(),
            players,
        })
    }
    /// The log's map.
    pub fn map(&self) -> &str {
        &self.map
    }
    /// The log's upload time in Unix seconds.
    pub fn date(&self) -> i64 {
        self.date
    }
    /// The log's length in seconds.
    pub fn length(&self) -> u32 {
        self.length
    }
    /// Two of the log's players, whom the demo must also hold.
    pub fn players(&self) -> &[SteamId] {
        &self.players
    }
}

/// One demo as a demos.tf list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DemoSummary {
    id: u64,
    url: String,
    name: String,
    map: String,
    time: i64,
    duration: u32,
    server: String,
    red: String,
    blue: String,
    player_count: u32,
}

impl DemoSummary {
    /// The demo's demos.tf id.
    pub fn id(&self) -> u64 {
        self.id
    }
    /// Where the demo file downloads from, empty while the demo is private.
    pub fn url(&self) -> &str {
        &self.url
    }
    /// The demo's file name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The map it was recorded on.
    pub fn map(&self) -> &str {
        &self.map
    }
    /// The upload time in Unix seconds.
    pub fn time(&self) -> i64 {
        self.time
    }
    /// The recording's length in seconds.
    pub fn duration(&self) -> u32 {
        self.duration
    }
    /// The server's name.
    pub fn server(&self) -> &str {
        &self.server
    }
    /// The RED team's name, empty when the list gave none.
    pub fn red(&self) -> &str {
        &self.red
    }
    /// The BLU team's name, empty when the list gave none.
    pub fn blue(&self) -> &str {
        &self.blue
    }
    /// How many players the demo recorded, 0 when the list gave none.
    pub fn player_count(&self) -> u32 {
        self.player_count
    }
    /// The match format, derived from the map and the player count.
    pub fn format(&self) -> DemoFormat {
        DemoFormat::derive(&self.map, self.player_count)
    }
}

/// One player a demo recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DemoPlayer {
    steam_id: SteamId,
    name: String,
}

impl DemoPlayer {
    /// The player's Steam ID.
    pub fn steam_id(&self) -> SteamId {
        self.steam_id
    }
    /// The name the demo recorded.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// One demo with its players, as demos.tf's demo page answers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DemoDetail {
    summary: DemoSummary,
    players: Vec<DemoPlayer>,
}

impl DemoDetail {
    /// The demo itself.
    pub fn summary(&self) -> &DemoSummary {
        &self.summary
    }
    /// Every player with a valid Steam ID, in demos.tf's order.
    pub fn players(&self) -> &[DemoPlayer] {
        &self.players
    }
}

/// The demo recorded for `query`: the one candidate on its map, uploaded within five minutes
/// and about as long, with a public url. Two such candidates are a rematch, so neither is.
pub fn pick_demo<'a>(query: &DemoQuery, candidates: &'a [DemoSummary]) -> Option<&'a DemoSummary> {
    let length = i64::from(query.length);
    let length_slack = LENGTH_SLACK.max(length / 10);
    let mut close = candidates.iter().filter(|demo| {
        demo.map == query.map
            && demo.time.abs_diff(query.date) <= TIME_SLACK
            && (i64::from(demo.duration) - length).abs() <= length_slack
            && !demo.url.is_empty()
    });
    let first = close.next()?;
    close.next().is_none().then_some(first)
}

// The wire shapes: id, time and duration are required so any other JSON is Unreadable;
// text fields may be missing or null, as `url` is on a private demo.
#[derive(Deserialize)]
struct WireDemo {
    id: u64,
    time: i64,
    duration: u32,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    map: Option<String>,
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    red: Option<String>,
    #[serde(default)]
    blue: Option<String>,
    #[serde(default, rename = "playerCount")]
    player_count: Option<u32>,
}

#[derive(Deserialize)]
struct WireDetail {
    #[serde(flatten)]
    demo: WireDemo,
    #[serde(default)]
    players: Vec<WirePlayer>,
}

#[derive(Deserialize)]
struct WirePlayer {
    #[serde(default)]
    steamid: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

impl From<WireDemo> for DemoSummary {
    fn from(w: WireDemo) -> Self {
        DemoSummary {
            id: w.id,
            url: w.url.unwrap_or_default(),
            name: w.name.unwrap_or_default(),
            map: w.map.unwrap_or_default(),
            time: w.time,
            duration: w.duration,
            server: w.server.unwrap_or_default(),
            red: w.red.unwrap_or_default(),
            blue: w.blue.unwrap_or_default(),
            player_count: w.player_count.unwrap_or_default(),
        }
    }
}

/// A demos.tf list body; anything but a JSON array of demos is `Unreadable`.
pub(crate) fn parse_list(body: &[u8]) -> Result<Vec<DemoSummary>, DemosError> {
    let wire: Vec<WireDemo> = serde_json::from_slice(body).map_err(|_| DemosError::Unreadable)?;
    Ok(wire.into_iter().map(DemoSummary::from).collect())
}

/// A demos.tf demo body; anything but a demo object is `Unreadable`. A player whose
/// `steamid` is not a SteamID64 is left out.
pub(crate) fn parse_detail(body: &[u8]) -> Result<DemoDetail, DemosError> {
    let wire: WireDetail = serde_json::from_slice(body).map_err(|_| DemosError::Unreadable)?;
    let players = wire
        .players
        .into_iter()
        .filter_map(|p| {
            let id64 = p.steamid?.parse::<u64>().ok()?;
            Some(DemoPlayer {
                steam_id: SteamId::from_id64(id64)?,
                name: p.name.unwrap_or_default(),
            })
        })
        .collect();
    Ok(DemoDetail {
        summary: wire.demo.into(),
        players,
    })
}

#[cfg(test)]
#[path = "demo_tests.rs"]
mod tests;
