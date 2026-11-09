//! mastercomfig's base and override VPKs, whose release the addon-store sync records.

use rusqlite::OptionalExtension;

use super::StoreClient;
use crate::error::QueryError;

/// The release asset holding mastercomfig itself.
pub(crate) const BASE_ASSET: &str = "mastercomfig-base.vpk";

/// mastercomfig's latest release as the last addon-store sync saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MastercomfigRelease {
    tag: String,
    base_url: String,
    override_urls: [Option<String>; 3],
}

impl MastercomfigRelease {
    /// The release tag, such as `9.100.1`.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// The download URL of `mastercomfig-base.vpk`.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The download URL of `mastercomfig-addon-override-<n>.vpk`; None for n outside 1..=3 or
    /// an asset the release lacks.
    pub fn override_url(&self, n: u8) -> Option<&str> {
        let slot = usize::from(n).checked_sub(1)?;
        self.override_urls.get(slot)?.as_deref()
    }
}

/// The 0-based slot of an exact `mastercomfig-addon-override-<1..=3>.vpk` asset name.
pub(crate) fn override_slot(asset: &str) -> Option<usize> {
    (1..=3).find_map(|n| (asset == format!("mastercomfig-addon-override-{n}.vpk")).then_some(n - 1))
}

impl StoreClient {
    /// The recorded release, or None before any sync saw one. Blocking.
    pub fn mastercomfig_release(&self) -> Result<Option<MastercomfigRelease>, QueryError> {
        let row = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT tag, base_url, override_urls FROM mastercomfig_release WHERE id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, String>(2)?)),
                )
                .optional()
            })
            .map_err(|e| QueryError::Database {
                message: e.to_string(),
            })?;
        // A row whose override list does not parse reads as a release without overrides.
        Ok(row.map(|(tag, base_url, urls)| MastercomfigRelease {
            tag,
            base_url,
            override_urls: serde_json::from_str(&urls).unwrap_or_default(),
        }))
    }
}
