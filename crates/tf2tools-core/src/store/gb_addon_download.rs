//! Downloading a GameBanana add-on's newest file into a scratch dir and listing the
//! payloads it could install. Nothing here writes to `tf/custom`.

use std::path::Path;

use tempfile::TempDir;

use super::download::save_name;
use super::pacer::Priority;
use super::{AddonFile, AddonProfile, AddonSection, DownloadProgress, StoreClient};
use walkdir::WalkDir;

use crate::archive::{VPK_MAGIC, extract_archive, sniff};
use crate::error::{BrowseError, GbAddonCheckError, GbAddonFetchError, LibraryError};
use crate::library::{GbAddonCheck, GbId, Library, io_at};
use crate::payloads::{MAX_PAYLOADS, Payload, find_payloads};

/// A downloaded, extracted add-on waiting to be installed. Its scratch dir, which holds
/// every payload, is removed when it is dropped.
#[derive(Debug)]
#[non_exhaustive]
pub struct GbAddonDownload {
    /// Held for its Drop, which removes the scratch dir.
    pub(crate) _scratch: TempDir,
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

    /// The payloads to install over an add-on that had `previous` chosen, without asking,
    /// or None to ask. A lone payload is kept whatever it is called; otherwise the previous
    /// names are kept, in their order, only when every one of them is still offered.
    pub fn kept_payloads(&self, previous: &[String]) -> Option<Vec<String>> {
        if let [only] = self.payloads.as_slice() {
            return Some(vec![only.name().to_owned()]);
        }
        let all_offered = !previous.is_empty()
            && previous
                .iter()
                .all(|name| self.payloads.iter().any(|p| p.name() == name.as_str()));
        all_offered.then(|| previous.to_vec())
    }
}

impl StoreClient {
    /// Checks every due add-on once, through the pacer at background priority, and returns
    /// how many were checked. Unreachable (Network, RateLimited, Throttled) stops the run;
    /// any other failure records `Unchanged` for that item and goes on.
    pub async fn check_gb_addon_updates(&self, now: i64) -> Result<usize, GbAddonCheckError> {
        let library = Library::new(self.db.clone(), self.dir.clone());
        self.check_gb_items(library.gb_addons_due_for_check(now)?, |id, check| {
            library.record_gb_addon_check(id, check, now)
        })
        .await
    }

    /// Checks every due GameBanana HUD item once, exactly as add-ons are checked.
    pub async fn check_gb_hud_updates(&self, now: i64) -> Result<usize, GbAddonCheckError> {
        let library = Library::new(self.db.clone(), self.dir.clone());
        self.check_gb_items(library.gb_huds_due_for_check(now)?, |id, check| {
            library.record_gb_hud_check(id, check, now)
        })
        .await
    }

    /// Asks GameBanana for each of `ids` in turn and hands what it learned to `record`.
    async fn check_gb_items(
        &self,
        ids: Vec<GbId>,
        record: impl Fn(GbId, GbAddonCheck) -> Result<(), LibraryError>,
    ) -> Result<usize, GbAddonCheckError> {
        let catalog = self.addon_catalog();
        let mut checked = 0;
        for gb_id in ids {
            // Only an unreachable GameBanana ends the run, so the rest stay due for later;
            // one item's bad page waits a day like a clean check.
            let check = match catalog.profile_at(gb_id, Priority::Background).await {
                Ok(profile) => profile
                    .files()
                    .iter()
                    .map(AddonFile::date_added)
                    .max()
                    .map_or(GbAddonCheck::Unchanged, |file_date| GbAddonCheck::Latest {
                        file_date,
                    }),
                Err(BrowseError::Gone { .. } | BrowseError::Http { status: 404, .. }) => {
                    GbAddonCheck::Gone
                }
                Err(
                    e @ (BrowseError::Network { .. }
                    | BrowseError::RateLimited { .. }
                    | BrowseError::Throttled { .. }),
                ) => return Err(e.into()),
                Err(_) => GbAddonCheck::Unchanged,
            };
            record(gb_id, check)?;
            checked += 1;
        }
        Ok(checked)
    }
}

impl StoreClient {
    /// Why [`download_gb_addon`](Self::download_gb_addon) would refuse `profile` from its
    /// listing alone, before any request: no files, a failed virus scan, or a listed size
    /// over this client's configured cap. None when the download may start.
    pub fn gb_addon_refusal(&self, profile: &AddonProfile) -> Option<GbAddonFetchError> {
        self.listed_file(profile, self.config.gb_addon_size_cap())
            .err()
    }

