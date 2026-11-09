//! mastercomfig's base and override VPKs, whose release the addon-store sync records.

use std::path::Path;

use rusqlite::OptionalExtension;
use tokio::io::AsyncWriteExt;

use super::{StoreClient, StoreKind};
use crate::archive::VPK_MAGIC;
use crate::error::{InstallMastercomfigError, QueryError};
use crate::library::{game_custom_dir, record_install, remove_file_if_present, write_setup_hook};
use crate::templates::BASE_VPK;
use crate::tf2::Tf2Install;

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

impl StoreClient {
    /// Downloads the recorded release's base VPK through a temp file in `tf/custom`, renames it
    /// over `mastercomfig-base.vpk`, deletes its .sound.cache, records the state row (keeping
    /// a saved preset and modules, else Medium and none) and writes `setup_hook.cfg`.
    /// Returns the installed tag. Holds the store's download lock throughout. Needs a tokio
    /// runtime.
    pub async fn install_mastercomfig(
        &self,
        install: &Tf2Install,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<String, InstallMastercomfigError> {
        // Waiting first lets a queued install see a sync's fresh release and a moved game.
        let _turn = self.download_lock.lock().await;
        let database = |e: rusqlite::Error| InstallMastercomfigError::Database {
            message: e.to_string(),
        };
        let release = self
            .mastercomfig_release()
            .map_err(|e| match e {
                QueryError::Database { message } => InstallMastercomfigError::Database { message },
                e @ QueryError::NotPaged { .. } => InstallMastercomfigError::Database {
                    message: e.to_string(),
                },
            })?
            .ok_or(InstallMastercomfigError::NoRelease)?;
        let custom = game_custom_dir(install)?;

        let io_at = |path: &Path| {
            let path = path.to_path_buf();
            move |source| InstallMastercomfigError::Io { path, source }
        };
        tokio::fs::create_dir_all(&custom)
            .await
            .map_err(io_at(&custom))?;
        // The dotted .part name is no VPK the game loads; it deletes itself on early return.
        let part = tempfile::Builder::new()
            .prefix(".tf2tools-")
            .suffix(".part")
            .tempfile_in(&custom)
            .map_err(io_at(&custom))?;
        self.fetch_base(release.base_url(), part.path(), progress)
            .await?;
        let dest = custom.join(BASE_VPK);
        part.persist(&dest).map_err(|e| io_at(&dest)(e.error))?;

        // The game rebuilds a deleted sound cache; a stale one would point into the old VPK.
        remove_file_if_present(&custom.join(format!("{BASE_VPK}.sound.cache")))?;
        // A failure from here on leaves the new VPK in place; installing again recovers it.
        let preset = record_install(&self.db, release.tag()).map_err(database)?;
        write_setup_hook(&custom, preset)?;
        tracing::info!(version = release.tag(), "mastercomfig installed");
        Ok(release.tag().to_owned())
    }

    /// Streams the base VPK into `dest`, failing as soon as it passes the addon cap or its
    /// first bytes are not a VPK's.
    async fn fetch_base(
        &self,
        url: &str,
        dest: &Path,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<(), InstallMastercomfigError> {
        let limit = self.config.size_cap(StoreKind::Addons);
        let io = |source| InstallMastercomfigError::Io {
            path: dest.to_path_buf(),
            source,
        };
        let not_a_vpk = || InstallMastercomfigError::NotAVpk {
            url: url.to_owned(),
        };
        let mut body = self.http.open(url).await?;
        let total = body.total();
        let mut out = tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(dest)
            .await
            .map_err(io)?;
        let mut head = Vec::with_capacity(VPK_MAGIC.len());
        let mut downloaded = 0u64;
        while let Some(chunk) = body.chunk().await? {
            downloaded += chunk.len() as u64;
            if downloaded > limit {
                return Err(InstallMastercomfigError::TooLarge {
                    url: url.to_owned(),
                    limit,
                });
            }
            if head.len() < VPK_MAGIC.len() {
                let take = (VPK_MAGIC.len() - head.len()).min(chunk.len());
                head.extend_from_slice(&chunk[..take]);
                if !VPK_MAGIC.starts_with(&head) {
                    return Err(not_a_vpk());
                }
            }
            out.write_all(&chunk).await.map_err(io)?;
            progress(downloaded, total);
        }
        if head.as_slice() != VPK_MAGIC {
            return Err(not_a_vpk());
        }
        out.flush().await.map_err(io)?;
        out.sync_all().await.map_err(io)?;
        Ok(())
    }
}
