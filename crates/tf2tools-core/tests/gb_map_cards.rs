//! Paged reads of the synced maps catalogue and of the library's map rows.

use crate::common;

use common::{StoreEnv, sql, store_env};
use tf2tools_core::{GbId, GbMapCard, GbMapQuery, GbMapShow, GbMapSort};

const MD: GbMapSort = GbMapSort::MostDownloaded;

fn seed(env: &StoreEnv) {
    sql(&env.dir)
        .execute_batch(
            "
        INSERT INTO store_gb_map_categories (category_id, name, gamemode_id) VALUES
            (5373, 'Control Points', 5373), (5383, 'Achievement/Idle', 5383),
            (5388, 'Trade', 5383), (5385, 'King of the Hill', 5385);
        INSERT INTO store_gb_maps (gb_id, name, author, like_count, date_added, category_id,
            category_name, gamemode_id, download_rank, obsolete, mature, fetched_at) VALUES
            (10, 'cp_alpha', 'vex', 5, 300, 5373, 'Control Points', 5373, 1, 0, 0, 'x'),
            (11, 'trade_plaza', 'mira', 4, 500, 5388, 'Trade', 5383, 2, 0, 0, 'x'),
            (12, 'koth_beta', 'vex', 3, 100, 5385, 'King of the Hill', 5385, 3, 1, 0, 'x'),
            (13, 'cp_mature', 'odd', 2, 400, 5373, 'Control Points', 5373, 4, 0, 1, 'x'),
            (14, 'cp_tie_a', 'zed', 1, 200, 5373, 'Control Points', 5373, 5, 0, 0, 'x'),
            (15, 'cp_tie_b', 'zed', 1, 200, 5373, 'Control Points', 5373, 5, 0, 0, 'x'),
            (16, 'odd_100%', 'q_t', 0, 50, NULL, NULL, NULL, 6, 0, 0, 'x');
        INSERT INTO library_gb_addons (gb_id, section, name, author, file_id, file_date,
            latest_file_date, checked_at, payloads, files, installed, gone) VALUES
            (11, 'maps', 'trade_plaza', 'mira', 1, 100, 200, 0, '[]', '[]', 1, 0),
            (12, 'maps', 'koth_beta', 'vex', 1, 100, 100, 0, '[]', '[]', 0, 0),
            (99, 'maps', 'gone_map', 'old', 1, 100, 100, 0, '[]', '[]', 1, 1),
            (50, 'huds', 'some hud', 'x', 1, 100, 100, 0, '[]', '[]', 1, 0);
    ",
        )
        .unwrap();
}

fn ids(cards: &[GbMapCard]) -> Vec<u64> {
    cards.iter().map(|c| c.card.id.get()).collect()
}

fn read(env: &StoreEnv, q: &GbMapQuery) -> Vec<u64> {
    ids(&env.client.gb_map_cards(q).unwrap())
}

/// Every page of `q`, each continuing after the last card of the one before.
fn pages(env: &StoreEnv, mut q: GbMapQuery) -> Vec<Vec<u64>> {
    let mut out = Vec::new();
    loop {
        let cards = env.client.gb_map_cards(&q).unwrap();
        out.push(ids(&cards));
        let Some(last) = cards.last() else { return out };
        q = q.clone().with_after(last.cursor.clone());
        assert!(out.len() < 10, "paging never ended: {out:?}");
    }
}

fn card(env: &StoreEnv, q: &GbMapQuery, id: u64) -> GbMapCard {
    env.client
        .gb_map_cards(q)
        .unwrap()
        .into_iter()
        .find(|c| c.card.id == GbId::new(id))
        .unwrap()
}

#[tokio::test]
async fn most_downloaded_reads_rank_first_with_ties_on_the_newest_id_and_mature_hidden() {
    let env = store_env().await;
    seed(&env);
    assert_eq!(
        read(&env, &GbMapQuery::new(MD, 60)),
        [10, 11, 12, 15, 14, 16]
    );
    assert_eq!(
        read(&env, &GbMapQuery::new(MD, 60).with_mature(true)),
        [10, 11, 12, 13, 15, 14, 16]
    );
    assert_eq!(
        read(&env, &GbMapQuery::new(GbMapSort::Newest, 60)),
        [11, 10, 15, 14, 12, 16]
    );
}

