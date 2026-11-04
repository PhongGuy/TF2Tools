//! Paged store reads: sorts, keyset cursors, search, category, counts and single items.

mod common;

use common::{sql, temp_data_dir};
use tempfile::TempDir;
use tf2tools_core::{
    Database, Endpoints, GbId, QueryError, StoreCard, StoreClient, StoreItemId, StoreKind,
    StoreQuery, StoreSort,
};

const SEED: &str = r#"
    INSERT INTO store_sounds (gb_id, name, author, category, thumbnail_url, like_count,
        view_count, tags_json, date_added, fetched_at) VALUES
        (1, 'Alpha Ding', 'ann', 'hitsound', 'https://images.gamebanana.com/img/av/1.png', 50, 10, '["Purity: Fun"]', 1000, 'x'),
        (2, 'Beta Boom', 'bob', 'killsound', NULL, 50, 30, '[]', 3000, 'x'),
        (3, 'Gamma', 'Cara_x', 'sound', NULL, 70, 20, 'not json', 2000, 'x'),
        (4, 'Delta 100%', 'dan', 'hitsound', NULL, 10, 40, '[]', 4000, 'x'),
        (5, 'Epsilon', 'eve', 'sound', NULL, 0, 0, '[]', 5000, 'x');
    INSERT INTO library_sounds (id, folder_name, name, original_filename, source, store_gb_id,
        sound_type, created_at) VALUES ('s2', 'beta', 'Beta', 'b.wav', 'store', 2, 'wav', 'x');
    INSERT INTO store_crosshairs (gb_id, name, author, thumbnail_url, like_count, view_count,
        date_added, fetched_at) VALUES
        (10, 'Dot', 'vex', 'https://images.gamebanana.com/img/ss/mods/100-90_a.jpg', 5, 1, 10, 'x'),
        (11, 'Cross', 'vex', NULL, 9, 2, 20, 'x');
    INSERT INTO library_crosshairs (id, folder_name, name, source_type, source, store_gb_id,
        created_at) VALUES ('c10', 'dot', 'Dot', 'vtf', 'store', 10, 'x');
    INSERT INTO store_huds_gb (gb_id, name, author, like_count, view_count, date_added, fetched_at)
        VALUES (20, 'DarHUD', 'DarSitam', 445, 9, 1, 'x');
    INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at) VALUES
        ('rayshud', 'rayshud', 'raysfire', 'https://github.com/raysfire/rayshud', 'a22e', '{}', 'x');
"#;

fn env() -> (TempDir, StoreClient) {
    let (tmp, dir) = temp_data_dir();
    let client = StoreClient::new(
        Database::open(&dir).unwrap(),
        dir.clone(),
        Endpoints::default(),
    );
    sql(&dir).execute_batch(SEED).unwrap();
    (tmp, client)
}

fn ids(cards: &[StoreCard]) -> Vec<u64> {
    cards
        .iter()
        .map(|c| match &c.id {
            StoreItemId::GameBanana(id) => id.get(),
            other => panic!("{other:?}"),
        })
        .collect()
}

fn sounds(client: &StoreClient, q: &StoreQuery) -> Vec<u64> {
    ids(&client.store_cards(StoreKind::Sounds, q).unwrap())
}

#[test]
fn each_sort_orders_by_its_key_then_newest_id() {
    let (_tmp, client) = env();
    assert_eq!(
        sounds(&client, &StoreQuery::new(StoreSort::MostLiked, 60)),
        [3, 2, 1, 4, 5]
    );
    assert_eq!(
        sounds(&client, &StoreQuery::new(StoreSort::MostViewed, 60)),
        [4, 2, 3, 1, 5]
    );
    assert_eq!(
        sounds(&client, &StoreQuery::new(StoreSort::Newest, 60)),
        [5, 4, 2, 3, 1]
    );
}

