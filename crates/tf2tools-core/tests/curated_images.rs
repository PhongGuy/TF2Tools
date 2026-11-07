//! Curated (hud-db) store items carry their screenshot URLs.

use crate::common;

use common::{sql, temp_data_dir};
use tf2tools_core::{Database, Endpoints, StoreClient, StoreItemId, StoreKind};

const BASE: &str = "https://raw.githubusercontent.com/mastercomfig/hud-db/main/hud-resources";

const ROWS: &str = r#"
    INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at) VALUES
        ('rayshud', 'rayshud', 'raysfire', NULL, NULL, '{"name":"rayshud","resources":["a1b2","c3d4"]}', 'x'),
        ('plain', 'plain', 'nobody', NULL, NULL, '{}', 'x'),
        ('broken', 'broken', 'nobody', NULL, NULL, 'not json', 'x');
"#;

fn client() -> (tempfile::TempDir, StoreClient) {
    let (tmp, dir) = temp_data_dir();
    let client = StoreClient::new(
        Database::open(&dir).unwrap(),
        dir.clone(),
        Endpoints::default(),
    );
    sql(&dir).execute_batch(ROWS).unwrap();
    (tmp, client)
}

#[test]
fn curated_items_list_their_hud_db_resources_as_images() {
    let (_tmp, client) = client();
    let items = client.store_items(StoreKind::CuratedHuds).unwrap();
    let by_id = |id: &str| {
        items
            .iter()
            .find(|i| *i.id() == StoreItemId::Curated(id.to_owned()))
            .unwrap()
    };
    let first = format!("{BASE}/rayshud/a1b2.webp");
    assert_eq!(
        by_id("rayshud").images(),
        [first.clone(), format!("{BASE}/rayshud/c3d4.webp")]
    );
    assert_eq!(by_id("rayshud").thumbnail_url(), Some(first.as_str()));
    for id in ["plain", "broken"] {
        assert!(by_id(id).images().is_empty(), "{id}");
        assert_eq!(by_id(id).thumbnail_url(), None, "{id}");
    }
}

#[test]
fn one_curated_item_carries_its_images_too() {
    let (_tmp, client) = client();
    let item = client
        .store_item(
            StoreKind::CuratedHuds,
            &StoreItemId::Curated("rayshud".into()),
        )
        .unwrap()
        .unwrap();
    assert_eq!(item.images().len(), 2);
}

#[test]
fn curated_image_paths_are_percent_encoded_with_slashes_kept() {
    let (_tmp, dir) = temp_data_dir();
    let client = StoreClient::new(
        Database::open(&dir).unwrap(),
        dir.clone(),
        Endpoints::default(),
    );
    sql(&dir)
        .execute_batch(
            r#"INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at) VALUES
                ('my hud', 'my hud', 'x', NULL, NULL, '{"resources":["shot one","sub/a b","q?x#y%z","plain-1_x.y~"]}', 'x');"#,
        )
        .unwrap();
    let item = client
        .store_item(
            StoreKind::CuratedHuds,
            &StoreItemId::Curated("my hud".into()),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        item.images(),
        [
            format!("{BASE}/my%20hud/shot%20one.webp"),
            format!("{BASE}/my%20hud/sub/a%20b.webp"),
            format!("{BASE}/my%20hud/q%3Fx%23y%25z.webp"),
            format!("{BASE}/my%20hud/plain-1_x.y~.webp"),
        ]
    );
}
