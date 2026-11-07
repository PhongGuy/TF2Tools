//! Community maps from serveme.tf: the list page parsed into `store_maps`, and map files
//! downloaded into `tf/maps`, recorded in `downloaded_maps` so only those are ever removed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::params;
use tokio::io::AsyncWriteExt;

use super::StoreClient;
use super::SyncOutcome;
use super::http::API_BODY_LIMIT;
use super::sync::{database, is_stale, malformed, set_meta};
use crate::error::{MapDownloadError, MapRemoveError, QueryError, SyncError};
use crate::tf2::Tf2Install;
use crate::timestamp;

/// The `store_cache_meta` key prefix of the map list.
const KEY: &str = "maps";
/// The first bytes of every Source engine map file.
const BSP_MAGIC: &[u8; 4] = b"VBSP";
/// The longest map name accepted; serveme's longest is under 70.
const MAX_NAME: usize = 128;

/// One map as serveme.tf lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommunityMap {
    name: String,
    times_played: u64,
    first_played: Option<String>,
    last_played: Option<String>,
    size: u64,
}

impl CommunityMap {
    /// The map name, which is also its file name without `.bsp`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// How many serveme reservations played it.
    pub fn times_played(&self) -> u64 {
        self.times_played
    }

    /// The first day it was played, as `YYYY-MM-DD`, if ever.
    pub fn first_played(&self) -> Option<&str> {
        self.first_played.as_deref()
    }

    /// The last day it was played, as `YYYY-MM-DD`, if ever.
    pub fn last_played(&self) -> Option<&str> {
        self.last_played.as_deref()
    }

    /// The `.bsp` file's size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }
}

/// Where a listed map stands in the game folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MapState {
    /// The game does not have it.
    Available,
    /// TF2Tools downloaded it into `tf/maps`, so it may be removed.
    Downloaded,
    /// The game already has it from somewhere else; it is never touched.
    InGame,
}

/// A cached map and where it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MapEntry {
    map: CommunityMap,
    state: MapState,
}

impl MapEntry {
    /// The map as listed.
    pub fn map(&self) -> &CommunityMap {
        &self.map
    }

    /// Where it stands in the game folder.
    pub fn state(&self) -> MapState {
        self.state
    }
}

/// True for a name that is safe as a file name on every platform: ASCII letters, digits,
/// `_`, `-` and `.`, not starting with a dot and never `..`.
fn is_map_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.starts_with('.')
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

/// Every well-formed row of serveme.tf's `/maps` page, in page order. A row without a
/// usable name, play count or byte size is skipped, so one odd row never loses the list.
pub fn parse_serveme_maps(html: &str) -> Vec<CommunityMap> {
    html.split("<tr").filter_map(parse_row).collect()
}

// A row's first cell holds the name span, the next the play count, then (unless the site
// merged them into one cell for a never-played map) the two dates; the last cell is the
// download link, whose title carries the exact byte size.
fn parse_row(row: &str) -> Option<CommunityMap> {
    let name_at = row.find("map-name-compact")?;
    let name = attribute(&row[name_at..], "title")?;
    if !is_map_name(name) {
        return None;
    }
    let cells = cells(row);
    let date = |cell: &String| Some(cell.clone()).filter(|d| is_date(d));
    let (played, first_played, last_played) = match cells.as_slice() {
        [_, played, first, last, _, ..] => (played, date(first), date(last)),
        [_, played, _, ..] => (played, None, None),
        _ => return None,
    };
    let times_played = played.replace(',', "").parse().ok()?;
    let link = &row[row.rfind("<a ")?..];
    let size = attribute(link, "title")?
        .strip_suffix(" bytes")?
        .parse()
        .ok()?;
    Some(CommunityMap {
        name: name.to_owned(),
        times_played,
        first_played,
        last_played,
        size,
    })
}

/// The value of the first `key="..."` in `text`.
fn attribute<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let open = format!("{key}=\"");
    let start = text.find(&open)? + open.len();
    let len = text[start..].find('"')?;
    Some(&text[start..start + len])
}

/// The text of every `<td>` in `row`, tags stripped and trimmed.
fn cells(row: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = row;
    while let Some(open) = rest.find("<td") {
        let after = &rest[open..];
        let Some(body_at) = after.find('>') else {
            break;
        };
        let body = &after[body_at + 1..];
        let end = body.find("</td>").unwrap_or(body.len());
        out.push(strip_tags(&body[..end]).trim().to_owned());
        rest = &body[end..];
    }
    out
}

fn strip_tags(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text
}

fn is_date(text: &str) -> bool {
    chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok()
}

/// The lowercased stems of the `.bsp` files directly in `dir`; a missing dir has none.
/// Lowercased because Windows matches file names without regard to case.
fn bsp_stems(dir: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return HashSet::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.strip_suffix(".bsp").map(str::to_owned)
        })
        .collect()
}

