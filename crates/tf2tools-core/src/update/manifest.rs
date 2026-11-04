//! The update manifest: one release version plus a payload per platform key, decoded
//! from JSON and validated before anything trusts a URL, hash or size in it.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::Deserialize;

use super::install::InstallTarget;
use crate::error::UpdateCheckError;

/// A validated manifest. Asset URLs are normalised and sha256 digests lowercase.
#[derive(Debug, Clone)]
pub(crate) struct Manifest {
    pub(crate) version: semver::Version,
    pub(crate) notes: String,
    pub(crate) pub_date: String,
    pub(crate) platforms: BTreeMap<String, ManifestAsset>,
}

/// One platform's payload as the manifest names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManifestAsset {
    pub(crate) url: String,
    pub(crate) sha256: String,
    pub(crate) size: u64,
}

#[derive(Deserialize)]
struct RawManifest {
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    pub_date: String,
    #[serde(deserialize_with = "unique_keys")]
    platforms: BTreeMap<String, RawAsset>,
}

// serde's map impl lets a repeated key overwrite the earlier one; a manifest that names a
// platform twice is ambiguous, so it is refused like a repeated top-level field.
fn unique_keys<'de, D>(deserializer: D) -> Result<BTreeMap<String, RawAsset>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = BTreeMap<String, RawAsset>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a map of platform keys to assets")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut out = BTreeMap::new();
            while let Some((key, asset)) = map.next_entry::<String, RawAsset>()? {
                if out.contains_key(&key) {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate platform {:?}",
                        clip(&key)
                    )));
                }
                out.insert(key, asset);
            }
            Ok(out)
        }
    }
    deserializer.deserialize_map(Visitor)
}

#[derive(Deserialize)]
struct RawAsset {
    url: String,
    sha256: String,
    size: u64,
}

impl Manifest {
    /// Decodes and validates a manifest. `github` is `Endpoints::github()`, without a
    /// trailing slash; every asset URL must be a release download of the TF2Tools repo
    /// under it. Any failure is `Malformed` naming the platform key and field.
    pub(crate) fn parse(bytes: &[u8], github: &str) -> Result<Manifest, UpdateCheckError> {
        let raw: RawManifest =
            serde_json::from_slice(bytes).map_err(|e| malformed(e.to_string()))?;
        let version = semver::Version::parse(&raw.version)
            .map_err(|e| malformed(format!("version {:?}: {e}", clip(&raw.version))))?;
        let prefix = format!("{github}/PhongGuy/TF2Tools/releases/download/");
        let platforms = raw
            .platforms
            .into_iter()
            .map(|(key, asset)| {
                let asset = validate_asset(&key, asset, &prefix)?;
                Ok((key, asset))
            })
            .collect::<Result<_, UpdateCheckError>>()?;
        Ok(Manifest {
            version,
            notes: raw.notes,
            pub_date: raw.pub_date,
            platforms,
        })
    }
}

// The URL is judged after normalisation, so `..` and `%2e%2e` segments cannot climb out
// of the release downloads. What follows the prefix must be exactly a tag and a file
// name, with no encoded separator or dot that a later decode could turn into a path.
fn validate_asset(
    key: &str,
    raw: RawAsset,
    prefix: &str,
) -> Result<ManifestAsset, UpdateCheckError> {
    let url = reqwest::Url::parse(&raw.url)
        .ok()
        .filter(|url| url.query().is_none() && url.fragment().is_none())
        .map(String::from)
        .filter(|url| url.strip_prefix(prefix).is_some_and(is_tag_and_file))
        .ok_or_else(|| {
            malformed(format!(
                "{key}: url {:?} is not a TF2Tools release download",
                clip(&raw.url)
            ))
        })?;
    if raw.sha256.len() != 64 || !raw.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(malformed(format!(
            "{key}: sha256 {:?} is not 64 hex digits",
            clip(&raw.sha256)
        )));
    }
    if raw.size == 0 {
        return Err(malformed(format!("{key}: size must be at least 1")));
    }
    Ok(ManifestAsset {
        url,
        sha256: raw.sha256.to_ascii_lowercase(),
        size: raw.size,
    })
}

