//! Reading the store tables back for the grids, listing an item's downloadable files
//! and fetching audio previews; ported from v5's `load_all_*`, `fetch_item_files` and
//! `fetch_preview_audio`.

use rusqlite::{Connection, OptionalExtension, Row, params_from_iter};

use super::http::under_prefix;
use super::pacer::Priority;
use super::{StoreClient, StoreKind, gamebanana};
use crate::error::{FilesError, PreviewError, QueryError};
use crate::library::{AddonName, GbId};

/// Which store item: a GameBanana id, a hud-db HUD id, or a mastercomfig addon asset.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StoreItemId {
    /// A GameBanana HUD, sound or crosshair.
    GameBanana(GbId),
    /// A curated hud-db HUD, by its `hud_id`.
    Curated(String),
    /// A mastercomfig addon, by its asset name.
    Addon(AddonName),
}

impl StoreItemId {
    fn describe(&self) -> String {
        match self {
            StoreItemId::GameBanana(id) => format!("GameBanana item {}", id.get()),
            StoreItemId::Curated(id) => format!("curated HUD {id}"),
            StoreItemId::Addon(name) => format!("addon {}", name.as_str()),
        }
    }
}

/// One cached store row. Columns a store does not have read as None, empty or 0.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct StoreItem {
    id: StoreItemId,
    kind: StoreKind,
    name: String,
    author: String,
    category: Option<String>,
    preview_url: Option<String>,
    thumbnail_url: Option<String>,
    images: Vec<String>,
    tags: Vec<String>,
    like_count: i64,
    view_count: i64,
    date_added: i64,
    profile_url: String,
    fetched_at: String,
    repo: Option<String>,
    hash: Option<String>,
    data_json: Option<String>,
    description: String,
    file_size: u64,
    download_count: u64,
    release_tag: Option<String>,
}

impl StoreItem {
    fn new(id: StoreItemId, kind: StoreKind, name: String, author: String) -> Self {
        Self {
            id,
            kind,
            name,
            author,
            category: None,
            preview_url: None,
            thumbnail_url: None,
            images: Vec::new(),
            tags: Vec::new(),
            like_count: 0,
            view_count: 0,
            date_added: 0,
            profile_url: String::new(),
            fetched_at: String::new(),
            repo: None,
            hash: None,
            data_json: None,
            description: String::new(),
            file_size: 0,
            download_count: 0,
            release_tag: None,
        }
    }

    /// The item's id within its store.
    pub fn id(&self) -> &StoreItemId {
        &self.id
    }

    /// The store the item came from.
    pub fn kind(&self) -> StoreKind {
        self.kind
    }

    /// The display name; an addon's `display_name`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The submitter; empty for addons.
    pub fn author(&self) -> &str {
        &self.author
    }

    /// `hitsound`, `killsound` or `sound`; sounds only.
    pub fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    /// The audio preview address; sounds only.
    pub fn preview_url(&self) -> Option<&str> {
        self.preview_url.as_deref()
    }

    /// The card thumbnail address, if the store has one.
    pub fn thumbnail_url(&self) -> Option<&str> {
        self.thumbnail_url.as_deref()
    }

    /// The gallery images of a GameBanana HUD or crosshair.
    pub fn images(&self) -> &[String] {
        &self.images
    }

    /// GameBanana tags.
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    /// GameBanana likes.
    pub fn like_count(&self) -> i64 {
        self.like_count
    }

    /// GameBanana views.
    pub fn view_count(&self) -> i64 {
        self.view_count
    }

    /// The GameBanana upload time, in Unix seconds.
    pub fn date_added(&self) -> i64 {
        self.date_added
    }

    /// The GameBanana page address.
    pub fn profile_url(&self) -> &str {
        &self.profile_url
    }

    /// When the row was last synced, as stored.
    pub fn fetched_at(&self) -> &str {
        &self.fetched_at
    }

    /// A curated HUD's GitHub repository address.
    pub fn repo(&self) -> Option<&str> {
        self.repo.as_deref()
    }

    /// A curated HUD's pinned commit.
    pub fn hash(&self) -> Option<&str> {
        self.hash.as_deref()
    }

    /// A curated HUD's hud-db JSON record.
    pub fn data_json(&self) -> Option<&str> {
        self.data_json.as_deref()
    }

    /// An addon's description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// An addon's size in bytes.
    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// An addon's GitHub download count.
    pub fn download_count(&self) -> u64 {
        self.download_count
    }

