//! The logs.tf cache: tracked players, the active player and pruning, over the library database.

use std::collections::HashMap;
use std::io::{Read, Write};

use rusqlite::types::ValueRef;
use rusqlite::{OptionalExtension, params};

use super::averages::{CLASS_SUMS, ClassAverage, ClassSums, WINDOW_LOGS};
use super::weapons::{
    BackfillStep, ClassWeapons, MARK_COUNTED_SQL, UNCOUNTED_LEFT_SQL, UNCOUNTED_SQL, WEAPON_LOGS,
    WEAPON_PENDING, WEAPON_SUMS, WeaponBackfill, WeaponSum, write_weapon_rows,
};
use super::{
    LOG_BODY_LIMIT, LogDetail, LogsRange, PROVISIONAL_SECS, SteamId, logstf_class,
    logstf_class_name, parse_log,
};
use crate::db::Database;
use crate::error::LogsCacheError;
use crate::weapons::TfClass;

/// The `settings` key naming the player whose chip is selected.
const ACTIVE_KEY: &str = "logstf_active_player";

/// The `settings` key holding the chosen history range's key.
const RANGE_KEY: &str = "logstf_range";

/// The `settings` key prefix, followed by a SteamID64, for the range a player was last listed in.
const LISTED_RANGE_PREFIX: &str = "logstf_listed_range:";

/// Deletes every log no tracked player is a member of; memberships and class rows cascade.
const PRUNE_SQL: &str = "DELETE FROM logstf_logs WHERE id NOT IN (
        SELECT m.log_id FROM logstf_player_logs m
        JOIN logstf_players p ON p.steam_id = m.steam_id)";

/// `logstf_logs.state` for a log logs.tf answered 404 for.
pub(super) const STATE_GONE: i64 = 1;
/// `logstf_logs.state` for a log whose body could not be read.
pub(super) const STATE_UNREADABLE: i64 = 2;

/// A player on the logs page's chip row.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TrackedPlayer {
    steam_id: SteamId,
    name: String,
    nickname: Option<String>,
    last_synced: Option<i64>,
}

impl TrackedPlayer {
    /// A player that has never been synced.
    pub fn new(steam_id: SteamId, name: impl Into<String>) -> Self {
        TrackedPlayer {
            steam_id,
            name: name.into(),
            nickname: None,
            last_synced: None,
        }
    }

    /// The same player with the nickname the user chose; a blank one counts as none.
    pub fn with_nickname(mut self, nickname: Option<&str>) -> Self {
        self.nickname = clean_nickname(nickname);
        self
    }

    /// The player's Steam ID.
    pub fn steam_id(&self) -> SteamId {
        self.steam_id
    }

    /// The Steam name, or the SteamID64 as text when it could not be looked up.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The nickname the user gave the player, kept apart from the Steam name.
    pub fn nickname(&self) -> Option<&str> {
        self.nickname.as_deref()
    }

    /// The nickname when the player has one, else the Steam name.
    pub fn display_name(&self) -> &str {
        self.nickname.as_deref().unwrap_or(&self.name)
    }

    /// When the player's logs last finished syncing, in unix seconds.
    pub fn last_synced(&self) -> Option<i64> {
        self.last_synced
    }
}

/// Where a cached log stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogState {
    /// Listed but not downloaded yet.
    Pending,
    /// Downloaded and readable.
    Ready,
    /// logs.tf no longer has it.
    Gone,
    /// Its body could not be read as a log.
    Unreadable,
}

/// One log in a player's list, with that player's line from it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LogRow {
    id: u64,
    title: String,
    map: String,
    date: i64,
    length: Option<u32>,
    state: LogState,
    classes: Vec<TfClass>,
    seconds: u32,
    kills: u32,
    deaths: u32,
    assists: u32,
    dmg: u32,
    dt: Option<u32>,
}

impl LogRow {
    /// The logs.tf log id.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The log's title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The map played.
    pub fn map(&self) -> &str {
        &self.map
    }

    /// When the log was uploaded, in unix seconds.
    pub fn date(&self) -> i64 {
        self.date
    }

    /// The match length in seconds, once downloaded.
    pub fn length(&self) -> Option<u32> {
        self.length
    }

    /// Whether the log is downloaded, gone or unreadable.
    pub fn state(&self) -> LogState {
        self.state
    }

    /// The classes the player played, longest first; empty unless `Ready`.
    pub fn classes(&self) -> &[TfClass] {
        &self.classes
    }

    /// The player's time across all classes.
    pub fn seconds(&self) -> u32 {
        self.seconds
    }

    /// The player's kills.
    pub fn kills(&self) -> u32 {
        self.kills
    }