#[test]
fn keyset_pages_continue_after_the_last_card_through_ties() {
    let (_tmp, client) = env();
    for (sort, expected) in [
        (
            StoreSort::MostLiked,
            vec![vec![3, 2], vec![1, 4], vec![5], vec![]],
        ),
        (
            StoreSort::Newest,
            vec![vec![5, 4], vec![2, 3], vec![1], vec![]],
        ),
    ] {
        let mut q = StoreQuery::new(sort, 2);
        for page in expected {
            let cards = client.store_cards(StoreKind::Sounds, &q).unwrap();
            assert_eq!(ids(&cards), page, "{sort:?}");
            if let Some(last) = cards.last() {
                q = q.clone().with_after(last.cursor.clone());
            }
        }
    }
}

#[test]
fn search_matches_name_or_author_literally_and_ignores_case() {
    let (_tmp, client) = env();
    let q = |text: &str| StoreQuery::new(StoreSort::MostLiked, 60).with_search(text);
    assert_eq!(sounds(&client, &q("ding")), [1]);
    assert_eq!(sounds(&client, &q("BOB")), [2]);
    assert_eq!(sounds(&client, &q("_")), [3]);
    assert_eq!(sounds(&client, &q("100%")), [4]);
    assert_eq!(sounds(&client, &q("  ")), [3, 2, 1, 4, 5]);
}

#[test]
fn category_filters_sounds_and_keeps_the_sort() {
    let (_tmp, client) = env();
    let liked = StoreQuery::new(StoreSort::MostLiked, 60).with_category("hitsound");
    assert_eq!(sounds(&client, &liked), [1, 4]);
    let viewed = StoreQuery::new(StoreSort::MostViewed, 60).with_category("hitsound");
    assert_eq!(sounds(&client, &viewed), [4, 1]);
}

#[test]
fn count_matches_the_filters_and_ignores_the_cursor() {
    let (_tmp, client) = env();
    let all = StoreQuery::new(StoreSort::MostLiked, 2);
    assert_eq!(client.store_count(StoreKind::Sounds, &all).unwrap(), 5);
    let first = client.store_cards(StoreKind::Sounds, &all).unwrap();
    let after = all.clone().with_after(first[1].cursor.clone());
    assert_eq!(client.store_count(StoreKind::Sounds, &after).unwrap(), 5);
    let hits = all.clone().with_category("hitsound");
    assert_eq!(client.store_count(StoreKind::Sounds, &hits).unwrap(), 2);
    assert_eq!(
        client
            .store_count(StoreKind::Sounds, &all.with_search("_"))
            .unwrap(),
        1
    );
}

#[test]
fn cards_carry_their_columns_and_library_state() {
    let (_tmp, client) = env();
    let cards = client
        .store_cards(
            StoreKind::Sounds,
            &StoreQuery::new(StoreSort::MostLiked, 60),
        )
        .unwrap();
    let alpha = cards.iter().find(|c| c.name == "Alpha Ding").unwrap();
    assert_eq!(alpha.author, "ann");
    assert_eq!(
        alpha.thumbnail_url.as_deref(),
        Some("https://images.gamebanana.com/img/av/1.png")
    );
    assert_eq!(alpha.tags, ["Purity: Fun"]);
    assert_eq!(
        (alpha.like_count, alpha.view_count, alpha.date_added),
        (50, 10, 1000)
    );
    assert!(!alpha.in_library);
    assert!(
        cards
            .iter()
            .find(|c| c.name == "Beta Boom")
            .unwrap()
            .in_library
    );
    assert!(
        cards
            .iter()
            .find(|c| c.name == "Gamma")
            .unwrap()
            .tags
            .is_empty()
    );

    let q = StoreQuery::new(StoreSort::MostLiked, 60);
    let crosshairs = client.store_cards(StoreKind::Crosshairs, &q).unwrap();
    assert_eq!(ids(&crosshairs), [11, 10]);
    assert!(crosshairs[1].in_library && !crosshairs[0].in_library);
    assert_eq!(
        ids(&client.store_cards(StoreKind::GameBananaHuds, &q).unwrap()),
        [20]
    );
}

