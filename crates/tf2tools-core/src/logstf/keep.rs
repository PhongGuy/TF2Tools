//! How long a full logs.tf log stays stored after it was last opened or fetched.

/// How many days a counted log keeps its full body before a trim pass drops it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum KeepFullLogs {
    /// One day.
    Day1,
    /// Three days.
    Days3,
    /// A week; the default.
    #[default]
    Days7,
    /// Two weeks.
    Days14,
    /// Thirty days.
    Days30,
    /// Ninety days.
    Days90,
    /// Every full log is kept.
    Forever,
}

impl KeepFullLogs {
    /// Every choice, shortest first.
    pub const ALL: [KeepFullLogs; 7] = [
        KeepFullLogs::Day1,
        KeepFullLogs::Days3,
        KeepFullLogs::Days7,
        KeepFullLogs::Days14,
        KeepFullLogs::Days30,
        KeepFullLogs::Days90,
        KeepFullLogs::Forever,
    ];

    /// The days a full log is kept; `None` for `Forever`.
    pub fn days(self) -> Option<i64> {
        Some(match self {
            KeepFullLogs::Day1 => 1,
            KeepFullLogs::Days3 => 3,
            KeepFullLogs::Days7 => 7,
            KeepFullLogs::Days14 => 14,
            KeepFullLogs::Days30 => 30,
            KeepFullLogs::Days90 => 90,
            KeepFullLogs::Forever => return None,
        })
    }

    /// The stable name the settings table stores.
    pub fn key(self) -> &'static str {
        match self {
            KeepFullLogs::Day1 => "1d",
            KeepFullLogs::Days3 => "3d",
            KeepFullLogs::Days7 => "7d",
            KeepFullLogs::Days14 => "14d",
            KeepFullLogs::Days30 => "30d",
            KeepFullLogs::Days90 => "90d",
            KeepFullLogs::Forever => "forever",
        }
    }

    /// The choice `key` names; `None` for anything else.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|keep| keep.key() == key)
    }
}