    /// The player's deaths.
    pub fn deaths(&self) -> u32 {
        self.deaths
    }

    /// The player's assists.
    pub fn assists(&self) -> u32 {
        self.assists
    }

    /// The player's damage.
    pub fn dmg(&self) -> u32 {
        self.dmg
    }

    /// The player's damage taken; `None` for a log stored before the cache kept it.
    pub fn dt(&self) -> Option<u32> {
        self.dt
    }
}

/// One entry of a player's logs.tf list, as the sync keeps it.
#[derive(Debug, Clone)]
pub(crate) struct ListedLog {
    pub(crate) id: i64,
    pub(crate) title: String,
    pub(crate) map: String,
    pub(crate) date: i64,
}

/// The logs.tf tables of the library database.
#[derive(Debug, Clone)]
pub struct LogsCache {
    db: Database,
}

impl LogsCache {
    /// A cache over `db`, which must already be migrated.
    pub fn new(db: Database) -> Self {
        LogsCache { db }
    }

    /// Every tracked player, in the order they were added.
    pub fn players(&self) -> Result<Vec<TrackedPlayer>, LogsCacheError> {
        let rows = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT steam_id, name, nickname, last_synced FROM logstf_players
                     ORDER BY position",
                )?;
                stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(db_error)?;
        // Only valid IDs are ever written, so a row outside the range is skipped, not fatal.
        Ok(rows
            .into_iter()
            .filter_map(|(id, name, nickname, last_synced)| {
                Some(TrackedPlayer {
                    steam_id: from_sql_id(id)?,
                    name,
                    nickname,
                    last_synced,
                })
            })
            .collect())
    }

    /// Appends a new player at the end, or updates a tracked one's name in place unless the
    /// new name is the id64 placeholder; either way the player becomes active. A nickname on
    /// `player` is stored; without one, a tracked player keeps theirs.
    pub fn add_player(&self, player: &TrackedPlayer) -> Result<(), LogsCacheError> {
        let placeholder = player.steam_id.to_string();
        self.db
            .write(|conn| {
                let id = sql_id(player.steam_id)?;
                let tx = conn.transaction()?;
                let tracked = tx
                    .query_row(
                        "SELECT 1 FROM logstf_players WHERE steam_id = ?1",
                        [id],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some();
                if !tracked {
                    tx.execute(
                        "INSERT INTO logstf_players (steam_id, name, nickname, position)
                         VALUES (?1, ?2, ?3,
                                 (SELECT COALESCE(MAX(position), -1) + 1 FROM logstf_players))",
                        params![id, player.name, player.nickname],
                    )?;
                } else if player.name != placeholder {
                    tx.execute(
                        "UPDATE logstf_players SET name = ?2 WHERE steam_id = ?1",
                        params![id, player.name],
                    )?;
                }
                if tracked && player.nickname.is_some() {
                    tx.execute(
                        "UPDATE logstf_players SET nickname = ?2 WHERE steam_id = ?1",
                        params![id, player.nickname],
                    )?;
                }
                tx.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                    params![ACTIVE_KEY, placeholder],
                )?;
                tx.commit()
            })
            .map_err(db_error)
    }

    /// Gives a tracked player `nickname`, or clears it when `None` or blank; an untracked
    /// player is left alone.
    pub fn set_nickname(&self, id: SteamId, nickname: Option<&str>) -> Result<(), LogsCacheError> {
        let nickname = clean_nickname(nickname);
        self.db
            .write(|conn| {
                conn.execute(
                    "UPDATE logstf_players SET nickname = ?2 WHERE steam_id = ?1",
                    params![sql_id(id)?, nickname],
                )
            })
            .map(drop)
            .map_err(db_error)
    }

    /// Deletes the player, their memberships, their listed-range setting and then every log
    /// no remaining tracked player is a member of, in one transaction.
    pub fn remove_player(&self, id: SteamId) -> Result<(), LogsCacheError> {
        self.db
            .write(|conn| {
                let listed_key = format!("{LISTED_RANGE_PREFIX}{}", id.id64());
                let id = sql_id(id)?;
                let tx = conn.transaction()?;
                tx.execute("DELETE FROM logstf_player_logs WHERE steam_id = ?1", [id])?;
                tx.execute("DELETE FROM logstf_players WHERE steam_id = ?1", [id])?;
                tx.execute("DELETE FROM settings WHERE key = ?1", [listed_key])?;
                tx.execute(PRUNE_SQL, [])?;
                tx.commit()
            })
            .map_err(db_error)
    }

    /// The `logstf_active_player` setting when it names a tracked player, else the first.
    pub fn active_player(&self) -> Result<Option<SteamId>, LogsCacheError> {
        // Sorting the named player ahead of the rest makes a stale or missing setting fall
        // through to the first chip in one query.
        let id = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT steam_id FROM logstf_players
                     ORDER BY CAST(steam_id AS TEXT) =
                         (SELECT value FROM settings WHERE key = ?1) DESC, position
                     LIMIT 1",
                    [ACTIVE_KEY],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
            })
            .map_err(db_error)?;
        Ok(id.and_then(from_sql_id))
    }

    /// Selects `id`'s chip; the setting is kept even if `id` is not tracked.
    pub fn set_active_player(&self, id: SteamId) -> Result<(), LogsCacheError> {
        self.db
            .write(|conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                    params![ACTIVE_KEY, id.to_string()],
                )
            })
            .map(drop)
            .map_err(db_error)
    }

    /// The chosen history range; a missing or unknown setting reads as the default, three months.
    pub fn range(&self) -> Result<LogsRange, LogsCacheError> {
        let key = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [RANGE_KEY],
                    |row| row.get::<_, String>(0),
                )
                .optional()
            })
            .map_err(db_error)?;
        Ok(key
            .as_deref()
            .and_then(LogsRange::from_key)
            .unwrap_or_default())
    }

    /// Stores `range` as the chosen history range.
    pub fn set_range(&self, range: LogsRange) -> Result<(), LogsCacheError> {
        self.db
            .write(|conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                    params![RANGE_KEY, range.key()],
                )
            })
            .map(drop)
            .map_err(db_error)
    }

    /// The player's logs in `range`'s window ending at their newest log, newest first.
    pub fn logs(&self, id: SteamId, range: LogsRange) -> Result<Vec<LogRow>, LogsCacheError> {
        // One row per (log, class played), longest class first and ties by class name; a log with no class rows
        // comes once with NULL stats. Consecutive rows of one log fold into its LogRow.
        let sql = format!(
            "{WINDOW_LOGS}
             SELECT l.id, l.title, l.map, l.date, l.length, l.state, l.body IS NOT NULL,
                    r.class, r.seconds, r.kills, r.deaths, r.assists, r.dmg, r.dt
             FROM logstf_logs l
             LEFT JOIN logstf_class_rows r ON r.log_id = l.id AND r.steam_id = ?1
             WHERE l.id IN (SELECT id FROM window_logs)
             ORDER BY l.date DESC, l.id DESC, r.seconds DESC, r.class"
        );
        self.db
            .read(|conn| {
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params![sql_id(id)?, range.reach_secs()])?;
                let mut out: Vec<LogRow> = Vec::new();
                while let Some(row) = rows.next()? {
                    let log_id = u64::try_from(row.get::<_, i64>(0)?).unwrap_or(0);
                    if out.last().is_none_or(|last| last.id != log_id) {
                        let state = match (row.get::<_, i64>(5)?, row.get::<_, bool>(6)?) {
                            (STATE_GONE, _) => LogState::Gone,
                            (STATE_UNREADABLE, _) => LogState::Unreadable,
                            (_, true) => LogState::Ready,
                            (_, false) => LogState::Pending,
                        };
                        out.push(LogRow {
                            id: log_id,
                            title: row.get(1)?,
                            map: row.get(2)?,
                            date: row.get(3)?,
                            length: row.get::<_, Option<i64>>(4)?.map(clamp_u32),
                            state,
                            classes: Vec::new(),
                            seconds: 0,
                            kills: 0,
                            deaths: 0,
                            assists: 0,
                            dmg: 0,
                            dt: None,
                        });
                    }
                    let Some(log) = out.last_mut() else { continue };
                    let class = row.get::<_, Option<String>>(7)?;
                    let Some(class) = class.as_deref().and_then(logstf_class) else {
                        continue;
                    };
                    if log.state != LogState::Ready {
                        continue;
                    }
                    let stat = |i| row.get::<_, i64>(i).map(clamp_u32);
                    // Damage taken is the player's whole-log figure, repeated on each class row.
                    if log.classes.is_empty() {
                        log.dt = row.get::<_, Option<i64>>(13)?.map(clamp_u32);
                    }
                    log.classes.push(class);
                    log.seconds = log.seconds.saturating_add(stat(8)?);
                    log.kills = log.kills.saturating_add(stat(9)?);
                    log.deaths = log.deaths.saturating_add(stat(10)?);
                    log.assists = log.assists.saturating_add(stat(11)?);
                    log.dmg = log.dmg.saturating_add(stat(12)?);
                }
                Ok(out)
            })
            .map_err(db_error)
    }

    /// The stored `/json/<id>` body of a downloaded log; `None` for any other. A compressed
    /// body that does not inflate to UTF-8 within `LOG_BODY_LIMIT` is a `Database` error.
    pub fn log_body(&self, log_id: u64) -> Result<Option<String>, LogsCacheError> {
        let Ok(log_id) = i64::try_from(log_id) else {
            return Ok(None);
        };
        match self.read_body(log_id)? {
            BodyRead::Missing => Ok(None),
            BodyRead::Damaged => Err(damaged(log_id)),
            BodyRead::Text(body) => Ok(Some(body)),
        }
    }

    /// Counts weapon rows for up to `limit` stored logs without them, oldest id first.
    ///
    /// A damaged or unparsable body is marked counted with no rows; a database error leaves
    /// the current log unmarked so the next run retries it.
    pub fn count_weapons(&self, limit: usize) -> Result<WeaponBackfill, LogsCacheError> {
        self.count_weapons_with(limit, &|_, _| {})
    }

    /// `count_weapons` with `hook` called before each log's body read and before its write.
    /// Both calls sit outside `db.write`, so a hook may write through the cache itself.
    pub(super) fn count_weapons_with(
        &self,
        limit: usize,
        hook: &dyn Fn(BackfillStep, i64),
    ) -> Result<WeaponBackfill, LogsCacheError> {
        self.backfill(&WEAPON_PASS, limit, hook)
    }

    /// Fills the damage taken on class rows of up to `limit` logs stored before the cache
    /// kept it, oldest id first, from each log's stored body. A row whose player the body
    /// lacks stays unknown; errors behave as in `count_weapons`.
    pub fn fill_damage_taken(&self, limit: usize) -> Result<WeaponBackfill, LogsCacheError> {
        self.backfill(&DAMAGE_TAKEN_PASS, limit, &|_, _| {})
    }

    // One batch of `pass`: each pending log's body is read and parsed outside the write lock,
    // then written and marked done in one transaction guarded by the pass's own selection.
    fn backfill(
        &self,
        pass: &BodyPass,
        limit: usize,
        hook: &dyn Fn(BackfillStep, i64),
    ) -> Result<WeaponBackfill, LogsCacheError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let ids = self
            .db
            .read(|conn| {
                conn.prepare(pass.pending)?
                    .query_map([limit], |row| row.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(db_error)?;
        let mut counted = 0;
        for id in ids {
            hook(BackfillStep::BeforeRead, id);
            // Read and parse outside the write lock; None means "count it with no rows".
            let detail = match self.read_body(id)? {
                BodyRead::Missing => continue,
                BodyRead::Damaged => {
                    tracing::warn!(
                        id,
                        pass = pass.name,
                        "stored logs.tf body is damaged; marking it done without rows"
                    );
                    None
                }
                BodyRead::Text(body) => match parse_log(&body) {
                    Ok(detail) => Some(detail),
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            id,
                            pass = pass.name,
                            "stored logs.tf body does not parse; marking it done without rows"
                        );
                        None
                    }
                },
            };
            hook(BackfillStep::BeforeWrite, id);
            let wrote = self
                .db
                .write(|conn| {
                    let tx = conn.transaction()?;
                    // The guard first: if the log changed since the read, dropping `tx`
                    // unwritten keeps whatever that change left.
                    if tx.execute(pass.mark, [id])? == 0 {
                        return Ok(false);
                    }
                    if let Some(detail) = &detail {
                        (pass.write)(&tx, id, detail)?;
                    }
                    tx.commit()?;
                    Ok(true)
                })
                .map_err(db_error)?;
            counted += usize::from(wrote);
        }
        let left: i64 = self
            .db
            .read(|conn| conn.query_row(pass.left, [], |row| row.get(0)))
            .map_err(db_error)?;
        Ok(WeaponBackfill::new(
            counted,
            usize::try_from(left).unwrap_or(0),
        ))
    }

    /// Log `log_id`'s stored body, telling a damaged one apart from a database error.
    pub(super) fn read_body(&self, log_id: i64) -> Result<BodyRead, LogsCacheError> {
        // Copy the stored value out as is and inflate after the read, so the connection is
        // free while inflating. A BLOB is a compressed body; TEXT is an earlier build's plain one.
        let stored = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT body FROM logstf_logs WHERE id = ?1",
                    [log_id],
                    |row| {
                        Ok(match row.get_ref(0)? {
                            ValueRef::Null => None,
                            ValueRef::Blob(bytes) => Some(Stored::Compressed(bytes.to_vec())),
                            ValueRef::Text(text) => Some(Stored::Plain(text.to_vec())),
                            ValueRef::Integer(_) | ValueRef::Real(_) => Some(Stored::Other),
                        })
                    },
                )
                .optional()
            })
            .map_err(db_error)?
            .flatten();
        Ok(match stored {
            None => BodyRead::Missing,
            Some(Stored::Plain(text)) => {
                String::from_utf8(text).map_or(BodyRead::Damaged, BodyRead::Text)
            }
            Some(Stored::Compressed(bytes)) => {
                inflate(&bytes).map_or(BodyRead::Damaged, BodyRead::Text)
            }
            Some(Stored::Other) => BodyRead::Damaged,
        })
    }

    /// The nine classes in TfClass::ALL order, over exactly the logs `logs` returns.
    pub fn class_averages(
        &self,
        id: SteamId,
        range: LogsRange,
    ) -> Result<[ClassAverage; 9], LogsCacheError> {
        let sql = format!("{WINDOW_LOGS} {CLASS_SUMS}");
        let sums = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare(&sql)?;
                stmt.query_map(params![sql_id(id)?, range.reach_secs()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        ClassSums {
                            seconds: row.get(1)?,
                            logs: row.get(2)?,
                            kills: row.get(3)?,
                            deaths: row.get(4)?,
                            assists: row.get(5)?,
                            dmg: row.get(6)?,
                            heal: row.get(7)?,
                            ubers: row.get(8)?,
                        },
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(db_error)?;
        // A stored name logstf_class rejects is skipped rather than failing the page.
        let by_class: HashMap<TfClass, ClassSums> = sums
            .into_iter()
            .filter_map(|(name, s)| Some((logstf_class(&name)?, s)))
            .collect();
        Ok(TfClass::ALL.map(|class| {
            ClassAverage::from_sums(class, by_class.get(&class).copied().unwrap_or_default())
        }))
    }

    /// The player's weapon sums on `class` over exactly the logs `logs` returns.
    pub fn class_weapons(
        &self,
        id: SteamId,
        range: LogsRange,
        class: TfClass,
    ) -> Result<ClassWeapons, LogsCacheError> {
        self.class_weapons_with(id, range, class, &|| {})
    }

    /// `class_weapons` with `between` called after the sums read and before the counts, so a
    /// test can commit a backfill batch in that gap.
    pub(super) fn class_weapons_with(
        &self,
        id: SteamId,
        range: LogsRange,
        class: TfClass,
        between: &dyn Fn(),
    ) -> Result<ClassWeapons, LogsCacheError> {
        let sums_sql = format!("{WINDOW_LOGS} {WEAPON_SUMS}");
        let logs_sql = format!("{WINDOW_LOGS} {WEAPON_LOGS}");
        let pending_sql = format!("{WINDOW_LOGS} {WEAPON_PENDING}");
        self.db
            .read(|conn| {
                // One read transaction, so a batch committing between the three statements
                // cannot leave `logs` and `pending` describing a different state than the sums.
                let tx = conn.unchecked_transaction()?;
                let args = params![sql_id(id)?, range.reach_secs(), logstf_class_name(class)];
                let weapons = tx
                    .prepare(&sums_sql)?
                    .query_map(args, |row| {
                        Ok(WeaponSum::new(
                            row.get(0)?,
                            [row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?],
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                between();
                let logs: i64 = tx.query_row(&logs_sql, args, |row| row.get(0))?;
                let pending: i64 = tx.query_row(&pending_sql, args, |row| row.get(0))?;
                Ok(ClassWeapons::new(logs, pending, weapons))
            })
            .map_err(db_error)
    }

    /// Deletes every log no tracked player is a member of, whatever its age; returns how
    /// many. Memberships and class rows go with them through the foreign-key cascade.
    pub fn prune(&self) -> Result<usize, LogsCacheError> {
        self.db
            .write(|conn| conn.execute(PRUNE_SQL, []))
            .map_err(db_error)
    }
}

impl LogsCache {
    /// `Some(last_synced)` for a tracked player, `None` for one with no row.
    pub(crate) fn sync_state(&self, id: SteamId) -> Result<Option<Option<i64>>, LogsCacheError> {
        self.db
            .read(|conn| {
                conn.query_row(
                    "SELECT last_synced FROM logstf_players WHERE steam_id = ?1",
                    [sql_id(id)?],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()
            })
            .map_err(db_error)
    }

    /// Records `logs` and `player`'s membership of each in one transaction, then returns the
    /// ids among them that need a download, in the order given.
    ///
    /// A log needs one while it is not gone or unreadable and either has no body, or was
    /// fetched while still provisional (within `PROVISIONAL_SECS` of upload) and has since
    /// settled.
    pub(crate) fn record_list(
        &self,
        player: SteamId,
        logs: &[ListedLog],
        now: i64,
    ) -> Result<Vec<i64>, LogsCacheError> {
        self.db
            .write(|conn| {
                let player = sql_id(player)?;
                let tx = conn.transaction()?;
                let mut wanted = Vec::new();
                {
                    let mut upsert = tx.prepare(
                        "INSERT INTO logstf_logs (id, title, map, date) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT(id) DO UPDATE SET
                             title = excluded.title, map = excluded.map, date = excluded.date",
                    )?;
                    let mut member = tx.prepare(
                        "INSERT OR IGNORE INTO logstf_player_logs (steam_id, log_id) VALUES (?1, ?2)",
                    )?;
                    let mut needs = tx.prepare(NEEDS_DOWNLOAD_SQL)?;
                    for log in logs {
                        upsert.execute(params![log.id, log.title, log.map, log.date])?;
                        member.execute(params![player, log.id])?;
                        if needs.exists(params![log.id, PROVISIONAL_SECS, now])? {
                            wanted.push(log.id);
                        }
                    }
                }
                tx.commit()?;
                Ok(wanted)
            })
            .map_err(db_error)
    }

    /// Whether `log_id` still needs a download by `record_list`'s rule; false once a body was
    /// stored meanwhile or the row is gone.
    pub(crate) fn needs_download(&self, log_id: i64, now: i64) -> Result<bool, LogsCacheError> {
        self.db
            .read(|conn| {
                conn.prepare(NEEDS_DOWNLOAD_SQL)?
                    .exists(params![log_id, PROVISIONAL_SECS, now])
            })
            .map_err(db_error)
    }

    /// Stores a downloaded log: its body and length, a class row per player per class played
    /// (Medic heals, ubers and drops on the medic row only), its weapon rows, and real names for
    /// any player still named by the offline placeholder. Old rows go first, so a refetch
    /// replaces them.
    /// Returns false, storing nothing, when the log's row is gone (pruned or its player removed).
    pub(crate) fn ingest(
        &self,
        log_id: i64,
        body: &str,
        detail: &LogDetail,
        now: i64,
    ) -> Result<bool, LogsCacheError> {
        let stored = deflate(body)?;
        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                let updated = tx.execute(
                    "UPDATE logstf_logs SET body = ?2, fetched_at = ?3, length = ?4, state = 0
                     WHERE id = ?1",
                    params![log_id, stored, now, detail.length()],
                )?;
                // The row vanished between listing and download; its class rows have no log
                // to point at, so there is nothing to store.
                if updated == 0 {
                    return Ok(false);
                }
                tx.execute("DELETE FROM logstf_class_rows WHERE log_id = ?1", [log_id])?;
                {
                    let mut row = tx.prepare(
                        "INSERT INTO logstf_class_rows (log_id, steam_id, class, seconds, kills,
                             deaths, assists, dmg, heal, ubers, drops, dt)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    )?;
                    let mut rename = tx.prepare(
                        "UPDATE logstf_players SET name = ?2
                         WHERE steam_id = ?1 AND name = CAST(steam_id AS TEXT)",
                    )?;
                    let taken = damage_taken(detail);
                    for ((id, class), stats) in fold_class_rows(detail) {
                        let [seconds, kills, deaths, assists, dmg, heal, ubers, drops] = stats;
                        row.execute(params![
                            log_id,
                            sql_id(id)?,
                            logstf_class_name(class),
                            seconds,
                            kills,
                            deaths,
                            assists,
                            dmg,
                            heal,
                            ubers,
                            drops,
                            taken.get(&id).copied()
                        ])?;
                    }
                    for p in detail.players() {
                        rename.execute(params![sql_id(p.steam_id())?, p.name()])?;
                    }
                }
                write_weapon_rows(&tx, log_id, detail)?;
                tx.execute(
                    "UPDATE logstf_logs SET weapons_counted = 1, dt_counted = 1 WHERE id = ?1",
                    [log_id],
                )?;
                tx.commit()?;
                Ok(true)
            })
            .map_err(db_error)
    }

    /// Marks a log `STATE_GONE` or `STATE_UNREADABLE` as of `now`. Any earlier body, class
    /// and weapon rows go too, so a marked log never counts and the needs-download rule skips it.
    pub(crate) fn mark_failed(
        &self,
        log_id: i64,
        state: i64,
        now: i64,
    ) -> Result<(), LogsCacheError> {
        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "UPDATE logstf_logs SET state = ?2, fetched_at = ?3, body = NULL WHERE id = ?1",
                    params![log_id, state, now],
                )?;
                tx.execute("DELETE FROM logstf_class_rows WHERE log_id = ?1", [log_id])?;
                tx.execute("DELETE FROM logstf_weapon_rows WHERE log_id = ?1", [log_id])?;
                tx.commit()
            })
            .map_err(db_error)
    }

    /// Handles a log whose download could not be read. A log that already holds a good body
    /// (a provisional refetch) keeps it and is stamped settled as of `now`; any other is marked
    /// unreadable. Returns true when the log was marked.
    pub(crate) fn mark_unreadable(&self, log_id: i64, now: i64) -> Result<bool, LogsCacheError> {
        let kept = self
            .db
            .write(|conn| {
                conn.execute(
                    "UPDATE logstf_logs SET fetched_at = ?2 WHERE id = ?1 AND body IS NOT NULL",
                    params![log_id, now],
                )
            })
            .map_err(db_error)?;
        if kept > 0 {
            return Ok(false);
        }
        self.mark_failed(log_id, STATE_UNREADABLE, now)?;
        Ok(true)
    }

    /// Whether any of `player`'s logs `logs` would return is still pending.
    pub(crate) fn has_pending(
        &self,
        player: SteamId,
        range: LogsRange,
    ) -> Result<bool, LogsCacheError> {
        let sql = format!(
            "{WINDOW_LOGS}
             SELECT 1 FROM logstf_logs
             WHERE id IN (SELECT id FROM window_logs) AND state = 0 AND body IS NULL"
        );
        self.db
            .read(|conn| {
                conn.prepare(&sql)?
                    .exists(params![sql_id(player)?, range.reach_secs()])
            })
            .map_err(db_error)
    }

    /// The range `player` was last fully listed in. A missing or unknown row reads as
    /// three months, the window every earlier build listed, so an upgrade does not relist.
    pub(crate) fn listed_range(&self, player: SteamId) -> Result<LogsRange, LogsCacheError> {
        let key = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [format!("{LISTED_RANGE_PREFIX}{}", player.id64())],
                    |row| row.get::<_, String>(0),
                )
                .optional()
            })
            .map_err(db_error)?;
        Ok(key
            .as_deref()
            .and_then(LogsRange::from_key)
            .unwrap_or(LogsRange::Months3))
    }

    /// Records that `player`'s list was read through to the end of `range`.
    pub(crate) fn set_listed_range(
        &self,
        player: SteamId,
        range: LogsRange,
    ) -> Result<(), LogsCacheError> {
        self.db
            .write(|conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                    params![
                        format!("{LISTED_RANGE_PREFIX}{}", player.id64()),
                        range.key()
                    ],
                )
            })
            .map(drop)
            .map_err(db_error)
    }

    /// Stamps `player` as synced at `now`.
    pub(crate) fn mark_synced(&self, player: SteamId, now: i64) -> Result<(), LogsCacheError> {
        self.db
            .write(|conn| {
                conn.execute(
                    "UPDATE logstf_players SET last_synced = ?2 WHERE steam_id = ?1",
                    params![sql_id(player)?, now],
                )
            })
            .map(drop)
            .map_err(db_error)
    }
}

