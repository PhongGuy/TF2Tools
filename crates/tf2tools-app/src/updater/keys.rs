//! The startup update source: core's client over the production key slots, plus a test key
//! and loopback feed that only a build with the `update-test-key` feature reads.

use tf2tools_core::{Endpoints, InstallTarget, StoreConfig, UPDATE_PUBLIC_KEYS, UpdateClient};

use super::UpdateSource;
use super::backend::{Deadlines, core_backend};

// A release workflow sets this variable, so a release build can never carry the seam.
#[cfg(feature = "update-test-key")]
const _: () = assert!(
    option_env!("TF2TOOLS_RELEASE_BUILD").is_none(),
    "the update-test-key feature must never be in a release build"
);

/// One extra trusted key and an optional loopback feed base, read from the environment only
/// in builds with the `update-test-key` feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestSeam {
    pub key: String,
    pub base: Option<String>,
}

/// `raw` trimmed, when it is exactly one minisign public key: 56 standard-base64 characters.
pub fn parse_test_key(raw: &str) -> Option<String> {
    let key = raw.trim();
    let base64 = |c: char| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=');
    (key.len() == 56 && key.chars().all(base64)).then(|| key.to_owned())
}

/// `raw` without a trailing '/', when it is `http://` on 127.0.0.1, localhost or [::1] with no
/// port or a port in 1..=65535.
pub fn parse_test_base(raw: &str) -> Option<String> {
    let rest = raw.trim().strip_prefix("http://")?;
    let authority = rest.split('/').next().unwrap_or_default();
    // The host ends at the port's colon; IPv6's brackets keep its colons out of the split.
    let (host, port) = match authority.strip_prefix("[::1]") {
        Some(after) if after.is_empty() || after.starts_with(':') => ("[::1]", after),
        Some(_) => return None,
        None => authority
            .find(':')
            .map_or((authority, ""), |at| authority.split_at(at)),
    };
    // A colon needs 1 to 5 digits that name a connectable port; the digit check comes first
    // because `u16` parsing also accepts a leading '+'.
    let port_ok = match port.strip_prefix(':') {
        None => true,
        Some(digits) => {
            (1..=5).contains(&digits.len())
                && digits.chars().all(|c| c.is_ascii_digit())
                && digits.parse::<u16>().is_ok_and(|port| port != 0)
        }
    };
    let host_ok = matches!(host, "127.0.0.1" | "localhost" | "[::1]");
    (host_ok && port_ok).then(|| raw.trim().trim_end_matches('/').to_owned())
}

/// With the feature: Some when `var("TF2TOOLS_UPDATE_TEST_PUBKEY")` parses; a bad base is dropped.
/// Without it: None, and `var` is never called.
#[cfg(feature = "update-test-key")]
pub fn test_seam_with(var: impl Fn(&str) -> Option<String>) -> Option<TestSeam> {
    let key = parse_test_key(&var("TF2TOOLS_UPDATE_TEST_PUBKEY")?)?;
    let base = var("TF2TOOLS_UPDATE_TEST_BASE").and_then(|raw| {
        let base = parse_test_base(&raw);
        if base.is_none() {
            tracing::warn!("ignoring an update test base that is not a loopback http address");
        }
        base
    });
    Some(TestSeam { key, base })
}

/// With the feature: Some when `var("TF2TOOLS_UPDATE_TEST_PUBKEY")` parses; a bad base is dropped.
/// Without it: None, and `var` is never called.
#[cfg(not(feature = "update-test-key"))]
pub fn test_seam_with(var: impl Fn(&str) -> Option<String>) -> Option<TestSeam> {
    let _ = var;
    None
}

/// The production slots, then the seam's key.
pub fn trusted_keys(production: &[&str], seam: Option<&TestSeam>) -> Vec<String> {
    production
        .iter()
        .map(|key| (*key).to_owned())
        .chain(seam.map(|s| s.key.clone()))
        .collect()
}

/// A source with no backend, so updates are Unavailable; the release page sits under
/// `endpoints.github()`.
pub fn unavailable_source(endpoints: &Endpoints) -> UpdateSource {
    UpdateSource {
        backend: None,
        current: env!("CARGO_PKG_VERSION").to_owned(),
        releases_page: format!("{}/PhongGuy/TF2Tools/releases/latest", endpoints.github()),
    }
}

/// Core's client over `endpoints` (github replaced by the seam's base; loopback-only when
/// `offline` or a base is set), `InstallTarget::detect()`, `CARGO_PKG_VERSION`, default deadlines,
/// releases page `{github}/PhongGuy/TF2Tools/releases/latest`. No backend when the client
/// cannot be built (no key, or a key that does not decode).
pub fn production_source(
    endpoints: &Endpoints,
    offline: bool,
    seam: Option<TestSeam>,
) -> UpdateSource {
    let base = seam.as_ref().and_then(|s| s.base.clone());
    let loopback_only = offline || base.is_some();
    let endpoints = match base {
        Some(base) => endpoints.clone().with_github(base),
        None => endpoints.clone(),
    };
    let mut source = unavailable_source(&endpoints);
    let keys = trusted_keys(UPDATE_PUBLIC_KEYS, seam.as_ref());
    let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
    let config = StoreConfig::default().with_loopback_only(loopback_only);
    // Core refuses an empty or undecodable key list, which is exactly "updates unavailable".
    match UpdateClient::new(&endpoints, &config, &keys) {
        Ok(client) => {
            source.backend = Some(core_backend(
                client,
                InstallTarget::detect(),
                &source.current,
                Deadlines::default(),
            ));
        }
        Err(err) => tracing::info!(error = %err, "automatic updates are unavailable"),
    }
    source
}

/// `production_source` with the seam read from the process environment.
pub fn source_from_environment(endpoints: &Endpoints, offline: bool) -> UpdateSource {
    production_source(
        endpoints,
        offline,
        test_seam_with(|name| std::env::var(name).ok()),
    )
}