    /// `profile`'s newest file when its listing passes every pre-download check, with
    /// `limit` as the size cap.
    pub(super) fn listed_file<'p>(
        &self,
        profile: &'p AddonProfile,
        limit: u64,
    ) -> Result<&'p AddonFile, GbAddonFetchError> {
        let file = profile.files().first().ok_or(GbAddonFetchError::NoFiles {
            gb_id: profile.id(),
        })?;
        if file.av_result() != "clean" {
            return Err(GbAddonFetchError::VirusScan {
                file_name: file.name().to_owned(),
                result: file.av_result().to_owned(),
            });
        }
        if file.size().is_some_and(|size| size > limit) {
            return Err(GbAddonFetchError::TooLarge {
                url: file.download_url().to_owned(),
                limit,
            });
        }
        Ok(file)
    }

    /// Downloads `profile`'s newest file for `section` into `<data>/tmp`, one download at a
    /// time, then extracts it and finds its payloads. The scratch dir lives in the returned
    /// download and goes when it is dropped; every error removes it.
    pub async fn download_gb_addon(
        &self,
        section: AddonSection,
        profile: &AddonProfile,
        progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    ) -> Result<GbAddonDownload, GbAddonFetchError> {
        let (scratch, file, payloads) = self
            .fetch_unpacked(profile, self.config.gb_addon_size_cap(), progress, unpack)
            .await?;
        Ok(GbAddonDownload {
            _scratch: scratch,
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

    /// Downloads `profile`'s newest file, at most `limit` bytes, into a scratch dir under
    /// `<data>/tmp`, one download at a time, and hands it to `unpack(saved, tree, file_name)`
    /// on a blocking thread. Returns the scratch dir, the file and what `unpack` made; every
    /// error removes the dir.
    pub(super) async fn fetch_unpacked<'p, T: Send + 'static>(
        &self,
        profile: &'p AddonProfile,
        limit: u64,
        progress: &(dyn Fn(DownloadProgress) + Send + Sync),
        unpack: impl FnOnce(&Path, &Path, &str) -> Result<T, GbAddonFetchError> + Send + 'static,
    ) -> Result<(TempDir, &'p AddonFile, T), GbAddonFetchError> {
        // Every refusal the listing alone can settle comes before any request is made.
        let file = self.listed_file(profile, limit)?;

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
        let file_name = file.name().to_owned();
        let (scratch, found) = tokio::task::spawn_blocking(move || {
            crate::panic_mark::marked(|| {
                let found = unpack(&saved, &scratch.path().join("tree"), &file_name)?;
                Ok::<_, GbAddonFetchError>((scratch, found))
            })
        })
        .await
        .map_err(|e| GbAddonFetchError::Interrupted {
            message: e.to_string(),
        })??;
        Ok((scratch, file, found))
    }
}

/// Unpacks `saved` into `tree` and returns its payloads, refusing a download that holds
/// only programs or no recognisable layout. `file_name` is the store's name for it.
fn unpack(saved: &Path, tree: &Path, file_name: &str) -> Result<Vec<Payload>, GbAddonFetchError> {
    std::fs::create_dir_all(tree).map_err(io_at(tree))?;
    let unknown = |payloads| GbAddonFetchError::UnknownLayout {
        file_name: file_name.to_owned(),
        payloads,
    };
    let only_exe = || GbAddonFetchError::OnlyExe {
        file_name: file_name.to_owned(),
    };

    // A bare VPK is installed as the file it is, so it moves into the tree whole;
    // anything else that is not an archive has nothing a walk could find.
    if has_extension(saved, "vpk") && sniff(saved)?.starts_with(VPK_MAGIC) {
        let dest = tree.join(saved.file_name().unwrap_or_default());
        std::fs::rename(saved, &dest).map_err(io_at(&dest))?;
    } else if extract_archive(saved, tree)? == saved {
        return Err(if has_extension(saved, "exe") {
            only_exe()
        } else {
            unknown(0)
        });
    }

    let payloads = find_payloads(tree)?;
    if payloads.is_empty() && holds_exe(tree) {
        return Err(only_exe());
    }
    if payloads.is_empty() || payloads.len() > MAX_PAYLOADS {
        return Err(unknown(payloads.len()));
    }
    Ok(payloads)
}

pub(super) fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// Whether any `.exe` sits in `tree`; an unreadable entry counts as none.
pub(super) fn holds_exe(tree: &Path) -> bool {
    WalkDir::new(tree)
        .into_iter()
        .filter_map(Result::ok)
        .any(|e| e.file_type().is_file() && has_extension(e.path(), "exe"))
}
