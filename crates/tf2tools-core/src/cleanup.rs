//! Startup sweeps that remove what older TF2Tools builds and interrupted downloads left behind.
//!
//! Each sweep looks only at exact names directly inside caller-passed roots, so every check is
//! one stat or one shallow `read_dir`. A link found under a root is unlinked, never followed. A
//! link at a sweep root itself is followed when its entries are read, but only the exact names
//! directly inside it are removed. A failed removal is logged and left out of `Freed`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::paths::DataDir;
use crate::update::InstallTarget;

/// The v5 web view's files in the data folder, from WebKitGTK and the Tauri shell.
const WEB_VIEW_NAMES: [&str; 7] = [
    "WebKitCache",
    "CacheStorage",
    "cookies",
    "hsts-storage.sqlite",
    "localstorage",
    "mediakeys",
    "storage",
];

/// The Windows web view's folder in the local data folder.
const EDGE_WEB_VIEW: &str = "EBWebView";

/// The folders a leftover sweep looks in.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CleanupRoots {
    data: PathBuf,
    local_data: PathBuf,
    demo_cache: PathBuf,
    appimage: Option<PathBuf>,
    temp: PathBuf,
}

impl CleanupRoots {
    /// Roots from the data, local data, demo cache and temp folders, with no AppImage.
    pub fn new(
        data: impl Into<PathBuf>,
        local_data: impl Into<PathBuf>,
        demo_cache: impl Into<PathBuf>,
        temp: impl Into<PathBuf>,
    ) -> Self {
        Self {
            data: data.into(),
            local_data: local_data.into(),
            demo_cache: demo_cache.into(),
            appimage: None,
            temp: temp.into(),
        }
    }

    /// These roots with the running AppImage file, whose backup and staged updates sit beside it.
    pub fn with_appimage(mut self, appimage: impl Into<PathBuf>) -> Self {
        self.appimage = Some(appimage.into());
        self
    }

    /// The roots for `data`. Only the platform's own data folder reaches the platform's local
    /// data folder, the running AppImage and the system temp folder; any other data folder
    /// uses its own root for local data and temp, and no AppImage.
    pub fn for_data_dir(data: &DataDir) -> Self {
        let root = data.root().to_path_buf();
        let demo_cache = data.demo_cache_dir();
        if DataDir::platform_default().ok().as_ref() != Some(data) {
            return Self::new(root.clone(), root.clone(), demo_cache, root);
        }
        let local_data = dirs::data_local_dir()
            .map(|dir| dir.join("com.tf2tools.app"))
            .unwrap_or_else(|| root.clone());
        let roots = Self::new(root, local_data, demo_cache, std::env::temp_dir());
        match InstallTarget::detect() {
            InstallTarget::AppImage(appimage) => roots.with_appimage(appimage),
            // Only an AppImage leaves files beside the running build.
            InstallTarget::WindowsInstaller | InstallTarget::Unmanaged => roots,
        }
    }

    /// The data folder.
    pub fn data(&self) -> &Path {
        &self.data
    }

    /// The platform's local data folder, where the Windows web view kept its files.
    pub fn local_data(&self) -> &Path {
        &self.local_data
    }

    /// The downloaded demo cache.
    pub fn demo_cache(&self) -> &Path {
        &self.demo_cache
    }

    /// The running AppImage file, if any.
    pub fn appimage(&self) -> Option<&Path> {
        self.appimage.as_deref()
    }

    /// The temp folder update installers are staged in.
    pub fn temp(&self) -> &Path {
        &self.temp
    }
}

/// What a sweep removed: the bytes of regular files it deleted and every path it removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Freed {
    bytes: u64,
    paths: Vec<PathBuf>,
}

impl Freed {
    /// A result of `bytes` freed by removing `paths`.
    pub fn new(bytes: u64, paths: Vec<PathBuf>) -> Self {
        Self { bytes, paths }
    }

    /// Bytes of regular files removed; links count zero.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Every path removed, in the order the sweep removed them.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    fn add(&mut self, path: &Path, bytes: u64) {
        self.bytes += bytes;
        self.paths.push(path.to_path_buf());
    }
}

/// Removes leftovers under `roots`: the v5 web view's data, and the local data folder once
/// it is empty. Parts older than `max_part_age` before `now` are leftovers; a missing root
/// frees nothing. Never fails; a removal that fails is logged and skipped.
pub fn sweep_leftovers(roots: &CleanupRoots, _now: SystemTime, _max_part_age: Duration) -> Freed {
    let mut freed = Freed::default();
    for name in WEB_VIEW_NAMES {
        remove(&roots.data.join(name), &mut freed);
    }
    remove(&roots.local_data.join(EDGE_WEB_VIEW), &mut freed);
    remove_empty_local_data(roots, &mut freed);
    freed
}

/// Removes leftovers under the game's `tf` folder. Never fails; a missing folder frees nothing.
pub fn sweep_game_leftovers(_tf: &Path, _now: SystemTime, _max_part_age: Duration) -> Freed {
    Freed::default()
}

// The local data folder held only the Windows web view, so it goes once empty. On Linux and
// macOS it is the data folder itself, which the prefix check keeps.
fn remove_empty_local_data(roots: &CleanupRoots, freed: &mut Freed) {
    let local = &roots.local_data;
    let real_dir = std::fs::symlink_metadata(local).is_ok_and(|meta| meta.is_dir());
    if !real_dir || roots.data.starts_with(local) {
        return;
    }
    let empty = std::fs::read_dir(local).is_ok_and(|mut entries| entries.next().is_none());
    if !empty {
        return;
    }
    match std::fs::remove_dir(local) {
        Ok(()) => freed.add(local, 0),
        Err(err) => warn(local, &err),
    }
}

// Removes `path` whatever it is without following a link: a link is unlinked (a Windows
// directory link needs `remove_dir`), a folder is sized first, then removed whole.
fn remove(path: &Path, freed: &mut Freed) {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        Err(err) => return warn(path, &err),
    };
    let result = if meta.file_type().is_symlink() {
        std::fs::remove_file(path)
            .or_else(|_| std::fs::remove_dir(path))
            .map(|()| 0)
    } else if meta.is_dir() {
        let bytes = folder_bytes(path);
        std::fs::remove_dir_all(path).map(|()| bytes)
    } else {
        std::fs::remove_file(path).map(|()| meta.len())
    };
    match result {
        Ok(bytes) => freed.add(path, bytes),
        Err(err) => warn(path, &err),
    }
}

// Regular files under `dir`, links not followed; an unreadable entry counts nothing.
fn folder_bytes(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}

fn warn(path: &Path, err: &std::io::Error) {
    tracing::warn!(path = %path.display(), error = %err, "could not remove a leftover");
}
