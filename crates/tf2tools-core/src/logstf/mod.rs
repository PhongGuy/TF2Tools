//! logs.tf: Steam IDs, the player cache and the log client.

mod averages;
mod cache;
mod client;
mod detail;
mod steam_id;
mod steam_xml;

use std::time::Duration;

use crate::weapons::TfClass;

pub use averages::ClassAverage;
pub use cache::{LogRow, LogState, LogsCache, TrackedPlayer};
pub use client::{LogsClient, LogsEndpoints, LogsProgress, LogsSyncSummary};
pub use detail::{
    ClassKills, ClassLine, HealTarget, LogColumns, LogDetail, MedicLine, PlayerLine, TeamTotals,
    parse_log,
};
pub use steam_id::{PlayerInput, SteamId, parse_player_input};

/// The SteamID64 of account number 0 in the public universe.
pub const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

/// The pause before each logs.tf or Steam request, so a sync stays polite.
pub const LOGS_SPACING: Duration = Duration::from_secs(1);

/// The largest log or profile body accepted.
pub const LOG_BODY_LIMIT: u64 = 2 * 1024 * 1024;

/// How far back the log list and the class averages reach.
pub const WINDOW_SECS: i64 = 90 * 86_400;

/// Logs uploaded longer ago than this are deleted after a sync.
pub const PRUNE_SECS: i64 = 120 * 86_400;

/// The most logs a player's window holds; older ones past it are left out.
pub const LOG_CAP: usize = 400;

/// A player synced more recently than this is not synced again unless forced.
pub const STALE_SECS: i64 = 30 * 60;

/// A log fetched within this long of its upload may still change and is fetched once more.
pub const PROVISIONAL_SECS: i64 = 60 * 60;

/// The start of the averages and list window for `now` (unix seconds).
pub fn window_start(now: i64) -> i64 {
    now - WINDOW_SECS
}

/// logs.tf's class name ("heavyweapons", "demoman", ...) as a `TfClass`; `None` for anything else.
pub fn logstf_class(name: &str) -> Option<TfClass> {
    Some(match name {
        "scout" => TfClass::Scout,
        "soldier" => TfClass::Soldier,
        "pyro" => TfClass::Pyro,
        "demoman" => TfClass::Demoman,
        "heavyweapons" => TfClass::Heavy,
        "engineer" => TfClass::Engineer,
        "medic" => TfClass::Medic,
        "sniper" => TfClass::Sniper,
        "spy" => TfClass::Spy,
        _ => return None,
    })
}

/// The inverse of `logstf_class`: the name logs.tf uses for `class`, as the cache stores it.
pub(crate) fn logstf_class_name(class: TfClass) -> &'static str {
    match class {
        TfClass::Scout => "scout",
        TfClass::Soldier => "soldier",
        TfClass::Pyro => "pyro",
        TfClass::Demoman => "demoman",
        TfClass::Heavy => "heavyweapons",
        TfClass::Engineer => "engineer",
        TfClass::Medic => "medic",
        TfClass::Sniper => "sniper",
        TfClass::Spy => "spy",
    }
}
