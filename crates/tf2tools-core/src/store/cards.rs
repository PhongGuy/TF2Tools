//! Paged reads of the GameBanana stores for the virtualized grids: one sort, optional
//! search and category filters, and a keyset cursor that continues after the last card.

use rusqlite::types::Value;

use super::items::{gb_id, string_list};
use super::{StoreClient, StoreItemId, StoreKind};
use crate::error::QueryError;

/// The order a store page is read in; ties fall back to the newest GameBanana id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StoreSort {
    /// Most likes first.
    MostLiked,
    /// Most views first.
    MostViewed,
    /// Most recently added first.
    Newest,
}

impl StoreSort {
    fn column(self) -> &'static str {
        match self {
            StoreSort::MostLiked => "like_count",
            StoreSort::MostViewed => "view_count",
            StoreSort::Newest => "date_added",
        }
    }
}

/// Where the next page starts; valid only for the sort of the page it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StoreCursor {
    key: i64,
    gb_id: i64,
}

/// One page request: a sort, optional filters, an optional cursor and a page size.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StoreQuery {
    /// The order of the cards.
    pub sort: StoreSort,
    /// Text matched literally and ASCII case-insensitively against name or author;
    /// blank means no filter.
    pub search: Option<String>,
    /// The sound category to keep; ignored by the other stores.
    pub category: Option<String>,
    /// Continue after this card; `None` reads the first page.
    pub after: Option<StoreCursor>,
    /// The most cards to return.
    pub limit: u32,
}

impl StoreQuery {
    /// The first page of `limit` cards in `sort` order, unfiltered.
    pub fn new(sort: StoreSort, limit: u32) -> Self {
        Self {
            sort,
            search: None,
            category: None,
            after: None,
            limit,
        }
    }

    /// Keep only cards whose name or author contains `text`.
    pub fn with_search(mut self, text: impl Into<String>) -> Self {
        self.search = Some(text.into());
        self
    }

    /// Keep only sounds in `category`.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Continue after `cursor`, taken from a card of the same sort.
    pub fn with_after(mut self, cursor: StoreCursor) -> Self {
        self.after = Some(cursor);
        self
    }
}

/// One grid card: the columns a store grid shows plus whether the library holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StoreCard {
    /// The GameBanana id.
    pub id: StoreItemId,
    /// The item's name.
    pub name: String,
    /// The submitter's name.
    pub author: String,
    /// The card image, if GameBanana has one.
    pub thumbnail_url: Option<String>,
    /// GameBanana's tags; empty when the cached list does not parse.
    pub tags: Vec<String>,
    /// Likes on GameBanana.
    pub like_count: i64,
    /// Views on GameBanana.
    pub view_count: i64,
    /// When it was added, in Unix seconds.
    pub date_added: i64,
    /// True when a library entry was downloaded from this item.
    pub in_library: bool,
    /// Sounds only: "hitsound", "killsound" or "sound"; None when empty or for other kinds.
    pub category: Option<String>,
    /// Sounds only: true when the row has a non-empty preview URL.
    pub has_preview: bool,
    /// Pass to `StoreQuery::with_after` to read the page after this card.
    pub cursor: StoreCursor,
}