fn is_tag_and_file(rest: &str) -> bool {
    let segments: Vec<&str> = rest.split('/').collect();
    segments.len() == 2
        && segments.iter().all(|segment| {
            let lower = segment.to_ascii_lowercase();
            !segment.is_empty() && !["%2f", "%5c", "%2e"].iter().any(|bad| lower.contains(bad))
        })
}

// Keeps an echoed manifest value short enough for a log line or a Settings message.
fn clip(value: &str) -> String {
    const MAX: usize = 80;
    match value.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{}...", &value[..cut]),
        None => value.to_owned(),
    }
}

/// A release newer than the running one, as offered to this install.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UpdateOffer {
    version: String,
    notes: String,
    pub_date: String,
    asset: Option<UpdateAsset>,
    release_page: String,
}

impl UpdateOffer {
    /// The offered version as semver text, such as `6.0.1`.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The release notes from the manifest, possibly empty.
    pub fn notes(&self) -> &str {
        &self.notes
    }

    /// The publication date from the manifest, possibly empty.
    pub fn pub_date(&self) -> &str {
        &self.pub_date
    }

    /// The payload this install can apply, or `None` when it can only be notified.
    pub fn asset(&self) -> Option<&UpdateAsset> {
        self.asset.as_ref()
    }

    /// The GitHub release page for the offered version.
    pub fn release_page(&self) -> &str {
        &self.release_page
    }
}

/// The payload for one platform: where to fetch it and what it must hash and weigh.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UpdateAsset {
    url: String,
    sha256: String,
    size: u64,
    kind: AssetKind,
}

/// The install kind a payload was selected for, so it is never applied to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetKind {
    AppImage,
    Nsis,
}

impl AssetKind {
    /// The suffix of the manifest's platform key, as in `linux-x86_64-appimage`.
    fn key(self) -> &'static str {
        match self {
            Self::AppImage => "appimage",
            Self::Nsis => "nsis",
        }
    }
}

impl UpdateAsset {
    /// The normalised release download URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The expected sha256 digest, 64 lowercase hex digits.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// The expected size in bytes, at least 1.
    pub fn size(&self) -> u64 {
        self.size
    }

    pub(crate) fn kind(&self) -> AssetKind {
        self.kind
    }
}

/// Ok(None) unless `manifest.version` is strictly newer than `current` by semver precedence
/// and the manifest has an entry for `target` on `os`/`arch`; a stable `current` is never
/// offered a prerelease. Malformed if `current` is not semver.
pub(crate) fn select(
    manifest: &Manifest,
    current: &str,
    target: &InstallTarget,
    os: &str,
    arch: &str,
    github: &str,
) -> Result<Option<UpdateOffer>, UpdateCheckError> {
    let current = semver::Version::parse(current)
        .map_err(|e| malformed(format!("current version {:?}: {e}", clip(current))))?;
    // A player on a stable release stays on stable; only a prerelease install opts into more.
    if current.pre.is_empty() && !manifest.version.pre.is_empty() {
        return Ok(None);
    }
    // Precedence, not Ord: build metadata alone must not make a release look newer.
    if manifest.version.cmp_precedence(&current) != Ordering::Greater {
        return Ok(None);
    }
    // Self-updating kinds need their exact payload; an unmanaged install only needs to
    // know a release exists for its OS so it can point at the release page.
    let asset = match target {
        InstallTarget::AppImage(_) => platform_asset(manifest, os, arch, AssetKind::AppImage),
        InstallTarget::WindowsInstaller => platform_asset(manifest, os, arch, AssetKind::Nsis),
        InstallTarget::Unmanaged => {
            let prefix = format!("{os}-");
            if !manifest
                .platforms
                .keys()
                .any(|key| key.starts_with(&prefix))
            {
                return Ok(None);
            }
            None
        }
    };
    if asset.is_none() && !matches!(target, InstallTarget::Unmanaged) {
        return Ok(None);
    }
    // Release tags carry no build metadata, so the page link drops it while the offer keeps it.
    let mut tag = manifest.version.clone();
    tag.build = semver::BuildMetadata::EMPTY;
    let version = manifest.version.to_string();
    Ok(Some(UpdateOffer {
        release_page: format!("{github}/PhongGuy/TF2Tools/releases/tag/v{tag}"),
        version,
        notes: manifest.notes.clone(),
        pub_date: manifest.pub_date.clone(),
        asset,
    }))
}

