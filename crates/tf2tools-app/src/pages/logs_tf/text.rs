//! The logs.tf page's wording and links, kept free of UI so tests read them without a window.

use std::time::Duration;

use gpui_kit::SharedString;
use tf2tools_core::{
    EventKind, LogsFetchError, LogsRange, LogsSyncError, Medigun, PlayerInputError, ResolveError,
    SteamId, Team, TfClass,
};

use crate::i18n::{Phrase, counted_plural, format, labelled, names, tr};
use crate::logs_sync::SyncProgress;

/// logs.tf's front page, which the page's credit opens.
pub const LOGS_TF_URL: &str = "https://logs.tf";

/// The empty page's explanation, shown until a player is tracked.
pub fn empty_text() -> SharedString {
    tr!("logs_tf.empty")
}

/// Why a logs.tf change could not be kept, in player words; the toasts say it the same way.
pub fn save_failed() -> SharedString {
    tr!("messages.logs_tf.save_failed")
}

/// Why a Steam lookup failed when Steam could not answer, in player words.
pub fn steam_unreachable() -> SharedString {
    tr!("logs_tf.steam_unreachable")
}

/// A player's chip while their removal waits for their sync to end.
pub fn removing() -> SharedString {
    tr!("logs_tf.removing")
}

/// The add dialog's Add button while Steam looks the player up.
pub fn looking_up() -> SharedString {
    tr!("logs_tf.looking_up")
}

/// One log's page on logs.tf.
pub fn log_url(id: u64) -> String {
    format!("{LOGS_TF_URL}/{id}")
}

/// A player's profile on logs.tf, addressed by their SteamID64.
pub fn profile_url(id: SteamId) -> String {
    format!("{LOGS_TF_URL}/profile/{}", id.id64())
}

/// The add dialog's message for a failed lookup.
pub fn resolve_message(error: &ResolveError) -> SharedString {
    resolve_phrase(error).text()
}

/// `resolve_message` as a phrase, worded each time it is read.
pub fn resolve_phrase(error: &ResolveError) -> Phrase {
    match error {
        ResolveError::NotFound => Phrase::new(|| tr!("logs_tf.not_found")),
        ResolveError::RateLimited { .. }
        | ResolveError::Http { .. }
        | ResolveError::Network { .. }
        | ResolveError::TooLarge { .. }
        | ResolveError::Malformed { .. }
        | _ => Phrase::new(steam_unreachable),
    }
}

/// The add dialog's message for text that names no Steam player.
pub fn player_input_error(error: &PlayerInputError) -> SharedString {
    player_input_phrase(error).text()
}

/// `player_input_error` as a phrase, worded each time it is read.
pub fn player_input_phrase(error: &PlayerInputError) -> Phrase {
    match error {
        PlayerInputError::NotSteam | _ => Phrase::new(|| tr!("logs_tf.not_steam")),
    }
}

/// Why a sync could not reach logs.tf or got an error page back, in player words.
pub fn logs_tf_down() -> SharedString {
    tr!("logs_tf.down")
}

/// Why logs.tf turned a request away after every retry, in player words.
pub fn logs_tf_busy() -> SharedString {
    tr!("logs_tf.busy")
}

/// A count with thousands separators in the player's language: "4,812".
pub fn count_text(n: usize) -> String {
    format::grouped(u64::try_from(n).unwrap_or(u64::MAX))
}

/// The progress line while a sync reads the player's list: "Finding logs: 2,000 of 4,812".
pub fn finding_text(listed: usize, total: usize) -> String {
    tr!(
        "logs_tf.progress.finding",
        listed = count_text(listed),
        total = count_text(total)
    )
    .to_string()
}

/// The progress line while a sync downloads logs, with the time left once it is a minute
/// or more: "Downloading logs: 120 of 4,812, about 2 h 40 min left".
pub fn progress_text(done: usize, total: usize, left: Duration) -> String {
    let (done, total) = (count_text(done), count_text(total));
    if left.as_secs() < 60 {
        return tr!("logs_tf.progress.downloading", done = done, total = total).to_string();
    }
    // Minutes round to the nearest, so 89 s reads "1 min" and 90 s "2 min".
    let minutes = (left.as_secs() + 30) / 60;
    tr!(
        "logs_tf.progress.downloading_left",
        done = done,
        total = total,
        time = format::time_left(minutes)
    )
    .to_string()
}

