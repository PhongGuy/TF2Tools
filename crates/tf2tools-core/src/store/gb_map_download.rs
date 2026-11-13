//! Downloading a GameBanana map item's newest file into a scratch dir and finding its
//! maps. Nothing here writes to `tf/custom`.

use std::path::Path;

use tempfile::TempDir;

use super::gb_addon_download::{has_extension, holds_exe};
use super::{AddonProfile, DownloadProgress, StoreClient};
use crate::archive::extract_archive;
use crate::error::GbAddonFetchError;
use crate::library::{GbId, io_at};
use crate::map_payloads::{MAX_MAPS, MapPayload, ends_with_ignore_case, find_maps};

/// A downloaded, extracted map item waiting to be installed. Its scratch dir, which holds
/// every map, is removed when it is dropped.
#[derive(Debug)]
#[non_exhaustive]
pub struct GbMapDownload {
    /// Held for its Drop, which removes the scratch dir.
    pub(crate) _scratch: TempDir,
    pub(crate) gb_id: GbId,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) file_id: u64,
    pub(crate) file_date: i64,
    pub(crate) file_name: String,
    pub(crate) maps: Vec<MapPayload>,
}

impl GbMapDownload {
    /// The GameBanana item this came from.
    pub fn gb_id(&self) -> GbId {
        self.gb_id
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

    /// When the downloaded file was added, in Unix seconds.
    pub fn file_date(&self) -> i64 {
        self.file_date
    }

    /// The store's name for the downloaded file.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// The maps the download holds, in walk order.
    pub fn maps(&self) -> &[MapPayload] {
        &self.maps
    }

    /// The scratch dir the maps were unpacked into, removed with the download.
    pub(crate) fn scratch_dir(&self) -> &Path {
        self._scratch.path()
    }
}

impl StoreClient {
    /// Why [`download_gb_map`](Self::download_gb_map) would refuse `profile` from its
    /// listing alone, before any request, judged against the map size cap.
    pub fn gb_map_refusal(&self, profile: &AddonProfile) -> Option<GbAddonFetchError> {
        self.listed_file(profile, self.config.map_size_cap()).err()
    }

    /// Downloads `profile`'s newest file under the map size cap, exactly as an add-on is
    /// downloaded, then finds its maps. The scratch dir lives in the returned download.
    pub async fn download_gb_map(
        &self,
        profile: &AddonProfile,
        progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    ) -> Result<GbMapDownload, GbAddonFetchError> {
        let cap = self.config.map_size_cap();
        let (scratch, file, maps) = self
            .fetch_unpacked(profile, cap, progress, move |saved, tree, name| {
                unpack_maps(saved, tree, name, cap)
            })
            .await?;
        Ok(GbMapDownload {
            _scratch: scratch,
            gb_id: profile.id(),
            name: profile.name().to_owned(),
            author: profile.author().to_owned(),
            file_id: file.id(),
            file_date: file.date_added(),
            file_name: file.name().to_owned(),
            maps,
        })
    }
}

/// Unpacks `saved` into `tree` and returns its maps, decompressing `.bsp.bz2` within `cap`
/// bytes. A bare map file moves in whole; programs alone or no map at all are refused.
fn unpack_maps(
    saved: &Path,
    tree: &Path,
    file_name: &str,
    cap: u64,
) -> Result<Vec<MapPayload>, GbAddonFetchError> {
    std::fs::create_dir_all(tree).map_err(io_at(tree))?;
    let unknown = |payloads| GbAddonFetchError::UnknownLayout {
        file_name: file_name.to_owned(),
        payloads,
    };
    let only_exe = || GbAddonFetchError::OnlyExe {
        file_name: file_name.to_owned(),
    };

    if is_bare_map(saved) {
        let dest = tree.join(saved.file_name().unwrap_or_default());
        std::fs::rename(saved, &dest).map_err(io_at(&dest))?;
    } else if extract_archive(saved, tree)? == saved {
        return Err(if has_extension(saved, "exe") {
            only_exe()
        } else {
            unknown(0)
        });
    }

    let maps = find_maps(tree, cap)?;
    if maps.is_empty() && holds_exe(tree) {
        return Err(only_exe());
    }
    if maps.is_empty() || maps.len() > MAX_MAPS {
        return Err(unknown(maps.len()));
    }
    Ok(maps)
}

/// Whether `saved` is a lone map file, compressed or not, which no extractor would open.
fn is_bare_map(saved: &Path) -> bool {
    let name = saved.file_name().unwrap_or_default().to_string_lossy();
    ends_with_ignore_case(&name, ".bsp.bz2") || ends_with_ignore_case(&name, ".bsp")
}
