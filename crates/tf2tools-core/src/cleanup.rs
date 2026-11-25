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

    /// Bytes of regular files removed; links count zero, and so does a folder only partly removed.
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

/// Removes leftovers under `roots`: the v5 web view's data, the local data folder once it is
/// empty, crash-left demo parts, stale update downloads, the previous AppImage backup and v5
/// webp thumbnails. Parts older than `max_part_age` before `now` are leftovers; a missing root
/// frees nothing. Never fails; a removal that fails is logged and skipped.
pub fn sweep_leftovers(roots: &CleanupRoots, now: SystemTime, max_part_age: Duration) -> Freed {
    let mut freed = Freed::default();
    for name in WEB_VIEW_NAMES {
        remove(&roots.data.join(name), &mut freed);
    }
    remove(&roots.local_data.join(EDGE_WEB_VIEW), &mut freed);
    remove_empty_local_data(roots, &mut freed);
    let aged = Aged { now, max_part_age };
    remove_aged(&roots.demo_cache, is_demo_part, aged, &mut freed);
    if let Some(appimage) = &roots.appimage {
        sweep_beside_appimage(appimage, aged, &mut freed);
    }
    remove_aged(&roots.temp, is_staged_installer, aged, &mut freed);
    remove_v5_thumbnails(&roots.data.join("library").join("huds"), &mut freed);
    freed
}

/// Removes download parts older than `max_part_age` before `now` that a crash left in the
/// game's `tf/custom` and `tf/maps` folders. Never fails; a missing folder frees nothing.
pub fn sweep_game_leftovers(tf: &Path, now: SystemTime, max_part_age: Duration) -> Freed {
    let mut freed = Freed::default();
    let aged = Aged { now, max_part_age };
    for dir in ["custom", "maps"] {
        remove_aged(&tf.join(dir), is_download_part, aged, &mut freed);
    }
    freed
}

#[derive(Clone, Copy)]
struct Aged {
    now: SystemTime,
    max_part_age: Duration,
}

impl Aged {
    // A modified time after `now` reads as fresh.
    fn is_old(self, meta: &std::fs::Metadata) -> bool {
        meta.modified().is_ok_and(|modified| {
            self.now
                .duration_since(modified)
                .is_ok_and(|age| age > self.max_part_age)
        })
    }
}

// Staged updates beside the AppImage age out; the previous build's backup goes at once, since
// the new build is already running.
fn sweep_beside_appimage(appimage: &Path, aged: Aged, freed: &mut Freed) {
    let (Some(dir), Some(name)) = (appimage.parent(), appimage.file_name()) else {
        return;
    };
    remove_aged(dir, is_staged_appimage, aged, freed);
    let mut bak = name.to_os_string();
    bak.push(".bak");
    let bak = dir.join(bak);
    if bak != appimage && is_regular_file(&bak) {
        remove(&bak, freed);
    }
}

// Removes the old enough regular files directly in `dir` whose name `ours` accepts.
fn remove_aged(dir: &Path, ours: fn(&str) -> bool, aged: Aged, freed: &mut Freed) {
    for path in entries(dir) {
        let named = path.file_name().and_then(|n| n.to_str()).is_some_and(ours);
        if named
            && std::fs::symlink_metadata(&path)
                .is_ok_and(|meta| meta.file_type().is_file() && aged.is_old(&meta))
        {
            remove(&path, freed);
        }
    }
}

// v6 renders each HUD's thumbnail as a jpg, so the v5 webp goes only once a jpg sits beside it,
// and only inside real (not linked) folders.
fn remove_v5_thumbnails(huds: &Path, freed: &mut Freed) {
    for hud in entries(huds) {
        let meta_dir = hud.join(".tf2tools");
        if !is_real_dir(&hud) || !is_real_dir(&meta_dir) {
            continue;
        }
        let webp = meta_dir.join("thumbnail.webp");
        if is_regular_file(&webp) && is_regular_file(&meta_dir.join("thumbnail.jpg")) {
            remove(&webp, freed);
        }
    }
}

// The paths directly in `dir`. A missing folder is silent; any other read error is logged.
fn entries(dir: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(dir) {
        Ok(read) => read
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => {
            tracing::warn!(path = %dir.display(), error = %err, "could not read a leftover folder");
            Vec::new()
        }
    }
}

fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file())
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_dir())
}

// `.{id}.dem.XXXXXX.part`, the demo client's temp file beside the cached `{id}.dem`.
fn is_demo_part(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('.').and_then(|r| r.strip_suffix(".part")) else {
        return false;
    };
    rest.split_once(".dem.").is_some_and(|(id, random)| {
        !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) && random6(random)
    })
}

// `.tf2tools-XXXXXX.part`, a store download's temp file in `tf/custom` or `tf/maps`.
fn is_download_part(name: &str) -> bool {
    name.strip_prefix(".tf2tools-")
        .and_then(|r| r.strip_suffix(".part"))
        .is_some_and(random6)
}

// `.tf2tools-update-<version>` and its `.part`, staged beside the AppImage.
fn is_staged_appimage(name: &str) -> bool {
    name.strip_prefix(".tf2tools-update-")
        .is_some_and(|rest| is_version(rest.strip_suffix(".part").unwrap_or(rest)))
}

// `tf2tools-update-<version>-setup.exe` and its `.part`, staged in temp.
fn is_staged_installer(name: &str) -> bool {
    let name = name.strip_suffix(".part").unwrap_or(name);
    name.strip_prefix("tf2tools-update-")
        .and_then(|r| r.strip_suffix("-setup.exe"))
        .is_some_and(is_version)
}

fn is_version(text: &str) -> bool {
    semver::Version::parse(text).is_ok()
}

fn random6(s: &str) -> bool {
    s.len() == 6 && s.bytes().all(|b| b.is_ascii_alphanumeric())
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
        // Keep both causes: on Windows a directory link fails `remove_file` but not `remove_dir`.
        std::fs::remove_file(path)
            .or_else(|file_err| {
                std::fs::remove_dir(path).map_err(|dir_err| {
                    std::io::Error::new(dir_err.kind(), format!("{file_err}; {dir_err}"))
                })
            })
            .map(|()| 0)
    } else if meta.is_dir() {
        // A partial `remove_dir_all` fails, so the bytes it did free are not counted.
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
