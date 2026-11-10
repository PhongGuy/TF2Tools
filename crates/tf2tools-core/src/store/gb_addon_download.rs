//! Downloading a GameBanana add-on's newest file into a scratch dir and listing the
//! payloads it could install. Nothing here writes to `tf/custom`.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::download::save_name;
use super::{AddonProfile, AddonSection, DownloadProgress, StoreClient};
use crate::archive::extract_archive;
use crate::error::GbAddonFetchError;
use crate::library::GbId;
use crate::payloads::{Payload, find_payloads};

/// A downloaded, extracted add-on waiting to be installed. Its scratch dir, which holds
/// every payload, is removed when it is dropped.
#[derive(Debug)]
#[non_exhaustive]
pub struct GbAddonDownload {
    #[expect(dead_code, reason = "held for its Drop, which removes the scratch dir")]
    pub(crate) scratch: TempDir,
    pub(crate) gb_id: GbId,
    pub(crate) section: &'static str,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) file_id: u64,
    pub(crate) file_date: i64,
    pub(crate) file_name: String,
    pub(crate) payloads: Vec<Payload>,
}

impl GbAddonDownload {
    /// The GameBanana item this came from.
    pub fn gb_id(&self) -> GbId {
        self.gb_id
    }

    /// The key of the add-on section it was downloaded for.
    pub fn section(&self) -> &'static str {
        self.section
    }

    /// The item's name on GameBanana.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The item's submitter on GameBanana.
    pub fn author(&self) -> &str {
        &self.author
    }

    /// The GameBanana id of the downloaded file.
    pub fn file_id(&self) -> u64 {
        self.file_id
    }

    /// The store's name for the downloaded file.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// When the downloaded file was added, in Unix seconds.
    pub fn file_date(&self) -> i64 {
        self.file_date
    }

    /// What the download could install, largest first.
    pub fn payloads(&self) -> &[Payload] {
        &self.payloads
    }
}

impl StoreClient {
    /// Downloads `profile`'s newest file for `section` into `<data>/tmp`, one download at a
    /// time, then extracts it and finds its payloads. The scratch dir lives in the returned
    /// download and goes when it is dropped; every error removes it.
    pub async fn download_gb_addon(
        &self,
        section: AddonSection,
        profile: &AddonProfile,
        progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    ) -> Result<GbAddonDownload, GbAddonFetchError> {
        // Every refusal the listing alone can settle comes before any request is made.
        let file = profile.files().first().ok_or(GbAddonFetchError::NoFiles {
            gb_id: profile.id(),
        })?;
        if file.av_result() != "clean" {
            return Err(GbAddonFetchError::VirusScan {
                file_name: file.name().to_owned(),
                result: file.av_result().to_owned(),
            });
        }
        let limit = self.config.gb_addon_size_cap();
        if file.size().is_some_and(|size| size > limit) {
            return Err(GbAddonFetchError::TooLarge {
                url: file.download_url().to_owned(),
                limit,
            });
        }

        let _turn = self.download_lock.lock().await;
        let tmp = self.dir.tmp_dir();
        tokio::fs::create_dir_all(&tmp).await.map_err(io_at(&tmp))?;
        let scratch = tempfile::Builder::new()
            .prefix("download-")
            .tempdir_in(&tmp)
            .map_err(io_at(&tmp))?;
        let saved = scratch.path().join(save_name(file.name()));
        self.stream_to(
            file.download_url(),
            file.md5(),
            file.name(),
            limit,
            &saved,
            progress,
        )
        .await?;

        // The blocking half owns the scratch dir, so a dropped future cannot remove files
        // it is still extracting; it hands the dir back only when the walk succeeded.
        // Tokio turns a panic in it into Interrupted, so it runs marked as handled.
        let (scratch, payloads) = tokio::task::spawn_blocking(move || {
            crate::panic_mark::marked(|| {
                let payloads = unpack(&saved, &scratch.path().join("tree"))?;
                Ok::<_, GbAddonFetchError>((scratch, payloads))
            })
        })
        .await
        .map_err(|e| GbAddonFetchError::Interrupted {
            message: e.to_string(),
        })??;

        Ok(GbAddonDownload {
            scratch,
            gb_id: profile.id(),
            section: section.key(),
            name: profile.name().to_owned(),
            author: profile.author().to_owned(),
            file_id: file.id(),
            file_date: file.date_added(),
            file_name: file.name().to_owned(),
            payloads,
        })
    }
}

/// Extracts `saved` into `tree` and walks the tree for payloads.
fn unpack(saved: &Path, tree: &Path) -> Result<Vec<Payload>, GbAddonFetchError> {
    std::fs::create_dir_all(tree).map_err(io_at(tree))?;
    extract_archive(saved, tree)?;
    Ok(find_payloads(tree)?)
}

fn io_at(path: &Path) -> impl FnOnce(std::io::Error) -> GbAddonFetchError + '_ {
    move |source| GbAddonFetchError::Io {
        path: PathBuf::from(path),
        source,
    }
}
