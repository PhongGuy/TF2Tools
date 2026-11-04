//! Store images: card thumbnails and screenshots, fetched only from the image hosts.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use super::StoreClient;
use super::http::{NetFailure, under_prefix};
use crate::error::ImageError;
use crate::panic_mark::{marked, marked_future};

/// The largest image accepted; store screenshots are far smaller.
const IMAGE_LIMIT: u64 = 10 * 1024 * 1024;

impl StoreClient {
    /// The bytes of a store image, from an allowlisted host (every redirect hop too), at most
    /// 10 MB. Needs a tokio runtime.
    pub async fn fetch_image(&self, url: &str) -> Result<Vec<u8>, ImageError> {
        let trusted = self
            .endpoints
            .image_prefixes()
            .iter()
            .any(|prefix| under_prefix(url, prefix));
        if !trusted {
            return Err(ImageError::UntrustedUrl {
                url: url.to_owned(),
            });
        }
        // The disk cache is best effort: a failed read is a miss and a failed write only warns.
        // Tokio catches a panic in this cache work, so it runs marked as handled.
        let cache = Arc::clone(&self.images);
        let key = url.to_owned();
        if let Ok(Some(bytes)) =
            tokio::task::spawn_blocking(move || marked(|| cache.load(&key))).await
        {
            return Ok(bytes);
        }
        // A plain GET: images are not API calls, so no spacing and no 429 retry. A 429 still
        // reads as rate limiting, so callers can back off.
        let body = match self.image_http.open(url).await {
            Err(NetFailure::Status { url, status: 429 }) => {
                return Err(ImageError::RateLimited { url });
            }
            other => other?,
        };
        let bytes = body.read_to_end(IMAGE_LIMIT).await?;
        // The caller has its bytes now; the cache write (and any sweep) finishes detached.
        let cache = Arc::clone(&self.images);
        let (key, copy) = (url.to_owned(), bytes.clone());
        tokio::spawn(marked_future(async move {
            let writer = Arc::clone(&cache);
            let failure =
                match tokio::task::spawn_blocking(move || marked(|| writer.store(&key, &copy)))
                    .await
                {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error.to_string()),
                    Err(error) => Some(error.to_string()),
                };
            if let Some(error) = failure {
                cache.warn_once(&error);
            }
        }));
        Ok(bytes)
    }
}

/// The cache size past which a write sweeps the cache.
pub(crate) const CACHE_HIGH: u64 = 256 * 1024 * 1024;
/// The cache size a sweep brings the cache down to.
pub(crate) const CACHE_LOW: u64 = 192 * 1024 * 1024;

/// Store images on disk, one file per URL, evicted oldest-mtime first. A hit refreshes the
/// mtime, so the mtime order is a least-recently-used order.
#[derive(Debug)]
pub(crate) struct DiskCache {
    dir: PathBuf,
    high: u64,
    low: u64,
    /// Bytes on disk; `None` until the first write scans the directory.
    size: Mutex<Option<u64>>,
    /// Set by the first failed write, so an unwritable cache warns once, not per image.
    warned: AtomicBool,
}

impl DiskCache {
    pub(crate) fn new(dir: PathBuf, high: u64, low: u64) -> Self {
        Self {
            dir,
            high,
            low,
            size: Mutex::new(None),
            warned: AtomicBool::new(false),
        }
    }

    /// Logs a failed cache write, but only the first one for this cache.
    fn warn_once(&self, error: &str) {
        if !self.warned.swap(true, Ordering::Relaxed) {
            tracing::warn!(dir = %self.dir.display(), error, "image cache write failed");
        }
    }