fn maps_dir(install: &Tf2Install) -> PathBuf {
    install.tf_dir().join("maps")
}

/// Where maps the game fetched from servers land; the game loads them like its own.
fn server_maps_dir(install: &Tf2Install) -> PathBuf {
    install.tf_dir().join("download").join("maps")
}

impl StoreClient {
    /// Fetches serveme.tf's map list and replaces the cached one when it is older than
    /// `sync_cache_hours` or `force` is set. A page with no usable rows is Malformed and the
    /// cached list stays. Needs a tokio runtime.
    pub async fn sync_maps(&self, force: bool) -> Result<SyncOutcome, SyncError> {
        let hours = self.db.settings().map_err(database)?.sync_cache_hours();
        let stale = self
            .db
            .read(|conn| is_stale(conn, KEY, hours))
            .map_err(database)?;
        if !force && !stale {
            return Ok(SyncOutcome::skipped());
        }
        let url = format!("{}/maps", self.endpoints.serveme);
        let body = self.http.get_api(&url, API_BODY_LIMIT).await?;
        let maps = parse_serveme_maps(&String::from_utf8_lossy(&body));
        if maps.is_empty() {
            return Err(malformed(url, "the page lists no maps"));
        }
        let fetched = u32::try_from(maps.len()).unwrap_or(u32::MAX);
        let rows = self
            .db
            .write(|conn| {
                let tx = conn.transaction()?;
                let now = timestamp::now();
                tx.execute("DELETE FROM store_maps", [])?;
                {
                    let mut insert = tx.prepare(
                        "INSERT OR REPLACE INTO store_maps
                         (name, times_played, first_played, last_played, size, fetched_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    )?;
                    for map in &maps {
                        insert.execute(params![
                            map.name,
                            i64::try_from(map.times_played).unwrap_or(i64::MAX),
                            map.first_played,
                            map.last_played,
                            i64::try_from(map.size).unwrap_or(i64::MAX),
                            now,
                        ])?;
                    }
                }
                set_meta(&tx, &format!("{KEY}_last_synced"), &now)?;
                let rows: i64 =
                    tx.query_row("SELECT COUNT(*) FROM store_maps", [], |r| r.get(0))?;
                tx.commit()?;
                Ok(u64::try_from(rows).unwrap_or(0))
            })
            .map_err(database)?;
        tracing::info!(rows, "map list sync finished");
        Ok(SyncOutcome {
            fresh: false,
            fetched,
            rows,
            rate_limited: 0,
        })
    }

    /// The cached map list by name, each with its state in `install`; without an install
    /// every map is Available. Blocking.
    pub fn maps(&self, install: Option<&Tf2Install>) -> Result<Vec<MapEntry>, QueryError> {
        let query_error = |e: rusqlite::Error| QueryError::Database {
            message: e.to_string(),
        };
        let (maps, downloaded) = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT name, times_played, first_played, last_played, size
                     FROM store_maps ORDER BY name COLLATE NOCASE, name",
                )?;
                let maps = stmt
                    .query_map([], |r| {
                        Ok(CommunityMap {
                            name: r.get(0)?,
                            times_played: u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                            first_played: r.get(2)?,
                            last_played: r.get(3)?,
                            size: u64::try_from(r.get::<_, i64>(4)?).unwrap_or(0),
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok((maps, self.downloaded_names(conn)?))
            })
            .map_err(query_error)?;
        let (own, server) = install
            .map(|i| (bsp_stems(&maps_dir(i)), bsp_stems(&server_maps_dir(i))))
            .unwrap_or_default();
        Ok(maps
            .into_iter()
            .map(|map| {
                let lower = map.name.to_lowercase();
                let state = if own.contains(&lower) && downloaded.contains(&map.name) {
                    MapState::Downloaded
                } else if own.contains(&lower) || server.contains(&lower) {
                    MapState::InGame
                } else {
                    MapState::Available
                };
                MapEntry { map, state }
            })
            .collect())
    }

    fn downloaded_names(&self, conn: &rusqlite::Connection) -> rusqlite::Result<HashSet<String>> {
        conn.prepare("SELECT name FROM downloaded_maps")?
            .query_map([], |r| r.get(0))?
            .collect()
    }

    /// Downloads map `name` from serveme's file server into `tf/maps/<name>.bsp` through a
    /// temporary file in the same folder, renamed only once the whole file has arrived and
    /// starts like a map. A map the game already has is never overwritten. `progress` gets
    /// the bytes received and the announced size after every chunk and must not block.
    /// Shares the store's one-at-a-time download lock. Needs a tokio runtime.
    pub async fn download_map(
        &self,
        install: &Tf2Install,
        name: &str,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<(), MapDownloadError> {
        if !is_map_name(name) {
            return Err(MapDownloadError::BadName {
                name: name.to_owned(),
            });
        }
        let _turn = self.download_lock.lock().await;
        self.refuse_present(install, name)?;

        let dir = maps_dir(install);
        let dest = dir.join(format!("{name}.bsp"));
        let io_at = |path: &Path| {
            let path = path.to_path_buf();
            move |source| MapDownloadError::Io { path, source }
        };
        tokio::fs::create_dir_all(&dir).await.map_err(io_at(&dir))?;
        // The temporary name starts with a dot and does not end in .bsp, so the game never
        // loads a half-written map; it is deleted on every early return.
        let part = tempfile::Builder::new()
            .prefix(".tf2tools-")
            .suffix(".part")
            .tempfile_in(&dir)
            .map_err(io_at(&dir))?;
        let size = self.fetch_map(name, part.path(), progress).await?;
        part.persist_noclobber(&dest).map_err(|e| {
            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                MapDownloadError::InGame {
                    name: name.to_owned(),
                }
            } else {
                MapDownloadError::Io {
                    path: dest.clone(),
                    source: e.error,
                }
            }
        })?;

        // An unrecorded file would read as the game's own and could never be removed, so a
        // failed record takes the file back out.
        let recorded = self.db.write(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO downloaded_maps (name, size, downloaded_at)
                 VALUES (?1, ?2, ?3)",
                params![
                    name,
                    i64::try_from(size).unwrap_or(i64::MAX),
                    timestamp::now()
                ],
            )
        });
        if let Err(e) = recorded {
            let _ = std::fs::remove_file(&dest);
            return Err(MapDownloadError::Database {
                message: e.to_string(),
            });
        }
        tracing::info!(map = name, size, "map downloaded");
        Ok(())
    }

