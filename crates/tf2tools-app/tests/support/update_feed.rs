//! A minisign-signed update feed on wiremock, on its own runtime, with throwaway keys.

use std::io::Cursor;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tf2tools_core::{Endpoints, StoreConfig, UpdateClient};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub fn fresh_key() -> minisign::KeyPair {
    minisign::KeyPair::generate_unencrypted_keypair().unwrap()
}

pub fn sign(key: &minisign::KeyPair, data: &[u8]) -> String {
    minisign::sign(Some(&key.pk), &key.sk, Cursor::new(data), None, None)
        .unwrap()
        .into_string()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub struct SignedFeed {
    pub rt: tokio::runtime::Runtime,
    pub server: MockServer,
    key: minisign::KeyPair,
}

impl SignedFeed {
    pub fn start() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(MockServer::start());
        Self {
            rt,
            server,
            key: fresh_key(),
        }
    }

    pub fn public_key(&self) -> String {
        self.key.pk.to_base64()
    }

    /// The github base the feed lives under: `{uri}/gh`.
    pub fn github(&self) -> String {
        format!("{}/gh", self.server.uri())
    }

    pub fn endpoints(&self) -> Endpoints {
        Endpoints::default().with_github(self.github())
    }

    /// A loopback-only client trusting only this feed's key.
    pub fn client(&self) -> UpdateClient {
        UpdateClient::new(
            &self.endpoints(),
            &StoreConfig::default().with_loopback_only(true),
            &[&self.public_key()],
        )
        .unwrap()
    }

    pub fn release_page(&self, version: &str) -> String {
        format!(
            "{}/PhongGuy/TF2Tools/releases/tag/v{version}",
            self.github()
        )
    }

    /// Publishes `version` with `payload` as this machine's AppImage, signed by the feed's key.
    pub fn publish(&self, version: &str, payload: &[u8]) {
        self.mount(
            version,
            payload,
            &sha256_hex(payload),
            &self.key,
            (Duration::ZERO, Duration::ZERO),
            "appimage",
        );
    }

    /// Like `publish`, but as this machine's Windows installer (platform suffix `nsis`).
    pub fn publish_installer(&self, version: &str, payload: &[u8]) {
        self.mount(
            version,
            payload,
            &sha256_hex(payload),
            &self.key,
            (Duration::ZERO, Duration::ZERO),
            "nsis",
        );
    }

    /// Like `publish`, but the manifest declares `sha256` for the payload.
    pub fn publish_with_hash(&self, version: &str, payload: &[u8], sha256: &str) {
        self.mount(
            version,
            payload,
            sha256,
            &self.key,
            (Duration::ZERO, Duration::ZERO),
            "appimage",
        );
    }

    /// Like `publish`, but the manifest is signed by `signer`.
    pub fn publish_signed_by(&self, version: &str, payload: &[u8], signer: &minisign::KeyPair) {
        self.mount(
            version,
            payload,
            &sha256_hex(payload),
            signer,
            (Duration::ZERO, Duration::ZERO),
            "appimage",
        );
    }

    /// Like `publish`, but the manifest answers after `manifest` and the payload after `payload`.
    pub fn publish_slow(
        &self,
        version: &str,
        payload: &[u8],
        manifest: Duration,
        payload_delay: Duration,
    ) {
        self.mount(
            version,
            payload,
            &sha256_hex(payload),
            &self.key,
            (manifest, payload_delay),
            "appimage",
        );
    }

    fn mount(
        &self,
        version: &str,
        payload: &[u8],
        sha256: &str,
        signer: &minisign::KeyPair,
        (manifest_delay, payload_delay): (Duration, Duration),
        suffix: &str,
    ) {
        let asset = format!(
            "/gh/PhongGuy/TF2Tools/releases/download/v{version}/tf2tools_{version}_x86_64.AppImage"
        );
        let platform = format!(
            "{}-{}-{suffix}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        let manifest = format!(
            r#"{{"version":"{version}","pub_date":"2026-11-01T12:00:00Z","notes":"Fixes.","platforms":{{"{platform}":{{"url":"{}{asset}","sha256":"{sha256}","size":{}}}}}}}"#,
            self.server.uri(),
            payload.len()
        );
        let signature = sign(signer, manifest.as_bytes());
        let base = "/gh/PhongGuy/TF2Tools/releases/latest/download/tf2tools-update.json";
        self.rt.block_on(async {
            Mock::given(method("GET"))
                .and(path(base))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_string(manifest)
                        .set_delay(manifest_delay),
                )
                .mount(&self.server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("{base}.minisig")))
                .respond_with(ResponseTemplate::new(200).set_body_string(signature))
                .mount(&self.server)
                .await;
            Mock::given(method("GET"))
                .and(path(asset))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_bytes(payload.to_vec())
                        .set_delay(payload_delay),
                )
                .mount(&self.server)
                .await;
        });
    }
}
