//! The Maps page's pure parts: the row it draws, the search and sorts, and its texts.

use std::cmp::Reverse;

use tf2tools_core::{MapEntry, MapState};

/// One listed map as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapRow {
    pub name: String,
    pub times_played: u64,
    /// `YYYY-MM-DD`, or None for a map never played.
    pub last_played: Option<String>,
    pub size: u64,
    pub state: MapState,
}

impl From<&MapEntry> for MapRow {
    fn from(entry: &MapEntry) -> Self {
        let map = entry.map();
        Self {
            name: map.name().to_owned(),
            times_played: map.times_played(),
            last_played: map.last_played().map(str::to_owned),
            size: map.size(),
            state: entry.state(),
        }
    }
}

/// How the list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapSort {
    Name,
    MostPlayed,
    LastPlayed,
}

/// The sort menu: label, then the sort. The first is the default.
pub const SORTS: [(&str, MapSort); 3] = [
    ("Name", MapSort::Name),
    ("Most played", MapSort::MostPlayed),
    ("Recently played", MapSort::LastPlayed),
];

/// The indexes of the rows whose name holds `query` (trimmed, any case), in `sort` order.
/// Ties fall back to the name, so the order never depends on the cache's.
pub fn visible(rows: &[MapRow], query: &str, sort: MapSort) -> Vec<usize> {
    let needle = query.trim().to_lowercase();
    let mut shown: Vec<usize> = (0..rows.len())
        .filter(|&i| needle.is_empty() || rows[i].name.to_lowercase().contains(&needle))
        .collect();
    let by_name = |r: &MapRow| (r.name.to_lowercase(), r.name.clone());
    match sort {
        MapSort::Name => shown.sort_by_cached_key(|&i| by_name(&rows[i])),
        MapSort::MostPlayed => {
            shown.sort_by_cached_key(|&i| (Reverse(rows[i].times_played), by_name(&rows[i])))
        }
        // ISO dates order as text; a never-played map (None) sorts after every date.
        MapSort::LastPlayed => shown.sort_by_cached_key(|&i| {
            let r = &rows[i];
            (
                r.last_played.is_none(),
                Reverse(r.last_played.clone()),
                Reverse(r.times_played),
                by_name(r),
            )
        }),
    }
    shown
}

/// The Last played cell: a "%Y-%m-%d" date in the player's language, other text as it is,
/// or "Never".
pub fn last_played_label(last: Option<&str>) -> String {
    let Some(last) = last else {
        return "Never".to_owned();
    };
    chrono::NaiveDate::parse_from_str(last, "%Y-%m-%d").map_or_else(
        |_| last.to_owned(),
        |d| crate::i18n::format::numeric_date(d).to_string(),
    )
}

/// "1 map", "4368 maps", or "2 of 4368 maps" while a search hides some.
pub fn count_label(shown: usize, total: usize) -> String {
    let n = u64::try_from(total).unwrap_or(u64::MAX);
    let count = total.to_string();
    if shown == total {
        crate::i18n::counted_plural("maps.count", n, &count, &[]).to_string()
    } else {
        let shown = shown.to_string();
        let vars = [("shown", shown.as_str())];
        crate::i18n::counted_plural("maps.count_of", n, &count, &vars).to_string()
    }
}
