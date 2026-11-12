//! The daily GameBanana check for HUD items, kept per item in `gb_hud_checks` so several
//! library HUDs of one item share a single request.

use super::gb_addons::{CHECK_INTERVAL, gb_id_column};
use super::{GbAddonCheck, GbId, Library, library_db};
use crate::error::LibraryError;

fn column(gb_id: GbId) -> Result<i64, LibraryError> {
    gb_id_column(gb_id, |message| LibraryError::Database { message })
}

impl Library {
    /// Adds a check row for each of `ids` not yet tracked; tracked items keep their check.
    pub fn ensure_gb_hud_checks(&self, ids: &[GbId]) -> Result<(), LibraryError> {
        let columns = ids
            .iter()
            .map(|&id| column(id))
            .collect::<Result<Vec<_>, _>>()?;
        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                {
                    let mut stmt =
                        tx.prepare("INSERT OR IGNORE INTO gb_hud_checks (gb_id) VALUES (?1)")?;
                    for id in &columns {
                        stmt.execute([id])?;
                    }
                }
                tx.commit()
            })
            .map_err(library_db)
    }

    /// HUD items never checked, then those checked more than a day before `now` or after
    /// it (the clock went back), oldest check first.
    pub fn gb_huds_due_for_check(&self, now: i64) -> Result<Vec<GbId>, LibraryError> {
        let ids = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT gb_id FROM gb_hud_checks
                     WHERE checked_at IS NULL OR checked_at < ?1 OR checked_at > ?2
                     ORDER BY checked_at ASC, gb_id ASC",
                )?;
                let stale = now.saturating_sub(CHECK_INTERVAL);
                let rows = stmt.query_map([stale, now], |row| row.get::<_, i64>(0))?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .map_err(library_db)?;
        Ok(ids
            .into_iter()
            .filter_map(|id| u64::try_from(id).ok().map(GbId::new))
            .collect())
    }

    /// Stores `check` and `checked_at = now` for `gb_id`; `Latest` sets `latest_file_date`
    /// and clears `gone`, `Gone` sets `gone`, and `Unchanged` moves only `checked_at`.
    pub fn record_gb_hud_check(
        &self,
        gb_id: GbId,
        check: GbAddonCheck,
        now: i64,
    ) -> Result<(), LibraryError> {
        let id = column(gb_id)?;
        self.db
            .write(|conn| match check {
                GbAddonCheck::Latest { file_date } => conn.execute(
                    "UPDATE gb_hud_checks SET checked_at = ?1, latest_file_date = ?2, gone = 0
                     WHERE gb_id = ?3",
                    rusqlite::params![now, file_date, id],
                ),
                GbAddonCheck::Unchanged => conn.execute(
                    "UPDATE gb_hud_checks SET checked_at = ?1 WHERE gb_id = ?2",
                    rusqlite::params![now, id],
                ),
                GbAddonCheck::Gone => conn.execute(
                    "UPDATE gb_hud_checks SET checked_at = ?1, gone = 1 WHERE gb_id = ?2",
                    rusqlite::params![now, id],
                ),
            })
            .map(drop)
            .map_err(library_db)
    }
}
