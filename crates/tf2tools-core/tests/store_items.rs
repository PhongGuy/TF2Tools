//! Reading whole store tables back for the store grids.

use crate::common;

use common::{sql, store_env};
use tf2tools_core::{AddonName, GbId, StoreItemId, StoreKind};

const SEED: &str = r#"
    INSERT INTO store_sounds (gb_id, name, author, category, preview_url, thumbnail_url,
        like_count, view_count, tags_json, profile_url, date_added, fetched_at)
    VALUES (93300, 'Jeremy', 'legend', 'hitsound', 'https://files.gamebanana.com/preview/sounds/j.mp3',
        'https://images.gamebanana.com/img/av/a.gif', 3, 61, '["a","b"]',
        'https://gamebanana.com/sounds/93300', 1789967930, '2026-10-16T05:00:00.000000+00:00');
    INSERT INTO store_crosshairs (gb_id, name, author, thumbnail_url, images_json, like_count,
        view_count, tags_json, profile_url, date_added, fetched_at)
    VALUES (456577, 'Kunai', 'Vexcenot', 'https://i.example/100.jpg', '["https://i.example/530.jpg"]',
        77, 99452, '[]', 'https://gamebanana.com/mods/456577', 1689567250, '2026-10-16T05:00:00.000000+00:00');
    INSERT INTO store_huds_gb (gb_id, name, author, thumbnail_url, images_json, tags_json, fetched_at)
    VALUES (463048, 'DarHUD', 'DarSitam', NULL, 'not json', 'not json', '2026-10-16T05:00:00.000000+00:00');
    INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at) VALUES
        ('rayshud', 'rayshud', 'raysfire', 'https://github.com/raysfire/rayshud', 'a22e',
         '{"name":"rayshud"}', '2026-10-16T05:00:00.000000+00:00'),
        ('budhud', 'budhud', 'Whisker', NULL, NULL, '{}', '2026-10-16T05:00:00.000000+00:00');
    INSERT INTO store_addons (asset_name, display_name, description, download_url, file_size,
        download_count, release_tag, fetched_at) VALUES
        ('mastercomfig-addon-flat-mouse.vpk', 'Flat Mouse', 'Raw input', 'https://github.com/x/flat.vpk',
         299, 40731, '9.100.1', '2026-10-16T05:00:00.000000+00:00'),
        ('../evil.vpk', 'Evil', '', 'https://x.example/e.vpk', 1, 1, '1', '2026-10-16T05:00:00.000000+00:00');
"#;

#[tokio::test]
async fn sounds_read_back_every_column() {
    let env = store_env().await;
    sql(&env.dir).execute_batch(SEED).unwrap();
    let items = env.client.store_items(StoreKind::Sounds).unwrap();
    let [item] = items.as_slice() else {
        panic!("{items:?}")
    };
    assert_eq!(item.id(), &StoreItemId::GameBanana(GbId::new(93300)));
    assert_eq!(item.kind(), StoreKind::Sounds);
    assert_eq!((item.name(), item.author()), ("Jeremy", "legend"));
    assert_eq!(item.category(), Some("hitsound"));
    assert_eq!(
        item.preview_url(),
        Some("https://files.gamebanana.com/preview/sounds/j.mp3")
    );
    assert_eq!(
        item.thumbnail_url(),
        Some("https://images.gamebanana.com/img/av/a.gif")
    );
    assert_eq!(item.tags(), ["a", "b"]);
    assert!(item.images().is_empty());
    assert_eq!(
        (item.like_count(), item.view_count(), item.date_added()),
        (3, 61, 1789967930)
    );
    assert_eq!(item.profile_url(), "https://gamebanana.com/sounds/93300");
    assert_eq!(item.fetched_at(), "2026-10-16T05:00:00.000000+00:00");
    assert_eq!(
        (item.repo(), item.hash(), item.data_json()),
        (None, None, None)
    );
    assert_eq!(item.release_tag(), None);
}

#[tokio::test]
async fn gamebanana_mods_read_images_and_tolerate_bad_json() {
    let env = store_env().await;
    sql(&env.dir).execute_batch(SEED).unwrap();
    let crosshairs = env.client.store_items(StoreKind::Crosshairs).unwrap();
    let [cross] = crosshairs.as_slice() else {
        panic!("{crosshairs:?}")
    };
    assert_eq!(cross.id(), &StoreItemId::GameBanana(GbId::new(456577)));
    assert_eq!(cross.images(), ["https://i.example/530.jpg"]);
    assert_eq!(cross.thumbnail_url(), Some("https://i.example/100.jpg"));
    assert_eq!((cross.category(), cross.preview_url()), (None, None));
    assert!(cross.tags().is_empty());

    let huds = env.client.store_items(StoreKind::GameBananaHuds).unwrap();
    let [hud] = huds.as_slice() else {
        panic!("{huds:?}")
    };
    assert_eq!((hud.name(), hud.author()), ("DarHUD", "DarSitam"));
    assert!(hud.images().is_empty() && hud.tags().is_empty());
    assert_eq!((hud.thumbnail_url(), hud.like_count()), (None, 0));
}

#[tokio::test]
async fn curated_huds_read_back_sorted_by_name() {
    let env = store_env().await;
    sql(&env.dir).execute_batch(SEED).unwrap();
    let items = env.client.store_items(StoreKind::CuratedHuds).unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.name()).collect();
    assert_eq!(names, ["budhud", "rayshud"]);
    let rays = &items[1];
    assert_eq!(rays.id(), &StoreItemId::Curated("rayshud".to_owned()));
    assert_eq!(rays.author(), "raysfire");
    assert_eq!(rays.repo(), Some("https://github.com/raysfire/rayshud"));
    assert_eq!(rays.hash(), Some("a22e"));
    assert_eq!(rays.data_json(), Some(r#"{"name":"rayshud"}"#));
    assert_eq!((items[0].repo(), items[0].hash()), (None, None));
}

#[tokio::test]
async fn addons_read_back_and_unsafe_names_are_left_out() {
    let env = store_env().await;
    sql(&env.dir).execute_batch(SEED).unwrap();
    let items = env.client.store_items(StoreKind::Addons).unwrap();
    let [addon] = items.as_slice() else {
        panic!("{items:?}")
    };
    let name = AddonName::new("mastercomfig-addon-flat-mouse.vpk").unwrap();
    assert_eq!(addon.id(), &StoreItemId::Addon(name));
    assert_eq!(
        (addon.name(), addon.author(), addon.description()),
        ("Flat Mouse", "", "Raw input")
    );
    assert_eq!((addon.file_size(), addon.download_count()), (299, 40731));
    assert_eq!(addon.release_tag(), Some("9.100.1"));
}

#[tokio::test]
async fn an_empty_store_lists_nothing() {
    let env = store_env().await;
    for kind in StoreKind::ALL {
        assert!(env.client.store_items(kind).unwrap().is_empty(), "{kind:?}");
    }
}
