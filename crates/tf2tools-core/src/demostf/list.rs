//! Browsing demos.tf's demo list: formats, ranges and paging.

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

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
