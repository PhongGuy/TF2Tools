//! Per-class averages over a player's window of logs.

use crate::weapons::TfClass;

/// Player ?1's window: their logs from ?2 seconds before their newest cached log up to it, or
/// all of them when ?2 is NULL. The list, the averages and the pending check all select from
/// it, so "N logs" always matches the grid; a player with no logs has an empty window.
pub(super) const WINDOW_LOGS: &str = "WITH window_logs AS (
        SELECT l.id FROM logstf_logs l
        JOIN logstf_player_logs m ON m.log_id = l.id AND m.steam_id = ?1
        WHERE ?2 IS NULL OR l.date >= (
            SELECT MAX(a.date) FROM logstf_logs a
            JOIN logstf_player_logs am ON am.log_id = a.id AND am.steam_id = ?1
        ) - ?2
    )";

/// The player's summed class rows over the window, one row per class played.
pub(super) const CLASS_SUMS: &str = "SELECT r.class, SUM(r.seconds), COUNT(DISTINCT r.log_id),
        SUM(r.kills), SUM(r.deaths), SUM(r.assists), SUM(r.dmg), SUM(r.heal), SUM(r.ubers)
    FROM logstf_class_rows r
    WHERE r.steam_id = ?1 AND r.log_id IN (SELECT id FROM window_logs)
    GROUP BY r.class";

/// One class's summed stats as `CLASS_SUMS` returns them, after the class name.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ClassSums {
    pub(super) seconds: i64,
    pub(super) logs: i64,
    pub(super) kills: i64,
    pub(super) deaths: i64,
    pub(super) assists: i64,
    pub(super) dmg: i64,
    pub(super) heal: i64,
    pub(super) ubers: i64,
}

/// A player's rates on one class over the window, weighted by time on the class.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ClassAverage {
    class: TfClass,
    seconds: u64,
    logs: u32,
    kills_per_min: f64,
    deaths_per_min: f64,
    assists_per_min: f64,
    dmg_per_min: f64,
    kad: f64,
    heal_per_min: Option<f64>,
    ubers_per_10min: Option<f64>,
}

impl ClassAverage {
    /// Rates from `sums`; a class with no time has every rate 0.0 and no Medic rates.
    pub(super) fn from_sums(class: TfClass, sums: ClassSums) -> Self {
        let secs = sums.seconds.max(0) as f64;
        let per = |n: i64, span: f64| {
            if secs > 0.0 {
                n as f64 / secs * span
            } else {
                0.0
            }
        };
        let medic =
            |n: i64, span: f64| (class == TfClass::Medic && secs > 0.0).then(|| per(n, span));
        ClassAverage {
            class,
            seconds: u64::try_from(sums.seconds).unwrap_or(0),
            logs: u32::try_from(sums.logs).unwrap_or(0),
            kills_per_min: per(sums.kills, 60.0),
            deaths_per_min: per(sums.deaths, 60.0),
            assists_per_min: per(sums.assists, 60.0),
            dmg_per_min: per(sums.dmg, 60.0),
            kad: (sums.kills + sums.assists) as f64 / sums.deaths.max(1) as f64,
            heal_per_min: medic(sums.heal, 60.0),
            ubers_per_10min: medic(sums.ubers, 600.0),
        }
    }

    /// The class.
    pub fn class(&self) -> TfClass {
        self.class
    }

    /// Total time on the class.
    pub fn seconds(&self) -> u64 {
        self.seconds
    }

    /// How many logs the player spent any time on the class in.
    pub fn logs(&self) -> u32 {
        self.logs
    }

    /// Kills per minute on the class.
    pub fn kills_per_min(&self) -> f64 {
        self.kills_per_min
    }

    /// Deaths per minute on the class.
    pub fn deaths_per_min(&self) -> f64 {
        self.deaths_per_min
    }

    /// Assists per minute on the class.
    pub fn assists_per_min(&self) -> f64 {
        self.assists_per_min
    }

    /// Damage per minute on the class.
    pub fn dmg_per_min(&self) -> f64 {
        self.dmg_per_min
    }

    /// (kills + assists) / deaths, with at least one death.
    pub fn kad(&self) -> f64 {
        self.kad
    }

    /// Healing per minute on Medic; `None` for other classes or no time on Medic.
    pub fn heal_per_min(&self) -> Option<f64> {
        self.heal_per_min
    }

    /// Ubers per ten minutes on Medic; `None` for other classes or no time on Medic.
    pub fn ubers_per_10min(&self) -> Option<f64> {
        self.ubers_per_10min
    }

    /// Whether the player spent any time on the class in the window.
    pub fn played(&self) -> bool {
        self.seconds > 0
    }
}
