//! Browsing demos.tf's demo list: formats, ranges and paging.

use std::collections::HashSet;

use super::demo::DemoSummary;
use crate::error::DemosError;
use crate::logstf::SteamId;

/// A match format, as demos.tf's list filters and the player count imply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DemoFormat {
    /// Nine a side.
    Highlander,
    /// Seven a side.
    Prolander,
    /// Six a side.
    Sixes,
    /// Four a side.
    Fours,
    /// Three a side.
    Threes,
    /// Two a side.
    Twos,
    /// One on one.
    Ones,
    /// Ultiduo, two a side on an ultiduo map.
    Ultiduo,
    /// BBall, on a bball map.
    BBall,
    /// Anything the counts and map do not place.
    Other,
}

impl DemoFormat {
    /// Every format, in declaration order.
    pub const ALL: [DemoFormat; 10] = [
        DemoFormat::Highlander,
        DemoFormat::Prolander,
        DemoFormat::Sixes,
        DemoFormat::Fours,
        DemoFormat::Threes,
        DemoFormat::Twos,
        DemoFormat::Ones,
        DemoFormat::Ultiduo,
        DemoFormat::BBall,
        DemoFormat::Other,
    ];

    /// The `type` value demos.tf's list narrows by, or `None` when the server has no such filter.
    /// Ultiduo shares `2v2`, so a client-side check must still apply `derive`.
    pub fn type_key(self) -> Option<&'static str> {
        match self {
            DemoFormat::Highlander => Some("hl"),
            DemoFormat::Prolander => Some("prolander"),
            DemoFormat::Sixes => Some("6v6"),
            DemoFormat::Fours => Some("4v4"),
            DemoFormat::Threes => Some("3v3"),
            DemoFormat::Twos | DemoFormat::Ultiduo => Some("2v2"),
            DemoFormat::Ones => Some("1v1"),
            DemoFormat::BBall | DemoFormat::Other => None,
        }
    }

    /// The format of a demo on `map` that recorded `player_count` players.
    pub fn derive(map: &str, player_count: u32) -> DemoFormat {
        // Map names decide the two special formats before any head count does.
        let map = map.to_lowercase();
        if map.contains("bball") || map.contains("ballin") {
            return DemoFormat::BBall;
        }
        if map.contains("ultiduo") {
            return DemoFormat::Ultiduo;
        }
        match player_count {
            17..=19 => DemoFormat::Highlander,
            14..=15 => DemoFormat::Prolander,
            11..=13 => DemoFormat::Sixes,
            7..=9 => DemoFormat::Fours,
            6 => DemoFormat::Threes,
            4 => DemoFormat::Twos,
            2 => DemoFormat::Ones,
            _ => DemoFormat::Other,
        }
    }
}

/// Days in seconds, so the reaches below read as the days they stand for.
const DAY: i64 = 86_400;

/// How far back from now a demo list reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum DemosRange {
    /// The last day.
    Day1,
    /// The last 7 days.
    #[default]
    Week1,
    /// The last 30 days.
    Month1,
    /// The last 90 days.
    Months3,
    /// The last 365 days.
    Year1,
    /// Every demo.
    All,
}

impl DemosRange {
    /// Every range in slider order, narrow to wide.
    pub const STOPS: [DemosRange; 6] = [
        DemosRange::Day1,
        DemosRange::Week1,
        DemosRange::Month1,
        DemosRange::Months3,
        DemosRange::Year1,
        DemosRange::All,
    ];

    /// How far back from now the range reaches, in seconds; `None` for `All`.
    pub fn reach_secs(self) -> Option<i64> {
        let days = match self {
            DemosRange::Day1 => 1,
            DemosRange::Week1 => 7,
            DemosRange::Month1 => 30,
            DemosRange::Months3 => 90,
            DemosRange::Year1 => 365,
            DemosRange::All => return None,
        };
        Some(days * DAY)
    }
}

/// What a demo list is filtered by. `now` is the caller's clock, so a query is reproducible.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DemoListQuery {
    range: DemosRange,
    now: i64,
    format: Option<DemoFormat>,
    map: Option<String>,
    players: Vec<SteamId>,
}

impl DemoListQuery {
    /// A query for `range` ending at `now` (unix seconds), with no other filter.
    pub fn new(range: DemosRange, now: i64) -> Self {
        DemoListQuery {
            range,
            now,
            format: None,
            map: None,
            players: Vec::new(),
        }
    }

