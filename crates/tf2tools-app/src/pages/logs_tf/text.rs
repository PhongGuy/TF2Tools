//! The logs.tf page's wording and links, kept free of UI so tests read them without a window.

use std::time::Duration;

use tf2tools_core::{
    EventKind, LogsFetchError, LogsRange, LogsSyncError, Medigun, ResolveError, SteamId, Team,
};

use crate::logs_sync::SyncProgress;

/// logs.tf's front page, which the page's credit opens.
pub const LOGS_TF_URL: &str = "https://logs.tf";

/// The empty page's explanation, shown until a player is tracked.
pub const EMPTY_TEXT: &str = "Track a player to see their class averages and logs from logs.tf.";

/// Why a logs.tf change could not be kept, in player words.
pub const SAVE_FAILED: &str = "Couldn't save logs.tf data on this computer.";

/// Why a Steam lookup failed when Steam could not answer, in player words.
pub const STEAM_UNREACHABLE: &str = "Can't reach Steam. Check your internet connection.";

/// A player's chip while their removal waits for their sync to end.
pub const REMOVING: &str = "Removing...";

/// The add dialog's Add button while Steam looks the player up.
pub const LOOKING_UP: &str = "Looking up...";

/// One log's page on logs.tf.
pub fn log_url(id: u64) -> String {
    format!("{LOGS_TF_URL}/{id}")
}

/// A player's profile on logs.tf, addressed by their SteamID64.
pub fn profile_url(id: SteamId) -> String {
    format!("{LOGS_TF_URL}/profile/{}", id.id64())
}

/// The add dialog's message for a failed lookup.
pub fn resolve_message(error: &ResolveError) -> &'static str {
    match error {
        ResolveError::NotFound => "We couldn't find that Steam profile.",
        ResolveError::RateLimited { .. }
        | ResolveError::Http { .. }
        | ResolveError::Network { .. }
        | ResolveError::TooLarge { .. }
        | ResolveError::Malformed { .. }
        | _ => STEAM_UNREACHABLE,
    }
}

/// Why a sync could not reach logs.tf or got an error page back, in player words.
pub const LOGS_TF_DOWN: &str = "logs.tf isn't responding right now. Try again later.";

/// Why logs.tf turned a request away after every retry, in player words.
pub const LOGS_TF_BUSY: &str = "logs.tf is busy. Try again in a few minutes.";

/// A count with thousands separators: "4,812".
pub fn count_text(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The progress line while a sync reads the player's list: "Finding logs: 2,000 of 4,812".
pub fn finding_text(listed: usize, total: usize) -> String {
    format!(
        "Finding logs: {} of {}",
        count_text(listed),
        count_text(total)
    )
}

/// The progress line while a sync downloads logs, with the time left once it is a minute
/// or more: "Downloading logs: 120 of 4,812, about 2 h 40 min left".
pub fn progress_text(done: usize, total: usize, left: Duration) -> String {
    let line = format!(
        "Downloading logs: {} of {}",
        count_text(done),
        count_text(total)
    );
    // Minutes round to the nearest, so 89 s reads "1 min" and 90 s "2 min".
    let minutes = (left.as_secs() + 30) / 60;
    let estimate = match (left.as_secs() < 60, minutes / 60, minutes % 60) {
        (true, ..) => return line,
        (false, 0, m) => format!("{m} min"),
        (false, h, 0) => format!("{h} h"),
        (false, h, m) => format!("{h} h {m} min"),
    };
    format!("{line}, about {estimate} left")
}

/// The header ring's tooltip, worded like the store rings: counts once logs download, and a
/// plain "Syncing logs.tf..." while the list is still read.
pub fn ring_label(progress: Option<SyncProgress>) -> String {
    match progress {
        Some(SyncProgress::Downloading { done, total, .. }) => {
            format!("Syncing logs.tf: {done} / {total}")
        }
        _ => "Syncing logs.tf...".to_owned(),
    }
}

/// The line for a running sync in whichever phase it is.
pub fn progress_line(progress: SyncProgress) -> String {
    match progress {
        SyncProgress::Finding { listed, total } => finding_text(listed, total),
        SyncProgress::Downloading { done, total, left } => progress_text(done, total, left),
    }
}

/// The line under the chips for a failed sync. A player removed while their sync waited is
/// no failure, so the page never asks for it.
pub fn sync_message(error: &LogsSyncError) -> &'static str {
    match error {
        LogsSyncError::Network { .. } => "You're offline. Showing saved logs.",
        LogsSyncError::RateLimited { .. } => LOGS_TF_BUSY,
        LogsSyncError::Malformed { .. } => "logs.tf sent something we didn't understand.",
        LogsSyncError::Database { .. } => SAVE_FAILED,
        LogsSyncError::Http { .. } | LogsSyncError::TooLarge { .. } | _ => LOGS_TF_DOWN,
    }
}

/// The open panel's line while a log not yet downloaded is fetched.
pub const LOG_LOADING: &str = "Loading log...";

/// The open panel's line when a log can't be fetched for lack of a connection.
pub const LOG_OFFLINE: &str = "Can't load this log while you're offline.";

/// The open panel's line for a failed fetch; None when the row's own note says it (gone,
/// unreadable, too large).
pub fn fetch_message(error: &LogsFetchError) -> Option<&'static str> {
    match error {
        LogsFetchError::Network { .. } => Some(LOG_OFFLINE),
        LogsFetchError::RateLimited { .. } => Some(LOGS_TF_BUSY),
        LogsFetchError::Http { .. } => Some(LOGS_TF_DOWN),
        LogsFetchError::Cache { .. } => Some(SAVE_FAILED),
        LogsFetchError::Gone | LogsFetchError::Unreadable | LogsFetchError::TooLarge { .. } => None,
        _ => Some(LOGS_TF_DOWN),
    }
}