impl StoreClient {
    /// One page of `kind`'s cached cards, read through the reader connection. Curated HUDs
    /// and Addons are NotPaged.
    pub fn store_cards(
        &self,
        kind: StoreKind,
        q: &StoreQuery,
    ) -> Result<Vec<StoreCard>, QueryError> {
        let (sql, params) = cards_sql(kind, q, false).ok_or(QueryError::NotPaged { kind })?;
        let sort = q.sort;
        self.db
            .read(|conn| {
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(&params), |r| {
                    let Some(id) = gb_id(r, 0)? else {
                        return Ok(None);
                    };
                    let (like_count, view_count, date_added): (i64, i64, i64) =
                        (r.get(5)?, r.get(6)?, r.get(7)?);
                    let sort_key = match sort {
                        StoreSort::MostLiked => like_count,
                        StoreSort::MostViewed => view_count,
                        StoreSort::Newest => date_added,
                    };
                    Ok(Some(StoreCard {
                        id,
                        name: r.get(1)?,
                        author: r.get(2)?,
                        thumbnail_url: r.get(3)?,
                        tags: string_list(&r.get::<_, String>(4)?),
                        like_count,
                        view_count,
                        date_added,
                        in_library: r.get(8)?,
                        category: r.get::<_, Option<String>>(9)?.filter(|c| !c.is_empty()),
                        has_preview: r.get(10)?,
                        cursor: StoreCursor {
                            key: sort_key,
                            gb_id: r.get(0)?,
                        },
                    }))
                })?;
                rows.filter_map(Result::transpose).collect()
            })
            .map_err(database)
    }

    /// How many cached cards of `kind` match `q`'s filters; the cursor and limit are
    /// ignored. Curated HUDs and Addons are NotPaged.
    pub fn store_count(&self, kind: StoreKind, q: &StoreQuery) -> Result<u64, QueryError> {
        let (sql, params) = cards_sql(kind, q, true).ok_or(QueryError::NotPaged { kind })?;
        let count: i64 = self
            .db
            .read(|conn| conn.query_row(&sql, rusqlite::params_from_iter(&params), |r| r.get(0)))
            .map_err(database)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

pub(super) fn database(e: rusqlite::Error) -> QueryError {
    QueryError::Database {
        message: e.to_string(),
    }
}

/// The page query (or, with `count`, its `COUNT(*)` form without the cursor) and its
/// parameters; `None` for the stores that are read whole. Every identifier is a crate
/// constant; an unsearched `ORDER BY` reads migration 10's `(key DESC, gb_id DESC)` indexes.
pub(super) fn cards_sql(
    kind: StoreKind,
    q: &StoreQuery,
    count: bool,
) -> Option<(String, Vec<Value>)> {
    let library = match kind {
        StoreKind::Sounds => "library_sounds",
        StoreKind::Crosshairs => "library_crosshairs",
        StoreKind::GameBananaHuds => "library_huds",
        StoreKind::CuratedHuds | StoreKind::Addons => return None,
    };
    let table = kind.table();
    // Only store_sounds has the category and preview columns; the others select constants.
    let sound_columns = match kind {
        StoreKind::Sounds => "s.category, COALESCE(s.preview_url, '') <> ''",
        StoreKind::CuratedHuds
        | StoreKind::GameBananaHuds
        | StoreKind::Crosshairs
        | StoreKind::Addons => "NULL, 0",
    };
    let key = q.sort.column();
    let mut filters = Vec::new();
    let mut params = Vec::new();
    if let (StoreKind::Sounds, Some(category)) = (kind, &q.category) {
        filters.push("s.category = ?".to_owned());
        params.push(Value::Text(category.clone()));
    }
    let search = q.search.as_deref().filter(|t| !t.trim().is_empty());
    let searching = search.is_some();
    if let Some(text) = search {
        let pattern = like_pattern(text);
        filters.push("(s.name LIKE ? ESCAPE '\\' OR s.author LIKE ? ESCAPE '\\')".to_owned());
        params.push(Value::Text(pattern.clone()));
        params.push(Value::Text(pattern));
    }
    if count {
        let clause = where_clause(&filters);
        return Some((format!("SELECT COUNT(*) FROM {table} s{clause}"), params));
    }
    if let Some(after) = &q.after {
        filters.push(format!("(s.{key}, s.gb_id) < (?, ?)"));
        params.push(Value::Integer(after.key));
        params.push(Value::Integer(after.gb_id));
    }
    let clause = where_clause(&filters);
    // LIKE cannot use an index, so a search scans the table once and top-N sorts the matches;
    // walking the sort index instead visits every row through a lookup when the term is rare.
    let scan = if searching { " NOT INDEXED" } else { "" };
    params.push(Value::Integer(i64::from(q.limit)));
    let sql = format!(
        "SELECT s.gb_id, s.name, s.author, s.thumbnail_url, s.tags_json, s.like_count,
                s.view_count, s.date_added,
                EXISTS (SELECT 1 FROM {library} l WHERE l.store_gb_id = s.gb_id),
                {sound_columns}
         FROM {table} s{scan}{clause}
         ORDER BY s.{key} DESC, s.gb_id DESC LIMIT ?"
    );
    Some((sql, params))
}

/// A LIKE pattern matching `text` anywhere, with LIKE's wildcards escaped by `\` so the
/// text matches literally.
pub(super) fn like_pattern(text: &str) -> String {
    let mut pattern = String::from("%");
    for c in text.chars() {
        if matches!(c, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

pub(super) fn where_clause(filters: &[String]) -> String {
    if filters.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", filters.join(" AND "))
    }
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

    /// 300 rows per paged store with tied sort keys and three sound categories, then ANALYZE,
    /// so the planner sees statistics like a synced store's.
    fn seed(db: &Database) {
        db.write(|c| {
            for table in ["store_sounds", "store_crosshairs", "store_huds_gb"] {
                c.execute_batch(&format!(
                    "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 300)
                     INSERT INTO {table} (gb_id, name, like_count, view_count, date_added, fetched_at)
                     SELECT i, 'item ' || i, i % 17, i % 29, i * 7, 'x' FROM n;"
                ))?;
            }
            c.execute_batch(
                "UPDATE store_sounds SET category = CASE gb_id % 3 WHEN 0 THEN 'hitsound'
                     WHEN 1 THEN 'killsound' ELSE 'sound' END;
                 ANALYZE;",
            )
        })
        .unwrap();
    }

    fn assert_index_plans(db: &Database, state: &str) {
        let cursor = StoreCursor { key: 5, gb_id: 9 };
        for kind in [
            StoreKind::Sounds,
            StoreKind::Crosshairs,
            StoreKind::GameBananaHuds,
        ] {
            for sort in [
                StoreSort::MostLiked,
                StoreSort::MostViewed,
                StoreSort::Newest,
            ] {
                for after in [None, Some(cursor.clone())] {
                    let mut q = StoreQuery::new(sort, 60);
                    q.after = after;
                    let (sql, params) = cards_sql(kind, &q, false).unwrap();
                    let plan = plan(db, &sql, &params);
                    assert!(plan.contains("USING"), "{state} {kind:?} {sort:?}: {plan}");
                    assert!(
                        !plan.contains("TEMP B-TREE"),
                        "{state} {kind:?} {sort:?}: {plan}"
                    );
                }
            }
        }
        let q = StoreQuery::new(StoreSort::MostLiked, 60).with_category("hitsound");
        let (sql, params) = cards_sql(StoreKind::Sounds, &q, false).unwrap();
        let plan = plan(db, &sql, &params);
        assert!(
            plan.contains("idx_store_sounds_cat_likes"),
            "{state}: {plan}"
        );
        assert!(!plan.contains("TEMP B-TREE"), "{state}: {plan}");
    }

    #[test]
    fn every_sort_pages_through_an_index_without_a_temp_sort() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&DataDir::at(tmp.path().join("data"))).unwrap();
        // Empty is a fresh install before its first sync; seeded is a synced store.
        assert_index_plans(&db, "empty");
        seed(&db);
        assert_index_plans(&db, "seeded");
        let q = StoreQuery::new(StoreSort::MostLiked, 60);
        assert!(cards_sql(StoreKind::CuratedHuds, &q, false).is_none());
    }

    // A LIKE filter cannot use an index, so walking a sort index for it visits every row
    // through a table lookup when the term is rare; one sequential scan bounds the work.
    #[test]
    fn a_search_page_scans_the_table_once_instead_of_walking_a_sort_index() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&DataDir::at(tmp.path().join("data"))).unwrap();
        seed(&db);
        let cursor = StoreCursor { key: 5, gb_id: 9 };
        for kind in [
            StoreKind::Sounds,
            StoreKind::Crosshairs,
            StoreKind::GameBananaHuds,
        ] {
            for sort in [
                StoreSort::MostLiked,
                StoreSort::MostViewed,
                StoreSort::Newest,
            ] {
                for after in [None, Some(cursor.clone())] {
                    for category in [None, Some("hitsound")] {
                        let mut q = StoreQuery::new(sort, 60).with_search("zq");
                        q.after = after.clone();
                        q.category = category.map(str::to_owned);
                        let (sql, params) = cards_sql(kind, &q, false).unwrap();
                        let plan = plan(&db, &sql, &params);
                        assert!(
                            plan.contains("SCAN s"),
                            "{kind:?} {sort:?} after={after:?} category={category:?}: {plan}"
                        );
                        assert!(
                            !plan.contains("SCAN s USING"),
                            "{kind:?} {sort:?} after={after:?} category={category:?}: {plan}"
                        );
                        assert!(
                            !plan.contains("SEARCH s"),
                            "{kind:?} {sort:?} after={after:?} category={category:?}: {plan}"
                        );
                    }
                }
            }
        }
    }
}