#[tokio::test]
async fn a_gamemode_holds_its_nested_categories_and_unknown_ones_show_only_under_all() {
    let env = store_env().await;
    seed(&env);
    assert_eq!(
        read(&env, &GbMapQuery::new(MD, 60).with_gamemode(Some(5383))),
        [11]
    );
    assert_eq!(
        read(&env, &GbMapQuery::new(MD, 60).with_gamemode(Some(5373))),
        [10, 15, 14]
    );
}

#[tokio::test]
async fn search_matches_name_or_author_literally_and_ignoring_case() {
    let env = store_env().await;
    seed(&env);
    let search = |text: &str| read(&env, &GbMapQuery::new(MD, 60).with_search(text));
    assert_eq!(search("VEX"), [10, 12]);
    assert_eq!(search("100%"), [16]);
    assert_eq!(search("q_t"), [16]);
    assert!(search("q%").is_empty());
}

#[tokio::test]
async fn the_cursor_pages_each_sort_to_an_empty_page() {
    let env = store_env().await;
    seed(&env);
    assert_eq!(
        pages(&env, GbMapQuery::new(MD, 2)),
        vec![vec![10, 11], vec![12, 15], vec![14, 16], vec![]]
    );
    // A page of 3 ends between 15 and 14, which share date 200.
    let newest = GbMapQuery::new(GbMapSort::Newest, 3);
    assert_eq!(
        pages(&env, newest),
        vec![vec![11, 10, 15], vec![14, 12, 16], vec![]]
    );
}

#[tokio::test]
async fn a_card_carries_the_catalogue_columns_and_its_library_row() {
    let env = store_env().await;
    seed(&env);
    let q = GbMapQuery::new(MD, 60);
    let trade = card(&env, &q, 11);
    assert_eq!(
        (trade.card.name.as_str(), trade.card.author.as_str()),
        ("trade_plaza", "mira")
    );
    assert_eq!(
        (trade.card.category_id, trade.card.category_name.as_deref()),
        (Some(5388), Some("Trade"))
    );
    assert_eq!((trade.card.like_count, trade.card.date_added), (4, 500));
    assert_eq!(trade.card.profile_url, "https://gamebanana.com/mods/11");
    assert!(trade.in_catalogue);
    let lib = trade
        .library
        .as_ref()
        .expect("installed from the Maps page");
    assert!(lib.installed && lib.has_update && !lib.gone);
    let koth = card(&env, &q, 12);
    assert!(koth.card.obsolete);
    assert!(!koth.library.as_ref().unwrap().installed);
    assert!(card(&env, &q, 10).library.is_none());
}

#[tokio::test]
async fn installed_lists_library_maps_then_rows_gone_from_the_catalogue() {
    let env = store_env().await;
    seed(&env);
    let installed = GbMapQuery::new(MD, 2).with_show(GbMapShow::Installed);
    assert_eq!(
        pages(&env, installed.clone()),
        vec![vec![11, 12], vec![99], vec![]]
    );
    let gone = card(
        &env,
        &GbMapQuery::new(MD, 60).with_show(GbMapShow::Installed),
        99,
    );
    assert!(!gone.in_catalogue);
    assert_eq!(
        (gone.card.name.as_str(), gone.card.author.as_str()),
        ("gone_map", "old")
    );
    assert_eq!(gone.card.profile_url, "https://gamebanana.com/mods/99");
    assert!(gone.library.as_ref().unwrap().gone);
    let newest = GbMapQuery::new(GbMapSort::Newest, 60).with_show(GbMapShow::Installed);
    assert_eq!(
        pages(
            &env,
            GbMapQuery::new(GbMapSort::Newest, 1).with_show(GbMapShow::Installed)
        ),
        vec![vec![11], vec![12], vec![99], vec![]]
    );
    assert_eq!(read(&env, &newest.with_gamemode(Some(5383))), [11]);
}
