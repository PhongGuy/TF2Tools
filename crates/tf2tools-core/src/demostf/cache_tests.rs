use super::DemoCache;
use crate::demostf::demo::parse_list;
use crate::demostf::{DemoSummary, DemosClient, DemosEndpoints};
use crate::error::DemoCacheError;
use crate::error::DemosError;
use crate::store::StoreConfig;
use serde_json::json;
use std::path::Path;
use std::time::{Duration, SystemTime};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn summary(id: u64, url: &str) -> DemoSummary {
    let body = json!([{"id": id, "url": url, "name": "m.dem", "server": "s", "duration": 1, "map": "cp_x", "time": 1, "playerCount": 12}]);
    parse_list(body.to_string().as_bytes()).unwrap().remove(0)
}
fn no_progress(_: u64, _: Option<u64>) {}
fn age(p: &Path, secs: u64) {
    std::fs::File::options()
        .write(true)
        .open(p)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}
fn names(dir: &Path) -> Vec<String> {
    let mut n: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    n.sort();
    n
}
async fn serving(body: Vec<u8>, expect: u64) -> (MockServer, DemosClient, String) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/freezer/aa/7.dem"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .expect(expect)
        .mount(&server)
        .await;
    let config = StoreConfig::default()
        .with_request_spacing(Duration::ZERO)
        .with_retry_backoff(Duration::from_millis(1));
    let client = DemosClient::new(DemosEndpoints::loopback(&server.uri())).with_config(config);
    let url = format!("{}/freezer/aa/7.dem", server.uri());
    (server, client, url)
}

#[tokio::test]
async fn a_missing_demo_downloads_into_the_cache_by_id() {
    let tmp = tempfile::tempdir().unwrap();
    let (_server, client, url) = serving(vec![1u8; 300], 1).await;
    let cache = DemoCache::new(tmp.path().join("demos"));
    let got = cache
        .open_or_download(&client, &summary(7, &url), &no_progress)
        .await
        .unwrap();
    assert_eq!(got, tmp.path().join("demos").join("7.dem"));
    assert_eq!(std::fs::read(&got).unwrap(), vec![1u8; 300]);
}

#[tokio::test]
async fn a_cached_demo_opens_without_asking_and_is_freshened() {
    let tmp = tempfile::tempdir().unwrap();
    let (_server, client, url) = serving(vec![1u8; 300], 0).await;
    let cache = DemoCache::new(tmp.path());
    std::fs::write(cache.path_for(7), b"old").unwrap();
    age(&cache.path_for(7), 3600);
    let got = cache
        .open_or_download(&client, &summary(7, &url), &no_progress)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&got).unwrap(), b"old");
    let modified = std::fs::metadata(&got).unwrap().modified().unwrap();
    assert!(modified.elapsed().unwrap() < Duration::from_secs(600));
}

#[tokio::test]
async fn a_private_demo_is_refused_without_asking() {
    let tmp = tempfile::tempdir().unwrap();
    let (_server, client, _url) = serving(vec![], 0).await;
    let cache = DemoCache::new(tmp.path());
    assert!(matches!(
        cache
            .open_or_download(&client, &summary(7, ""), &no_progress)
            .await,
        Err(DemoCacheError::Private)
    ));
}

#[tokio::test]
async fn a_download_past_the_high_mark_sweeps_oldest_first_but_never_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let (_server, client, url) = serving(vec![1u8; 300], 1).await;
    let cache = DemoCache::new(tmp.path()).with_marks(500, 350);
    for (id, secs) in [(1u64, 300), (2, 200), (3, 100)] {
        std::fs::write(cache.path_for(id), [0u8; 100]).unwrap();
        age(&cache.path_for(id), secs);
    }
    std::fs::write(tmp.path().join(".9.dem.x.part"), [0u8; 100]).unwrap();
    age(&tmp.path().join(".9.dem.x.part"), 999);
    cache
        .open_or_download(&client, &summary(7, &url), &no_progress)
        .await
        .unwrap();
    assert_eq!(names(tmp.path()), [".9.dem.x.part", "7.dem"]);
}

#[tokio::test]
async fn a_failed_download_is_a_download_error() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = DemoCache::new(tmp.path());
    let client = DemosClient::new(DemosEndpoints::loopback("http://127.0.0.1:9"));
    let err = cache
        .open_or_download(
            &client,
            &summary(7, "https://evil.example/7.dem"),
            &no_progress,
        )
        .await;
    assert!(matches!(
        err,
        Err(DemoCacheError::Download(
            crate::error::DemosError::ForeignUrl { .. }
        ))
    ));
}

#[tokio::test]
async fn save_to_copies_a_cached_demo_without_asking() {
    let tmp = tempfile::tempdir().unwrap();
    let (_server, client, url) = serving(vec![], 0).await;
    let cache = DemoCache::new(tmp.path().join("demos"));
    std::fs::create_dir_all(tmp.path().join("demos")).unwrap();
    std::fs::write(cache.path_for(7), b"cached demo").unwrap();
    let out = tmp.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let dest = out.join("saved.dem");
    cache
        .save_to(&client, &summary(7, &url), &dest, &no_progress)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"cached demo");
    assert_eq!(names(&out), ["saved.dem"], "no part file is left");
    assert_eq!(std::fs::read(cache.path_for(7)).unwrap(), b"cached demo");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "other tools and users can read the demo");
    }
}

#[tokio::test]
async fn save_to_downloads_a_missing_demo_and_leaves_the_cache_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let (_server, client, url) = serving(vec![2u8; 300], 1).await;
    let cache = DemoCache::new(tmp.path().join("demos"));
    let out = tmp.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let dest = out.join("saved.dem");
    cache
        .save_to(&client, &summary(7, &url), &dest, &no_progress)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), vec![2u8; 300]);
    assert_eq!(names(&out), ["saved.dem"]);
    assert!(
        !tmp.path().join("demos").exists(),
        "the cache is never filled"
    );
}

#[tokio::test]
async fn save_to_an_unwritable_folder_is_a_write_error_and_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (_server, client, url) = serving(vec![2u8; 300], 0).await;
    let cache = DemoCache::new(tmp.path().join("demos"));
    let dest = tmp.path().join("missing").join("saved.dem");
    // A miss fails on the part file before asking demos.tf.
    let missed = cache
        .save_to(&client, &summary(7, &url), &dest, &no_progress)
        .await;
    assert!(
        matches!(missed, Err(DemosError::Write { .. })),
        "{missed:?}"
    );
    // A hit fails the same way on the copy.
    std::fs::create_dir_all(tmp.path().join("demos")).unwrap();
    std::fs::write(cache.path_for(7), b"cached demo").unwrap();
    let hit = cache
        .save_to(&client, &summary(7, &url), &dest, &no_progress)
        .await;
    assert!(matches!(hit, Err(DemosError::Write { .. })), "{hit:?}");
    assert!(!tmp.path().join("missing").exists(), "nothing is written");
}