    /// Keeps only demos of `format`; `None` clears it.
    pub fn with_format(mut self, format: Option<DemoFormat>) -> Self {
        self.format = format;
        self
    }

    /// Keeps only demos on `map`; the name is trimmed and an empty one clears the filter.
    pub fn with_map(mut self, map: &str) -> Self {
        let map = map.trim();
        self.map = (!map.is_empty()).then(|| map.to_string());
        self
    }

    /// Keeps only demos every one of `players` played in; an empty slice clears the filter.
    pub fn with_players(mut self, players: &[SteamId]) -> Self {
        self.players = players.to_vec();
        self
    }

    /// The range the list reaches back over.
    pub fn range(&self) -> DemosRange {
        self.range
    }

    /// The format filter, if any.
    pub fn format(&self) -> Option<DemoFormat> {
        self.format
    }

    /// The map filter, if any.
    pub fn map(&self) -> Option<&str> {
        self.map.as_deref()
    }

    /// The players filter; empty when unset.
    pub fn players(&self) -> &[SteamId] {
        &self.players
    }
}

/// Where in a demo list a page starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DemoCursor {
    /// The newest demos.
    First,
    /// Demos with an id below this one.
    BeforeId(u64),
    /// This page number, counting from 1.
    Page(u32),
}

/// One page of a demo list and where the next one starts.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DemoPage {
    demos: Vec<DemoSummary>,
    next: Option<DemoCursor>,
}

impl DemoPage {
    pub(crate) fn new(demos: Vec<DemoSummary>, next: Option<DemoCursor>) -> Self {
        Self { demos, next }
    }

    /// The demos on this page, after the client-side format filter.
    pub fn demos(&self) -> &[DemoSummary] {
        &self.demos
    }

    /// Where the next page starts, or `None` once the list is exhausted.
    pub fn next(&self) -> Option<DemoCursor> {
        self.next
    }
}

/// Appends the demos of `page` whose id `list` lacks and returns how many were added.
/// New demos shift the pages, so a later page can repeat rows the list already holds.
pub fn merge_demos(list: &mut Vec<DemoSummary>, page: &[DemoSummary]) -> usize {
    let seen: HashSet<u64> = list.iter().map(DemoSummary::id).collect();
    let before = list.len();
    list.extend(page.iter().filter(|d| !seen.contains(&d.id())).cloned());
    list.len() - before
}

/// The query string pairs for one list request, in the order demos.tf is asked.
/// The players filter cannot page by id (the server errors), so that pairing is refused.
pub(crate) fn query_pairs(
    query: &DemoListQuery,
    cursor: DemoCursor,
) -> Result<Vec<(&'static str, String)>, DemosError> {
    if !query.players.is_empty() && matches!(cursor, DemoCursor::BeforeId(_)) {
        return Err(DemosError::Cursor);
    }
    let mut pairs = Vec::new();
    if let Some(reach) = query.range.reach_secs() {
        pairs.push(("after", (query.now - reach).to_string()));
    }
    if let Some(map) = &query.map {
        pairs.push(("map", map.clone()));
    }
    if let Some(key) = query.format.and_then(DemoFormat::type_key) {
        pairs.push(("type", key.to_string()));
    }
    if !query.players.is_empty() {
        let ids: Vec<String> = query.players.iter().map(|p| p.id64().to_string()).collect();
        pairs.push(("players", ids.join(",")));
    }
    match cursor {
        DemoCursor::First => {}
        DemoCursor::Page(n) => pairs.push(("page", n.to_string())),
        DemoCursor::BeforeId(id) => pairs.push(("before_id", id.to_string())),
    }
    Ok(pairs)
}

/// Where the page after `raw` starts; `raw` is the unfiltered page the server sent.
pub(crate) fn next_cursor(
    query: &DemoListQuery,
    cursor: DemoCursor,
    raw: &[DemoSummary],
) -> Option<DemoCursor> {
    let lowest = raw.iter().map(DemoSummary::id).min()?;
    if query.players.is_empty() {
        return Some(DemoCursor::BeforeId(lowest));
    }
    let current = match cursor {
        DemoCursor::Page(n) => n,
        DemoCursor::First | DemoCursor::BeforeId(_) => 1,
    };
    Some(DemoCursor::Page(current.saturating_add(1)))
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
