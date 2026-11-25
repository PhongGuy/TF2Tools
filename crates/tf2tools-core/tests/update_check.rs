//! The update check: a signed manifest over the store HTTP path, verified before parsing.

use crate::common;

use std::path::PathBuf;

use common::update::{
    MANIFEST, SIGNATURE, TestKey, appimage_key, asset_url, client_for, feed, manifest_json,
    notes_doc, notes_requests, publish, publish_with_notes, serve_feed, sha256_hex,
};
use tf2tools_core::{
    Endpoints, InstallTarget, NotesStatus, ReleaseNotes, StoreConfig, UPDATE_PUBLIC_KEYS,
    UpdateAsset, UpdateCheckError, UpdateClient, UpdateOffer,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn appimage() -> InstallTarget {
    InstallTarget::AppImage(PathBuf::from("/apps/tf2tools.AppImage"))
}

fn assert_send_sync<T: Send + Sync + std::fmt::Debug + 'static>() {}

#[test]
fn update_types_are_send_sync_debug() {
    assert_send_sync::<UpdateClient>();
    assert_send_sync::<UpdateOffer>();
    assert_send_sync::<UpdateAsset>();
    assert_send_sync::<InstallTarget>();
    assert_send_sync::<UpdateCheckError>();
    assert_send_sync::<ReleaseNotes>();
    assert_send_sync::<NotesStatus>();
}

#[tokio::test]
async fn a_newer_signed_manifest_is_offered() {
    let feed = feed().await;
    publish(&feed, "6.0.1", b"new appimage").await;
    let offer = feed
        .client
        .check("6.0.0", &appimage())
        .await
        .unwrap()
        .unwrap();
    let uri = feed.server.uri();
    assert_eq!(offer.version(), "6.0.1");
    assert_eq!(offer.notes(), "Fixes.");
    assert_eq!(offer.pub_date(), "2026-11-01T12:00:00Z");
    assert_eq!(
        offer.release_page(),
        format!("{uri}/gh/PhongGuy/TF2Tools/releases/tag/v6.0.1")
    );
    let asset = offer.asset().unwrap();
    assert_eq!(asset.url(), asset_url(&uri, "6.0.1"));
    assert_eq!(asset.sha256(), sha256_hex(b"new appimage"));
    assert_eq!(asset.size(), 12);
}

#[tokio::test]
async fn the_same_version_is_up_to_date() {
    let feed = feed().await;
    publish(&feed, "6.0.1", b"x").await;
    assert_eq!(feed.client.check("6.0.1", &appimage()).await.unwrap(), None);
}

#[tokio::test]
async fn unmanaged_gets_an_offer_without_an_asset() {
    let feed = feed().await;
    publish(&feed, "6.0.1", b"x").await;
    let offer = feed
        .client
        .check("6.0.0", &InstallTarget::Unmanaged)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.asset(), None);
    assert!(offer.release_page().ends_with("/releases/tag/v6.0.1"));
}

#[tokio::test]
async fn a_signed_manifest_without_this_platform_is_up_to_date() {
    let feed = feed().await;
    let url = format!(
        "{}/gh/PhongGuy/TF2Tools/releases/download/v6.0.1/TF2Tools.exe",
        feed.server.uri()
    );
    let sha = "ab".repeat(32);
    let manifest = manifest_json(
        "6.0.1",
        &[("windows-x86_64-nsis", url.as_str(), sha.as_str(), 10)],
    );
    serve_feed(&feed.server, &manifest, &feed.key.sign(manifest.as_bytes())).await;
    assert_eq!(feed.client.check("6.0.0", &appimage()).await.unwrap(), None);
}

#[tokio::test]
async fn every_request_carries_the_configured_user_agent() {
    let server = MockServer::start().await;
    let key = TestKey::new();
    let manifest = manifest_json("6.0.1", &[]);
    serve_feed(&server, &manifest, &key.sign(manifest.as_bytes())).await;
    let config = StoreConfig::default().with_user_agent("tf2tools-test-agent");
    let client = UpdateClient::new(
        &Endpoints::loopback(&server.uri()),
        &config,
        &[&key.public()],
    )
    .unwrap();
    assert_eq!(client.check("6.0.0", &appimage()).await.unwrap(), None);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(
            request.headers.get("user-agent").unwrap(),
            "tf2tools-test-agent"
        );
    }
}

#[tokio::test]
async fn a_manifest_signed_by_another_key_is_a_bad_signature() {
    let feed = feed().await;
    let stranger = TestKey::new();
    let manifest = manifest_json("6.0.1", &[]);
    serve_feed(&feed.server, &manifest, &stranger.sign(manifest.as_bytes())).await;
    let err = feed.client.check("6.0.0", &appimage()).await.unwrap_err();
    assert!(matches!(err, UpdateCheckError::BadSignature), "{err:?}");
    assert_eq!(err.to_string(), "the update could not be verified");
}

