//! Community maps from serveme.tf: parsing the list page, caching it, and the install and
//! remove rules for `tf/maps`.

mod common;

use std::path::{Path, PathBuf};

use common::{StoreEnv, count, hours_ago, meta, set_meta, sql, store_env, store_fixture};
use tempfile::TempDir;
use tf2tools_core::{
    CommunityMap, MapDownloadError, MapRemoveError, MapState, SyncError, Tf2Install,
    parse_serveme_maps,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

fn page() -> String {
    String::from_utf8(store_fixture("serveme-maps.html")).unwrap()
}

fn summary(map: &CommunityMap) -> (String, u64, Option<String>, Option<String>, u64) {
    (
        map.name().to_owned(),
        map.times_played(),
        map.first_played().map(str::to_owned),
        map.last_played().map(str::to_owned),
        map.size(),
    )
}

fn s(text: &str) -> Option<String> {
    Some(text.to_owned())
}

/// A game folder with an empty `tf/maps`, as a fresh TF2 install has.
fn game() -> (TempDir, Tf2Install) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("tf/maps")).unwrap();
    let install = Tf2Install::from_path(tmp.path()).unwrap();
    (tmp, install)
}

fn map_file(install: &Tf2Install, name: &str) -> PathBuf {
    install.tf_dir().join("maps").join(format!("{name}.bsp"))
}

fn bsp(len: usize) -> Vec<u8> {
    let mut body = b"VBSP".to_vec();
    body.resize(len, 7);
    body
}

async fn serve_list(env: &StoreEnv, body: String) {
    Mock::given(method("GET"))
        .and(path("/serveme/maps"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&env.server)
        .await;
}

async fn serve_map(env: &StoreEnv, name: &str, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(format!("/fastdl/maps/{name}.bsp")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&env.server)
        .await;
}

fn leftovers(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn states(env: &StoreEnv, install: Option<&Tf2Install>) -> Vec<(String, MapState)> {
    env.client
        .maps(install)
        .unwrap()
        .iter()
        .map(|e| (e.map().name().to_owned(), e.state()))
        .collect()
}

fn state_of(env: &StoreEnv, install: &Tf2Install, name: &str) -> MapState {
    states(env, Some(install))
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, state)| state)
        .unwrap()
}

#[test]
fn the_list_page_parses_every_well_formed_row() {
    let maps: Vec<_> = parse_serveme_maps(&page()).iter().map(summary).collect();
    assert_eq!(
        maps,
        [
            (
                "123_okwtfisthis".to_owned(),
                77,
                s("2019-02-08"),
                s("2026-01-11"),
                851_424
            ),
            ("1Bathrooms".to_owned(), 0, None, None, 15_294_902),
            ("1cp_afkd".to_owned(), 0, None, None, 2_739_456),
            (
                "1cp_cage".to_owned(),
                33,
                s("2019-07-22"),
                s("2024-10-21"),
                6_972_104
            ),
            (
                "ze_gris_skial_5".to_owned(),
                1,
                s("2025-09-20"),
                s("2025-09-20"),
                59_168_240
            ),
        ]
    );
}

#[test]
fn a_page_without_rows_parses_to_nothing() {
    assert!(parse_serveme_maps("<html><body>Down for maintenance</body></html>").is_empty());
}

#[tokio::test]
async fn a_sync_caches_the_list_and_a_second_one_is_fresh() {
    let env = store_env().await;
    serve_list(&env, page()).await;
    let outcome = env.client.sync_maps(false).await.unwrap();
    assert!(!outcome.is_fresh());
    assert_eq!(outcome.rows(), 5);
    assert_eq!(count(&env.dir, "store_maps"), 5);
    assert!(meta(&env.dir, "maps_last_synced").is_some());

    let again = env.client.sync_maps(false).await.unwrap();
    assert!(again.is_fresh());
    let requests = env.server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let agent = requests[0]
        .headers
        .get("user-agent")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(agent.starts_with("TF2Tools/"), "{agent}");
    assert!(!agent.contains('@'), "{agent}");
}

#[tokio::test]
async fn a_forced_sync_drops_maps_the_site_no_longer_lists() {
    let env = store_env().await;
    sql(&env.dir)
        .execute(
            "INSERT INTO store_maps (name, times_played, size, fetched_at) VALUES ('gone', 1, 1, ?1)",
            [hours_ago(1)],
        )
        .unwrap();
    set_meta(&env.dir, "maps_last_synced", &hours_ago(1));
    serve_list(&env, page()).await;
    env.client.sync_maps(true).await.unwrap();
    let names: Vec<String> = states(&env, None).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        names,
        [
            "123_okwtfisthis",
            "1Bathrooms",
            "1cp_afkd",
            "1cp_cage",
            "ze_gris_skial_5"
        ]
    );
}

#[tokio::test]
async fn a_page_with_no_maps_fails_and_keeps_the_cached_list() {
    let env = store_env().await;
    sql(&env.dir)
        .execute(
            "INSERT INTO store_maps (name, times_played, size, fetched_at) VALUES ('kept', 1, 1, ?1)",
            [hours_ago(1)],
        )
        .unwrap();
    serve_list(&env, "<html>maintenance</html>".to_owned()).await;
    let err = env.client.sync_maps(true).await.unwrap_err();
    assert!(matches!(err, SyncError::Malformed { .. }), "{err:?}");
    assert_eq!(count(&env.dir, "store_maps"), 1);
}

#[tokio::test]
async fn without_tf2_every_map_is_available() {
    let env = store_env().await;
    serve_list(&env, page()).await;
    env.client.sync_maps(false).await.unwrap();
    assert!(
        states(&env, None)
            .iter()
            .all(|(_, state)| *state == MapState::Available)
    );
}

