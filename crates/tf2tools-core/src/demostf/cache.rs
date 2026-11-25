//! A size-capped disk cache of downloaded demos, keyed by demo id.

use std::path::{Path, PathBuf};

use super::client::{part_beside, write_failure};
use super::{DemoSummary, DemosClient};
use crate::error::{DemoCacheError, DemoClearError, DemosError};
use crate::store::{dir_size, sweep};

/// The cache size past which a download triggers a sweep.
pub const DEMO_CACHE_HIGH: u64 = 2 * 1024 * 1024 * 1024;
/// The size a sweep deletes down to.
pub const DEMO_CACHE_LOW: u64 = 1024 * 1024 * 1024;

/// Downloaded demos in one folder, named by id, trimmed least recently opened first.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DemoCache {
    dir: PathBuf,
    high: u64,
    low: u64,
}

impl DemoCache {
    /// A cache in `dir` with the default marks. Touches no files.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            high: DEMO_CACHE_HIGH,
            low: DEMO_CACHE_LOW,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_marks(mut self, high: u64, low: u64) -> Self {
        self.high = high;
        self.low = low;
        self
    }

    /// Where demo `id` lives in the cache.
    pub fn path_for(&self, id: u64) -> PathBuf {
        self.dir.join(format!("{id}.dem"))
    }

    /// The cached file for `demo`, downloaded first when missing. A hit asks nothing and
    /// refreshes the file's age; a private demo is refused without a request; a download
    /// past the high mark sweeps older files down to the low mark, never the new one.
    pub async fn open_or_download(
        &self,
        client: &DemosClient,
        demo: &DemoSummary,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<PathBuf, DemoCacheError> {
        let path = self.path_for(demo.id());
        if std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() > 0) {
            // Best effort: the sweep orders by mtime, so an open counts as use.
            if let Ok(file) = std::fs::File::options().write(true).open(&path) {
                let _ = file.set_modified(std::time::SystemTime::now());
            }
            return Ok(path);
        }
        if demo.url().is_empty() {
            return Err(DemoCacheError::Private);
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| DemoCacheError::Folder {
            message: e.to_string(),
        })?;
        client.download_demo(demo, &path, progress).await?;
        self.trim(&path);
        Ok(path)
    }

    /// Saves `demo` to `dest`: a cached copy when Watch already fetched it, else a fresh
    /// download straight to `dest`. Never fills or sweeps the cache.
    pub async fn save_to(
        &self,
        client: &DemosClient,
        demo: &DemoSummary,
        dest: &Path,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<(), DemosError> {
        let cached = self.path_for(demo.id());
        let hit = std::fs::metadata(&cached).is_ok_and(|m| m.is_file() && m.len() > 0);
        if !hit {
            return client.download_demo(demo, dest, progress).await;
        }
        // The copy is blocking file work, so it runs off the small async runtime.
        // Tokio turns a panic in it into a Write error, so it runs marked as handled.
        let dest = dest.to_owned();
        let len = tokio::task::spawn_blocking(move || {
            crate::panic_mark::marked(|| copy_through_part(&cached, &dest))
        })
        .await
        .map_err(|e| DemosError::Write {
            message: e.to_string(),
        })??;
        progress(len, Some(len));
        Ok(())
    }

    /// Deletes every finished demo (`<id>.dem` files) and returns the bytes freed.
    /// Partial downloads, links and anything else are kept; a missing folder frees 0.
    ///
    /// # Errors
    /// `DemoClearError::Io` when the folder cannot be listed, or for the first demo that
    /// could not be deleted; the rest are still tried.
    pub fn clear(&self) -> Result<u64, DemoClearError> {
        let folder_error = |source| DemoClearError::Io {
            path: self.dir.clone(),
            source,
        };
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(folder_error(e)),
        };
        let mut freed = 0_u64;
        let mut first_error = None;
        for entry in entries {
            let path = entry.map_err(folder_error)?.path();
            if !path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_demo_name)
            {
                continue;
            }
            // Only plain files go, so a link never deletes what it points at.
            let len = match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_file() => meta.len(),
                _ => continue,
            };
            // A demo already gone was freed by someone else, so it still counts.
            match std::fs::remove_file(&path) {
                Ok(()) => freed = freed.saturating_add(len),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    freed = freed.saturating_add(len);
                }
                Err(source) => {
                    first_error.get_or_insert(DemoClearError::Io { path, source });
                }
            }
        }
        first_error.map_or(Ok(freed), Err)
    }

    /// Sweeps when over the high mark, sparing `keep` and in-flight dot files. Failures only warn.
    fn trim(&self, keep: &Path) {
        match dir_size(&self.dir) {
            Ok(size) if size > self.high => {
                let spare = |p: &Path| {
                    p == keep
                        || p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                };
                if let Err(e) = sweep(&self.dir, self.low, &spare) {
                    tracing::warn!("demo cache sweep failed: {e}");
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("demo cache size check failed: {e}"),
        }
    }
}

/// Whether `name` is a finished demo, `<digits>.dem`; partial downloads never match.
fn is_demo_name(name: &str) -> bool {
    name.strip_suffix(".dem")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

/// Copies `src` to `dest` through a part file readable by all, returning the byte count.
/// Same part-file scheme as a download, so a failed copy never touches `dest`.
fn copy_through_part(src: &Path, dest: &Path) -> Result<u64, DemosError> {
    let mut part = part_beside(dest)?;
    let mut file = std::fs::File::open(src).map_err(write_failure)?;
    let len = std::io::copy(&mut file, part.as_file_mut()).map_err(write_failure)?;
    part.as_file().sync_all().map_err(write_failure)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        part.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o644))
            .map_err(write_failure)?;
    }
    part.persist(dest).map_err(|e| write_failure(e.error))?;
    Ok(len)
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
