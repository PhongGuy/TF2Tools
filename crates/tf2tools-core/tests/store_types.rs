//! The store's public types: v5's defaults, sharing across tasks, and the per-store sync
//! status the Settings page shows.

use crate::common;

use std::time::Duration;

use common::{sql, temp_data_dir};
use tf2tools_core::{CommunityMap, MapDownloadError, MapEntry, MapRemoveError, MapState};
use tf2tools_core::{
    Database, DownloadError, DownloadPhase, DownloadProgress, Endpoints, FilesError, ImageError,
    PreviewError, QueryError, StoreClient, StoreConfig, StoreFile, StoreItem, StoreItemId,
    StoreKind, StoreSyncStatus, SyncError, SyncOutcome, SyncProgress,
};

fn assert_send_sync<T: Send + Sync + std::fmt::Debug + 'static>() {}

fn assert_send<T: Send>(_: &T) {}

#[test]
fn store_types_are_send_sync_debug() {
    assert_send_sync::<StoreClient>();
    assert_send_sync::<Endpoints>();
    assert_send_sync::<StoreConfig>();
    assert_send_sync::<StoreKind>();
    assert_send_sync::<StoreSyncStatus>();
    assert_send_sync::<SyncError>();
    assert_send_sync::<QueryError>();
    assert_send_sync::<FilesError>();
    assert_send_sync::<DownloadError>();
    assert_send_sync::<PreviewError>();
    assert_send_sync::<ImageError>();
    assert_send_sync::<StoreItem>();
    assert_send_sync::<StoreItemId>();
    assert_send_sync::<StoreFile>();
    assert_send_sync::<SyncProgress>();
    assert_send_sync::<SyncOutcome>();
    assert_send_sync::<DownloadProgress>();
    assert_send_sync::<DownloadPhase>();
    assert_send_sync::<CommunityMap>();
    assert_send_sync::<MapEntry>();
    assert_send_sync::<MapState>();
    assert_send_sync::<MapDownloadError>();
    assert_send_sync::<MapRemoveError>();
}

#[test]
fn item_files_and_preview_futures_are_send() {
    let (_tmp, dir) = temp_data_dir();
    let client = StoreClient::new(
        Database::open(&dir).unwrap(),
        dir.clone(),
        Endpoints::default(),
    );
    sql(&dir)
        .execute(
            "INSERT INTO store_sounds (gb_id, name, fetched_at) VALUES (1, 'a', 'x')",
            [],
        )
        .unwrap();
    let item = client.store_items(StoreKind::Sounds).unwrap().remove(0);
    // Built and dropped unpolled: only the futures' types are under test.
    assert_send(&client.item_files(StoreKind::Sounds, item.id()));
    assert_send(&client.preview_audio(&item));
    assert_send(&client.fetch_image("https://images.gamebanana.com/a.jpg"));
}

#[test]
fn store_kinds_list_all_five_in_order() {
    assert_eq!(
        StoreKind::ALL,
        [
            StoreKind::CuratedHuds,
            StoreKind::GameBananaHuds,
            StoreKind::Sounds,
            StoreKind::Crosshairs,
            StoreKind::Addons,
        ]
    );
}

#[test]
fn config_defaults_are_v5s_and_can_be_raised() {
    const MB: u64 = 1024 * 1024;
    let config = StoreConfig::default();
    assert_eq!(config.page_limit(StoreKind::GameBananaHuds, 800), 500);
    assert_eq!(config.page_limit(StoreKind::Crosshairs, 97), 500);
    assert_eq!(config.page_limit(StoreKind::Sounds, 19718), 1447);
    assert_eq!(config.page_limit(StoreKind::Sounds, 30), 3);
    assert_eq!(config.page_limit(StoreKind::Sounds, 0), 500);
    assert_eq!(config.request_spacing(), Duration::from_millis(200));
    assert_eq!(config.retry_backoff(), Duration::from_secs(2));
    let caps: Vec<u64> = StoreKind::ALL.iter().map(|k| config.size_cap(*k)).collect();
    assert_eq!(caps, [200 * MB, 200 * MB, 100 * MB, 50 * MB, 50 * MB]);
    assert_eq!(
        (config.connect_timeout(), config.read_timeout()),
        (Duration::from_secs(10), Duration::from_secs(30))
    );

    let raised = config
        .with_page_limit(StoreKind::Sounds, 2000)
        .with_request_spacing(Duration::ZERO)
        .with_retry_backoff(Duration::from_millis(5))
        .with_size_cap(StoreKind::Sounds, 10)
        .with_timeouts(Duration::from_secs(1), Duration::from_secs(2));
    assert_eq!(raised.page_limit(StoreKind::Sounds, 19718), 2000);
    assert_eq!(raised.page_limit(StoreKind::Crosshairs, 97), 500);
    assert_eq!(raised.request_spacing(), Duration::ZERO);
    assert_eq!(raised.retry_backoff(), Duration::from_millis(5));
    assert_eq!(raised.size_cap(StoreKind::Sounds), 10);
    assert_eq!(raised.size_cap(StoreKind::Crosshairs), 50 * MB);
    assert_eq!(
        (raised.connect_timeout(), raised.read_timeout()),
        (Duration::from_secs(1), Duration::from_secs(2))
    );
}