/// The header ring's tooltip, worded like the store rings: counts once logs download, and a
/// plain "Syncing logs.tf..." while the list is still read.
pub fn ring_label(progress: Option<SyncProgress>) -> String {
    match progress {
        Some(SyncProgress::Downloading { done, total, .. }) => {
            tr!("logs_tf.ring.progress", done = done, total = total).to_string()
        }
        _ => tr!("logs_tf.ring.listing").to_string(),
    }
}

/// The line for a running sync in whichever phase it is.
pub fn progress_line(progress: SyncProgress) -> String {
    match progress {
        SyncProgress::Finding { listed, total } => finding_text(listed, total),
        SyncProgress::Downloading { done, total, left } => progress_text(done, total, left),
    }
}

/// The line under the chips for a failed sync, `has_saved` when the player has logs saved
/// to fall back on. A player removed while their sync waited is no failure, so the page never
/// asks for it.
pub fn sync_message(error: &LogsSyncError, has_saved: bool) -> SharedString {
    sync_message_phrase(error, has_saved).text()
}

/// `sync_message` as a phrase, worded each time it is read.
pub fn sync_message_phrase(error: &LogsSyncError, has_saved: bool) -> Phrase {
    match error {
        LogsSyncError::Network { .. } if has_saved => {
            Phrase::new(|| tr!("logs_tf.sync.offline_saved"))
        }
        LogsSyncError::Network { .. } => Phrase::new(|| tr!("logs_tf.sync.unreachable")),
        LogsSyncError::RateLimited { .. } => Phrase::new(logs_tf_busy),
        LogsSyncError::Malformed { .. } => Phrase::new(|| tr!("logs_tf.sync.malformed")),
        LogsSyncError::Database { .. } => Phrase::new(save_failed),
        LogsSyncError::Http { .. } | LogsSyncError::TooLarge { .. } | _ => logs_tf_down_phrase(),
    }
}

/// `logs_tf_down` as a phrase, worded each time it is read.
pub fn logs_tf_down_phrase() -> Phrase {
    Phrase::new(logs_tf_down)
}

/// The toast title once a log's zip is saved; its description names the file.
pub fn downloaded() -> SharedString {
    tr!("logs_tf.download.done")
}

/// The toast title when Download log fails; its description says why.
pub fn download_failed() -> SharedString {
    tr!("logs_tf.download.failed")
}

/// Why Download log failed, when the save itself could not be written.
pub fn write_failed() -> SharedString {
    tr!("logs_tf.download.write_failed")
}

/// Why Download log failed, in the player's words.
pub fn download_reason(error: &LogsFetchError) -> SharedString {
    match error {
        LogsFetchError::Gone => tr!("logs_tf.download.gone"),
        LogsFetchError::Network { .. } => tr!("logs_tf.download.offline"),
        LogsFetchError::RateLimited { .. } => logs_tf_busy(),
        LogsFetchError::TooLarge { .. } => tr!("logs_tf.download.too_big"),
        // The lint only wants every known variant named; a wildcard that shares a known arm's
        // text joins that arm, one with its own text stands alone.
        LogsFetchError::Unreadable
        | LogsFetchError::Http { .. }
        | LogsFetchError::Cache { .. }
        | _ => logs_tf_down(),
    }
}

/// The open panel's line while a log not yet downloaded is fetched.
pub fn log_loading() -> SharedString {
    tr!("logs_tf.open.loading")
}

/// The open panel's line when a log can't be fetched for lack of a connection.
pub fn log_offline() -> SharedString {
    tr!("logs_tf.open.offline")
}

/// The open panel's line when logs.tf no longer serves a trimmed log.
pub fn full_log_gone() -> SharedString {
    tr!("logs_tf.open.full_gone")
}

/// The open panel's line when a trimmed log can't be downloaded for lack of a connection.
pub fn trimmed_offline() -> SharedString {
    tr!("logs_tf.open.trimmed_offline")
}

/// The open panel's line for a failed fetch; None when the row's own note says it (gone,
/// unreadable, too large).
pub fn fetch_message(error: &LogsFetchError) -> Option<SharedString> {
    fetch_phrase(error).map(|phrase| phrase.text())
}

