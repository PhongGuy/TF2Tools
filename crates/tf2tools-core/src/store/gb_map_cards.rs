//! Paged reads of the synced GameBanana maps catalogue: one sort, optional gamemode and
//! search filters, and a keyset cursor that continues after the last card.

use rusqlite::types::Value;

use super::StoreClient;
use super::addon_catalog::AddonCard;
use super::cards::{database, like_pattern, where_clause};
use crate::GbId;
use crate::error::QueryError;

/// The order the catalogue is read in; ties fall back to the newest GameBanana id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GbMapSort {
    /// GameBanana's most-downloaded order, top first.
    MostDownloaded,
    /// Most recently added first.
    Newest,
}

/// Where the next page starts; valid only for the query of the page it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GbMapCursor {
    group: i64,
    key: i64,
    gb_id: i64,
}

/// One page request over the maps catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GbMapQuery {
    /// The order of the cards.
    pub sort: GbMapSort,
    /// The gamemode (a first-level category id) to keep; `None` shows every map.
    pub gamemode: Option<u32>,
    /// Text matched literally and ASCII case-insensitively against name or author; blank
    /// means no filter.
    pub search: Option<String>,
    /// Show maps GameBanana rates mature.
    pub mature: bool,
    /// Continue after this card; `None` reads the first page.
    pub after: Option<GbMapCursor>,
    /// The most cards to return.
    pub limit: u32,
}

impl GbMapQuery {
    /// The first page of `limit` cards in `sort` order, unfiltered, mature maps hidden.
    pub fn new(sort: GbMapSort, limit: u32) -> Self {
        Self {
            sort,
            gamemode: None,
            search: None,
            mature: false,
            after: None,
            limit,
        }
    }

    /// Keep only maps of `gamemode`, or every map with `None`.
    pub fn with_gamemode(mut self, gamemode: Option<u32>) -> Self {
        self.gamemode = gamemode;
        self
    }

    /// Keep only maps whose name or author contains `text`.
    pub fn with_search(mut self, text: impl Into<String>) -> Self {
        self.search = Some(text.into());
        self
    }

    /// Show or hide mature maps.
    pub fn with_mature(mut self, mature: bool) -> Self {
        self.mature = mature;
        self
    }

    /// Continue after `cursor`, taken from a card of the same query.
    pub fn with_after(mut self, cursor: GbMapCursor) -> Self {
        self.after = Some(cursor);
        self
    }
}

/// What the library holds of a map.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GbMapLibrary {
    /// Its files are in the game.
    pub installed: bool,
    /// GameBanana has a newer file than the installed one.
    pub has_update: bool,
    /// GameBanana no longer serves it.
    pub gone: bool,
}

/// One grid card: the catalogue's columns plus the library's row, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GbMapCard {
    /// The map as the add-on card draws it.
    pub card: AddonCard,
    /// The library's row for this map; `None` when it was never downloaded.
    pub library: Option<GbMapLibrary>,
    /// The synced catalogue lists this map.
    pub in_catalogue: bool,
    /// Pass to `GbMapQuery::with_after` to read the page after this card.
    pub cursor: GbMapCursor,
}

impl StoreClient {
    /// One page of the synced maps catalogue, read through the reader connection. Blocking.
    pub fn gb_map_cards(&self, q: &GbMapQuery) -> Result<Vec<GbMapCard>, QueryError> {
        let (sql, params) = catalogue_sql(q);
        let sort = q.sort;
        self.db
            .read(|conn| {
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(&params), |r| {
                    let raw_id: i64 = r.get(0)?;
                    let Ok(id) = u64::try_from(raw_id) else {
                        return Ok(None);
                    };
                    let date_added: i64 = r.get(5)?;
                    let profile_url: String = r.get(10)?;
                    let key = match sort {
                        GbMapSort::MostDownloaded => r.get(11)?,
                        GbMapSort::Newest => date_added,
                    };
                    let library = match r.get::<_, Option<bool>>(12)? {
                        Some(installed) => Some(GbMapLibrary {
                            installed,
                            has_update: r.get(13)?,
                            gone: r.get(14)?,
                        }),
                        None => None,
                    };
                    Ok(Some(GbMapCard {
                        card: AddonCard {
                            id: GbId::new(id),
                            name: r.get(1)?,
                            author: r.get(2)?,
                            thumbnail_url: r.get(3)?,
                            like_count: r.get(4)?,
                            date_added,
                            category_id: r
                                .get::<_, Option<i64>>(6)?
                                .and_then(|c| u32::try_from(c).ok()),
                            category_name: r.get(7)?,
                            obsolete: r.get(8)?,
                            mature: r.get(9)?,
                            profile_url: if profile_url.is_empty() {
                                format!("https://gamebanana.com/mods/{id}")
                            } else {
                                profile_url
                            },
                        },
                        library,
                        in_catalogue: true,
                        cursor: GbMapCursor {
                            group: 0,
                            key,
                            gb_id: raw_id,
                        },
                    }))
                })?;
                rows.filter_map(Result::transpose).collect()
            })
            .map_err(database)
    }
}