#[tokio::test]
async fn a_tampered_manifest_is_a_bad_signature() {
    let feed = feed().await;
    let manifest = manifest_json("6.0.1", &[]);
    let signature = feed.key.sign(manifest.as_bytes());
    serve_feed(&feed.server, &format!("{manifest} "), &signature).await;
    let err = feed.client.check("6.0.0", &appimage()).await.unwrap_err();
    assert!(matches!(err, UpdateCheckError::BadSignature), "{err:?}");
}

#[tokio::test]
async fn the_second_key_slot_verifies() {
    let server = MockServer::start().await;
    let (old, new) = (TestKey::new(), TestKey::new());
    let key = appimage_key();
    let url = asset_url(&server.uri(), "6.0.1");
    let sha = "ab".repeat(32);
    let manifest = manifest_json("6.0.1", &[(key.as_str(), url.as_str(), sha.as_str(), 1)]);
    serve_feed(&server, &manifest, &new.sign(manifest.as_bytes())).await;
    let client = client_for(&server, &[&old.public(), &new.public()]);
    assert!(client.check("6.0.0", &appimage()).await.unwrap().is_some());
}

#[tokio::test]
async fn a_missing_signature_is_a_bad_signature() {
    let feed = feed().await;
    Mock::given(method("GET"))
        .and(path(MANIFEST))
        .respond_with(ResponseTemplate::new(200).set_body_string(manifest_json("6.0.1", &[])))
        .mount(&feed.server)
        .await;
    let err = feed.client.check("6.0.0", &appimage()).await.unwrap_err();
    assert!(matches!(err, UpdateCheckError::BadSignature), "{err:?}");
}