/// `fetch_message` as a phrase, worded each time it is read.
pub fn fetch_phrase(error: &LogsFetchError) -> Option<Phrase> {
    match error {
        LogsFetchError::Network { .. } => Some(Phrase::new(log_offline)),
        LogsFetchError::RateLimited { .. } => Some(Phrase::new(logs_tf_busy)),
        LogsFetchError::Cache { .. } => Some(Phrase::new(save_failed)),
        LogsFetchError::FullLogGone => Some(Phrase::new(full_log_gone)),
        LogsFetchError::Gone | LogsFetchError::Unreadable | LogsFetchError::TooLarge { .. } => None,
        LogsFetchError::Http { .. } | _ => Some(logs_tf_down_phrase()),
    }
}

/// Time on a class, rounded down to whole minutes: "45m", "1h 30m", "0m".
pub fn time_played_text(seconds: u64) -> String {
    format::hours_minutes(seconds / 60).to_string()
}

/// A rate with one decimal, half rounding up as on logs.tf: "0.6", 0.25 reads "0.3".
pub fn one_decimal(value: f64) -> String {
    format::decimal_point(&half_up(value))
}

// One decimal with an English point, half rounding up; the one rounding rule for tenths.
fn half_up(value: f64) -> String {
    format!("{:.1}", (value * 10.).round() / 10.)
}

/// A rate as a whole number, rounded to nearest, for the app's own numbers: "136".
pub fn whole(value: f64) -> String {
    format!("{value:.0}")
}

/// What a log row shows for a value it does not have.
pub const UNKNOWN: &str = "\u{2013}";

/// A log row's per-minute rate: `n` over the whole log, floored as the open log's DA/M and
/// DT/M are, or a dash while either is unknown.
pub fn per_min_text(n: Option<u32>, length: Option<u32>) -> String {
    // Whole-number math, as core's per-minute rates, so an exact answer never floors one less.
    match (n, length.filter(|&length| length > 0)) {
        (Some(n), Some(length)) => (u64::from(n) * 60 / u64::from(length)).to_string(),
        _ => UNKNOWN.to_owned(),
    }
}

/// A log row's KA/D with one decimal, no deaths counting as one as on logs.tf.
pub fn kapd_text(kills: u32, assists: u32, deaths: u32) -> String {
    one_decimal((f64::from(kills) + f64::from(assists)) / f64::from(deaths.max(1)))
}

/// A count a log row may not have: the number, or a dash.
pub fn known_text(n: Option<u32>) -> String {
    n.map_or_else(|| UNKNOWN.to_owned(), |n| n.to_string())
}

/// A number logs.tf shows whole, cut down as logs.tf does: 154.9 reads "154". These counts
/// are never negative, so anything below zero, -0.0 included, reads "0".
pub fn floored(value: f64) -> String {
    floor_whole(value).to_string()
}

// The cast saturates, so a negative or NaN value lands on 0 rather than printing "-0".
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn floor_whole(value: f64) -> u64 {
    value.floor().max(0.0) as u64
}

/// A history range's stop label under the slider: "1 month" up to "All".
pub fn range_label(range: LogsRange) -> SharedString {
    let months = |n: u64| counted_plural("logs_tf.range.months", n, &n.to_string(), &[]);
    let years = |n: u64| counted_plural("logs_tf.range.years", n, &n.to_string(), &[]);
    match range {
        LogsRange::Month1 => months(1),
        LogsRange::Months3 => months(3),
        LogsRange::Months6 => months(6),
        LogsRange::Year1 => years(1),
        LogsRange::Years2 => years(2),
        LogsRange::All | _ => tr!("logs_tf.range.all"),
    }
}

/// The line above the class table: the range, then how many logs it holds. The range counts
/// back from `newest`, the newest log's upload time, so it reads as "Last 3 months, 87 logs"
/// only while that log is at most a week before `now`; an older one names the day the range
/// ends: "1 year up to 13 Dec 2022, 274 logs". `All` reads "All logs, 4,812".
pub fn summary_text(range: LogsRange, logs: usize, newest: Option<i64>, now: i64) -> String {
    const RECENT: i64 = 7 * 86_400;
    let count = count_text(logs);
    if range == LogsRange::All {
        return tr!("logs_tf.summary.all", count = count).to_string();
    }
    let n = u64::try_from(logs).unwrap_or(u64::MAX);
    let text = match newest.filter(|&newest| now - newest > RECENT) {
        Some(newest) => counted_plural(
            "logs_tf.summary.up_to",
            n,
            &count,
            &[("range", &range_label(range)), ("date", &day_text(newest))],
        ),
        // "Last month" and "Last year" drop the stop label's "1", so each has its own key.
        None => match range {
            LogsRange::Month1 => counted_plural("logs_tf.summary.last_month", n, &count, &[]),
            LogsRange::Year1 => counted_plural("logs_tf.summary.last_year", n, &count, &[]),
            other @ (LogsRange::Months3 | LogsRange::Months6 | LogsRange::Years2 | _) => {
                counted_plural(
                    "logs_tf.summary.last",
                    n,
                    &count,
                    &[("span", &range_label(other))],
                )
            }
        },
    };
    text.to_string()
}

