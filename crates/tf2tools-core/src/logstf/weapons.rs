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

/// Player ?1's summed weapon rows for class ?3 over `WINDOW_LOGS`, in display order.
pub(super) const WEAPON_SUMS: &str = "SELECT weapon, SUM(kills), SUM(dmg), SUM(shots), SUM(hits)
    FROM logstf_weapon_rows
    WHERE steam_id = ?1 AND class = ?3 AND log_id IN (SELECT id FROM window_logs)
    GROUP BY weapon
    ORDER BY SUM(dmg) DESC, SUM(kills) DESC, weapon";

/// How many window logs hold weapon rows for player ?1 on class ?3.
pub(super) const WEAPON_LOGS: &str = "SELECT COUNT(DISTINCT log_id) FROM logstf_weapon_rows
    WHERE steam_id = ?1 AND class = ?3 AND log_id IN (SELECT id FROM window_logs)";

/// Window logs where player ?1 played class ?3 but the weapon rows are not yet counted.
pub(super) const WEAPON_PENDING: &str = "SELECT COUNT(*) FROM logstf_logs l
    WHERE l.weapons_counted = 0 AND l.id IN (SELECT id FROM window_logs)
      AND EXISTS (SELECT 1 FROM logstf_class_rows r
                  WHERE r.log_id = l.id AND r.steam_id = ?1 AND r.class = ?3)";

/// The selection `count_weapons` works through, shared by its guard so the two cannot drift.
macro_rules! uncounted {
    () => {
        "body IS NOT NULL AND state = 0 AND weapons_counted = 0"
    };
}

/// Up to ?1 uncounted log ids, oldest first.
pub(super) const UNCOUNTED_SQL: &str = concat!(
    "SELECT id FROM logstf_logs WHERE ",
    uncounted!(),
    " ORDER BY id LIMIT ?1"
);

/// How many logs are still uncounted.
pub(super) const UNCOUNTED_LEFT_SQL: &str =
    concat!("SELECT COUNT(*) FROM logstf_logs WHERE ", uncounted!());

/// Marks log ?1 counted only while it is still the uncounted log that was read; zero rows
/// changed means it was pruned, marked failed or re-ingested in between.
pub(super) const MARK_COUNTED_SQL: &str = concat!(
    "UPDATE logstf_logs SET weapons_counted = 1 WHERE id = ?1 AND ",
    uncounted!()
);

/// What one `count_weapons` batch did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct WeaponBackfill {
    counted: usize,
    left: usize,
}

impl WeaponBackfill {
    pub(super) fn new(counted: usize, left: usize) -> Self {
        WeaponBackfill { counted, left }
    }

    /// Logs this batch marked counted, damaged ones included.
    pub fn counted(&self) -> usize {
        self.counted
    }

    /// Logs still waiting to be counted after the batch.
    pub fn left(&self) -> usize {
        self.left
    }
}

/// One weapon's stats summed over a player's window on one class.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct WeaponSum {
    name: String,
    kills: i64,
    dmg: i64,
    shots: i64,
    hits: i64,
}

impl WeaponSum {
    pub(super) fn new(name: String, [kills, dmg, shots, hits]: WeaponStats) -> Self {
        WeaponSum {
            name,
            kills,
            dmg,
            shots,
            hits,
        }
    }

    /// The logs.tf weapon name, such as `scattergun`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Kills with the weapon.
    pub fn kills(&self) -> i64 {
        self.kills
    }

    /// Damage dealt with the weapon.
    pub fn dmg(&self) -> i64 {
        self.dmg
    }

    /// Shots fired; 0 for weapons logs.tf does not count shots for.
    pub fn shots(&self) -> i64 {
        self.shots
    }

    /// Shots that hit.
    pub fn hits(&self) -> i64 {
        self.hits
    }

    /// hits / shots; `None` when shots is 0.
    pub fn accuracy(&self) -> Option<f64> {
        (self.shots > 0).then(|| self.hits as f64 / self.shots as f64)
    }
}

/// A player's weapons on one class over the window, with how many logs back them.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ClassWeapons {
    logs: u32,
    pending: u32,
    weapons: Vec<WeaponSum>,
}

impl ClassWeapons {
    pub(super) fn new(logs: i64, pending: i64, weapons: Vec<WeaponSum>) -> Self {
        ClassWeapons {
            logs: u32::try_from(logs).unwrap_or(0),
            pending: u32::try_from(pending).unwrap_or(0),
            weapons,
        }
    }

    /// Distinct logs in the window with weapon rows for the class.
    pub fn logs(&self) -> u32 {
        self.logs
    }

    /// Logs in the window with class rows for the class whose weapons are not yet counted.
    pub fn pending(&self) -> u32 {
        self.pending
    }

    /// Damage high to low, then kills high to low, then name.
    pub fn weapons(&self) -> &[WeaponSum] {
        &self.weapons
    }
}

#[cfg(test)]
#[path = "weapons_tests.rs"]
mod tests;
