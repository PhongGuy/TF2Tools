//! The synced GameBanana map catalogue: its URLs, record and category parsers, and the
//! upserts behind the maps walk.

use rusqlite::{Connection, params};
use serde_json::Value;

use super::addon_api;
use super::addon_catalog::{AddonCard, AddonSort};
use super::gamebanana::{self, IndexPage, Records, int, text};
use super::pacer::Priority;
use super::sync::{self, Confirm, Walk, database, malformed};
use super::{StoreClient, SyncOutcome, SyncProgress};
use crate::error::{QueryError, SyncError};

/// GameBanana's TF2 Maps root category.
pub(crate) const MAPS_CATEGORY: u32 = 5371;
pub(crate) const MAPS_PAGE_SIZE: u32 = addon_api::INDEX_PAGE_SIZE;

/// The most-downloaded listing's `page` of TF2 maps, 50 to a page.
pub(crate) fn index_url(base: &str, page: u32) -> String {
    addon_api::index_url(base, MAPS_CATEGORY, AddonSort::MostDownloaded, page)
}

/// The children of `parent`, A to Z, empty categories included so the tree is whole.
pub(crate) fn categories_url(base: &str, parent: u32) -> String {
    format!(
        "{}&_bShowEmpty=true",
        addon_api::categories_url(base, parent)
    )
}

/// One `store_gb_maps` row before its rank and gamemode are known.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MapRecord {
    pub(crate) card: AddonCard,
    pub(crate) view_count: i64,
    pub(crate) date_modified: i64,
}

/// Parses a maps index page into `Records::Maps`; an error only when the body is not JSON.
pub(crate) fn parse_index(body: &[u8]) -> Result<IndexPage, serde_json::Error> {
    gamebanana::index_page(body, |records| {
        Records::Maps(records.iter().filter_map(parse_map).collect())
    })
}

fn parse_map(record: &Value) -> Option<MapRecord> {
    let card = addon_api::parse_record(record)?.card;
    Some(MapRecord {
        card,
        view_count: int(&record["_nViewCount"]),
        date_modified: int(&record["_tsDateModified"]),
    })
}

/// One `Mod/Categories` row with how many child categories it has.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CategoryRow {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) children: u32,
}

/// Parses a `Mod/Categories` reply; rows without an id or a name are skipped.
pub(crate) fn parse_categories(body: &[u8]) -> Result<Vec<CategoryRow>, serde_json::Error> {
    let rows: Vec<Value> = serde_json::from_slice(body)?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let id = u32::try_from(int(&row["_idRow"]))
                .ok()
                .filter(|&id| id > 0)?;
            let name = text(&row["_sName"]);
            (!name.is_empty()).then(|| CategoryRow {
                id,
                name,
                children: u32::try_from(int(&row["_nCategoryCount"])).unwrap_or(0),
            })
        })
        .collect())
}

/// A category of the maps tree and the first-level category (gamemode) it sits under; a
/// gamemode is its own gamemode.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MapCategory {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) gamemode: u32,
}

/// Replaces the stored category tree with `rows`.
pub(crate) fn replace_categories(conn: &Connection, rows: &[MapCategory]) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM store_gb_map_categories", [])?;
    let mut stmt = conn.prepare(
        "INSERT INTO store_gb_map_categories (category_id, name, gamemode_id) VALUES (?1, ?2, ?3)",
    )?;
    for row in rows {
        stmt.execute(params![row.id, row.name, row.gamemode])?;
    }
    Ok(())
}

