//! The history range: how far back from a player's newest log the list, averages and sync reach.

/// Days in seconds, so the reaches below read as the days they stand for.
const DAY: i64 = 86_400;

/// How much of a player's history the page shows and the sync fetches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[non_exhaustive]
pub enum LogsRange {
    /// The 30 days ending at the newest log.
    Month1,
    /// The 90 days ending at the newest log; the window every earlier build used.
    #[default]
    Months3,
    /// The 180 days ending at the newest log.
    Months6,
    /// The 365 days ending at the newest log.
    Year1,
    /// The 730 days ending at the newest log.
    Years2,
    /// Every log the player has.
    All,
}

impl LogsRange {
    /// Every range in slider order, narrow to wide.
    pub const STOPS: [LogsRange; 6] = [
        LogsRange::Month1,
        LogsRange::Months3,
        LogsRange::Months6,
        LogsRange::Year1,
        LogsRange::Years2,
        LogsRange::All,
    ];

    /// How far back from the newest log the range reaches, in seconds; `None` for `All`.
    pub fn reach_secs(self) -> Option<i64> {
        let days = match self {
            LogsRange::Month1 => 30,
            LogsRange::Months3 => 90,
            LogsRange::Months6 => 180,
            LogsRange::Year1 => 365,
            LogsRange::Years2 => 730,
            LogsRange::All => return None,
        };
        Some(days * DAY)
    }

    /// The stable name the settings table stores.
    pub fn key(self) -> &'static str {
        match self {
            LogsRange::Month1 => "1m",
            LogsRange::Months3 => "3m",
            LogsRange::Months6 => "6m",
            LogsRange::Year1 => "1y",
            LogsRange::Years2 => "2y",
            LogsRange::All => "all",
        }
    }

    /// The range `key` names; `None` for anything else.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::STOPS.into_iter().find(|range| range.key() == key)
    }
}

/// The start of `range`'s window ending at `anchor`, the player's newest log (unix seconds);
/// `None` for `All`, which has no start.
pub fn window_start(anchor: i64, range: LogsRange) -> Option<i64> {
    range.reach_secs().map(|reach| anchor - reach)
}