// A day on this computer's clock as a player writes it: "13 Dec 2022".
fn day_text(unix: i64) -> String {
    local_time(unix)
        .map(|t| format::day_month_year(t.date()).to_string())
        .unwrap_or_default()
}

/// The class weapons block's line when the class's logs carry no weapon stats at all.
pub fn no_weapons_text() -> SharedString {
    tr!("logs_tf.weapons.none")
}

/// The class weapons block's title: "Scout weapons · 3 months · 1 log", "All" for every log.
pub fn weapons_title(class: TfClass, range: LogsRange, logs: u32) -> String {
    counted_plural(
        "logs_tf.weapons_title",
        u64::from(logs),
        &count_text(usize::try_from(logs).unwrap_or(usize::MAX)),
        &[
            ("class", &names::class_name(class)),
            ("range", &range_label(range)),
        ],
    )
    .to_string()
}

/// The class weapons block's note while older logs still wait for their weapons counted.
pub fn counting_text(left: u32) -> String {
    tr!("logs_tf.weapons.counting", left = left).to_string()
}

/// The log list's line when logs.tf has no logs for the active player.
pub fn no_logs_text() -> SharedString {
    tr!("logs_tf.list.no_logs")
}

/// A row's note in place of its stats while its log downloads.
pub fn log_pending() -> SharedString {
    tr!("common.size.downloading")
}

/// A row's note when logs.tf no longer has its log.
pub fn log_gone() -> SharedString {
    tr!("logs_tf.list.gone")
}

/// A row's note when its log's body could not be read.
pub fn log_unreadable() -> SharedString {
    tr!("logs_tf.list.unreadable")
}

/// A round's length or an event's time, minutes padded to two digits as logs.tf's rounds
/// table writes them: "05:30", "00:50".
pub fn round_length_text(seconds: u32) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// A match length as minutes and seconds, as logs.tf writes it: "28:34", "60:00".
pub fn duration_text(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// An upload time as a date and minute on this computer's clock: "2026-10-17 20:56".
pub fn date_time_text(unix: i64) -> String {
    local_time(unix)
        .map(|t| format::date_time(t, false).to_string())
        .unwrap_or_default()
}

/// An upload time as a date on this computer's clock: "2026-10-17".
pub fn date_text(unix: i64) -> String {
    local_time(unix)
        .map(|t| format::numeric_date(t.date()).to_string())
        .unwrap_or_default()
}

// An upload time on this computer's wall clock, or None when it is out of chrono's range.
fn local_time(unix: i64) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::from_timestamp(unix, 0).map(|t| t.with_timezone(&chrono::Local).naive_local())
}

/// `part` as a whole percent of `whole`, rounded to nearest: "47%"; a whole of 0 reads "0%".
pub fn share_text(part: u32, whole: u32) -> String {
    format::percent(percent(part, whole)).to_string()
}

/// `part` as a whole percent of `whole` cut down, as logs.tf shows heal and charge shares:
/// 21.9% reads "21%"; a whole of 0 reads "0%".
pub fn floored_share_text(part: u32, whole: u32) -> String {
    let share = (u64::from(part) * 100)
        .checked_div(u64::from(whole))
        .unwrap_or(0);
    format::percent(share).to_string()
}

/// Hits as a whole percent of shots: "43%"; with no shots there is no accuracy, "-".
pub fn accuracy_text(hits: u32, shots: u32) -> String {
    if shots == 0 {
        return "-".to_owned();
    }
    format::percent(percent(hits, shots)).to_string()
}

// Integer half-up rounding, so 4.5% reads "5%".
fn percent(part: u32, whole: u32) -> u64 {
    if whole == 0 {
        return 0;
    }
    (u64::from(part) * 100 + u64::from(whole) / 2) / u64::from(whole)
}