#[tokio::test]
async fn a_download_lands_in_tf_maps_and_shows_as_downloaded() {
    let env = store_env().await;
    let (_game, install) = game();
    serve_list(&env, page()).await;
    env.client.sync_maps(false).await.unwrap();
    serve_map(&env, "1cp_cage", bsp(5000)).await;
    let seen = std::sync::Mutex::new(Vec::new());
    env.client
        .download_map(&install, "1cp_cage", &|done, _total| {
            seen.lock().unwrap().push(done)
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(map_file(&install, "1cp_cage")).unwrap(),
        bsp(5000)
    );
    assert_eq!(seen.into_inner().unwrap().last(), Some(&5000));
    assert_eq!(leftovers(&install.tf_dir().join("maps")), ["1cp_cage.bsp"]);
    assert_eq!(state_of(&env, &install, "1cp_cage"), MapState::Downloaded);
    assert_eq!(state_of(&env, &install, "1cp_afkd"), MapState::Available);
}

#[tokio::test]
async fn a_download_that_is_not_a_map_is_refused_and_leaves_nothing() {
    let env = store_env().await;
    let (_game, install) = game();
    serve_map(&env, "1cp_cage", b"<html>not found</html>".to_vec()).await;
    let err = env
        .client
        .download_map(&install, "1cp_cage", &|_, _| {})
        .await
        .unwrap_err();
    assert!(matches!(err, MapDownloadError::NotAMap { .. }), "{err:?}");
    assert!(leftovers(&install.tf_dir().join("maps")).is_empty());
    assert_eq!(count(&env.dir, "downloaded_maps"), 0);
}

#[tokio::test]
async fn a_name_that_is_not_a_plain_file_name_is_refused_before_any_request() {
    let env = store_env().await;
    let (_game, install) = game();
    for name in ["../evil", "a/b", "a\\b", "..", "", "cp_x.bsp\0", ".hidden"] {
        let err = env
            .client
            .download_map(&install, name, &|_, _| {})
            .await
            .unwrap_err();
        assert!(
            matches!(err, MapDownloadError::BadName { .. }),
            "{name}: {err:?}"
        );
    }
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_map_already_in_the_game_is_never_overwritten_or_removed() {
    let env = store_env().await;
    let (_game, install) = game();
    serve_list(&env, page()).await;
    env.client.sync_maps(false).await.unwrap();
    std::fs::write(map_file(&install, "1cp_afkd"), b"VBSP mine").unwrap();
    serve_map(&env, "1cp_afkd", bsp(100)).await;

    assert_eq!(state_of(&env, &install, "1cp_afkd"), MapState::InGame);
    let err = env
        .client
        .download_map(&install, "1cp_afkd", &|_, _| {})
        .await
        .unwrap_err();
    assert!(matches!(err, MapDownloadError::InGame { .. }), "{err:?}");
    let err = env.client.remove_map(&install, "1cp_afkd").unwrap_err();
    assert!(
        matches!(err, MapRemoveError::NotDownloaded { .. }),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read(map_file(&install, "1cp_afkd")).unwrap(),
        b"VBSP mine"
    );
}

#[tokio::test]
async fn a_map_the_game_downloaded_from_a_server_shows_as_in_game() {
    let env = store_env().await;
    let (_game, install) = game();
    serve_list(&env, page()).await;
    env.client.sync_maps(false).await.unwrap();
    let downloads = install.tf_dir().join("download/maps");
    std::fs::create_dir_all(&downloads).unwrap();
    std::fs::write(downloads.join("1CP_CAGE.bsp"), b"VBSP").unwrap();
    assert_eq!(state_of(&env, &install, "1cp_cage"), MapState::InGame);
}

#[tokio::test]
async fn remove_deletes_a_downloaded_map_and_it_is_available_again() {
    let env = store_env().await;
    let (_game, install) = game();
    serve_list(&env, page()).await;
    env.client.sync_maps(false).await.unwrap();
    serve_map(&env, "1cp_cage", bsp(64)).await;
    env.client
        .download_map(&install, "1cp_cage", &|_, _| {})
        .await
        .unwrap();
    let err = env
        .client
        .download_map(&install, "1cp_cage", &|_, _| {})
        .await
        .unwrap_err();
    assert!(
        matches!(err, MapDownloadError::AlreadyDownloaded { .. }),
        "{err:?}"
    );

    env.client.remove_map(&install, "1cp_cage").unwrap();
    assert!(!map_file(&install, "1cp_cage").exists());
    assert_eq!(count(&env.dir, "downloaded_maps"), 0);
    assert_eq!(state_of(&env, &install, "1cp_cage"), MapState::Available);
}

#[tokio::test]
async fn a_downloaded_map_deleted_by_hand_can_be_downloaded_again() {
    let env = store_env().await;
    let (_game, install) = game();
    serve_list(&env, page()).await;
    env.client.sync_maps(false).await.unwrap();
    serve_map(&env, "1cp_cage", bsp(64)).await;
    env.client
        .download_map(&install, "1cp_cage", &|_, _| {})
        .await
        .unwrap();
    std::fs::remove_file(map_file(&install, "1cp_cage")).unwrap();
    assert_eq!(state_of(&env, &install, "1cp_cage"), MapState::Available);
    env.client
        .download_map(&install, "1cp_cage", &|_, _| {})
        .await
        .unwrap();
    assert_eq!(state_of(&env, &install, "1cp_cage"), MapState::Downloaded);
}
