//! The streamed demo download: progress, caps, refusals and what is left on disk.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{DemosClient, DemosEndpoints};
use crate::demostf::DemoSummary;
use crate::demostf::demo::parse_list;
use crate::error::DemosError;
use crate::store::StoreConfig;

fn summary(url: &str) -> DemoSummary {
    let body = json!([{"id": 7, "url": url, "name": "match-7.dem", "server": "s",
                       "duration": 1800, "map": "cp_process_f12", "time": 1, "playerCount": 12}]);
    parse_list(body.to_string().as_bytes()).unwrap().remove(0)
}

fn bytes() -> Vec<u8> {
    (0..3000u32).map(|i| (i % 251) as u8).collect()
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A client on a mock server serving `reply` at /files/aa/7.dem, and that file's url.
async fn serving(reply: ResponseTemplate) -> (MockServer, DemosClient, String) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/files/aa/7.dem"))
        .respond_with(reply)
        .mount(&server)
        .await;
    let config = StoreConfig::default()
        .with_request_spacing(Duration::ZERO)
        .with_retry_backoff(Duration::from_millis(1));
    let client = DemosClient::new(DemosEndpoints::loopback(&server.uri())).with_config(config);
    let url = format!("{}/files/aa/7.dem", server.uri());
    (server, client, url)
}

fn no_progress(_: u64, _: Option<u64>) {}

// The app runs every download on its tokio runtime, so the future must be Send.
#[tokio::test]
async fn a_demo_downloads_on_a_spawned_task() {
    let (_server, client, url) = serving(ResponseTemplate::new(200).set_body_bytes(bytes())).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("m.dem");
    let target = dest.clone();
    let job = tokio::spawn(async move {
        client
            .download_demo(&summary(&url), &target, &no_progress)
            .await
    });
    job.await.unwrap().unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), bytes());
}

#[tokio::test]
async fn a_demo_streams_into_place_reporting_progress() {
    let (_server, client, url) = serving(ResponseTemplate::new(200).set_body_bytes(bytes())).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("m.dem");
    let seen = Mutex::new(Vec::new());
    let report = |done: u64, total: Option<u64>| seen.lock().unwrap().push((done, total));
    client
        .download_demo(&summary(&url), &dest, &report)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), bytes());
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.last(), Some(&(3000, Some(3000))));
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "{seen:?}");
    assert!(
        seen.iter().all(|&(_, total)| total == Some(3000)),
        "{seen:?}"
    );
    assert_eq!(entries(dir.path()), ["m.dem"]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "other tools and users can read the demo");
    }
}

#[tokio::test]
async fn a_finished_download_replaces_an_older_file() {
    let (_server, client, url) = serving(ResponseTemplate::new(200).set_body_bytes(bytes())).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("m.dem");
    std::fs::write(&dest, b"old").unwrap();
    client
        .download_demo(&summary(&url), &dest, &no_progress)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), bytes());
    assert_eq!(entries(dir.path()), ["m.dem"]);
}

#[tokio::test]
async fn a_demo_demos_tf_lost_is_gone_and_the_old_file_stays() {
    let (_server, client, url) = serving(ResponseTemplate::new(404)).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("m.dem");
    std::fs::write(&dest, b"old").unwrap();
    let failed = client
        .download_demo(&summary(&url), &dest, &no_progress)
        .await;
    assert!(matches!(failed, Err(DemosError::Gone)), "{failed:?}");
    assert_eq!(std::fs::read(&dest).unwrap(), b"old");
    assert_eq!(entries(dir.path()), ["m.dem"]);
}

#[tokio::test]
async fn a_demo_past_the_cap_stops_and_leaves_no_part_file() {
    let (_server, client, url) = serving(ResponseTemplate::new(200).set_body_bytes(bytes())).await;
    let client = client.with_limit(100);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("m.dem");
    std::fs::write(&dest, b"old").unwrap();
    let failed = client
        .download_demo(&summary(&url), &dest, &no_progress)
        .await;
    assert!(
        matches!(failed, Err(DemosError::TooLarge { limit: 100, .. })),
        "{failed:?}"
    );
    assert_eq!(std::fs::read(&dest).unwrap(), b"old");
    assert_eq!(entries(dir.path()), ["m.dem"]);
}

#[tokio::test]
async fn a_url_off_the_files_host_is_refused_unasked() {
    let (server, client, _) = serving(ResponseTemplate::new(200).set_body_bytes(bytes())).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("m.dem");
    let base = server.uri();
    for url in [
        "https://evil.example/aa/7.dem".to_owned(),
        format!("{base}/filesx/aa/7.dem"),
        format!("{base}/demos/7"),
    ] {
        let failed = client
            .download_demo(&summary(&url), &dest, &no_progress)
            .await;
        assert!(
            matches!(failed, Err(DemosError::ForeignUrl { .. })),
            "{url}: {failed:?}"
        );
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "nothing was asked"
    );
    assert!(entries(dir.path()).is_empty(), "nothing was written");
}

#[tokio::test]
async fn a_folder_that_cannot_be_written_fails_before_asking() {
    let (server, client, url) = serving(ResponseTemplate::new(200).set_body_bytes(bytes())).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("no such folder").join("m.dem");
    let failed = client
        .download_demo(&summary(&url), &dest, &no_progress)
        .await;
    assert!(
        matches!(failed, Err(DemosError::Write { .. })),
        "{failed:?}"
    );
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "nothing was asked"
    );
}

#[tokio::test]
async fn clean_downloads_ease_a_widened_gate() {
    use crate::store::pacer::{EASE_AFTER, Priority};

    let (server, _, url) = serving(ResponseTemplate::new(200).set_body_bytes(bytes())).await;
    let base = Duration::from_millis(1);
    let config = StoreConfig::default()
        .with_request_spacing(base)
        .with_retry_backoff(Duration::from_millis(1));
    let client = DemosClient::new(DemosEndpoints::loopback(&server.uri())).with_config(config);
    client.pacer.rate_limited();
    assert_eq!(client.pacer.gap_of(Priority::Interactive), base * 2);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("m.dem");
    for _ in 0..EASE_AFTER {
        client
            .download_demo(&summary(&url), &dest, &no_progress)
            .await
            .unwrap();
    }
    assert_eq!(client.pacer.gap_of(Priority::Interactive), base);
}

#[tokio::test]
async fn a_429_on_a_download_widens_the_gate_and_is_no_clean_answer() {
    use crate::store::pacer::{EASE_AFTER, Priority};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/files/aa/7.dem"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/aa/7.dem"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes()))
        .mount(&server)
        .await;
    let base = Duration::from_millis(1);
    let config = StoreConfig::default()
        .with_request_spacing(base)
        .with_retry_backoff(Duration::from_millis(1));
    let client = DemosClient::new(DemosEndpoints::loopback(&server.uri())).with_config(config);
    let url = format!("{}/files/aa/7.dem", server.uri());
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("m.dem");
    client
        .download_demo(&summary(&url), &dest, &no_progress)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), bytes());
    assert_eq!(client.pacer.gap_of(Priority::Interactive), base * 2);
    // Had the retried download counted as a clean answer, one fewer would ease the gate.
    for _ in 1..EASE_AFTER {
        client
            .download_demo(&summary(&url), &dest, &no_progress)
            .await
            .unwrap();
    }
    assert_eq!(client.pacer.gap_of(Priority::Interactive), base * 2);
}