    /// The cache file for `url`: the lowercase hex SHA-256 of the URL under the cache dir.
    pub(crate) fn path_for(&self, url: &str) -> PathBuf {
        let name: String = Sha256::digest(url.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.dir.join(name)
    }

    /// The cached bytes for `url`, or `None` on a miss or a read error. A hit sets the
    /// file's mtime to now. An empty file is a miss and is removed.
    pub(crate) fn load(&self, url: &str) -> Option<Vec<u8>> {
        let path = self.path_for(url);
        let bytes = std::fs::read(&path).ok()?;
        if bytes.is_empty() {
            let _ = std::fs::remove_file(&path);
            return None;
        }
        // Failing to refresh the time only makes this file an earlier sweep candidate.
        if let Ok(file) = std::fs::File::options().write(true).open(&path) {
            let _ = file.set_modified(SystemTime::now());
        }
        Some(bytes)
    }

    /// Writes `bytes` for `url` through a temp file and a rename, so a reader never sees half
    /// a file, then sweeps down to the low mark once the cache is past the high mark.
    pub(crate) fn store(&self, url: &str, bytes: &[u8]) -> std::io::Result<()> {
        let mut size = self
            .size
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::fs::create_dir_all(&self.dir)?;
        let current = match *size {
            Some(current) => current,
            None => dir_size(&self.dir)?,
        };
        let path = self.path_for(url);
        let replaced = std::fs::metadata(&path).map_or(0, |meta| meta.len());
        let mut tmp = tempfile::NamedTempFile::new_in(&self.dir)?;
        tmp.write_all(bytes)?;
        tmp.persist(&path).map_err(|error| error.error)?;
        let grown = current.saturating_sub(replaced) + bytes.len() as u64;
        *size = Some(if grown > self.high {
            sweep(&self.dir, self.low)?
        } else {
            grown
        });
        Ok(())
    }
}

/// The total length of the files directly in `dir`.
fn dir_size(dir: &Path) -> std::io::Result<u64> {
    Ok(files(dir)?.iter().map(|(_, len, _)| len).sum())
}

/// Every file directly in `dir` with its length and mtime.
fn files(dir: &Path) -> std::io::Result<Vec<(PathBuf, u64, SystemTime)>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            found.push((entry.path(), meta.len(), meta.modified()?));
        }
    }
    Ok(found)
}

/// Deletes oldest-mtime files in `dir` until at most `low` bytes remain; returns the bytes left.
pub(crate) fn sweep(dir: &Path, low: u64) -> std::io::Result<u64> {
    let mut found = files(dir)?;
    found.sort_by_key(|(_, _, modified)| *modified);
    let mut total: u64 = found.iter().map(|(_, len, _)| len).sum();
    for (path, len, _) in found {
        if total <= low {
            break;
        }
        // A file another writer already removed counts as removed.
        match std::fs::remove_file(&path) {
            Ok(()) => total -= len,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => total -= len,
            Err(error) => return Err(error),
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn age(path: &Path, secs: u64) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(secs))
            .unwrap();
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_cache_file_is_named_by_the_sha256_of_the_url() {
        let cache = DiskCache::new(PathBuf::from("/c"), 1, 1);
        assert_eq!(
            cache.path_for("abc"),
            Path::new("/c/ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
    }

    #[test]
    fn a_stored_image_loads_back_and_a_miss_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(tmp.path().join("images"), CACHE_HIGH, CACHE_LOW);
        cache.store("u", b"abc").unwrap();
        assert_eq!(cache.load("u"), Some(b"abc".to_vec()));
        assert_eq!(cache.load("v"), None);
    }

    #[test]
    fn sweep_deletes_the_oldest_files_down_to_the_low_mark() {
        let tmp = tempfile::tempdir().unwrap();
        for (name, secs) in [("a", 400), ("b", 300), ("c", 200), ("d", 100)] {
            let path = tmp.path().join(name);
            std::fs::write(&path, [0u8; 100]).unwrap();
            age(&path, secs);
        }
        assert_eq!(sweep(tmp.path(), 250).unwrap(), 200);
        assert_eq!(names(tmp.path()), ["c", "d"]);
    }

    #[test]
    fn storing_past_the_high_mark_sweeps_to_the_low_mark() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("images");
        let cache = DiskCache::new(dir.clone(), 300, 150);
        for (url, secs) in [("u1", 1000), ("u2", 900), ("u3", 800)] {
            cache.store(url, &[0u8; 100]).unwrap();
            age(&cache.path_for(url), secs);
        }
        assert_eq!(names(&dir).len(), 3, "300 bytes is not past the high mark");
        cache.store("u4", &[0u8; 100]).unwrap();
        let newest = cache.path_for("u4");
        assert_eq!(names(&dir), [newest.file_name().unwrap().to_string_lossy()]);
    }
}