    /// InGame or AlreadyDownloaded when the game can already load a map named `name`.
    fn refuse_present(&self, install: &Tf2Install, name: &str) -> Result<(), MapDownloadError> {
        let lower = name.to_lowercase();
        let own = bsp_stems(&maps_dir(install)).contains(&lower);
        if !own && !bsp_stems(&server_maps_dir(install)).contains(&lower) {
            return Ok(());
        }
        let ours = own
            && self
                .db
                .read(|conn| self.downloaded_names(conn))
                .map_err(|e| MapDownloadError::Database {
                    message: e.to_string(),
                })?
                .contains(name);
        let name = name.to_owned();
        Err(if ours {
            MapDownloadError::AlreadyDownloaded { name }
        } else {
            MapDownloadError::InGame { name }
        })
    }

    /// Streams the map into `dest`, failing as soon as it passes the cap or its first bytes
    /// are not a map's. Returns the bytes received.
    async fn fetch_map(
        &self,
        name: &str,
        dest: &Path,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<u64, MapDownloadError> {
        let url = format!("{}/{name}.bsp", self.endpoints.map_downloads);
        let limit = self.config.map_size_cap;
        let io = |source| MapDownloadError::Io {
            path: dest.to_path_buf(),
            source,
        };
        let not_a_map = || MapDownloadError::NotAMap {
            name: name.to_owned(),
        };
        let mut body = self.http.open(&url).await?;
        let total = body.total();
        let mut out = tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(dest)
            .await
            .map_err(io)?;
        let mut head = Vec::with_capacity(BSP_MAGIC.len());
        let mut downloaded = 0u64;
        while let Some(chunk) = body.chunk().await? {
            downloaded += chunk.len() as u64;
            if downloaded > limit {
                return Err(MapDownloadError::TooLarge { url, limit });
            }
            if head.len() < BSP_MAGIC.len() {
                let take = (BSP_MAGIC.len() - head.len()).min(chunk.len());
                head.extend_from_slice(&chunk[..take]);
                if !BSP_MAGIC.starts_with(&head) {
                    return Err(not_a_map());
                }
            }
            out.write_all(&chunk).await.map_err(io)?;
            progress(downloaded, total);
        }
        if head.as_slice() != BSP_MAGIC {
            return Err(not_a_map());
        }
        out.flush().await.map_err(io)?;
        out.sync_all().await.map_err(io)?;
        Ok(downloaded)
    }

    /// Deletes `tf/maps/<name>.bsp` and forgets it, only when TF2Tools downloaded it. A file
    /// already gone by hand is not an error. Blocking.
    pub fn remove_map(&self, install: &Tf2Install, name: &str) -> Result<(), MapRemoveError> {
        let database = |e: rusqlite::Error| MapRemoveError::Database {
            message: e.to_string(),
        };
        let ours = is_map_name(name)
            && self
                .db
                .read(|conn| self.downloaded_names(conn))
                .map_err(database)?
                .contains(name);
        if !ours {
            return Err(MapRemoveError::NotDownloaded {
                name: name.to_owned(),
            });
        }
        let path = maps_dir(install).join(format!("{name}.bsp"));
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(MapRemoveError::Io { path, source }),
        }
        self.db
            .write(|conn| conn.execute("DELETE FROM downloaded_maps WHERE name = ?1", [name]))
            .map_err(database)?;
        tracing::info!(map = name, "map removed");
        Ok(())
    }
}
