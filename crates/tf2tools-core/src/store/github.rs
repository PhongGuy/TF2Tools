//! The two GitHub-backed stores: curated HUDs from hud-db's main-branch archive and
//! addon VPKs from mastercomfig's latest release. Each sync replaces its store in one
//! transaction, so a failure leaves the previous catalogue intact.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde_json::{Map, Value};
use walkdir::WalkDir;

use super::http::{API_BODY_LIMIT, NetFailure};
use super::mastercomfig::{BASE_ASSET, override_slot};
use super::sync::{database, finish, malformed, stale};
use super::{StoreClient, StoreKind, SyncOutcome, SyncProgress};
use crate::archive::extract_archive;
use crate::error::SyncError;
use crate::library::{AddonName, display_info, is_override};
use crate::timestamp;

/// One hud-db `hud-data/<id>.json` file as a `store_huds` row.
struct CuratedHud {
    id: String,
    name: String,
    author: String,
    repo: Option<String>,
    hash: Option<String>,
    data_json: String,
}

/// One addon VPK asset of the latest mastercomfig release.
struct AddonAsset {
    name: AddonName,
    url: String,
    size: i64,
    downloads: i64,
}

/// Syncs the curated HUD store from hud-db's archive.
pub(crate) async fn curated(
    client: &StoreClient,
    force: bool,
    progress: &(dyn Fn(SyncProgress) + Send + Sync),
) -> Result<SyncOutcome, SyncError> {
    let kind = StoreKind::CuratedHuds;
    if !force && !stale(client, kind)? {
        return Ok(SyncOutcome::skipped());
    }
    // v5 ran both GitHub syncs and downloads under one guard; re-check once we hold it,
    // since the sync we waited behind may have just refreshed this store.
    let _guard = client.download_lock.lock().await;
    if !force && !stale(client, kind)? {
        return Ok(SyncOutcome::skipped());
    }
    let start = timestamp::now();

    let url = format!(
        "{}/mastercomfig/hud-db/archive/refs/heads/main.zip",
        client.endpoints.github()
    );
    let bytes = client
        .http
        .open(&url)
        .await?
        .read_to_end(client.config.size_cap(kind))
        .await?;
    let tmp = client.dir.tmp_dir();
    // Tokio turns a panic in the read into Interrupted, so it runs marked as handled.
    let huds = tokio::task::spawn_blocking(move || {
        crate::panic_mark::marked(|| read_hud_db(&tmp, &bytes))
    })
    .await
    .map_err(|e| SyncError::Interrupted {
        message: e.to_string(),
    })??
    .ok_or_else(|| malformed(url, "the archive has no hud-data directory"))?;

    let fetched = u32::try_from(huds.len()).unwrap_or(u32::MAX);
    let rows = replace(client, kind, &start, |conn| {
        let mut stmt = conn.prepare(
            "INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(hud_id) DO UPDATE SET name = excluded.name,
                 author = excluded.author, repo = excluded.repo, hash = excluded.hash,
                 data_json = excluded.data_json, fetched_at = excluded.fetched_at",
        )?;
        for h in &huds {
            stmt.execute(params![
                h.id,
                h.name,
                h.author,
                h.repo,
                h.hash,
                h.data_json,
                start
            ])?;
        }
        Ok(())
    })?;
    progress(SyncProgress {
        fetched,
        total: fetched,
    });
    Ok(SyncOutcome {
        fresh: false,
        fetched,
        rows,
        rate_limited: 0,
    })
}

