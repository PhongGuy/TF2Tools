//! The logs.tf cache: tracked players, the active player and pruning, over the library database.

use rusqlite::{OptionalExtension, params};

use super::SteamId;
use crate::db::Database;
use crate::error::LogsCacheError;

/// The `settings` key naming the player whose chip is selected.
const ACTIVE_KEY: &str = "logstf_active_player";

/// A player on the logs page's chip row.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TrackedPlayer {
    steam_id: SteamId,
    name: String,
    last_synced: Option<i64>,
}

impl TrackedPlayer {
    /// A player that has never been synced.
    pub fn new(steam_id: SteamId, name: impl Into<String>) -> Self {
        TrackedPlayer {
            steam_id,
            name: name.into(),
            last_synced: None,
        }
    }

    /// The player's Steam ID.
    pub fn steam_id(&self) -> SteamId {
        self.steam_id
    }

    /// The Steam name, or the SteamID64 as text when it could not be looked up.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// When the player's logs last finished syncing, in unix seconds.
    pub fn last_synced(&self) -> Option<i64> {
        self.last_synced
    }
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
                    "SELECT steam_id, name, last_synced FROM logstf_players ORDER BY position",
                )?;
                stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(db_error)?;
        // Only valid IDs are ever written, so a row outside the range is skipped, not fatal.
        Ok(rows
            .into_iter()
            .filter_map(|(id, name, last_synced)| {
                Some(TrackedPlayer {
                    steam_id: from_sql_id(id)?,
                    name,
                    last_synced,
                })
            })
            .collect())
    }

    /// Appends a new player at the end, or updates a tracked one's name in place unless the
    /// new name is the id64 placeholder; either way the player becomes active.
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
                        "INSERT INTO logstf_players (steam_id, name, position)
                         VALUES (?1, ?2, (SELECT COALESCE(MAX(position), -1) + 1 FROM logstf_players))",
                        params![id, player.name],
                    )?;
                } else if player.name != placeholder {
                    tx.execute(
                        "UPDATE logstf_players SET name = ?2 WHERE steam_id = ?1",
                        params![id, player.name],
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

    /// Deletes the player row and their memberships; their logs stay until pruned.
    pub fn remove_player(&self, id: SteamId) -> Result<(), LogsCacheError> {
        self.db
            .write(|conn| {
                let id = sql_id(id)?;
                let tx = conn.transaction()?;
                tx.execute("DELETE FROM logstf_player_logs WHERE steam_id = ?1", [id])?;
                tx.execute("DELETE FROM logstf_players WHERE steam_id = ?1", [id])?;
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

    /// Deletes logs uploaded before `before` (unix seconds); returns how many.
    /// Memberships and class rows go with them through the foreign-key cascade.
    pub fn prune(&self, before: i64) -> Result<usize, LogsCacheError> {
        self.db
            .write(|conn| conn.execute("DELETE FROM logstf_logs WHERE date < ?1", [before]))
            .map_err(db_error)
    }
}

/// `id` as SQLite stores it; every public-universe SteamID64 fits in an i64.
fn sql_id(id: SteamId) -> rusqlite::Result<i64> {
    i64::try_from(id.id64()).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn from_sql_id(id: i64) -> Option<SteamId> {
    SteamId::from_id64(u64::try_from(id).ok()?)
}

fn db_error(e: rusqlite::Error) -> LogsCacheError {
    LogsCacheError::Database {
        message: e.to_string(),
    }
}