/// The catalogue page query and its parameters. Browsing walks one of migration 20's
/// `(gamemode?, key, gb_id DESC)` indexes; a search scans the table once instead.
fn catalogue_sql(q: &GbMapQuery) -> (String, Vec<Value>) {
    let mut filters = Vec::new();
    let mut params = Vec::new();
    if let Some(gamemode) = q.gamemode {
        filters.push("m.gamemode_id = ?".to_owned());
        params.push(Value::Integer(i64::from(gamemode)));
    }
    if !q.mature {
        filters.push("m.mature = 0".to_owned());
    }
    let search = q.search.as_deref().filter(|t| !t.trim().is_empty());
    if let Some(text) = search {
        let pattern = like_pattern(text);
        filters.push("(m.name LIKE ? ESCAPE '\\' OR m.author LIKE ? ESCAPE '\\')".to_owned());
        params.push(Value::Text(pattern.clone()));
        params.push(Value::Text(pattern));
    }
    // Rank ascends while the id tiebreak descends, so its keyset cannot be one row-value
    // comparison; the split form still seeks the rank index.
    let order = match q.sort {
        GbMapSort::MostDownloaded => {
            if let Some(after) = &q.after {
                filters.push(
                    "m.download_rank >= ? AND (m.download_rank > ? OR m.gb_id < ?)".to_owned(),
                );
                params.push(Value::Integer(after.key));
                params.push(Value::Integer(after.key));
                params.push(Value::Integer(after.gb_id));
            }
            "m.download_rank, m.gb_id DESC"
        }
        GbMapSort::Newest => {
            if let Some(after) = &q.after {
                filters.push("(m.date_added, m.gb_id) < (?, ?)".to_owned());
                params.push(Value::Integer(after.key));
                params.push(Value::Integer(after.gb_id));
            }
            "m.date_added DESC, m.gb_id DESC"
        }
    };
    let clause = where_clause(&filters);
    let scan = if search.is_some() { " NOT INDEXED" } else { "" };
    params.push(Value::Integer(i64::from(q.limit)));
    let sql = format!(
        "SELECT m.gb_id, m.name, m.author, m.thumbnail_url, m.like_count, m.date_added,
                m.category_id, m.category_name, m.obsolete, m.mature, m.profile_url,
                m.download_rank, l.installed, l.latest_file_date > l.file_date, l.gone
         FROM store_gb_maps m{scan}
         LEFT JOIN library_gb_addons l ON l.gb_id = m.gb_id AND l.section = 'maps'{clause}
         ORDER BY {order} LIMIT ?"
    );
    (sql, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::paths::DataDir;
    use rusqlite::types::Value;

    fn plan(db: &Database, sql: &str, params: &[Value]) -> String {
        db.read(|c| {
            let mut stmt = c.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| {
                r.get::<_, String>(3)
            })?;
            rows.collect::<rusqlite::Result<Vec<String>>>()
        })
        .unwrap()
        .join(" | ")
    }

    fn seed(db: &Database) {
        db.write(|c| {
            c.execute_batch(
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 300)
                 INSERT INTO store_gb_maps (gb_id, name, date_added, gamemode_id, download_rank, mature, fetched_at)
                 SELECT i, 'map ' || i, i * 7, CASE WHEN i % 5 = 0 THEN NULL ELSE 5370 + i % 4 END,
                        (i * 37) % 300 + 1, i % 50 = 0, 'x' FROM n;
                 ANALYZE;",
            )
        })
        .unwrap();
    }

    fn queries(search: Option<&str>) -> Vec<GbMapQuery> {
        let cursor = GbMapCursor {
            group: 0,
            key: 5,
            gb_id: 9,
        };
        let mut out = Vec::new();
        for sort in [GbMapSort::MostDownloaded, GbMapSort::Newest] {
            for gamemode in [None, Some(5373)] {
                for after in [None, Some(cursor.clone())] {
                    let mut q = GbMapQuery::new(sort, 60).with_gamemode(gamemode);
                    q.after = after;
                    q.search = search.map(str::to_owned);
                    out.push(q);
                }
            }
        }
        out
    }

    // Browsing walks an index with no temp sort, empty or synced; a search scans once.
    #[test]
    fn browsing_pages_through_an_index_and_a_search_scans_the_catalogue_once() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&DataDir::at(tmp.path().join("data"))).unwrap();
        for state in ["empty", "seeded"] {
            if state == "seeded" {
                seed(&db);
            }
            for q in queries(None) {
                let (sql, params) = catalogue_sql(&q);
                let plan = plan(&db, &sql, &params);
                assert!(
                    plan.contains("USING") && !plan.contains("TEMP B-TREE"),
                    "{state} {q:?}: {plan}"
                );
            }
        }
        for q in queries(Some("zq")) {
            let (sql, params) = catalogue_sql(&q);
            let plan = plan(&db, &sql, &params);
            assert!(
                plan.contains("SCAN m")
                    && !plan.contains("SCAN m USING")
                    && !plan.contains("SEARCH m"),
                "{q:?}: {plan}"
            );
        }
    }
}