#[tokio::test]
async fn a_missing_manifest_is_status_404_and_the_signature_is_not_fetched() {
    let feed = feed().await;
    let err = feed.client.check("6.0.0", &appimage()).await.unwrap_err();
    assert!(
        matches!(err, UpdateCheckError::Status { status: 404, .. }),
        "{err:?}"
    );
    assert_eq!(feed.server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn an_oversized_manifest_is_too_large() {
    let feed = feed().await;
    let manifest = "x".repeat(64 * 1024 + 1);
    serve_feed(&feed.server, &manifest, &feed.key.sign(manifest.as_bytes())).await;
    let err = feed.client.check("6.0.0", &appimage()).await.unwrap_err();
    assert!(
        matches!(err, UpdateCheckError::TooLarge { limit: 65536, .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_signed_manifest_with_a_foreign_asset_url_is_malformed() {
    let feed = feed().await;
    let key = appimage_key();
    let sha = "ab".repeat(32);
    let manifest = manifest_json(
        "6.0.1",
        &[(
            key.as_str(),
            "https://evil.example/tf2tools.AppImage",
            sha.as_str(),
            1,
        )],
    );
    serve_feed(&feed.server, &manifest, &feed.key.sign(manifest.as_bytes())).await;
    let err = feed.client.check("6.0.0", &appimage()).await.unwrap_err();
    assert!(matches!(err, UpdateCheckError::Malformed { .. }), "{err:?}");
}

#[tokio::test]
async fn a_closed_port_is_a_network_error() {
    let key = TestKey::new();
    let client = UpdateClient::new(
        &Endpoints::loopback("http://127.0.0.1:1"),
        &StoreConfig::default(),
        &[&key.public()],
    )
    .unwrap();
    let err = client.check("6.0.0", &appimage()).await.unwrap_err();
    assert!(matches!(err, UpdateCheckError::Network { .. }), "{err:?}");
}

#[tokio::test]
async fn loopback_only_refuses_github_before_sending() {
    let key = TestKey::new();
    let config = StoreConfig::default().with_loopback_only(true);
    let client = UpdateClient::new(&Endpoints::default(), &config, &[&key.public()]).unwrap();
    match client.check("6.0.0", &appimage()).await {
        Err(UpdateCheckError::Network { message, .. }) => {
            assert!(message.starts_with("offline"), "{message}")
        }
        other => panic!("expected an offline Network error, got {other:?}"),
    }
}

#[test]
fn a_client_needs_at_least_one_decodable_key() {
    let endpoints = Endpoints::default();
    let config = StoreConfig::default();
    for keys in [&[][..], &["not a key"][..]] {
        let err = UpdateClient::new(&endpoints, &config, keys).unwrap_err();
        assert!(matches!(err, UpdateCheckError::BadKey { .. }), "{err:?}");
    }
    let slots = std::hint::black_box(UPDATE_PUBLIC_KEYS);
    if slots.is_empty() {
        let err = UpdateClient::new(&endpoints, &config, slots).unwrap_err();
        assert!(matches!(err, UpdateCheckError::BadKey { .. }), "{err:?}");
    }
}

#[tokio::test]
async fn the_manifest_and_signature_follow_a_redirect_to_another_host() {
    // GitHub answers releases/latest/download with a hop to another host.
    let feed = feed().await;
    let cdn = MockServer::start().await;
    let key = appimage_key();
    let url = asset_url(&feed.server.uri(), "6.0.1");
    let sha = sha256_hex(b"payload");
    let manifest = manifest_json("6.0.1", &[(key.as_str(), url.as_str(), sha.as_str(), 7)]);
    let signature = feed.key.sign(manifest.as_bytes());
    for (from, to, body) in [
        (MANIFEST, "/cdn/manifest", manifest),
        (SIGNATURE, "/cdn/signature", signature),
    ] {
        Mock::given(method("GET"))
            .and(path(from))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", format!("{}{to}", cdn.uri())),
            )
            .mount(&feed.server)
            .await;
        Mock::given(method("GET"))
            .and(path(to))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&cdn)
            .await;
    }
    let offer = feed
        .client
        .check("6.0.0", &appimage())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.version(), "6.0.1");
    assert_eq!(cdn.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn an_offer_carries_the_notes_after_the_installed_version() {
    let feed = feed().await;
    let notes = notes_doc(&[
        ("6.2.0", Some("6.1.0")),
        ("6.1.0", Some("6.0.2")),
        ("6.0.2", Some("6.0.1")),
    ]);
    publish_with_notes(&feed, "6.2.0", &notes, Some(&notes)).await;
    let offer = feed
        .client
        .check("6.1.0", &appimage())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.notes_status(), NotesStatus::Complete);
    let versions: Vec<&str> = offer.releases().iter().map(ReleaseNotes::version).collect();
    assert_eq!(versions, ["6.2.0"]);
    assert_eq!(
        offer.releases()[0].groups()[0].items()[0].text(),
        "Fix in 6.2.0"
    );
}

#[tokio::test]
async fn notes_that_fail_a_check_leave_the_offer_without_notes() {
    let good = notes_doc(&[("6.2.0", Some("6.1.0"))]);
    // Same length and valid JSON, so only the sha256 check can refuse it.
    let other = notes_doc(&[("6.2.0", Some("6.1.1"))]);
    let longer = [good.clone(), b" ".to_vec()].concat();
    let shorter = good[..good.len() - 1].to_vec();
    let schema2 = br#"{"schema":2,"releases":[]}"#.to_vec();
    // (name, bytes the manifest declares, bytes served when any)
    type Case = (&'static str, Vec<u8>, Option<Vec<u8>>);
    let cases: Vec<Case> = vec![
        ("wrong sha256", good.clone(), Some(other)),
        ("longer than declared", good.clone(), Some(longer)),
        ("shorter than declared", good.clone(), Some(shorter)),
        ("schema 2", schema2.clone(), Some(schema2)),
        ("not json", b"nope".to_vec(), Some(b"nope".to_vec())),
        ("missing", good.clone(), None),
    ];
    for (name, declared, served) in cases {
        let feed = feed().await;
        publish_with_notes(&feed, "6.2.0", &declared, served.as_deref()).await;
        let offer = feed
            .client
            .check("6.1.0", &appimage())
            .await
            .unwrap_or_else(|e| panic!("{name}: {e:?}"))
            .unwrap_or_else(|| panic!("{name}: no offer"));
        assert_eq!(offer.version(), "6.2.0", "{name}");
        assert_eq!(offer.notes_status(), NotesStatus::Unavailable, "{name}");
        assert!(offer.releases().is_empty(), "{name}");
    }
}

#[tokio::test]
async fn a_manifest_without_notes_asset_offers_without_fetching_notes() {
    let feed = feed().await;
    publish(&feed, "6.0.1", b"x").await;
    let offer = feed
        .client
        .check("6.0.0", &appimage())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.notes_status(), NotesStatus::NotPublished);
    assert_eq!(notes_requests(&feed).await, 0);
}

#[tokio::test]
async fn notes_with_no_release_in_the_gap_are_not_published() {
    let feed = feed().await;
    let notes = notes_doc(&[("6.1.0", Some("6.0.0"))]);
    publish_with_notes(&feed, "6.2.0", &notes, Some(&notes)).await;
    let offer = feed
        .client
        .check("6.1.0", &appimage())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.notes_status(), NotesStatus::NotPublished);
    assert!(offer.releases().is_empty());
    assert_eq!(notes_requests(&feed).await, 1);
}

#[tokio::test]
async fn notes_declared_over_64_kib_are_not_fetched() {
    let feed = feed().await;
    let big = vec![b' '; 64 * 1024 + 1];
    publish_with_notes(&feed, "6.2.0", &big, Some(&big)).await;
    let offer = feed
        .client
        .check("6.1.0", &appimage())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offer.notes_status(), NotesStatus::Unavailable);
    assert_eq!(notes_requests(&feed).await, 0);
}

#[tokio::test]
async fn no_offer_fetches_no_notes() {
    let feed = feed().await;
    let notes = notes_doc(&[("6.2.0", Some("6.1.0"))]);
    publish_with_notes(&feed, "6.2.0", &notes, Some(&notes)).await;
    assert_eq!(feed.client.check("6.2.0", &appimage()).await.unwrap(), None);
    assert_eq!(notes_requests(&feed).await, 0);
}
