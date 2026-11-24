//! A size-capped disk cache of downloaded demos, keyed by demo id.

use std::path::{Path, PathBuf};

use super::{DemoSummary, DemosClient};
use crate::error::DemoCacheError;
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

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