/// Upserts one listing page: rank = (page - 1) * 50 + position + 1, and the gamemode is
/// looked up in the stored tree, NULL when the category is unknown.
pub(crate) fn upsert_maps(
    conn: &Connection,
    rows: &[MapRecord],
    page: u32,
    fetched_at: &str,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO store_gb_maps (gb_id, name, author, thumbnail_url, like_count, view_count,
             date_added, date_modified, category_id, category_name, gamemode_id, download_rank,
             obsolete, mature, profile_url, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
             (SELECT gamemode_id FROM store_gb_map_categories WHERE category_id = ?9),
             ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(gb_id) DO UPDATE SET name = excluded.name, author = excluded.author,
             thumbnail_url = excluded.thumbnail_url, like_count = excluded.like_count,
             view_count = excluded.view_count, date_added = excluded.date_added,
             date_modified = excluded.date_modified, category_id = excluded.category_id,
             category_name = excluded.category_name, gamemode_id = excluded.gamemode_id,
             download_rank = excluded.download_rank, obsolete = excluded.obsolete,
             mature = excluded.mature, profile_url = excluded.profile_url,
             fetched_at = excluded.fetched_at",
    )?;
    let first = i64::from(page.saturating_sub(1)) * i64::from(MAPS_PAGE_SIZE) + 1;
    for (position, r) in (0_i64..).zip(rows) {
        // GameBanana ids fit an i64; one that did not would be no map of TF2's.
        let Ok(gb_id) = i64::try_from(r.card.id.get()) else {
            continue;
        };
        let c = &r.card;
        stmt.execute(params![
            gb_id,
            c.name,
            c.author,
            c.thumbnail_url,
            c.like_count,
            r.view_count,
            c.date_added,
            r.date_modified,
            c.category_id,
            c.category_name,
            first + position,
            c.obsolete,
            c.mature,
            c.profile_url,
            fetched_at
        ])?;
    }
    Ok(())
}

/// The most-downloaded maps listing, checkpointed under `gb_maps_*` and confirmed only by
/// `_nRecordCount`, which can run ahead of the listing.
fn walk() -> Walk {
    Walk {
        key: "gb_maps",
        table: "store_gb_maps",
        page_size: MAPS_PAGE_SIZE,
        page_limit: Box::new(|config, total| config.gb_maps_page_limit(total)),
        url: Box::new(|endpoints, page| Some(index_url(endpoints.gamebanana(), page))),
        parse: Box::new(parse_index),
        confirm: Confirm::CountOrRepeatedEnd,
        page_meta: None,
    }
}

/// Syncs the maps catalogue like a GameBanana store. A fresh walk first replaces the
/// category tree, so each page's gamemodes resolve; a resumed walk reuses the stored one.
pub(crate) async fn sync(
    client: &StoreClient,
    force: bool,
    progress: &(dyn Fn(SyncProgress) + Send + Sync),
) -> Result<SyncOutcome, SyncError> {
    let walk = walk();
    let Some(start) = sync::begin(client, &walk, force, false)? else {
        return Ok(SyncOutcome::skipped());
    };
    if !start.resumed {
        refresh_categories(client).await?;
    }
    sync::run(client, &walk, start, progress).await
}

/// Reads the two-level tree under the maps root and stores it in one transaction; a failed
/// call leaves the old tree in place.
async fn refresh_categories(client: &StoreClient) -> Result<(), SyncError> {
    // An empty root is a GameBanana hiccup, not a map section without gamemodes.
    let modes = fetch_categories(client, MAPS_CATEGORY).await?;
    if modes.is_empty() {
        tracing::warn!("GameBanana listed no map gamemodes; the stored tree is kept");
        return Ok(());
    }
    let mut tree = Vec::new();
    for mode in modes {
        let children = if mode.children > 0 {
            fetch_categories(client, mode.id).await?
        } else {
            Vec::new()
        };
        tree.push(MapCategory {
            id: mode.id,
            name: mode.name,
            gamemode: mode.id,
        });
        tree.extend(children.into_iter().map(|child| MapCategory {
            id: child.id,
            name: child.name,
            gamemode: mode.id,
        }));
    }
    client
        .db
        .write(|conn| {
            let tx = conn.transaction()?;
            replace_categories(&tx, &tree)?;
            tx.commit()
        })
        .map_err(database)
}

async fn fetch_categories(
    client: &StoreClient,
    parent: u32,
) -> Result<Vec<CategoryRow>, SyncError> {
    let url = categories_url(client.endpoints.gamebanana(), parent);
    let body = client.gamebanana_get(&url, Priority::Background).await?;
    parse_categories(&body).map_err(|e| malformed(url, &e.to_string()))
}

/// One first-level category of the maps tree, offered as a gamemode filter.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GbMapGamemode {
    /// Its GameBanana category id.
    pub id: u32,
    /// Its name, such as "Control Points".
    pub name: String,
}