/// Whether a players table cell reads zero, whole or to one decimal, so it can be muted.
pub fn reads_zero(cell: &str) -> bool {
    cell == "0" || cell == format::decimal_point("0.0")
}

/// The Show Killstreaks button's words with the log's count: "Show Killstreaks (13)".
pub fn streaks_label(n: usize) -> String {
    tr!("logs_tf.streaks", count = n).to_string()
}

/// What a nickname marker's tooltip says, naming the player as the log does.
pub fn nickname_note(log_name: &str) -> String {
    tr!("logs_tf.nickname_note", name = log_name).to_string()
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

/// A Medic's healing with its rate per minute: "28170 (986/m)".
pub fn healing_text(heal: u32, per_min: f64) -> String {
    format!("{heal} ({}/m)", floored(per_min))
}

/// One medigun's charges and their share of all `total`: "Medigun: 12 (100%)".
pub fn charge_text(medigun: &Medigun, n: u32, total: u32) -> String {
    labelled(
        medigun_name(medigun),
        format!("{n} ({})", floored_share_text(n, total)),
    )
}

/// A time in whole seconds, cut down as logs.tf shows it: 58.9 reads "58 s".
pub fn seconds_text(secs: f64) -> String {
    format::seconds(floor_whole(secs)).to_string()
}

/// A time to a tenth of a second: "6.5 s".
pub fn tenths_text(secs: f64) -> String {
    format::tenths(&half_up(secs)).to_string()
}

/// (Type, Description) for one round event, players named through `name_of`. A charge names
/// its medigun only when it is not the stock one; an unknown type shows its raw name alone.
pub fn event_text(kind: &EventKind, name_of: &dyn Fn(SteamId) -> String) -> (String, String) {
    let (ty, parts) = event_parts(kind, name_of, &|_| None);
    (ty, parts.into_iter().map(|(text, _)| text).collect())
}

/// One stretch of an event's description and the team whose colour it takes, if any.
pub type EventPart = (String, Option<Team>);

/// An event's type and its description in stretches. Each player's name carries their own
/// team, `team_of` for one the event does not place, and a team's name carries that team.
pub fn event_parts(
    kind: &EventKind,
    name_of: &dyn Fn(SteamId) -> String,
    team_of: &dyn Fn(SteamId) -> Option<Team>,
) -> (String, Vec<EventPart>) {
    let plain = |text: SharedString| (text.to_string(), None);
    let (ty, parts) = match kind {
        EventKind::PointCap { team, point } => (
            tr!("logs_tf.event.point_capture"),
            vec![
                (team_name(*team).to_owned(), Some(*team)),
                plain(tr!("logs_tf.event.captured", point = point)),
            ],
        ),
        EventKind::Charge {
            team,
            medic,
            medigun,
        } => {
            let mut parts = vec![(name_of(*medic), Some(*team))];
            if *medigun != Medigun::MediGun {
                parts.push((format!(" ({})", medigun_name(medigun)), None));
            }
            (tr!("logs_tf.event.ubercharge"), parts)
        }
        EventKind::MedicDeath {
            team,
            medic,
            killer,
        } => {
            let mut parts = vec![(name_of(*medic), Some(*team))];
            match killer {
                Some(killer) => {
                    parts.push(plain(tr!("logs_tf.event.killed_by")));
                    parts.push((name_of(*killer), team_of(*killer)));
                }
                None => parts.push(plain(tr!("logs_tf.event.died"))),
            }
            (tr!("logs_tf.event.medic_death"), parts)
        }
        EventKind::Drop { team, medic } => (
            tr!("logs_tf.event.drop"),
            vec![
                (name_of(*medic), Some(*team)),
                plain(tr!("logs_tf.event.dropped")),
            ],
        ),
        EventKind::RoundWin { team } => (
            tr!("logs_tf.event.round_end"),
            vec![
                plain(tr!("logs_tf.event.round_win")),
                (team_name(*team).to_owned(), Some(*team)),
            ],
        ),
        EventKind::Other { name } => return (name.clone(), Vec::new()),
        // A kind added to core later still reads as an event, never as debug text.
        _ => return (tr!("logs_tf.event.unknown").to_string(), Vec::new()),
    };
    (ty.to_string(), parts)
}
