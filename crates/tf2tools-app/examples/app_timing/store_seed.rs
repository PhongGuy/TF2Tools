//! Store rows at the real catalogue counts for timing runs, every image under a loopback fake.
//!
//! The sound rows are shared with `fake_gb`, so a forced sync against the fake rewrites each
//! seeded row with the values it already holds.

use std::io::Cursor;

use sha2::{Digest, Sha256};
use tf2tools_core::{DataDir, Database};

use crate::runs::stamp_synced;

/// The loopback port the fake GameBanana serves on, so one seed serves warm and cold runs.
pub const FAKE_PORT: u16 = 47613;

/// The fake server's base URL on `port`.
pub fn fake_base(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// How many rows each store table gets, and whether the image cache is pre-filled.
#[derive(Debug, Clone, Copy)]
pub struct StoreSeed {
    pub sounds: usize,
    pub gb_huds: usize,
    pub curated: usize,
    pub crosshairs: usize,
    pub addons: usize,
    pub warm_images: bool,
}

impl StoreSeed {
    /// The live catalogue's counts, measured on a synced v5 library.
    pub const REAL: StoreSeed = StoreSeed {
        sounds: 19_716,
        gb_huds: 551,
        curated: 304,
        crosshairs: 88,
        addons: 11,
        warm_images: false,
    };
}

/// One store sound exactly as core stores it, so the seed and the fake's pages agree.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundRow {
    pub gb_id: i64,
    pub name: String,
    pub author: String,
    pub category: &'static str,
    pub preview_url: String,
    pub thumbnail_url: String,
    pub like_count: i64,
    pub view_count: i64,
    pub tags_json: String,
    pub profile_url: String,
    pub date_added: i64,
}

/// Sound `gb_id` under `base`: categories alternate so the pills filter, likes and views are
/// scattered so the sorts differ, and the date falls as the id rises (id 1 is newest).
pub fn sound_row(base: &str, gb_id: i64) -> SoundRow {
    SoundRow {
        gb_id,
        name: format!("Sound {gb_id:05}"),
        author: format!("author{:02}", gb_id % 50),
        category: if gb_id % 2 == 0 {
            "hitsound"
        } else {
            "killsound"
        },
        preview_url: format!("{base}/preview/{gb_id}.mp3"),
        thumbnail_url: image_url(base, "sounds", &gb_id.to_string()),
        like_count: gb_id * 7919 % 5000,
        view_count: gb_id * 104_729 % 1_000_000,
        tags_json: r#"["Purity: Fun"]"#.into(),
        profile_url: format!("{base}/sounds/{gb_id}"),
        date_added: 1_700_000_000 - gb_id,
    }
}

fn image_url(base: &str, kind: &str, id: &str) -> String {
    format!("{base}/img/{kind}/{id}.jpg")
}

