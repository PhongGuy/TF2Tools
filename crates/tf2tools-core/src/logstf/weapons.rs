//! Per-weapon stats for logs.tf: one row per player, class and weapon of each stored log,
//! written inside `ingest`'s transaction so the rows always match the stored body.

use std::collections::HashMap;

use rusqlite::{Transaction, params};

use super::cache::sql_id;
use super::{LogDetail, SteamId, logstf_class_name};
use crate::weapons::TfClass;

/// Replaces log `log_id`'s weapon rows with one per player, class and weapon in `detail`,
/// summing a class or weapon a player is listed with twice.
pub(super) fn write_weapon_rows(
    tx: &Transaction<'_>,
    log_id: i64,
    detail: &LogDetail,
) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM logstf_weapon_rows WHERE log_id = ?1", [log_id])?;
    let mut row = tx.prepare(
        "INSERT INTO logstf_weapon_rows (log_id, steam_id, class, weapon, kills, dmg, shots, hits)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for ((id, class, weapon), [kills, dmg, shots, hits]) in fold_weapon_rows(detail) {
        row.execute(params![
            log_id,
            sql_id(id)?,
            logstf_class_name(class),
            weapon,
            kills,
            dmg,
            shots,
            hits
        ])?;
    }
    Ok(())
}

/// One weapon row's stats: kills, dmg, shots, hits.
type WeaponStats = [i64; 4];

/// The log's weapon lines keyed by (player, class, weapon), summing repeats so the primary
/// key holds when a log lists a class twice for one player, as `fold_class_rows` does.
fn fold_weapon_rows(detail: &LogDetail) -> HashMap<(SteamId, TfClass, String), WeaponStats> {
    let mut rows: HashMap<(SteamId, TfClass, String), WeaponStats> = HashMap::new();
    for p in detail.players() {
        for c in p.classes() {
            for w in c.weapons() {
                let row = rows
                    .entry((p.steam_id(), c.class(), w.name().to_owned()))
                    .or_default();
                for (sum, n) in row
                    .iter_mut()
                    .zip([w.kills(), w.dmg(), w.shots(), w.hits()])
                {
                    *sum += i64::from(n);
                }
            }
        }
    }
    rows
}

#[cfg(test)]
#[path = "weapons_tests.rs"]
mod tests;
