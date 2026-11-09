//! Browsing GameBanana add-ons live: sections, steps, search, categories and profiles.

use std::sync::Mutex;

use serde_json::{Value, json};
use tf2tools_core::{
    AddonCard, AddonCursor, AddonQuery, AddonSection, AddonSort, AddonStep, BrowseError,
};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{StoreEnv, store_env, store_fixture};

fn section(key: &str) -> AddonSection {
    AddonSection::by_key(key).unwrap()
}

/// A record shaped like the trimmed Index fixtures, under root `root` and subcategory `sub`.
fn record(id: u64, root: u32, sub: Option<u32>, mature: bool) -> Value {
    let mut r = json!({
        "_idRow": id, "_sModelName": "Mod", "_sName": format!("Mod {id}"),
        "_sProfileUrl": format!("https://gamebanana.com/mods/{id}"), "_tsDateAdded": 1_700_000_000_u64 + id,
        "_nLikeCount": 3, "_bIsObsolete": false, "_bHasContentRatings": false,
        "_sInitialVisibility": if mature { "warn" } else { "show" }, "_aSubmitter": {"_sName": "author"},
        "_aRootCategory": {"_sName": "Root", "_sProfileUrl": format!("https://gamebanana.com/mods/cats/{root}")},
    });
    if let Some(sub) = sub {
        r["_aSubCategory"] = json!({"_sName": "Sub", "_sProfileUrl": format!("https://gamebanana.com/mods/cats/{sub}")});
    }
    r
}

fn listing(records: Vec<Value>, complete: bool) -> Vec<u8> {
    json!({"_aMetadata": {"_nRecordCount": 999, "_bIsComplete": complete}, "_aRecords": records})
        .to_string()
        .into_bytes()
}

/// `n` records with ids from `first`, all in subcategory `sub` of `root`.
fn many(first: u64, n: u64, root: u32, sub: u32) -> Vec<Value> {
    (first..first + n)
        .map(|id| record(id, root, Some(sub), false))
        .collect()
}

async fn mount_index(server: &MockServer, category: u32, sort: &str, page: u32, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path("/apiv11/Mod/Index"))
        .and(query_param(
            "_aFilters[Generic_Category]",
            category.to_string(),
        ))
        .and(query_param("_sSort", sort))
        .and(query_param("_nPage", page.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(server)
        .await;
}

async fn requests_to(server: &MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == route)
        .count()
}

async fn run(
    env: &StoreEnv,
    query: &AddonQuery,
    from: AddonCursor,
) -> (Result<AddonStep, BrowseError>, Vec<Vec<AddonCard>>) {
    let chunks = Mutex::new(Vec::new());
    let sink = |cards: Vec<AddonCard>| chunks.lock().unwrap().push(cards);
    let result = env.client.addon_catalog().step(query, from, &sink).await;
    (result, chunks.into_inner().unwrap())
}

fn flat_ids(chunks: &[Vec<AddonCard>]) -> Vec<u64> {
    chunks.iter().flatten().map(|c| c.id.get()).collect()
}

/// The ids of a fixture page's records whose subcategory is not `excluded`, in order.
fn kept_in(name: &str, excluded: u32) -> Vec<u64> {
    let raw: Value = serde_json::from_slice(&store_fixture(name)).unwrap();
    let suffix = format!("/{excluded}");
    raw["_aRecords"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            !r["_aSubCategory"]["_sProfileUrl"]
                .as_str()
                .is_some_and(|u| u.ends_with(&suffix))
        })
        .map(|r| r["_idRow"].as_u64().unwrap())
        .collect()
}

const MD: &str = "Generic_MostDownloaded";

#[test]
fn the_sections_table_matches_the_spec() {
    let rows: Vec<_> = AddonSection::ALL
        .iter()
        .map(|s| (s.key(), s.title(), s.root_category(), s.exclude().to_vec()))
        .collect();
    assert_eq!(
        rows,
        [
            ("skins", "Skins", 7951, vec![]),
            ("effects", "Effects", 1090, vec![1105]),
            ("guis", "GUIs", 1644, vec![1649]),
            ("textures", "Textures", 587, vec![]),
            ("gamefiles", "Game files", 2774, vec![]),
        ]
    );
    assert_eq!(
        AddonSection::by_key("guis").map(AddonSection::root_category),
        Some(1644)
    );
    assert_eq!(AddonSection::by_key("maps"), None);
    assert_eq!(
        AddonSection::ALL[0].weapon_parents(),
        [7962, 7960, 7956, 7968, 7954, 7958, 7964, 7966, 7952]
    );
    assert!(
        AddonSection::ALL[1..]
            .iter()
            .all(|s| s.weapon_parents().is_empty())
    );
}

