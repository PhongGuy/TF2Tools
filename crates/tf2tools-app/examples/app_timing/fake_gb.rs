//! A loopback stand-in for GameBanana: the sound index pages and slow card images.

use std::time::Duration;

use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use crate::store_seed::{SoundRow, fake_base, sound_row, thumbnail_jpeg};

/// Core's GameBanana index page size.
const PAGE_SIZE: u32 = 15;

/// A wiremock server on its own runtime, serving only on 127.0.0.1.
pub struct FakeGb {
    server: MockServer,
    rt: tokio::runtime::Runtime,
}

impl FakeGb {
    /// Serves on 127.0.0.1:`port` (0 picks one): `GET /apiv11/Sound/Index?_nPage=N` returns
    /// page N of `total` sounds newest first, 15 per page, gb_id = (N - 1) * 15 + i + 1 (the
    /// seed's own ids, whose dates also run newest first from id 1), so a sync upserts in place;
    /// `GET /img/*` returns one 100x56 JPEG after `image_delay`.
    pub fn start(port: u16, total: u32, image_delay: Duration) -> Result<FakeGb, String> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| format!("fake runtime: {e}"))?;
        let listener = std::net::TcpListener::bind(("127.0.0.1", port))
            .map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
        let bound = listener.local_addr().map_err(|e| e.to_string())?.port();
        let base = fake_base(bound);
        let jpeg = thumbnail_jpeg()?;
        let server = rt.block_on(async {
            let server = MockServer::builder().listener(listener).start().await;
            Mock::given(method("GET"))
                .and(path("/apiv11/Sound/Index"))
                .respond_with(move |req: &Request| {
                    let page = req
                        .url
                        .query_pairs()
                        .find(|(k, _)| k == "_nPage")
                        .and_then(|(_, v)| v.parse().ok())
                        .unwrap_or(1);
                    ResponseTemplate::new(200).set_body_json(sound_page(&base, total, page))
                })
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path_regex("^/img/"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "image/jpeg")
                        .set_body_bytes(jpeg)
                        .set_delay(image_delay),
                )
                .mount(&server)
                .await;
            server
        });
        Ok(FakeGb { server, rt })
    }

    pub fn base(&self) -> String {
        self.server.uri()
    }

    /// Every request the fake has received so far.
    pub fn requests(&self) -> usize {
        self.rt
            .block_on(self.server.received_requests())
            .map_or(0, |r| r.len())
    }

    /// The requests so far for a card image, `GET /img/*`.
    pub fn image_requests(&self) -> usize {
        self.rt
            .block_on(self.server.received_requests())
            .map_or(0, |r| {
                r.iter()
                    .filter(|req| req.url.path().starts_with("/img/"))
                    .count()
            })
    }
}

/// Index page `page` (from 1) as core parses it. Every page up to the last is full; a page past
/// the end is empty and complete, never an empty page before the end, which core reads as malformed.
fn sound_page(base: &str, total: u32, page: u32) -> Value {
    let first = page.saturating_sub(1).saturating_mul(PAGE_SIZE);
    let last = page.saturating_mul(PAGE_SIZE).min(total);
    let records: Vec<Value> = (first + 1..=last)
        .map(|id| sound_record(&sound_row(base, i64::from(id))))
        .collect();
    json!({
        "_aMetadata": {
            "_nRecordCount": total,
            "_bIsComplete": last >= total,
            "_nPerpage": PAGE_SIZE,
        },
        "_aRecords": records,
    })
}

/// A Sound record carrying every field core's upsert writes, so a sync leaves the seed unchanged.
fn sound_record(r: &SoundRow) -> Value {
    let root = if r.category == "hitsound" {
        "Hitsounds"
    } else {
        "Killsounds"
    };
    let tags: Value = serde_json::from_str(&r.tags_json).unwrap_or_else(|_| json!([]));
    json!({
        "_idRow": r.gb_id,
        "_sModelName": "Sound",
        "_sName": r.name,
        "_sProfileUrl": r.profile_url,
        "_tsDateAdded": r.date_added,
        "_aTags": tags,
        "_aPreviewMedia": {"_aMetadata": {"_sAudioUrl": r.preview_url}},
        "_aSubmitter": {"_sName": r.author, "_sAvatarUrl": r.thumbnail_url},
        "_aRootCategory": {"_sName": root},
        "_nLikeCount": r.like_count,
        "_nViewCount": r.view_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tf2tools_core::{DataDir, Database, Endpoints, StoreClient, StoreConfig, StoreKind};

    fn offline_client(dir: &DataDir, base: &str, pages: u32) -> StoreClient {
        StoreClient::new(
            Database::open(dir).unwrap(),
            dir.clone(),
            Endpoints::loopback(base),
        )
        .with_config(
            StoreConfig::default()
                .with_loopback_only(true)
                .with_request_spacing(Duration::ZERO)
                .with_page_limit(StoreKind::Sounds, pages),
        )
    }

    #[test]
    fn core_syncs_sounds_from_the_fake() {
        let fake = FakeGb::start(0, 19_716, Duration::ZERO).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::at(tmp.path().join("data"));
        let client = offline_client(&dir, &fake.base(), 3);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(client.sync(StoreKind::Sounds, true, &|_| {}))
            .unwrap();
        let conn = rusqlite::Connection::open(dir.db_path()).unwrap();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM store_sounds", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 45);
        assert!(fake.requests() >= 3);
    }

    #[test]
    fn images_arrive_after_the_delay_and_decode() {
        let fake = FakeGb::start(0, 15, Duration::from_millis(150)).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::at(tmp.path().join("data"));
        let client = offline_client(&dir, &fake.base(), 1);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let started = Instant::now();
        let bytes = rt
            .block_on(client.fetch_image(&format!("{}/img/sounds/7.jpg", fake.base())))
            .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(150));
        assert_eq!(fake.image_requests(), 1);
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!((img.width(), img.height()), (100, 56));
    }

    #[test]
    fn a_full_sync_leaves_the_seeded_sounds_unchanged() {
        let fake = FakeGb::start(0, 30, Duration::ZERO).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let seed = crate::store_seed::StoreSeed {
            sounds: 30,
            gb_huds: 0,
            curated: 0,
            crosshairs: 0,
            addons: 0,
            warm_images: false,
        };
        let dir = DataDir::at(tmp.path().to_path_buf());
        crate::store_seed::seed_store(&dir, &seed, &fake.base()).unwrap();
        let rows = || {
            let conn = rusqlite::Connection::open(dir.db_path()).unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT gb_id, name, author, category, preview_url, thumbnail_url, like_count,
                         view_count, tags_json, profile_url, date_added FROM store_sounds ORDER BY gb_id",
                )
                .unwrap();
            let all: Vec<String> = stmt
                .query_map([], |r| {
                    Ok((0..11)
                        .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                        .collect::<Vec<_>>()
                        .join("|"))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            all
        };
        let before = rows();
        let client = offline_client(&dir, &fake.base(), 3);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(client.sync(StoreKind::Sounds, true, &|_| {}))
            .unwrap();
        assert_eq!(rows(), before);
    }
}