    /// The mastercomfig release an addon comes from.
    pub fn release_tag(&self) -> Option<&str> {
        self.release_tag.as_deref()
    }
}

/// One downloadable file of a store item.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StoreFile {
    kind: StoreKind,
    item: StoreItemId,
    file_name: String,
    size: Option<u64>,
    md5: Option<String>,
    url: String,
    release_tag: Option<String>,
}

impl StoreFile {
    /// The store the file belongs to.
    pub fn kind(&self) -> StoreKind {
        self.kind
    }

    /// The item the file belongs to.
    pub fn item(&self) -> &StoreItemId {
        &self.item
    }

    /// The file name the store gives it.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// The size the store reports, if any.
    pub fn size(&self) -> Option<u64> {
        self.size
    }

    /// The MD5 hex digest the store reports, if any.
    pub fn md5(&self) -> Option<&str> {
        self.md5.as_deref()
    }

    /// The download address.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The mastercomfig release an addon file comes from.
    pub(super) fn release_tag(&self) -> Option<&str> {
        self.release_tag.as_deref()
    }
}

impl StoreClient {
    /// The whole store table, read through the reader connection. Curated HUDs come back
    /// ordered by name, others in table order; addon rows with an unsafe name are left out.
    pub fn store_items(&self, kind: StoreKind) -> Result<Vec<StoreItem>, QueryError> {
        self.db
            .read(|conn| match kind {
                StoreKind::Sounds => read_sounds(conn, None),
                StoreKind::Crosshairs | StoreKind::GameBananaHuds => read_mods(conn, kind, None),
                StoreKind::CuratedHuds => read_curated(conn, self.endpoints.hud_resources(), None),
                StoreKind::Addons => read_addons(conn, None),
            })
            .map_err(|e| QueryError::Database {
                message: e.to_string(),
            })
    }

    /// One cached row of `kind`'s store, or None when the id is absent or belongs to another
    /// kind of store. Blocking; reads through the reader connection.
    pub fn store_item(
        &self,
        kind: StoreKind,
        id: &StoreItemId,
    ) -> Result<Option<StoreItem>, QueryError> {
        // An id of the wrong shape, or a GameBanana id SQLite cannot hold, has no row to read.
        let rows = match (kind, id) {
            (StoreKind::Sounds, StoreItemId::GameBanana(gb)) => {
                let Ok(gb) = i64::try_from(gb.get()) else {
                    return Ok(None);
                };
                self.db.read(|conn| read_sounds(conn, Some(gb)))
            }
            (StoreKind::Crosshairs | StoreKind::GameBananaHuds, StoreItemId::GameBanana(gb)) => {
                let Ok(gb) = i64::try_from(gb.get()) else {
                    return Ok(None);
                };
                self.db.read(|conn| read_mods(conn, kind, Some(gb)))
            }
            (StoreKind::CuratedHuds, StoreItemId::Curated(hud_id)) => self
                .db
                .read(|conn| read_curated(conn, self.endpoints.hud_resources(), Some(hud_id))),
            (StoreKind::Addons, StoreItemId::Addon(name)) => {
                self.db.read(|conn| read_addons(conn, Some(name.as_str())))
            }
            _ => return Ok(None),
        };
        rows.map(|rows| rows.into_iter().next())
            .map_err(|e| QueryError::Database {
                message: e.to_string(),
            })
    }