/// Matches log ?1 while it needs a download: not gone or unreadable, and without a body or
/// fetched while provisional (within ?2 seconds of upload) and settled by ?3.
const NEEDS_DOWNLOAD_SQL: &str = "SELECT 1 FROM logstf_logs WHERE id = ?1 AND state = 0
    AND (body IS NULL OR (fetched_at - date < ?2 AND ?3 - date >= ?2))";

/// One row's stats: seconds, kills, deaths, assists, dmg, heal, ubers, drops.
type ClassStats = [i64; 8];

/// The log's class rows keyed by (player, class), summing repeats: a log may list a class
/// twice for one player, or name one player under two keys. A player's Medic totals count
/// once per player line, on the medic row only.
fn fold_class_rows(detail: &LogDetail) -> HashMap<(SteamId, TfClass), ClassStats> {
    let mut rows: HashMap<(SteamId, TfClass), ClassStats> = HashMap::new();
    for p in detail.players() {
        for c in p.classes() {
            let row = rows.entry((p.steam_id(), c.class())).or_default();
            let add = [c.seconds(), c.kills(), c.deaths(), c.assists(), c.dmg()];
            for (sum, n) in row.iter_mut().zip(add) {
                *sum += i64::from(n);
            }
        }
        if let Some(row) = rows.get_mut(&(p.steam_id(), TfClass::Medic))
            && p.classes().iter().any(|c| c.class() == TfClass::Medic)
        {
            for (sum, n) in row[5..].iter_mut().zip([p.heal(), p.ubers(), p.drops()]) {
                *sum += i64::from(n);
            }
        }
    }
    rows
}

