//! Store images: the host allowlist, redirects, the size cap and the disk cache.

mod common;

use common::{StoreEnv, store_env, write_file};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tf2tools_core::ImageError;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const JPEG: &[u8] = b"\xff\xd8\xff\xe0 not really a jpeg";

async fn serve(env: &StoreEnv, at: &str, status: u16, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_bytes(body))
        .mount(&env.server)
        .await;
}

fn url(env: &StoreEnv, at: &str) -> String {
    format!("{}{at}", env.server.uri())
}

#[tokio::test]
async fn an_allowlisted_image_is_fetched() {
    let env = store_env().await;
    serve(&env, "/img/a.jpg", 200, JPEG.to_vec()).await;
    assert_eq!(
        env.client
            .fetch_image(&url(&env, "/img/a.jpg"))
            .await
            .unwrap(),
        JPEG
    );
}

#[tokio::test]
async fn an_image_off_the_allowlist_is_refused_without_a_request() {
    let env = store_env().await;
    for refused in [
        "https://evil.example/a.jpg".to_owned(),
        url(&env, "/imgx/a.jpg"),
    ] {
        let result = env.client.fetch_image(&refused).await;
        assert!(
            matches!(&result, Err(ImageError::UntrustedUrl { url }) if *url == refused),
            "{result:?}"
        );
    }
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_image_redirected_off_the_allowlist_is_refused() {
    let env = store_env().await;
    Mock::given(method("GET"))
        .and(path("/img/r.jpg"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", url(&env, "/other/a.jpg")),
        )
        .mount(&env.server)
        .await;
    let result = env.client.fetch_image(&url(&env, "/img/r.jpg")).await;
    let refused = url(&env, "/other/a.jpg");
    assert!(
        matches!(&result, Err(ImageError::UntrustedUrl { url }) if *url == refused),
        "{result:?}"
    );
}

#[tokio::test]
async fn an_image_over_ten_megabytes_is_too_large() {
    let env = store_env().await;
    serve(&env, "/img/big.jpg", 200, vec![0u8; 10 * 1024 * 1024 + 1]).await;
    let result = env.client.fetch_image(&url(&env, "/img/big.jpg")).await;
    assert!(
        matches!(
            result,
            Err(ImageError::TooLarge {
                limit: 10_485_760,
                ..
            })
        ),
        "{result:?}"
    );
}

#[tokio::test]
async fn an_error_status_is_reported() {
    let env = store_env().await;
    serve(&env, "/img/gone.jpg", 404, Vec::new()).await;
    let result = env.client.fetch_image(&url(&env, "/img/gone.jpg")).await;
    assert!(
        matches!(result, Err(ImageError::Http { status: 404, .. })),
        "{result:?}"
    );
}

fn cache_file(env: &StoreEnv, url: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let name: String = Sha256::digest(url.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    env.dir.image_cache_dir().join(name)
}

async fn serve_once(env: &StoreEnv, at: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .expect(1)
        .mount(&env.server)
        .await;
}

#[tokio::test]
async fn a_fetched_image_is_cached_on_disk_and_served_from_there() {
    let env = store_env().await;
    serve_once(&env, "/img/a.jpg", JPEG).await;
    let image = url(&env, "/img/a.jpg");
    assert_eq!(env.client.fetch_image(&image).await.unwrap(), JPEG);
    cached_eventually(&env, &image, JPEG).await;
    // serve_once expects one request, so a second network fetch fails the test on drop.
    assert_eq!(env.client.fetch_image(&image).await.unwrap(), JPEG);
}

#[tokio::test]
async fn a_cache_hit_refreshes_the_file_time() {
    let env = store_env().await;
    serve_once(&env, "/img/a.jpg", JPEG).await;
    let image = url(&env, "/img/a.jpg");
    env.client.fetch_image(&image).await.unwrap();
    cached_eventually(&env, &image, JPEG).await;
    let file = cache_file(&env, &image);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(2 * 86_400))
        .unwrap();
    env.client.fetch_image(&image).await.unwrap();
    let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    assert!(age < Duration::from_secs(60), "{age:?}");
}

#[tokio::test]
async fn an_unwritable_cache_still_returns_the_image() {
    let env = store_env().await;
    // A file where the cache directory belongs makes every cache read and write fail.
    write_file(&env.dir.image_cache_dir(), b"not a directory");
    serve(&env, "/img/a.jpg", 200, JPEG.to_vec()).await;
    assert_eq!(
        env.client
            .fetch_image(&url(&env, "/img/a.jpg"))
            .await
            .unwrap(),
        JPEG
    );
}

#[tokio::test]
async fn a_refused_image_is_not_cached() {
    let env = store_env().await;
    serve(&env, "/img/big.jpg", 200, vec![0u8; 10 * 1024 * 1024 + 1]).await;
    let image = url(&env, "/img/big.jpg");
    assert!(env.client.fetch_image(&image).await.is_err());
    assert!(!cache_file(&env, &image).exists());
}

/// Polls until the cache file for `at` holds `bytes`; the write lands after the fetch returns.
async fn cached_eventually(env: &StoreEnv, at: &str, bytes: &[u8]) {
    let file = cache_file(env, at);
    for _ in 0..300 {
        if std::fs::read(&file).ok().as_deref() == Some(bytes) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{} never cached", at);
}

#[tokio::test]
async fn an_empty_cache_file_is_a_miss() {
    let env = store_env().await;
    let at = url(&env, "/img/empty.jpg");
    write_file(&cache_file(&env, &at), b"");
    serve_once(&env, "/img/empty.jpg", JPEG).await;
    assert_eq!(env.client.fetch_image(&at).await.unwrap(), JPEG);
    cached_eventually(&env, &at, JPEG).await;
}

#[tokio::test]
async fn a_429_is_rate_limited() {
    let env = store_env().await;
    serve(&env, "/img/busy.jpg", 429, Vec::new()).await;
    let at = url(&env, "/img/busy.jpg");
    let result = env.client.fetch_image(&at).await;
    assert!(
        matches!(&result, Err(ImageError::RateLimited { url }) if *url == at),
        "{result:?}"
    );
}