/// A 100x56 JPEG, the size of a GameBanana card thumbnail.
pub fn thumbnail_jpeg() -> Result<Vec<u8>, String> {
    let img = image::RgbImage::from_pixel(100, 56, image::Rgb([200, 120, 40]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg)
        .map_err(|e| format!("encode jpeg: {e}"))?;
    Ok(out.into_inner())
}

/// Writes store rows into the template's database (opened through core so migrations run);
/// every image URL is `{base}/img/<kind>/<id>.jpg` (curated: core's `{base}/img/hud-resources/
/// <hud_id>/<res>.webp`); stamps `{key}_last_synced` now and `sounds_sync_order=newest` so no
/// sync is due. With `warm_images`, writes one 100x56 JPEG per image URL into
/// `dir.image_cache_dir()` (file name: hex SHA-256 of the URL). Returns the image URLs.
pub fn seed_store(dir: &DataDir, seed: &StoreSeed, base: &str) -> Result<Vec<String>, String> {
    drop(Database::open(dir).map_err(|e| format!("open db: {e}"))?);
    let mut conn = rusqlite::Connection::open(dir.db_path()).map_err(sql)?;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false);
    let mut urls = Vec::new();

    // One transaction per table: 19.7k autocommitted inserts would take minutes.
    let tx = conn.transaction().map_err(sql)?;
    for id in 1..=seed.sounds as i64 {
        let r = sound_row(base, id);
        tx.execute(
            "INSERT INTO store_sounds (gb_id, name, author, category, preview_url, thumbnail_url,
                 like_count, view_count, tags_json, profile_url, date_added, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                r.gb_id,
                r.name,
                r.author,
                r.category,
                r.preview_url,
                r.thumbnail_url,
                r.like_count,
                r.view_count,
                r.tags_json,
                r.profile_url,
                r.date_added,
                now
            ],
        )
        .map_err(sql)?;
        urls.push(r.thumbnail_url);
    }
    tx.commit().map_err(sql)?;

    // GameBanana HUDs and crosshairs share a shape; each card's thumbnail is its one gallery image.
    for (table, kind, count, name) in [
        ("store_huds_gb", "huds", seed.gb_huds, "HUD"),
        (
            "store_crosshairs",
            "crosshairs",
            seed.crosshairs,
            "Crosshair",
        ),
    ] {
        let tx = conn.transaction().map_err(sql)?;
        for id in 1..=count as i64 {
            let thumb = image_url(base, kind, &id.to_string());
            let images = serde_json::to_string(&[&thumb]).map_err(|e| e.to_string())?;
            tx.execute(
                &format!(
                    "INSERT INTO {table} (gb_id, name, author, thumbnail_url, images_json,
                         like_count, view_count, tags_json, profile_url, date_added, fetched_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '[\"Minimal\"]', ?8, ?9, ?10)"
                ),
                rusqlite::params![
                    id,
                    format!("{name} {id:03}"),
                    format!("author{:02}", id % 20),
                    thumb,
                    images,
                    id * 7919 % 3000,
                    id * 104_729 % 500_000,
                    format!("{base}/{kind}/{id}"),
                    1_700_000_000 - id,
                    now
                ],
            )
            .map_err(sql)?;
            urls.push(thumb);
        }
        tx.commit().map_err(sql)?;
    }

    // Curated images come from core's hud-resources base, which `Endpoints::loopback` puts
    // under `{base}/img/`; every OS is listed so the default OS filter shows every row.
    let tx = conn.transaction().map_err(sql)?;
    for i in 0..seed.curated {
        let hud_id = format!("hud{i:03}");
        let data_json = serde_json::json!({
            "resources": ["1"],
            "traits": {"os": ["windows", "mac", "linux"], "gamemode": [], "res": []},
        })
        .to_string();
        tx.execute(
            "INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
             VALUES (?1, ?2, ?3, ?4, 'abc123', ?5, ?6)",
            rusqlite::params![
                hud_id,
                format!("Curated {i:03}"),
                format!("author{:02}", i % 20),
                format!("{base}/gh/curated/{hud_id}"),
                data_json,
                now
            ],
        )
        .map_err(sql)?;
        urls.push(format!("{base}/img/hud-resources/{hud_id}/1.webp"));
    }
    tx.commit().map_err(sql)?;

    let tx = conn.transaction().map_err(sql)?;
    for i in 0..seed.addons {
        let asset = format!("addon-{i:02}.vpk");
        tx.execute(
            "INSERT INTO store_addons (asset_name, display_name, description, download_url,
                 file_size, download_count, release_tag, fetched_at)
             VALUES (?1, ?2, 'Utility addon', ?3, 2355, ?4, '9.100.1', ?5)",
            rusqlite::params![
                asset,
                format!("Addon {i:02}"),
                format!("{base}/gh/addons/{asset}"),
                989 + i as i64,
                now
            ],
        )
        .map_err(sql)?;
    }
    tx.commit().map_err(sql)?;

    stamp_synced(&conn, &now).map_err(sql)?;

    if seed.warm_images {
        warm(dir, &urls)?;
    }
    Ok(urls)
}

/// Pre-fills the image cache under the file names core's disk cache looks up.
fn warm(dir: &DataDir, urls: &[String]) -> Result<(), String> {
    let cache = dir.image_cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| format!("image cache: {e}"))?;
    let jpeg = thumbnail_jpeg()?;
    for url in urls {
        let name: String = Sha256::digest(url.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        std::fs::write(cache.join(name), &jpeg).map_err(|e| format!("cache {url}: {e}"))?;
    }
    Ok(())
}

fn sql(e: rusqlite::Error) -> String {
    format!("seed store: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn a_small_seed_writes_each_store_and_warms_its_images() {
        let tmp = tempfile::tempdir().unwrap();
        let seed = StoreSeed {
            sounds: 30,
            gb_huds: 5,
            curated: 3,
            crosshairs: 4,
            addons: 2,
            warm_images: true,
        };
        let dir = tf2tools_core::DataDir::at(tmp.path().to_path_buf());
        let urls = seed_store(&dir, &seed, "http://127.0.0.1:47613").unwrap();
        let conn = Connection::open(dir.db_path()).unwrap();
        assert_eq!(count(&conn, "store_sounds"), 30);
        assert_eq!(count(&conn, "store_huds_gb"), 5);
        assert_eq!(count(&conn, "store_huds"), 3);
        assert_eq!(count(&conn, "store_crosshairs"), 4);
        assert_eq!(count(&conn, "store_addons"), 2);
        assert!(!urls.is_empty());
        assert!(
            urls.iter()
                .all(|u| u.starts_with("http://127.0.0.1:47613/img/"))
        );
        let cached = std::fs::read_dir(dir.image_cache_dir()).unwrap().count();
        assert_eq!(cached, urls.len());
        let order: String = conn
            .query_row(
                "SELECT value FROM store_cache_meta WHERE key = 'sounds_sync_order'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(order, "newest");
    }

    #[test]
    fn a_cold_seed_writes_no_cache_files() {
        let tmp = tempfile::tempdir().unwrap();
        let seed = StoreSeed {
            warm_images: false,
            ..StoreSeed {
                sounds: 5,
                gb_huds: 2,
                curated: 1,
                crosshairs: 1,
                addons: 1,
                warm_images: true,
            }
        };
        let dir = tf2tools_core::DataDir::at(tmp.path().to_path_buf());
        seed_store(&dir, &seed, "http://127.0.0.1:47613").unwrap();
        assert!(std::fs::read_dir(dir.image_cache_dir()).map_or(true, |d| d.count() == 0));
    }
}