/// Syncs the addon store from mastercomfig's latest GitHub release.
pub(crate) async fn addons(
    client: &StoreClient,
    force: bool,
    progress: &(dyn Fn(SyncProgress) + Send + Sync),
) -> Result<SyncOutcome, SyncError> {
    let kind = StoreKind::Addons;
    if !force && !stale(client, kind)? {
        return Ok(SyncOutcome::skipped());
    }
    let _guard = client.download_lock.lock().await;
    if !force && !stale(client, kind)? {
        return Ok(SyncOutcome::skipped());
    }
    let start = timestamp::now();

    let url = format!(
        "{}/repos/mastercomfig/mastercomfig/releases/latest",
        client.endpoints.github_api()
    );
    // GitHub answers 403 once the unauthenticated hourly limit is spent.
    let body = match client.http.get_api(&url, API_BODY_LIMIT).await {
        Err(NetFailure::Status { url, status: 403 }) => {
            return Err(SyncError::RateLimited { url });
        }
        other => other?,
    };
    let release: Map<String, Value> =
        serde_json::from_slice(&body).map_err(|e| malformed(url.clone(), &e.to_string()))?;
    let tag = release
        .get("tag_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    // The base and override VPKs are mastercomfig itself, not addons: their URLs go to
    // the recorded release and the overrides stay out of the store.
    let raw_assets: Vec<&Map<String, Value>> = release
        .get("assets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .collect();
    let mut base_url = None;
    let mut override_urls: [Option<&str>; 3] = [None; 3];
    for asset in &raw_assets {
        let (Some(name), Some(url)) = (
            asset.get("name").and_then(Value::as_str),
            asset.get("browser_download_url").and_then(Value::as_str),
        ) else {
            continue;
        };
        if name == BASE_ASSET {
            base_url = Some(url);
        } else if let Some(slot) = override_slot(name) {
            override_urls[slot] = Some(url);
        }
    }
    let assets: Vec<AddonAsset> = raw_assets
        .iter()
        .filter_map(|asset| {
            let name = AddonName::new(asset.get("name")?.as_str()?)?;
            Some(AddonAsset {
                name: (!is_override(&name)).then_some(name)?,
                url: asset.get("browser_download_url")?.as_str()?.to_owned(),
                size: asset.get("size").and_then(Value::as_i64).unwrap_or(0),
                downloads: asset
                    .get("download_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            })
        })
        .collect();

    let fetched = u32::try_from(assets.len()).unwrap_or(u32::MAX);
    let rows = replace(client, kind, &start, |conn| {
        let mut stmt = conn.prepare(
            "INSERT INTO store_addons (asset_name, display_name, description, download_url,
                 file_size, download_count, release_tag, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(asset_name) DO UPDATE SET display_name = excluded.display_name,
                 description = excluded.description, download_url = excluded.download_url,
                 file_size = excluded.file_size, download_count = excluded.download_count,
                 release_tag = excluded.release_tag, fetched_at = excluded.fetched_at",
        )?;
        for a in &assets {
            let (display, description) = display_info(&a.name);
            stmt.execute(params![
                a.name.as_str(),
                display,
                description,
                a.url,
                a.size,
                a.downloads,
                tag,
                start
            ])?;
        }
        if let Some(base_url) = base_url {
            let overrides = serde_json::to_string(&override_urls).unwrap_or_default();
            conn.execute(
                "INSERT OR REPLACE INTO mastercomfig_release (id, tag, base_url, override_urls)
                 VALUES (1, ?1, ?2, ?3)",
                params![tag, base_url, overrides],
            )?;
        }
        Ok(())
    })?;
    progress(SyncProgress {
        fetched,
        total: fetched,
    });
    Ok(SyncOutcome {
        fresh: false,
        fetched,
        rows,
        rate_limited: 0,
    })
}

/// In one transaction: `upsert` writes this sync's rows stamped `start`, rows older than
/// `start` are dropped and `{key}_last_synced` is set. Returns the table's row count.
fn replace(
    client: &StoreClient,
    kind: StoreKind,
    start: &str,
    upsert: impl FnOnce(&Connection) -> rusqlite::Result<()>,
) -> Result<u64, SyncError> {
    client
        .db
        .write(|conn| {
            let tx = conn.transaction()?;
            upsert(&tx)?;
            let rows = finish(&tx, kind, start)?;
            tx.commit()?;
            Ok(rows)
        })
        .map_err(database)
}

/// Extracts the archive in a scratch dir under `tmp` (removed on every path) and reads
/// each `hud-data/*.json` in path order, skipping files that are not JSON objects.
/// None when the archive has no `hud-data` directory.
fn read_hud_db(tmp: &Path, bytes: &[u8]) -> Result<Option<Vec<CuratedHud>>, SyncError> {
    std::fs::create_dir_all(tmp).map_err(|e| io(tmp, e))?;
    let scratch = tempfile::Builder::new()
        .prefix("hud-db-")
        .tempdir_in(tmp)
        .map_err(|e| io(tmp, e))?;
    let archive = scratch.path().join("hud-db.zip");
    std::fs::write(&archive, bytes).map_err(|e| io(&archive, e))?;
    let dest = scratch.path().join("extracted");
    std::fs::create_dir(&dest).map_err(|e| io(&dest, e))?;
    extract_archive(&archive, &dest)?;

    // The real archive's root has many entries, so the content root cannot be trusted
    // to land on it; search a few levels down instead.
    let Some(data_dir) = WalkDir::new(scratch.path())
        .min_depth(1)
        .max_depth(3)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .find(|e| e.file_type().is_dir() && e.file_name() == "hud-data")
        .map(walkdir::DirEntry::into_path)
    else {
        return Ok(None);
    };

    let mut files: Vec<PathBuf> = std::fs::read_dir(&data_dir)
        .map_err(|e| io(&data_dir, e))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();

    let mut huds = Vec::with_capacity(files.len());
    for path in files {
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).map_err(|e| io(&path, e))?;
        let fields = match serde_json::from_str::<Map<String, Value>>(&text) {
            Ok(fields) => fields,
            Err(error) => {
                tracing::warn!(file = %path.display(), %error, "skipping unreadable hud-db entry");
                continue;
            }
        };
        let text_field = |key: &str| fields.get(key).and_then(Value::as_str).map(str::to_owned);
        huds.push(CuratedHud {
            id: id.to_owned(),
            name: text_field("name").unwrap_or_else(|| id.to_owned()),
            author: text_field("author").unwrap_or_else(|| "Unknown".to_owned()),
            repo: text_field("repo"),
            hash: text_field("hash"),
            data_json: text,
        });
    }
    Ok(Some(huds))
}

fn io(path: &Path, source: std::io::Error) -> SyncError {
    SyncError::Io {
        path: path.to_path_buf(),
        source,
    }
}