    /// GameBanana: `GET {gamebanana}/{Sound|Mod}/{id}?_csvProperties=_idRow,_sName,_aFiles`.
    /// Curated: one `{repo}/archive/{hash}.zip` named `{hud_id}.zip`. Addons: the release
    /// asset. An id that does not fit `kind` or has no usable row is NotInStore.
    pub async fn item_files(
        &self,
        kind: StoreKind,
        item: &StoreItemId,
    ) -> Result<Vec<StoreFile>, FilesError> {
        let not_in_store = || FilesError::NotInStore {
            what: item.describe(),
        };
        let file = |file_name: String, size, md5, url, release_tag| StoreFile {
            kind,
            item: item.clone(),
            file_name,
            size,
            md5,
            url,
            release_tag,
        };
        match (kind, item) {
            (
                StoreKind::Sounds | StoreKind::Crosshairs | StoreKind::GameBananaHuds,
                StoreItemId::GameBanana(id),
            ) => {
                let url = gamebanana::files_url(self.endpoints.gamebanana(), kind, id.get())
                    .ok_or_else(not_in_store)?;
                let body = self.gamebanana_get(&url, Priority::Interactive).await?;
                let files = gamebanana::parse_files(&body).map_err(|e| FilesError::Malformed {
                    url: url.clone(),
                    message: e.to_string(),
                })?;
                Ok(files
                    .into_iter()
                    .map(|f| file(f.file_name, f.size, f.md5, f.url, None))
                    .collect())
            }
            (StoreKind::CuratedHuds, StoreItemId::Curated(hud_id)) => {
                // A curated HUD downloads as the GitHub archive of its pinned commit.
                let row: Option<(Option<String>, Option<String>)> = self
                    .db
                    .read(|conn| {
                        conn.query_row(
                            "SELECT repo, hash FROM store_huds WHERE hud_id = ?1",
                            [hud_id],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()
                    })
                    .map_err(database)?;
                let Some((Some(repo), Some(hash))) = row else {
                    return Err(not_in_store());
                };
                let url = format!("{repo}/archive/{hash}.zip");
                Ok(vec![file(format!("{hud_id}.zip"), None, None, url, None)])
            }
            (StoreKind::Addons, StoreItemId::Addon(name)) => {
                let row: Option<(String, i64, String)> = self
                    .db
                    .read(|conn| {
                        conn.query_row(
                            "SELECT download_url, file_size, release_tag FROM store_addons
                             WHERE asset_name = ?1",
                            [name.as_str()],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                        )
                        .optional()
                    })
                    .map_err(database)?;
                let (url, size, tag) = row.ok_or_else(not_in_store)?;
                let size = u64::try_from(size).ok();
                Ok(vec![file(
                    name.as_str().to_owned(),
                    size,
                    None,
                    url,
                    Some(tag),
                )])
            }
            _ => Err(not_in_store()),
        }
    }

    /// The preview bytes of a store sound, from a URL under one of `preview_prefixes`,
    /// capped at the Sounds download size. An untrusted URL is refused before any request.
    pub async fn preview_audio(&self, item: &StoreItem) -> Result<Vec<u8>, PreviewError> {
        let url = item.preview_url().ok_or(PreviewError::NoPreview)?;
        let trusted = self
            .endpoints
            .preview_prefixes()
            .iter()
            .any(|prefix| under_prefix(url, prefix));
        if !trusted {
            return Err(PreviewError::UntrustedUrl {
                url: url.to_owned(),
            });
        }
        let limit = self.config.size_cap(StoreKind::Sounds);
        Ok(self.preview_http.get_api(url, limit).await?)
    }
}

fn database(e: rusqlite::Error) -> FilesError {
    FilesError::Database {
        message: e.to_string(),
    }
}

/// A JSON list of strings, or empty when the column holds anything else.
pub(super) fn string_list(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

/// ` WHERE <column> = ?1` when a reader is narrowed to one row, else nothing.
fn key_filter(column: &str, narrowed: bool) -> String {
    if narrowed {
        format!(" WHERE {column} = ?1")
    } else {
        String::new()
    }
}

/// A GameBanana id column; a negative id cannot be a GameBanana item.
pub(super) fn gb_id(row: &Row, index: usize) -> rusqlite::Result<Option<StoreItemId>> {
    let id: i64 = row.get(index)?;
    Ok(u64::try_from(id)
        .ok()
        .map(|id| StoreItemId::GameBanana(GbId::new(id))))
}

// Each reader returns the whole table, or only the row whose key matches when given one.
fn read_sounds(conn: &Connection, gb: Option<i64>) -> rusqlite::Result<Vec<StoreItem>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT gb_id, name, author, category, preview_url, thumbnail_url, like_count,
                view_count, tags_json, profile_url, date_added, fetched_at
         FROM store_sounds{}",
        key_filter("gb_id", gb.is_some())
    ))?;
    let rows = stmt.query_map(params_from_iter(gb), |r| {
        let Some(id) = gb_id(r, 0)? else {
            return Ok(None);
        };
        let mut item = StoreItem::new(id, StoreKind::Sounds, r.get(1)?, r.get(2)?);
        item.category = Some(r.get(3)?);
        item.preview_url = r.get(4)?;
        item.thumbnail_url = r.get(5)?;
        item.like_count = r.get(6)?;
        item.view_count = r.get(7)?;
        item.tags = string_list(&r.get::<_, String>(8)?);
        item.profile_url = r.get(9)?;
        item.date_added = r.get(10)?;
        item.fetched_at = r.get(11)?;
        Ok(Some(item))
    })?;
    rows.filter_map(Result::transpose).collect()
}