/// A backfill over stored bodies: the logs it waits on (oldest ?1 of them), how many are
/// left, the guarded mark of log ?1 as done, and what it writes from a parsed body.
struct BodyPass {
    name: &'static str,
    pending: &'static str,
    left: &'static str,
    mark: &'static str,
    write: fn(&rusqlite::Transaction<'_>, i64, &LogDetail) -> rusqlite::Result<()>,
}

const WEAPON_PASS: BodyPass = BodyPass {
    name: "weapons",
    pending: UNCOUNTED_SQL,
    left: UNCOUNTED_LEFT_SQL,
    mark: MARK_COUNTED_SQL,
    write: write_weapon_rows,
};

/// The selection the damage-taken pass works through, shared with its guard.
macro_rules! dt_pending {
    () => {
        "body IS NOT NULL AND state = 0 AND dt_counted = 0"
    };
}

const DAMAGE_TAKEN_PASS: BodyPass = BodyPass {
    name: "damage taken",
    pending: concat!(
        "SELECT id FROM logstf_logs WHERE ",
        dt_pending!(),
        " ORDER BY id LIMIT ?1"
    ),
    left: concat!("SELECT COUNT(*) FROM logstf_logs WHERE ", dt_pending!()),
    mark: concat!(
        "UPDATE logstf_logs SET dt_counted = 1 WHERE id = ?1 AND ",
        dt_pending!()
    ),
    write: write_damage_taken,
};

// Sets each player's damage taken on their class rows of log `log_id` that have none yet.
fn write_damage_taken(
    tx: &rusqlite::Transaction<'_>,
    log_id: i64,
    detail: &LogDetail,
) -> rusqlite::Result<()> {
    let mut set = tx.prepare(
        "UPDATE logstf_class_rows SET dt = ?3 WHERE log_id = ?1 AND steam_id = ?2 AND dt IS NULL",
    )?;
    for (id, dt) in damage_taken(detail) {
        set.execute(params![log_id, sql_id(id)?, dt])?;
    }
    Ok(())
}

/// Each player's damage taken over the log, summing a player named under two keys; empty
/// for a log that does not record it, so its rows stay unknown rather than zero.
fn damage_taken(detail: &LogDetail) -> HashMap<SteamId, i64> {
    let mut taken: HashMap<SteamId, i64> = HashMap::new();
    if !detail.columns().dt() {
        return taken;
    }
    for p in detail.players() {
        *taken.entry(p.steam_id()).or_default() += i64::from(p.dt());
    }
    taken
}

/// `id` as SQLite stores it; every public-universe SteamID64 fits in an i64.
pub(super) fn sql_id(id: SteamId) -> rusqlite::Result<i64> {
    i64::try_from(id.id64()).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

// A nickname trimmed, with a blank one meaning none.
fn clean_nickname(nickname: Option<&str>) -> Option<String> {
    nickname
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
}

/// A stored count as u32, clamped into range.
fn clamp_u32(n: i64) -> u32 {
    u32::try_from(n.max(0)).unwrap_or(u32::MAX)
}

fn from_sql_id(id: i64) -> Option<SteamId> {
    SteamId::from_id64(u64::try_from(id).ok()?)
}

/// A `logstf_logs.body` value as SQLite stored it.
enum Stored {
    Compressed(Vec<u8>),
    Plain(Vec<u8>),
    /// A number, which no build ever stored as a body.
    Other,
}

/// A stored body as `read_body` found it.
pub(super) enum BodyRead {
    /// No such log, or no body stored.
    Missing,
    /// A body that does not inflate or decode to text.
    Damaged,
    /// The body as inflated, decoded text.
    Text(String),
}

/// A body compressed for storage: zlib at level 6.
fn deflate(body: &str) -> Result<Vec<u8>, LogsCacheError> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
    encoder
        .write_all(body.as_bytes())
        .and_then(|()| encoder.finish())
        .map_err(|e| LogsCacheError::Database {
            message: format!("a log body could not be compressed: {e}"),
        })
}

/// A stored compressed body, or `None` when it is not zlib, not UTF-8, or inflates past
/// `LOG_BODY_LIMIT` (no log the client accepted could be that large).
fn inflate(bytes: &[u8]) -> Option<String> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(bytes)
        .take(LOG_BODY_LIMIT + 1)
        .read_to_end(&mut out)
        .ok()?;
    if out.len() as u64 > LOG_BODY_LIMIT {
        return None;
    }
    String::from_utf8(out).ok()
}

fn damaged(log_id: i64) -> LogsCacheError {
    LogsCacheError::Database {
        message: format!("the stored body of log {log_id} is damaged"),
    }
}

fn db_error(e: rusqlite::Error) -> LogsCacheError {
    LogsCacheError::Database {
        message: e.to_string(),
    }
}