#[test]
fn curated_huds_and_addons_are_not_paged() {
    let (_tmp, client) = env();
    let q = StoreQuery::new(StoreSort::MostLiked, 60);
    let cards = client.store_cards(StoreKind::CuratedHuds, &q);
    assert!(
        matches!(
            cards,
            Err(QueryError::NotPaged {
                kind: StoreKind::CuratedHuds
            })
        ),
        "{cards:?}"
    );
    let count = client.store_count(StoreKind::Addons, &q);
    assert!(
        matches!(
            count,
            Err(QueryError::NotPaged {
                kind: StoreKind::Addons
            })
        ),
        "{count:?}"
    );
}

#[test]
fn store_item_reads_one_row_of_the_right_store() {
    let (_tmp, client) = env();
    let gb = |id| StoreItemId::GameBanana(GbId::new(id));
    let gamma = client
        .store_item(StoreKind::Sounds, &gb(3))
        .unwrap()
        .unwrap();
    assert_eq!((gamma.name(), gamma.category()), ("Gamma", Some("sound")));
    assert_eq!(client.store_item(StoreKind::Sounds, &gb(99)).unwrap(), None);
    let cross = client
        .store_item(StoreKind::Crosshairs, &gb(11))
        .unwrap()
        .unwrap();
    assert_eq!(cross.name(), "Cross");
    let curated = StoreItemId::Curated("rayshud".to_owned());
    let rayshud = client
        .store_item(StoreKind::CuratedHuds, &curated)
        .unwrap()
        .unwrap();
    assert_eq!(rayshud.repo(), Some("https://github.com/raysfire/rayshud"));
    assert_eq!(
        client.store_item(StoreKind::Sounds, &curated).unwrap(),
        None
    );
}

#[test]
fn sound_cards_carry_their_category_and_whether_they_preview() {
    let (_tmp, dir) = temp_data_dir();
    let client = StoreClient::new(
        Database::open(&dir).unwrap(),
        dir.clone(),
        Endpoints::default(),
    );
    sql(&dir).execute_batch(SEED).unwrap();
    sql(&dir)
        .execute_batch(
            "UPDATE store_sounds SET preview_url = 'https://sounds.gamebanana.com/a.mp3' WHERE gb_id = 1;
             UPDATE store_sounds SET category = '' WHERE gb_id = 5;",
        )
        .unwrap();
    let q = StoreQuery::new(StoreSort::MostLiked, 60);
    let cards = client.store_cards(StoreKind::Sounds, &q).unwrap();
    let card = |name: &str| cards.iter().find(|c| c.name == name).unwrap().clone();
    assert_eq!(card("Alpha Ding").category.as_deref(), Some("hitsound"));
    assert!(card("Alpha Ding").has_preview);
    assert_eq!(card("Beta Boom").category.as_deref(), Some("killsound"));
    assert!(!card("Beta Boom").has_preview);
    assert_eq!(card("Gamma").category.as_deref(), Some("sound"));
    assert_eq!(card("Epsilon").category, None);
    let crosshairs = client.store_cards(StoreKind::Crosshairs, &q).unwrap();
    assert!(
        crosshairs
            .iter()
            .all(|c| c.category.is_none() && !c.has_preview)
    );
}

#[test]
fn an_empty_preview_url_is_no_preview() {
    let (_tmp, dir) = temp_data_dir();
    let client = StoreClient::new(
        Database::open(&dir).unwrap(),
        dir.clone(),
        Endpoints::default(),
    );
    sql(&dir).execute_batch(SEED).unwrap();
    sql(&dir)
        .execute_batch(
            "UPDATE store_sounds SET preview_url = '' WHERE gb_id = 2;
             UPDATE store_sounds SET preview_url = 'https://sounds.gamebanana.com/a.mp3' WHERE gb_id = 1;",
        )
        .unwrap();
    let q = StoreQuery::new(StoreSort::MostLiked, 60);
    let cards = client.store_cards(StoreKind::Sounds, &q).unwrap();
    let card = |name: &str| cards.iter().find(|c| c.name == name).unwrap().clone();
    assert!(card("Alpha Ding").has_preview);
    assert!(!card("Beta Boom").has_preview, "an empty URL plays nothing");
}