fn platform_asset(
    manifest: &Manifest,
    os: &str,
    arch: &str,
    kind: AssetKind,
) -> Option<UpdateAsset> {
    manifest
        .platforms
        .get(&format!("{os}-{arch}-{}", kind.key()))
        .map(|asset| UpdateAsset {
            url: asset.url.clone(),
            sha256: asset.sha256.clone(),
            size: asset.size,
            kind,
        })
}

fn malformed(message: String) -> UpdateCheckError {
    UpdateCheckError::Malformed { message }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GH: &str = "https://github.com";

    fn url(file: &str) -> String {
        format!("{GH}/PhongGuy/TF2Tools/releases/download/v6.0.1/{file}")
    }

    fn json(version: &str, asset_url: &str, sha: &str, size: u64) -> String {
        format!(
            r#"{{"version":"{version}","pub_date":"2026-11-01T12:00:00Z","notes":"Fixes.","platforms":{{"linux-x86_64-appimage":{{"url":"{asset_url}","sha256":"{sha}","size":{size}}}}}}}"#
        )
    }

    fn good() -> String {
        json(
            "6.0.1",
            &url("tf2tools_6.0.1_x86_64.AppImage"),
            &"ab".repeat(32),
            1234,
        )
    }

    fn malformed(body: &[u8]) -> String {
        match Manifest::parse(body, GH) {
            Err(UpdateCheckError::Malformed { message }) => message,
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn a_well_formed_manifest_parses() {
        let m = Manifest::parse(good().as_bytes(), GH).unwrap();
        assert_eq!(m.version, semver::Version::new(6, 0, 1));
        assert_eq!(m.notes, "Fixes.");
        assert_eq!(m.pub_date, "2026-11-01T12:00:00Z");
        assert_eq!(
            m.platforms["linux-x86_64-appimage"],
            ManifestAsset {
                url: url("tf2tools_6.0.1_x86_64.AppImage"),
                sha256: "ab".repeat(32),
                size: 1234,
            }
        );
    }

    #[test]
    fn the_sha256_is_kept_in_lowercase() {
        let body = json("6.0.1", &url("a.AppImage"), &"AB".repeat(32), 1);
        let m = Manifest::parse(body.as_bytes(), GH).unwrap();
        assert_eq!(m.platforms["linux-x86_64-appimage"].sha256, "ab".repeat(32));
    }

    #[test]
    fn an_asset_url_is_stored_normalised() {
        let body = json(
            "6.0.1",
            "HTTPS://GitHub.com/PhongGuy/TF2Tools/releases/download/v6.0.1/a.AppImage",
            &"ab".repeat(32),
            1,
        );
        let m = Manifest::parse(body.as_bytes(), GH).unwrap();
        assert_eq!(
            m.platforms["linux-x86_64-appimage"].url,
            "https://github.com/PhongGuy/TF2Tools/releases/download/v6.0.1/a.AppImage"
        );
    }

    #[test]
    fn notes_and_pub_date_are_optional_and_unknown_fields_are_ignored() {
        let body = format!(
            r#"{{"version":"6.0.1","future":1,"platforms":{{"linux-x86_64-appimage":{{"url":"{}","sha256":"{}","size":1,"extra":true}}}}}}"#,
            url("a.AppImage"),
            "ab".repeat(32)
        );
        let m = Manifest::parse(body.as_bytes(), GH).unwrap();
        assert_eq!((m.notes.as_str(), m.pub_date.as_str()), ("", ""));
        assert_eq!(m.platforms.len(), 1);
    }

    #[test]
    fn an_empty_platforms_table_parses() {
        let m = Manifest::parse(br#"{"version":"6.0.1","platforms":{}}"#, GH).unwrap();
        assert!(m.platforms.is_empty());
    }

    #[test]
    fn the_allowed_prefix_follows_the_github_base() {
        let base = "http://127.0.0.1:4000/gh";
        let asset = format!("{base}/PhongGuy/TF2Tools/releases/download/v6.0.1/a.AppImage");
        let body = json("6.0.1", &asset, &"ab".repeat(32), 1);
        assert!(Manifest::parse(body.as_bytes(), base).is_ok());
        assert!(malformed(body.as_bytes()).contains("url"));
    }

    #[test]
    fn asset_urls_outside_the_repo_release_downloads_are_malformed() {
        for bad in [
            "https://evil.example/PhongGuy/TF2Tools/releases/download/v6.0.1/a",
            "https://github.com.evil.example/PhongGuy/TF2Tools/releases/download/v6.0.1/a",
            "http://github.com/PhongGuy/TF2Tools/releases/download/v6.0.1/a",
            "https://github.com/Other/TF2Tools/releases/download/v6.0.1/a",
            "https://github.com/PhongGuy/TF2Tools/releases/downloadx/a",
            "https://github.com/PhongGuy/TF2Tools/releases/download/../../../evil/a",
            "https://github.com/PhongGuy/TF2Tools/releases/download/%2e%2e/%2E%2E/a",
            "https://github.com/PhongGuy/TF2Tools/releases/download/",
            "not a url",
        ] {
            let message = malformed(json("6.0.1", bad, &"ab".repeat(32), 1).as_bytes());
            assert!(message.contains("url"), "{bad}: {message}");
            assert!(
                message.contains("linux-x86_64-appimage"),
                "{bad}: {message}"
            );
        }
    }

    #[test]
    fn a_sha256_that_is_not_64_hex_digits_is_malformed() {
        for sha in [
            "ab".repeat(31) + "a",
            "ab".repeat(33),
            "zz".repeat(32),
            String::new(),
        ] {
            let message = malformed(json("6.0.1", &url("a"), &sha, 1).as_bytes());
            assert!(message.contains("sha256"), "{sha}: {message}");
        }
    }

    #[test]
    fn a_zero_size_is_malformed() {
        let message = malformed(json("6.0.1", &url("a"), &"ab".repeat(32), 0).as_bytes());
        assert!(message.contains("size"), "{message}");
    }

    #[test]
    fn a_version_that_is_not_semver_is_malformed() {
        for version in ["v6.0.1", "6.0", "", "latest"] {
            let message = malformed(json(version, &url("a"), &"ab".repeat(32), 1).as_bytes());
            assert!(message.contains("version"), "{version}: {message}");
        }
    }

    #[test]
    fn broken_json_is_malformed() {
        for body in [
            b"not json".as_slice(),
            &[0xff, 0xfe, 0xfd],
            br#"{"platforms":{}}"#,
            br#"{"version":"6.0.1"}"#,
            br#"{"version":6,"platforms":{}}"#,
            br#"{"version":"6.0.1","platforms":{"k":{"url":"x","sha256":"y","size":-1}}}"#,
        ] {
            malformed(body);
        }
    }

    #[test]
    fn asset_urls_with_encoded_separators_or_extra_parts_are_malformed() {
        let base = "https://github.com/PhongGuy/TF2Tools/releases/download/";
        for suffix in [
            "..%2F..%2Fevil/a",
            "v6.0.1/..%2fevil",
            "%2e%2e%2fevil/a",
            "v6.0.1/a%5Cb",
            "v6.0.1/a%2Eb",
            "?x",
            "#f",
            "/",
            "v6.0.1/",
            "v6.0.1",
            "v6.0.1/a/b",
            "v6.0.1//a",
            "v6.0.1/a?x",
            "v6.0.1/a#f",
        ] {
            let bad = format!("{base}{suffix}");
            let message = malformed(json("6.0.1", &bad, &"ab".repeat(32), 1).as_bytes());
            assert!(message.contains("url"), "{bad}: {message}");
        }
    }

    #[test]
    fn a_duplicate_platform_key_is_malformed() {
        let asset = format!(
            r#"{{"url":"{}","sha256":"{}","size":1}}"#,
            url("a"),
            "ab".repeat(32)
        );
        let body = format!(
            r#"{{"version":"6.0.1","platforms":{{"linux-x86_64-appimage":{asset},"linux-x86_64-appimage":{asset}}}}}"#
        );
        let message = malformed(body.as_bytes());
        assert!(message.contains("linux-x86_64-appimage"), "{message}");
    }

    #[test]
    fn a_huge_bad_value_gives_a_short_message() {
        let huge = "x".repeat(10_000);
        for body in [
            json(&huge, &url("a"), &"ab".repeat(32), 1),
            json(
                "6.0.1",
                &format!("https://evil.example/{huge}"),
                &"ab".repeat(32),
                1,
            ),
            json("6.0.1", &url("a"), &huge, 1),
        ] {
            let message = malformed(body.as_bytes());
            assert!(message.len() < 300, "{} bytes", message.len());
        }
    }
}

#[cfg(test)]
mod select_tests {
    use super::*;
    use crate::update::install::InstallTarget;
    use std::path::PathBuf;

    const GH: &str = "https://github.com";

    fn manifest(version: &str, keys: &[&str]) -> Manifest {
        let sha = "cd".repeat(32);
        let entries: Vec<String> = keys
            .iter()
            .map(|key| {
                format!(
                    r#""{key}":{{"url":"{GH}/PhongGuy/TF2Tools/releases/download/v{version}/{key}.bin","sha256":"{sha}","size":42}}"#
                )
            })
            .collect();
        let body = format!(
            r#"{{"version":"{version}","notes":"n","pub_date":"d","platforms":{{{}}}}}"#,
            entries.join(",")
        );
        Manifest::parse(body.as_bytes(), GH).unwrap()
    }

    fn appimage() -> InstallTarget {
        InstallTarget::AppImage(PathBuf::from("/apps/tf2tools.AppImage"))
    }

    fn on_linux(m: &Manifest, current: &str, target: &InstallTarget) -> Option<UpdateOffer> {
        select(m, current, target, "linux", "x86_64", GH).unwrap()
    }

    #[test]
    fn a_newer_version_for_this_platform_is_offered() {
        let m = manifest("6.0.1", &["linux-x86_64-appimage", "windows-x86_64-nsis"]);
        let offer = on_linux(&m, "6.0.0", &appimage()).unwrap();
        assert_eq!(offer.version(), "6.0.1");
        assert_eq!((offer.notes(), offer.pub_date()), ("n", "d"));
        assert_eq!(
            offer.release_page(),
            "https://github.com/PhongGuy/TF2Tools/releases/tag/v6.0.1"
        );
        let asset = offer.asset().unwrap();
        assert_eq!(
            asset.url(),
            "https://github.com/PhongGuy/TF2Tools/releases/download/v6.0.1/linux-x86_64-appimage.bin"
        );
        assert_eq!(
            (asset.sha256(), asset.size()),
            ("cd".repeat(32).as_str(), 42)
        );
    }

    #[test]
    fn the_same_or_an_older_version_is_not_offered() {
        let m = manifest("6.0.1", &["linux-x86_64-appimage"]);
        for current in ["6.0.1", "6.0.2", "7.0.0"] {
            assert_eq!(on_linux(&m, current, &appimage()), None, "{current}");
        }
    }

    #[test]
    fn a_release_is_newer_than_its_prerelease() {
        let m = manifest("6.0.0", &["linux-x86_64-appimage"]);
        assert!(on_linux(&m, "6.0.0-alpha.0", &appimage()).is_some());
    }

    #[test]
    fn build_metadata_alone_is_not_newer() {
        let m = manifest("6.0.1+ci.7", &["linux-x86_64-appimage"]);
        assert_eq!(on_linux(&m, "6.0.1", &appimage()), None);
    }

    #[test]
    fn a_stable_install_is_never_offered_a_prerelease() {
        for version in ["6.0.1-alpha.1", "7.0.0-rc.1"] {
            let m = manifest(version, &["linux-x86_64-appimage", "linux-x86_64-deb"]);
            assert_eq!(on_linux(&m, "6.0.0", &appimage()), None, "{version}");
            assert_eq!(
                on_linux(&m, "6.0.0", &InstallTarget::Unmanaged),
                None,
                "{version} unmanaged"
            );
        }
    }

    #[test]
    fn a_prerelease_install_is_offered_newer_prereleases_and_stables() {
        for version in ["6.0.0-alpha.2", "6.0.1-beta.1", "6.0.0", "6.0.1"] {
            let m = manifest(version, &["linux-x86_64-appimage"]);
            let offer = on_linux(&m, "6.0.0-alpha.1", &appimage());
            assert_eq!(
                offer.map(|o| o.version().to_owned()),
                Some(version.to_owned())
            );
        }
    }

    #[test]
    fn the_release_page_leaves_out_build_metadata() {
        let m = manifest("6.0.2+ci.7", &["linux-x86_64-appimage"]);
        let offer = on_linux(&m, "6.0.1", &appimage()).unwrap();
        assert_eq!(
            offer.release_page(),
            "https://github.com/PhongGuy/TF2Tools/releases/tag/v6.0.2"
        );
        assert_eq!(offer.version(), "6.0.2+ci.7");
        let m = manifest("6.0.2-rc.1+ci.7", &["linux-x86_64-appimage"]);
        let offer = on_linux(&m, "6.0.2-alpha.1", &appimage()).unwrap();
        assert_eq!(
            offer.release_page(),
            "https://github.com/PhongGuy/TF2Tools/releases/tag/v6.0.2-rc.1"
        );
    }

    #[test]
    fn no_entry_for_this_platform_is_not_offered() {
        let m = manifest("6.0.1", &["windows-x86_64-nsis"]);
        assert_eq!(on_linux(&m, "6.0.0", &appimage()), None);
        let m = manifest("6.0.1", &["linux-x86_64-appimage"]);
        let other_arch = select(&m, "6.0.0", &appimage(), "linux", "aarch64", GH).unwrap();
        assert_eq!(other_arch, None);
    }

    #[test]
    fn unmanaged_takes_any_entry_for_its_os_without_an_asset() {
        let m = manifest("6.0.1", &["linux-x86_64-appimage"]);
        let offer = on_linux(&m, "6.0.0", &InstallTarget::Unmanaged).unwrap();
        assert_eq!(offer.asset(), None);
        assert_eq!(
            offer.release_page(),
            "https://github.com/PhongGuy/TF2Tools/releases/tag/v6.0.1"
        );
        let windows_only = manifest("6.0.1", &["windows-x86_64-nsis"]);
        assert_eq!(
            on_linux(&windows_only, "6.0.0", &InstallTarget::Unmanaged),
            None
        );
        let on_windows = select(
            &windows_only,
            "6.0.0",
            &InstallTarget::Unmanaged,
            "windows",
            "x86_64",
            GH,
        )
        .unwrap();
        assert!(on_windows.is_some());
    }

    #[test]
    fn the_windows_installer_takes_the_nsis_entry() {
        let m = manifest("6.0.1", &["linux-x86_64-appimage", "windows-x86_64-nsis"]);
        let offer = select(
            &m,
            "6.0.0",
            &InstallTarget::WindowsInstaller,
            "windows",
            "x86_64",
            GH,
        )
        .unwrap()
        .unwrap();
        assert!(
            offer
                .asset()
                .unwrap()
                .url()
                .ends_with("/windows-x86_64-nsis.bin")
        );
    }

    #[test]
    fn an_unparsable_current_version_is_malformed() {
        let m = manifest("6.0.1", &["linux-x86_64-appimage"]);
        match select(&m, "dev", &appimage(), "linux", "x86_64", GH) {
            Err(UpdateCheckError::Malformed { message }) => {
                assert!(message.contains("current"), "{message}")
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }
}
