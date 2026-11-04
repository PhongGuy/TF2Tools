//! A signed update feed on wiremock: a throwaway minisign key, the manifest and its
//! signature under the loopback github base, and an update client pointed at them.

use std::io::Cursor;
use std::path::Path;

use sha2::{Digest, Sha256};
use tf2tools_core::{Endpoints, InstallTarget, StoreConfig, UpdateClient, UpdateOffer};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const MANIFEST: &str = "/gh/PhongGuy/TF2Tools/releases/latest/download/tf2tools-update.json";
pub const SIGNATURE: &str =
    "/gh/PhongGuy/TF2Tools/releases/latest/download/tf2tools-update.json.minisig";

/// A minisign key pair made for one test; never the release key.
pub struct TestKey(minisign::KeyPair);

impl TestKey {
    pub fn new() -> Self {
        Self(minisign::KeyPair::generate_unencrypted_keypair().expect("keypair"))
    }

    pub fn public(&self) -> String {
        self.0.pk.to_base64()
    }

    pub fn sign(&self, data: &[u8]) -> String {
        minisign::sign(Some(&self.0.pk), &self.0.sk, Cursor::new(data), None, None)
            .expect("sign")
            .into_string()
    }
}

/// The manifest key an AppImage on this machine reads.
pub fn appimage_key() -> String {
    format!(
        "{}-{}-appimage",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn asset_path(version: &str) -> String {
    format!("/gh/PhongGuy/TF2Tools/releases/download/v{version}/tf2tools_{version}_x86_64.AppImage")
}

pub fn asset_url(uri: &str, version: &str) -> String {
    format!("{uri}{}", asset_path(version))
}

/// A manifest for `version` with one platform entry per (key, url, sha256, size).
pub fn manifest_json(version: &str, entries: &[(&str, &str, &str, u64)]) -> String {
    let platforms: Vec<String> = entries
        .iter()
        .map(|(key, url, sha, size)| {
            format!(r#""{key}":{{"url":"{url}","sha256":"{sha}","size":{size}}}"#)
        })
        .collect();
    format!(
        r#"{{"version":"{version}","pub_date":"2026-11-01T12:00:00Z","notes":"Fixes.","platforms":{{{}}}}}"#,
        platforms.join(",")
    )
}

/// Serves `manifest` and `signature` at the feed's two fixed paths.
pub async fn serve_feed(server: &MockServer, manifest: &str, signature: &str) {
    Mock::given(method("GET"))
        .and(path(MANIFEST))
        .respond_with(ResponseTemplate::new(200).set_body_string(manifest))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(SIGNATURE))
        .respond_with(ResponseTemplate::new(200).set_body_string(signature))
        .mount(server)
        .await;
}

pub async fn serve_asset(server: &MockServer, version: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(asset_path(version)))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

pub fn client_for(server: &MockServer, keys: &[&str]) -> UpdateClient {
    UpdateClient::new(
        &Endpoints::loopback(&server.uri()),
        &StoreConfig::default(),
        keys,
    )
    .expect("update client")
}

pub struct Feed {
    pub server: MockServer,
    pub key: TestKey,
    pub client: UpdateClient,
}

/// A wiremock server and a client trusting one fresh test key.
pub async fn feed() -> Feed {
    let server = MockServer::start().await;
    let key = TestKey::new();
    let client = client_for(&server, &[&key.public()]);
    Feed {
        server,
        key,
        client,
    }
}

/// Serves a manifest signed by the feed's key whose AppImage entry declares `declared`.
pub async fn publish_manifest(feed: &Feed, version: &str, declared: &[u8]) {
    let key = appimage_key();
    let url = asset_url(&feed.server.uri(), version);
    let sha = sha256_hex(declared);
    let manifest = manifest_json(
        version,
        &[(
            key.as_str(),
            url.as_str(),
            sha.as_str(),
            declared.len() as u64,
        )],
    );
    serve_feed(&feed.server, &manifest, &feed.key.sign(manifest.as_bytes())).await;
}

/// Publishes `payload` as the AppImage of `version`: the signed manifest and the asset.
pub async fn publish(feed: &Feed, version: &str, payload: &[u8]) {
    publish_manifest(feed, version, payload).await;
    serve_asset(&feed.server, version, payload).await;
}

/// The offer `feed` makes to version 6.0.0 running as the AppImage at `appimage`.
pub async fn appimage_offer(feed: &Feed, appimage: &Path) -> UpdateOffer {
    feed.client
        .check("6.0.0", &InstallTarget::AppImage(appimage.to_path_buf()))
        .await
        .expect("check")
        .expect("an offer")
}

/// The manifest key a Windows installer on this machine reads.
pub fn installer_key() -> String {
    format!("{}-{}-nsis", std::env::consts::OS, std::env::consts::ARCH)
}

/// Serves a signed manifest whose installer entry declares `declared`, and `served` as its body.
pub async fn publish_installer(feed: &Feed, version: &str, declared: &[u8], served: &[u8]) {
    let key = installer_key();
    let url = asset_url(&feed.server.uri(), version);
    let sha = sha256_hex(declared);
    let manifest = manifest_json(
        version,
        &[(
            key.as_str(),
            url.as_str(),
            sha.as_str(),
            declared.len() as u64,
        )],
    );
    serve_feed(&feed.server, &manifest, &feed.key.sign(manifest.as_bytes())).await;
    serve_asset(&feed.server, version, served).await;
}

/// The offer `feed` makes to version 6.0.0 installed by the Windows installer.
pub async fn installer_offer(feed: &Feed) -> UpdateOffer {
    feed.client
        .check("6.0.0", &InstallTarget::WindowsInstaller)
        .await
        .expect("check")
        .expect("an offer")
}