/// Time on a class, rounded down to whole minutes: "45m", "1h 30m", "0m".
pub fn time_played_text(seconds: u64) -> String {
    let minutes = seconds / 60;
    match minutes / 60 {
        0 => format!("{minutes}m"),
        hours => format!("{hours}h {}m", minutes % 60),
    }
}

/// A rate with one decimal: "0.6".
pub fn one_decimal(value: f64) -> String {
    format!("{value:.1}")
}

/// A rate as a whole number: "136".
pub fn whole(value: f64) -> String {
    format!("{value:.0}")
}

/// A history range's stop label under the slider: "1 month" up to "All".
pub fn range_label(range: LogsRange) -> &'static str {
    match range {
        LogsRange::Month1 => "1 month",
        LogsRange::Months3 => "3 months",
        LogsRange::Months6 => "6 months",
        LogsRange::Year1 => "1 year",
        LogsRange::Years2 => "2 years",
        LogsRange::All | _ => "All",
    }
}

/// The line under the class grid: the range, then how many logs it holds:
/// "Last 3 months, 87 logs", "Last month, 1 log", "All logs, 4,812".
pub fn summary_text(range: LogsRange, logs: usize) -> String {
    // "Last month" and "Last year" drop the stop label's "1".
    let span = match range {
        LogsRange::Month1 => "month",
        LogsRange::Year1 => "year",
        LogsRange::All => return format!("All logs, {}", count_text(logs)),
        other @ (LogsRange::Months3 | LogsRange::Months6 | LogsRange::Years2 | _) => {
            range_label(other)
        }
    };
    let noun = if logs == 1 { "log" } else { "logs" };
    format!("Last {span}, {} {noun}", count_text(logs))
}

/// The log list's line when logs.tf has no logs for the active player.
pub const NO_LOGS_TEXT: &str = "No logs on logs.tf for this player yet.";

/// A row's note in place of its stats while its log downloads.
pub const LOG_PENDING: &str = "Downloading...";

/// A row's note when logs.tf no longer has its log.
pub const LOG_GONE: &str = "This log was removed from logs.tf";

/// A row's note when its log's body could not be read.
pub const LOG_UNREADABLE: &str = "This log couldn't be read";

/// A match length as minutes and seconds, as logs.tf writes it: "28:34", "60:00".
pub fn duration_text(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// An upload time as a date on this computer's clock: "2026-10-17".
pub fn date_text(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default()
}

/// `part` as a whole percent of `whole`, rounded to nearest: "47%"; a whole of 0 reads "0%".
pub fn share_text(part: u32, whole: u32) -> String {
    format!("{}%", percent(part, whole))
}

/// Hits as a whole percent of shots: "43%"; with no shots there is no accuracy, "-".
pub fn accuracy_text(hits: u32, shots: u32) -> String {
    if shots == 0 {
        return "-".to_owned();
    }
    format!("{}%", percent(hits, shots))
}

// Integer half-up rounding, so 4.5% reads "5%" as on logs.tf.
fn percent(part: u32, whole: u32) -> u64 {
    if whole == 0 {
        return 0;
    }
    (u64::from(part) * 100 + u64::from(whole) / 2) / u64::from(whole)
}

/// Whether a players table cell reads zero, whole or to one decimal, so it can be muted.
pub fn reads_zero(cell: &str) -> bool {
    matches!(cell, "0" | "0.0")
}

/// A team's name as the app writes it: "BLU" or "RED".
pub fn team_name(team: Team) -> &'static str {
    match team {
        Team::Red => "RED",
        Team::Blue | _ => "BLU",
    }
}

/// A medigun's name as the game writes it; one logs.tf names otherwise keeps that name.
pub fn medigun_name(medigun: &Medigun) -> &str {
    match medigun {
        Medigun::Kritzkrieg => "Kritzkrieg",
        Medigun::QuickFix => "Quick-Fix",
        Medigun::Vaccinator => "Vaccinator",
        Medigun::Other(name) => name,
        Medigun::MediGun | _ => "Medigun",
    }
}

/// (Type, Description) for one round event, players named through `name_of`. A charge names
/// its medigun only when it is not the stock one; an unknown type shows its raw name alone.
pub fn event_text(kind: &EventKind, name_of: &dyn Fn(SteamId) -> String) -> (String, String) {
    let (ty, description) = match kind {
        EventKind::PointCap { team, point } => (
            "Point Capture",
            format!("{} captured point {point}", team_name(*team)),
        ),
        EventKind::Charge { medic, medigun, .. } => {
            let medic = name_of(*medic);
            let description = match medigun {
                Medigun::MediGun => medic,
                other @ (Medigun::Kritzkrieg
                | Medigun::QuickFix
                | Medigun::Vaccinator
                | Medigun::Other(_)
                | _) => format!("{medic} ({})", medigun_name(other)),
            };
            ("Übercharge", description)
        }
        EventKind::MedicDeath { medic, killer, .. } => {
            let medic = name_of(*medic);
            let description = match killer {
                Some(killer) => format!("{medic} killed by {}", name_of(*killer)),
                None => format!("{medic} died"),
            };
            ("Medic Death", description)
        }
        EventKind::Drop { medic, .. } => {
            ("Drop", format!("{} dropped a full charge", name_of(*medic)))
        }
        EventKind::RoundWin { team } => {
            ("Round end", format!("Round win for {}", team_name(*team)))
        }
        EventKind::Other { name } => return (name.clone(), String::new()),
        _ => return (format!("{kind:?}"), String::new()),
    };
    (ty.to_owned(), description)
}
