//! Network hardening: client timeouts and the preview redirect allowlist.

mod common;

use std::time::{Duration, Instant};

use common::{StoreEnv, fast_config, sql, store_env};
use tf2tools_core::{FilesError, GbId, PreviewError, StoreItemId, StoreKind};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

fn seed_preview(env: &StoreEnv, url: &str) {
    sql(&env.dir)
        .execute(
            "INSERT INTO store_sounds (gb_id, name, preview_url, fetched_at) VALUES (1, 'Ding', ?1, 'x')",
            [url],
        )
        .unwrap();
}

async fn redirect(env: &StoreEnv, from: &str, to: &str) {
    Mock::given(method("GET"))
        .and(path(from))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}{to}", env.server.uri())),
        )
        .mount(&env.server)
        .await;
}

async fn serve(env: &StoreEnv, at: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&env.server)
        .await;
}

async fn preview(env: &StoreEnv) -> Result<Vec<u8>, PreviewError> {
    let item = env.client.store_items(StoreKind::Sounds).unwrap().remove(0);
    env.client.preview_audio(&item).await
}

/// `/preview/r0` redirects `hops` times through `/preview/r1..` to a 200.
async fn redirect_chain(env: &StoreEnv, hops: usize) {
    for i in 0..hops {
        redirect(
            env,
            &format!("/preview/r{i}"),
            &format!("/preview/r{}", i + 1),
        )
        .await;
    }
    serve(env, &format!("/preview/r{hops}"), b"ID3 end").await;
    seed_preview(env, &format!("{}/preview/r0", env.server.uri()));
}

#[tokio::test]
async fn a_preview_redirect_under_the_allowlist_is_followed() {
    let env = store_env().await;
    redirect(&env, "/preview/a.mp3", "/preview/b.mp3").await;
    serve(&env, "/preview/b.mp3", b"ID3 b").await;
    seed_preview(&env, &format!("{}/preview/a.mp3", env.server.uri()));
    assert_eq!(preview(&env).await.unwrap(), b"ID3 b");
}

#[tokio::test]
async fn a_preview_redirect_off_the_allowlist_is_refused() {
    let env = store_env().await;
    redirect(&env, "/preview/a.mp3", "/elsewhere/x.mp3").await;
    Mock::given(method("GET"))
        .and(path("/elsewhere/x.mp3"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"evil".to_vec()))
        .expect(0)
        .mount(&env.server)
        .await;
    seed_preview(&env, &format!("{}/preview/a.mp3", env.server.uri()));
    let refused = format!("{}/elsewhere/x.mp3", env.server.uri());
    let result = preview(&env).await;
    assert!(
        matches!(&result, Err(PreviewError::UntrustedUrl { url }) if *url == refused),
        "{result:?}"
    );
}

#[tokio::test]
async fn five_preview_redirects_are_followed_but_six_are_not() {
    let env = store_env().await;
    redirect_chain(&env, 5).await;
    assert_eq!(preview(&env).await.unwrap(), b"ID3 end");
    let env = store_env().await;
    redirect_chain(&env, 6).await;
    let result = preview(&env).await;
    assert!(
        matches!(result, Err(PreviewError::Network { .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn api_requests_still_follow_redirects_anywhere() {
    let env = store_env().await;
    redirect(&env, "/apiv11/Sound/7", "/moved/7").await;
    let listing = serde_json::json!({"_idRow": 7, "_sName": "x", "_aFiles": [{
        "_sFile": "a.wav", "_nFilesize": 1, "_sDownloadUrl": "https://files.gamebanana.com/dl/1"
    }]});
    Mock::given(method("GET"))
        .and(path("/moved/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing))
        .mount(&env.server)
        .await;
    let id = StoreItemId::GameBanana(GbId::new(7));
    assert_eq!(
        env.client
            .item_files(StoreKind::Sounds, &id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn a_stalled_response_fails_after_the_read_timeout() {
    let env = store_env().await;
    let client = env.client.clone().with_config(
        fast_config().with_timeouts(Duration::from_secs(10), Duration::from_millis(200)),
    );
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/7"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"_aFiles": []}))
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&env.server)
        .await;
    let start = Instant::now();
    let result = client
        .item_files(StoreKind::Sounds, &StoreItemId::GameBanana(GbId::new(7)))
        .await;
    assert!(
        matches!(result, Err(FilesError::Network { .. })),
        "{result:?}"
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
}
