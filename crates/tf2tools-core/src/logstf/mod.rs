//! logs.tf: Steam IDs, the player cache and the log client.

mod cache;
mod client;
mod steam_id;
mod steam_xml;

use std::time::Duration;

use crate::weapons::TfClass;

pub use cache::{LogsCache, TrackedPlayer};
pub use client::{LogsClient, LogsEndpoints};
pub use steam_id::{PlayerInput, SteamId, parse_player_input};

/// The SteamID64 of account number 0 in the public universe.
pub const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

/// The pause before each logs.tf or Steam request, so a sync stays polite.
pub const LOGS_SPACING: Duration = Duration::from_secs(1);

/// The largest log or profile body accepted.
pub const LOG_BODY_LIMIT: u64 = 2 * 1024 * 1024;

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