#[tokio::test]
async fn the_guis_worst_case_streams_every_page_and_drops_every_hud() {
    let env = store_env().await;
    for page in 1..=5 {
        let name = format!("gb-addons-guis-p{}.json", page.min(4));
        mount_index(&env.server, 1644, MD, page, store_fixture(&name)).await;
    }
    let (mut expected, mut pages, mut streamed) = (Vec::new(), 0, 0);
    for page in 1..=4 {
        let kept = kept_in(&format!("gb-addons-guis-p{page}.json"), 1649);
        streamed += usize::from(!kept.is_empty());
        expected.extend(kept);
        pages = page;
        if expected.len() >= 50 {
            break;
        }
    }
    let query = AddonQuery::new(section("guis"), AddonSort::MostDownloaded).with_mature(true);
    let (step, chunks) = run(&env, &query, AddonCursor::START).await;
    let step = step.unwrap();
    assert_eq!(flat_ids(&chunks), expected);
    assert_eq!(
        chunks.len(),
        streamed,
        "one chunk per page that kept a card"
    );
    assert_eq!((step.pages(), step.cards()), (pages, expected.len()));
    assert_eq!(step.next().map(AddonCursor::page), Some(pages + 1));
    assert_eq!(
        requests_to(&env.server, "/apiv11/Mod/Index").await,
        pages as usize
    );
}

#[tokio::test]
async fn a_step_stops_after_the_page_that_reaches_fifty_cards() {
    let env = store_env().await;
    mount_index(
        &env.server,
        587,
        MD,
        1,
        listing(many(1, 30, 587, 600), false),
    )
    .await;
    mount_index(
        &env.server,
        587,
        MD,
        2,
        listing(many(31, 30, 587, 600), false),
    )
    .await;
    mount_index(
        &env.server,
        587,
        MD,
        3,
        listing(many(61, 30, 587, 600), false),
    )
    .await;
    let (step, chunks) = run(
        &env,
        &AddonQuery::new(section("textures"), AddonSort::MostDownloaded),
        AddonCursor::START,
    )
    .await;
    let step = step.unwrap();
    assert_eq!((step.pages(), step.cards(), chunks.len()), (2, 60, 2));
    assert_eq!(step.next().map(AddonCursor::page), Some(3));
}

#[tokio::test]
async fn a_step_never_makes_more_than_four_index_calls() {
    let env = store_env().await;
    for page in 1..=8 {
        mount_index(
            &env.server,
            1644,
            MD,
            page,
            listing(many(u64::from(page) * 100, 50, 1644, 1649), false),
        )
        .await;
    }
    let query = AddonQuery::new(section("guis"), AddonSort::MostDownloaded);
    let (step, chunks) = run(&env, &query, AddonCursor::START).await;
    let step = step.unwrap();
    assert!(chunks.is_empty(), "no empty chunks are sent");
    assert_eq!((step.pages(), step.cards()), (4, 0));
    assert_eq!(step.next().map(AddonCursor::page), Some(5));
    assert_eq!(requests_to(&env.server, "/apiv11/Mod/Index").await, 4);
    let (again, _) = run(&env, &query, step.next().unwrap()).await;
    let again = again.unwrap();
    assert_eq!(
        (again.pages(), again.next().map(AddonCursor::page)),
        (4, Some(9)),
        "pages 5 to 8"
    );
    assert_eq!(requests_to(&env.server, "/apiv11/Mod/Index").await, 8);
}

#[tokio::test]
async fn the_listing_end_stops_the_step_and_mature_and_excluded_records_are_dropped() {
    let env = store_env().await;
    let records = vec![
        record(1, 1090, Some(1094), false),
        record(2, 1090, Some(1105), false),
        record(3, 1090, None, false),
        record(4, 1090, Some(1094), true),
    ];
    mount_index(
        &env.server,
        1090,
        "Generic_Newest",
        1,
        listing(records, true),
    )
    .await;
    let query = AddonQuery::new(section("effects"), AddonSort::Newest);
    let (step, chunks) = run(&env, &query, AddonCursor::START).await;
    assert_eq!(
        flat_ids(&chunks),
        [1, 3],
        "a crosshair and a mature item are dropped; no subcategory is kept"
    );
    assert_eq!(step.unwrap().next(), None);
    let (_, shown) = run(&env, &query.clone().with_mature(true), AddonCursor::START).await;
    assert_eq!(flat_ids(&shown), [1, 3, 4]);
    assert!(shown[0].iter().find(|c| c.id.get() == 4).unwrap().mature);
}