fn read_mods(
    conn: &Connection,
    kind: StoreKind,
    gb: Option<i64>,
) -> rusqlite::Result<Vec<StoreItem>> {
    // The table name is one of the crate's own constants, never caller text.
    let mut stmt = conn.prepare(&format!(
        "SELECT gb_id, name, author, thumbnail_url, images_json, like_count, view_count,
                tags_json, profile_url, date_added, fetched_at
         FROM {}{}",
        kind.table(),
        key_filter("gb_id", gb.is_some())
    ))?;
    let rows = stmt.query_map(params_from_iter(gb), |r| {
        let Some(id) = gb_id(r, 0)? else {
            return Ok(None);
        };
        let mut item = StoreItem::new(id, kind, r.get(1)?, r.get(2)?);
        item.thumbnail_url = r.get(3)?;
        item.images = string_list(&r.get::<_, String>(4)?);
        item.like_count = r.get(5)?;
        item.view_count = r.get(6)?;
        item.tags = string_list(&r.get::<_, String>(7)?);
        item.profile_url = r.get(8)?;
        item.date_added = r.get(9)?;
        item.fetched_at = r.get(10)?;
        Ok(Some(item))
    })?;
    rows.filter_map(Result::transpose).collect()
}

fn read_curated(
    conn: &Connection,
    resources: &str,
    hud_id: Option<&str>,
) -> rusqlite::Result<Vec<StoreItem>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT hud_id, name, author, repo, hash, data_json, fetched_at
         FROM store_huds{} ORDER BY name",
        key_filter("hud_id", hud_id.is_some())
    ))?;
    let rows = stmt.query_map(params_from_iter(hud_id), |r| {
        let hud_id: String = r.get(0)?;
        let data_json: String = r.get(5)?;
        let images = curated_images(resources, &hud_id, &data_json);
        let id = StoreItemId::Curated(hud_id);
        let mut item = StoreItem::new(id, StoreKind::CuratedHuds, r.get(1)?, r.get(2)?);
        item.repo = r.get(3)?;
        item.hash = r.get(4)?;
        item.images = images;
        item.thumbnail_url = item.images.first().cloned();
        item.data_json = Some(data_json);
        item.fetched_at = r.get(6)?;
        Ok(item)
    })?;
    rows.collect()
}

/// The screenshot URLs a hud-db entry lists in `resources`, each `{base}/{hud_id}/{name}.webp`
/// with the id and name percent-encoded, or none when the JSON or the field is malformed.
fn curated_images(base: &str, hud_id: &str, data_json: &str) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct Resources {
        resources: Vec<String>,
    }
    serde_json::from_str::<Resources>(data_json).map_or_else(
        |_| Vec::new(),
        |data| {
            data.resources
                .iter()
                .map(|resource| {
                    format!(
                        "{base}/{}/{}.webp",
                        encode_path(hud_id),
                        encode_path(resource)
                    )
                })
                .collect()
        },
    )
}

/// Percent-encodes every UTF-8 byte outside the RFC 3986 unreserved set as `%XX`, keeping `/`
/// literal because hud-db (like v5) treats a slash in a resource as a subpath.
fn encode_path(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn read_addons(conn: &Connection, asset: Option<&str>) -> rusqlite::Result<Vec<StoreItem>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT asset_name, display_name, description, file_size, download_count,
                release_tag, fetched_at
         FROM store_addons{}",
        key_filter("asset_name", asset.is_some())
    ))?;
    let rows = stmt.query_map(params_from_iter(asset), |r| {
        // An asset name that could escape tf/custom never reaches the grid.
        let Some(name) = AddonName::new(r.get::<_, String>(0)?) else {
            return Ok(None);
        };
        let mut item = StoreItem::new(
            StoreItemId::Addon(name),
            StoreKind::Addons,
            r.get(1)?,
            String::new(),
        );
        item.description = r.get(2)?;
        item.file_size = u64::try_from(r.get::<_, i64>(3)?).unwrap_or(0);
        item.download_count = u64::try_from(r.get::<_, i64>(4)?).unwrap_or(0);
        item.release_tag = Some(r.get(5)?);
        item.fetched_at = r.get(6)?;
        Ok(Some(item))
    })?;
    rows.filter_map(Result::transpose).collect()
}