#[test]
fn endpoints_default_to_production_and_can_be_redirected() {
    let prod = Endpoints::default();
    assert_eq!(prod.gamebanana(), "https://gamebanana.com/apiv11");
    assert_eq!(prod.github_api(), "https://api.github.com");
    assert_eq!(prod.github(), "https://github.com");
    assert_eq!(
        prod.preview_prefixes(),
        [
            "https://files.gamebanana.com/",
            "https://sounds.gamebanana.com/"
        ]
    );
    assert_eq!(
        prod.image_prefixes(),
        [
            "https://images.gamebanana.com/",
            "https://raw.githubusercontent.com/",
            "https://wiki.teamfortress.com/w/images/"
        ]
    );
    let mock = prod
        .with_gamebanana("http://h/apiv11")
        .with_github_api("http://h/gh-api")
        .with_github("http://h/gh")
        .with_preview_prefixes(["http://h/preview/"])
        .with_image_prefixes(["http://h/img/"]);
    assert_eq!(
        (mock.gamebanana(), mock.github_api(), mock.github()),
        ("http://h/apiv11", "http://h/gh-api", "http://h/gh")
    );
    assert_eq!(mock.preview_prefixes(), ["http://h/preview/"]);
    assert_eq!(mock.image_prefixes(), ["http://h/img/"]);
}

#[test]
fn sync_status_on_a_fresh_database_is_empty() {
    let (_tmp, dir) = temp_data_dir();
    let client = StoreClient::new(Database::open(&dir).unwrap(), dir, Endpoints::default());
    let status = client.sync_status().unwrap();
    let kinds: Vec<StoreKind> = status.iter().map(|s| s.kind()).collect();
    assert_eq!(kinds, StoreKind::ALL);
    assert!(
        status
            .iter()
            .all(|s| s.last_synced().is_none() && s.item_count() == 0)
    );
}

#[test]
fn sync_status_reports_every_store_in_order() {
    let (_tmp, dir) = temp_data_dir();
    let db = Database::open(&dir).unwrap();
    sql(&dir)
        .execute_batch(
            "INSERT INTO store_cache_meta (key, value) VALUES
                ('sounds_last_synced', '2026-07-04T17:57:10.373011226+00:00'),
                ('huds_gb_last_synced', '2026-07-04T17:45:57.430Z'),
                ('sounds_sync_page', '12');
             INSERT INTO store_sounds (gb_id, name, fetched_at) VALUES (1, 'a', 'x'), (2, 'b', 'x');
             INSERT INTO store_addons (asset_name, display_name, download_url, release_tag, fetched_at)
                VALUES ('mastercomfig-addon-a.vpk', 'A', 'u', '1', 'x');",
        )
        .unwrap();
    let client = StoreClient::new(db, dir.clone(), Endpoints::default());
    let status = client.sync_status().unwrap();
    let rows: Vec<(StoreKind, Option<&str>, u64)> = status
        .iter()
        .map(|s| (s.kind(), s.last_synced(), s.item_count()))
        .collect();
    assert_eq!(
        rows,
        [
            (StoreKind::CuratedHuds, None, 0),
            (
                StoreKind::GameBananaHuds,
                Some("2026-07-04T17:45:57.430Z"),
                0
            ),
            (
                StoreKind::Sounds,
                Some("2026-07-04T17:57:10.373011226+00:00"),
                2
            ),
            (StoreKind::Crosshairs, None, 0),
            (StoreKind::Addons, None, 1),
        ]
    );
}