impl StoreClient {
    /// The gamemodes of the stored category tree, A to Z; empty before the first sync.
    /// Blocking.
    pub fn gb_map_gamemodes(&self) -> Result<Vec<GbMapGamemode>, QueryError> {
        self.db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT category_id, name FROM store_gb_map_categories
                     WHERE category_id = gamemode_id ORDER BY name COLLATE NOCASE, category_id",
                )?;
                let rows = stmt.query_map([], |r| {
                    Ok(GbMapGamemode {
                        id: r.get(0)?,
                        name: r.get(1)?,
                    })
                })?;
                rows.collect()
            })
            .map_err(|e| QueryError::Database {
                message: e.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::paths::DataDir;
    use serde_json::{Value, json};

    fn record(id: u64, sub: Option<u32>) -> Value {
        let mut r = json!({"_idRow": id, "_sModelName": "Mod", "_sName": format!("map_{id}"),
            "_sProfileUrl": format!("https://gamebanana.com/mods/{id}"), "_tsDateAdded": 1000 + id,
            "_tsDateModified": 2000 + id, "_nLikeCount": 3, "_nViewCount": 40, "_bIsObsolete": false,
            "_sInitialVisibility": "show", "_bHasContentRatings": false, "_aSubmitter": {"_sName": "vex"},
            "_aRootCategory": {"_sName": "Maps", "_sProfileUrl": "https://gamebanana.com/mods/cats/5371"}});
        if let Some(sub) = sub {
            r["_aSubCategory"] = json!({"_sName": format!("Cat {sub}"),
                "_sProfileUrl": format!("https://gamebanana.com/mods/cats/{sub}")});
        }
        r
    }

    fn rows(records: Vec<Value>) -> Vec<MapRecord> {
        let body = json!({"_aMetadata": {"_nRecordCount": 9, "_bIsComplete": false}, "_aRecords": records});
        match parse_index(body.to_string().as_bytes()).unwrap().records {
            Records::Maps(rows) => rows,
            other @ (Records::Sounds(_) | Records::Mods(_)) => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_urls_ask_for_fifty_most_downloaded_maps_and_every_category() {
        let url = index_url("b", 3);
        for part in [
            "b/Mod/Index?",
            "_nPage=3",
            "_nPerpage=50",
            "_aFilters[Generic_Category]=5371",
            "_aFilters[Generic_Game]=297",
            "_sSort=Generic_MostDownloaded",
        ] {
            assert!(url.contains(part), "{part}: {url}");
        }
        assert_eq!(
            categories_url("b", 5383),
            "b/Mod/Categories?_idCategoryRow=5383&_sSort=a_to_z&_bShowEmpty=true"
        );
    }

    #[test]
    fn an_index_page_parses_cards_counts_and_the_mature_and_obsolete_rules() {
        let mut mature = record(2, None);
        mature["_sInitialVisibility"] = "warn".into();
        mature["_bIsObsolete"] = true.into();
        let body = json!({"_aMetadata": {"_nRecordCount": 6802, "_bIsComplete": false},
            "_aRecords": [record(1, Some(5388)), mature, {"_idRow": 0}]});
        let page = parse_index(body.to_string().as_bytes()).unwrap();
        assert_eq!((page.total, page.more, page.records.len()), (6802, true, 2));
        let Records::Maps(rows) = page.records else {
            panic!("not maps")
        };
        let first = &rows[0];
        assert_eq!(
            (first.card.name.as_str(), first.card.author.as_str()),
            ("map_1", "vex")
        );
        assert_eq!(
            (first.card.category_id, first.card.category_name.as_deref()),
            (Some(5388), Some("Cat 5388"))
        );
        assert_eq!(
            (first.view_count, first.date_modified, first.card.date_added),
            (40, 2001, 1001)
        );
        assert!(!first.card.mature && !first.card.obsolete);
        assert!(rows[1].card.mature && rows[1].card.obsolete && rows[1].card.category_id.is_none());
    }

    #[test]
    fn category_rows_carry_their_child_count() {
        let body = json!([{"_idRow": 5383, "_sName": "Achievement/Idle", "_nCategoryCount": 1},
            {"_idRow": 5373, "_sName": "Control Points", "_nCategoryCount": 0}, {"_idRow": 0, "_sName": "x"}]);
        let rows = parse_categories(body.to_string().as_bytes()).unwrap();
        let got: Vec<_> = rows
            .iter()
            .map(|r| (r.id, r.name.as_str(), r.children))
            .collect();
        assert_eq!(
            got,
            [(5383, "Achievement/Idle", 1), (5373, "Control Points", 0)]
        );
    }

    #[test]
    fn upsert_ranks_by_page_position_and_resolves_gamemodes_from_the_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&DataDir::at(tmp.path().join("data"))).unwrap();
        let tree = [
            (5373, "Control Points", 5373),
            (5383, "Achievement/Idle", 5383),
            (5388, "Trade", 5383),
        ]
        .map(|(id, name, gamemode)| MapCategory {
            id,
            name: name.to_owned(),
            gamemode,
        });
        let read = |db: &Database| -> Vec<(i64, i64, Option<i64>, Option<i64>)> {
            db.read(|c| {
                let mut s = c.prepare("SELECT gb_id, download_rank, gamemode_id, category_id FROM store_gb_maps ORDER BY gb_id")?;
                let rows = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
                rows.collect()
            })
            .unwrap()
        };
        db.write(|c| {
            replace_categories(c, &tree)?;
            upsert_maps(
                c,
                &rows(vec![
                    record(1, Some(5373)),
                    record(2, Some(5388)),
                    record(3, Some(9999)),
                    record(4, None),
                ]),
                2,
                "t1",
            )
        })
        .unwrap();
        assert_eq!(
            read(&db),
            [
                (1, 51, Some(5373), Some(5373)),
                (2, 52, Some(5383), Some(5388)),
                (3, 53, None, Some(9999)),
                (4, 54, None, None)
            ]
        );
        // A row seen again takes its later rank, in place.
        db.write(|c| upsert_maps(c, &rows(vec![record(2, Some(5388))]), 1, "t2"))
            .unwrap();
        assert_eq!(read(&db)[1], (2, 1, Some(5383), Some(5388)));
        db.write(|c| replace_categories(c, &tree[..1])).unwrap();
        let n: i64 = db
            .read(|c| {
                c.query_row("SELECT COUNT(*) FROM store_gb_map_categories", [], |r| {
                    r.get(0)
                })
            })
            .unwrap();
        assert_eq!(n, 1, "the tree is replaced, not merged");
    }

    #[test]
    fn the_gamemodes_are_the_first_level_categories_a_to_z() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::at(tmp.path().join("data"));
        let client = StoreClient::new(
            Database::open(&dir).unwrap(),
            dir,
            crate::store::Endpoints::default(),
        );
        let tree = [
            (5383, "Achievement/Idle", 5383),
            (5388, "Trade", 5383),
            (5373, "Control Points", 5373),
        ]
        .map(|(id, name, gamemode)| MapCategory {
            id,
            name: name.to_owned(),
            gamemode,
        });
        client.db.write(|c| replace_categories(c, &tree)).unwrap();
        let modes: Vec<(u32, String)> = client
            .gb_map_gamemodes()
            .unwrap()
            .into_iter()
            .map(|g| (g.id, g.name))
            .collect();
        assert_eq!(
            modes,
            [
                (5383, "Achievement/Idle".to_owned()),
                (5373, "Control Points".to_owned())
            ]
        );
    }

    #[test]
    fn the_maps_page_limit_is_its_own_and_ten_percent_over_the_count() {
        let c = crate::store::StoreConfig::default();
        assert_eq!(
            (c.gb_maps_page_limit(6802), c.gb_maps_page_limit(0)),
            (151, 500)
        );
        assert_eq!(
            c.clone()
                .with_gb_maps_page_limit(3)
                .gb_maps_page_limit(6802),
            3
        );
        assert_eq!(
            c.with_page_limit(crate::store::StoreKind::Crosshairs, 2)
                .gb_maps_page_limit(6802),
            151
        );
    }
}
