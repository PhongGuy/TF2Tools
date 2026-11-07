//! Where the running copy of TF2Tools lives and whether it can replace itself.

use std::path::{Path, PathBuf};

use super::{StagedUpdate, sha256_hex};
use crate::error::UpdateApplyError;

/// How this copy of TF2Tools was installed, which decides whether it can update itself.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstallTarget {
    /// A running AppImage; the path is `$APPIMAGE`, the file that gets replaced.
    AppImage(PathBuf),
    /// Installed by the Windows installer: `uninstall.exe` sits beside the running exe.
    WindowsInstaller,
    /// A deb, `make install`, tarball or dev build: notify only.
    Unmanaged,
}

impl InstallTarget {
    /// On Windows `detect_installer_from(current_exe())`; elsewhere
    /// `detect_from($APPIMAGE, $APPDIR, current_exe())`, empty variables read as unset.
    /// Unmanaged when `current_exe` fails.
    pub fn detect() -> Self {
        let Ok(exe) = std::env::current_exe() else {
            return Self::Unmanaged;
        };
        if cfg!(windows) {
            return Self::detect_installer_from(&exe);
        }
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        Self::detect_from(var("APPIMAGE"), var("APPDIR"), &exe)
    }

    /// WindowsInstaller when `uninstall.exe` is a regular file beside `current_exe`, else
    /// Unmanaged.
    pub fn detect_installer_from(current_exe: &Path) -> Self {
        // The NSIS installer writes its uninstaller into the install dir; a dev build or a
        // copied exe has none, so it never runs installers.
        match std::fs::metadata(current_exe.with_file_name("uninstall.exe")) {
            Ok(meta) if meta.is_file() => Self::WindowsInstaller,
            _ => Self::Unmanaged,
        }
    }

    /// AppImage(appimage) only when both are set, `appimage` is absolute and `current_exe`
    /// lies inside `appdir` by path components; otherwise Unmanaged.
    pub fn detect_from(
        appimage: Option<PathBuf>,
        appdir: Option<PathBuf>,
        current_exe: &Path,
    ) -> Self {
        // A deb or `make install` binary launched from an AppImage inherits its variables,
        // so only a binary inside the mount counts; a relative or empty $APPDIR matches nothing.
        match (appimage, appdir) {
            (Some(appimage), Some(appdir))
                if appimage.is_absolute()
                    && appdir.is_absolute()
                    && current_exe.starts_with(&appdir) =>
            {
                Self::AppImage(appimage)
            }
            _ => Self::Unmanaged,
        }
    }
}

/// What `apply` did, and what the app does next.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Applied {
    /// The AppImage was replaced; the app spawns `relaunch` and quits.
    Replaced {
        /// The replaced AppImage, to start in place of the running process.
        relaunch: PathBuf,
    },
    /// The installer is running; the app quits so it can replace the files.
    InstallerStarted,
}

/// For an AppImage, backs up the AppImage as `<file name>.bak` (replacing an older one;
/// hard link, copy if linking fails), sets the staged file to 0755 and renames it over the
/// AppImage, putting the backup back if the renamed file is not the one checked. For the
/// Windows installer, re-checks the staged file's sha256 (Changed on a mismatch) and starts
/// it with `/P /R` without waiting. On error the staged file is removed, best effort.
pub fn apply(staged: StagedUpdate) -> Result<Applied, UpdateApplyError> {
    let result = match staged.target() {
        InstallTarget::WindowsInstaller => start_installer(&staged),
        InstallTarget::AppImage(_) | InstallTarget::Unmanaged => swap(&staged),
    };
    if result.is_err() {
        let _ = std::fs::remove_file(staged.path());
    }
    result
}

/// Starts exactly the file `download` verified: it is held open from the hash check until
/// the installer has started, so a file swapped in after the download is never run.
/// `/P` is NSIS passive mode (it closes a running TF2Tools itself) and `/R` restarts the app.
fn start_installer(staged: &StagedUpdate) -> Result<Applied, UpdateApplyError> {
    let path = staged.path();
    let io = |source| UpdateApplyError::Io {
        path: path.to_path_buf(),
        source,
    };
    let unsupported = || UpdateApplyError::Unsupported {
        path: Some(path.to_path_buf()),
    };
    let seen = std::fs::symlink_metadata(path).map_err(io)?;
    if !seen.is_file() {
        return Err(unsupported());
    }
    let file = open_pinned(path).map_err(io)?;
    let opened = file.metadata().map_err(io)?;
    if !same_file(&seen, &opened) {
        return Err(unsupported());
    }
    if sha256_hex(&file).map_err(io)? != staged.sha256 {
        return Err(UpdateApplyError::Changed {
            path: path.to_path_buf(),
        });
    }
    // Unix cannot pin a path while it is open, so it checks the name still points at the
    // hashed file; only tests start an installer there.
    #[cfg(unix)]
    match std::fs::symlink_metadata(path) {
        Ok(now) if same_file(&now, &opened) => {}
        _ => {
            return Err(UpdateApplyError::Changed {
                path: path.to_path_buf(),
            });
        }
    }
    spawn_installer(path).map_err(|source| UpdateApplyError::Spawn {
        path: path.to_path_buf(),
        source,
    })?;
    drop(file);
    Ok(Applied::InstallerStarted)
}

/// Starts the installer in passive mode. Linux refuses to run a file while any process
/// still holds it open for writing, and a process that forked mid-download holds that
/// handle until it execs, so a busy file is tried again for up to about a second.
fn spawn_installer(path: &Path) -> std::io::Result<std::process::Child> {
    let start = || {
        std::process::Command::new(path)
            .args(["/P", "/R"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    };
    let mut tries = 0;
    loop {
        match start() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 50 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            result => return result,
        }
    }
}

/// Opens the staged installer for reading. On Windows only reads are shared, so nothing
/// can rewrite, rename or delete it while it is held, and a link is opened as itself.
#[cfg(windows)]
fn open_pinned(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(windows))]
fn open_pinned(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(path)
}

/// Whether the opened handle is the regular file `seen` describes, so a link planted
/// between the check and the open is refused.
#[cfg(unix)]
fn same_file(seen: &std::fs::Metadata, opened: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    opened.is_file() && (seen.dev(), seen.ino()) == (opened.dev(), opened.ino())
}

/// Windows opened any link as itself, so a regular file behind the handle is the one seen.
#[cfg(not(unix))]
fn same_file(_seen: &std::fs::Metadata, opened: &std::fs::Metadata) -> bool {
    opened.is_file()
}

#[cfg(not(unix))]
fn swap(staged: &StagedUpdate) -> Result<Applied, UpdateApplyError> {
    let path = match staged.target() {
        InstallTarget::AppImage(appimage) => Some(appimage.clone()),
        _ => None,
    };
    Err(UpdateApplyError::Unsupported { path })
}

/// Every step before the rename leaves the AppImage untouched, and after it a file that is
/// not the checked one is replaced by the backup.
#[cfg(unix)]
fn swap(staged: &StagedUpdate) -> Result<Applied, UpdateApplyError> {
    use std::io::ErrorKind;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let InstallTarget::AppImage(appimage) = staged.target() else {
        return Err(UpdateApplyError::Unsupported { path: None });
    };
    let unsupported = |path: &Path| UpdateApplyError::Unsupported {
        path: Some(path.to_path_buf()),
    };
    // The rename is atomic only within one directory, which is where download staged it.
    let Some(file_name) = appimage.file_name() else {
        return Err(unsupported(appimage));
    };
    if staged.path().parent() != appimage.parent() {
        return Err(unsupported(staged.path()));
    }
    // A symlinked $APPIMAGE would be replaced by the file and leave the real one stale.
    let original = std::fs::symlink_metadata(appimage).map_err(|e| failure(appimage, e))?;
    if !original.is_file() {
        return Err(unsupported(appimage));
    }
    // The staged file is opened and matched against what lstat saw, so a link planted in
    // its place is refused instead of having its target chmodded and swapped in.
    let staged_meta =
        std::fs::symlink_metadata(staged.path()).map_err(|e| failure(staged.path(), e))?;
    if !staged_meta.is_file() {
        return Err(unsupported(staged.path()));
    }
    let staged_file = std::fs::File::open(staged.path()).map_err(|e| failure(staged.path(), e))?;
    let opened = staged_file
        .metadata()
        .map_err(|e| failure(staged.path(), e))?;
    if (opened.dev(), opened.ino()) != (staged_meta.dev(), staged_meta.ino()) {
        return Err(unsupported(staged.path()));
    }

    // The old backup is removed first, link or file alike, so the new one never writes
    // through a planted link.
    let mut bak_name = file_name.to_os_string();
    bak_name.push(".bak");
    let bak = appimage.with_file_name(bak_name);
    match std::fs::remove_file(&bak) {
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(failure(&bak, e)),
        _ => {}
    }
    back_up(appimage, &bak, original.permissions().mode()).map_err(|e| failure(&bak, e))?;

    staged_file
        .set_permissions(std::fs::Permissions::from_mode(0o755))
        .map_err(|e| failure(staged.path(), e))?;
    staged_file
        .sync_all()
        .map_err(|e| failure(staged.path(), e))?;
    rename_checked(staged.path(), &opened, appimage, &bak)?;
    sync_parent(appimage);
    drop(staged_file);
    Ok(Applied::Replaced {
        relaunch: appimage.clone(),
    })
}

/// Renames `staged` over `appimage`, then confirms the AppImage is now the file `checked`
/// describes; otherwise puts `bak` back over it. The caller keeps the checked file open.
#[cfg(unix)]
fn rename_checked(
    staged: &Path,
    checked: &std::fs::Metadata,
    appimage: &Path,
    bak: &Path,
) -> Result<(), UpdateApplyError> {
    use std::os::unix::fs::MetadataExt;
    std::fs::rename(staged, appimage).map_err(|e| failure(appimage, e))?;
    // Unix cannot rename by handle, so the check comes after the rename; the held handle
    // keeps the checked inode alive, so a planted file can never reuse its number.
    match std::fs::symlink_metadata(appimage) {
        Ok(now) if (now.dev(), now.ino()) == (checked.dev(), checked.ino()) => Ok(()),
        Ok(_) => restore(bak, appimage).and(Err(UpdateApplyError::Changed {
            path: staged.to_path_buf(),
        })),
        Err(e) => restore(bak, appimage).and(Err(failure(appimage, e))),
    }
}

/// Puts the backup back over an unverified AppImage and persists the folder; if that fails,
/// the unverified file stays and the original survives only as `bak`.
#[cfg(unix)]
fn restore(bak: &Path, appimage: &Path) -> Result<(), UpdateApplyError> {
    restore_with(bak, appimage, sync_parent)
}

/// `restore` with the folder sync supplied, so a test can see it run.
#[cfg(unix)]
fn restore_with(
    bak: &Path,
    appimage: &Path,
    sync: impl FnOnce(&Path),
) -> Result<(), UpdateApplyError> {
    std::fs::rename(bak, appimage).map_err(|e| failure(appimage, e))?;
    sync(appimage);
    Ok(())
}

/// Persists a rename onto `path` by syncing its folder; the rename has happened either way,
/// so this is best effort.
#[cfg(unix)]
fn sync_parent(path: &Path) {
    if let Some(dir) = path.parent()
        && let Ok(dir) = std::fs::File::open(dir)
    {
        let _ = dir.sync_all();
    }
}

/// Backs `appimage` up as `bak`: a hard link, or a fresh copy with `mode` when linking fails.
#[cfg(unix)]
fn back_up(appimage: &Path, bak: &Path, mode: u32) -> std::io::Result<()> {
    back_up_with(appimage, bak, mode, |from, to| std::fs::hard_link(from, to))
}

/// `back_up` with the link step supplied, so a test can refuse it as vfat does.
#[cfg(unix)]
fn back_up_with(
    appimage: &Path,
    bak: &Path,
    mode: u32,
    link: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    // vfat and exFAT refuse hard links with EPERM, so any link failure falls back to a
    // copy; a truly read-only directory then fails the copy's create as NotWritable.
    if link(appimage, bak).is_err() {
        copy_new(appimage, bak, mode)?;
    }
    Ok(())
}

/// Copies `from` into a file created fresh at `to`, which fails rather than follow a
/// link that appeared there since it was removed.
#[cfg(unix)]
fn copy_new(from: &Path, to: &Path, mode: u32) -> std::io::Result<()> {
    copy_new_from(&mut std::fs::File::open(from)?, to, mode)
}

/// Copies `source` into a file created fresh at `to`; a copy cut short is removed.
#[cfg(unix)]
fn copy_new_from(source: &mut impl std::io::Read, to: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut target = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode & 0o777)
        .open(to)?;
    // A copy cut short, most likely by a full disk, must not pass for a backup.
    let copied = std::io::copy(source, &mut target).and_then(|_| target.sync_all());
    if copied.is_err() {
        let _ = std::fs::remove_file(to);
    }
    copied
}

#[cfg(unix)]
fn denied(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
    )
}

/// Permission problems become `NotWritable`, so the app can point at the release page.
#[cfg(unix)]
fn failure(path: &Path, source: std::io::Error) -> UpdateApplyError {
    if denied(&source) {
        UpdateApplyError::NotWritable {
            path: path.to_path_buf(),
        }
    } else {
        UpdateApplyError::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use std::ffi::OsStr;

    struct Swap {
        _tmp: tempfile::TempDir,
        app: PathBuf,
        bak: PathBuf,
        staged: PathBuf,
    }

    // An AppImage holding "old" with its hard-linked backup, and a staged file holding "checked".
    fn swap_dir() -> Swap {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("tf2tools.AppImage");
        let bak = tmp.path().join("tf2tools.AppImage.bak");
        let staged = tmp.path().join(".tf2tools-update-6.0.1");
        std::fs::write(&app, b"old").unwrap();
        std::fs::hard_link(&app, &bak).unwrap();
        std::fs::write(&staged, b"checked").unwrap();
        Swap {
            _tmp: tmp,
            app,
            bak,
            staged,
        }
    }

    // Someone with write access to the directory plants another file under the staged name.
    fn plant(staged: &Path) {
        std::fs::remove_file(staged).unwrap();
        std::fs::write(staged, b"swapped").unwrap();
    }

    #[test]
    fn the_checked_staged_file_replaces_the_appimage() {
        let s = swap_dir();
        let held = std::fs::File::open(&s.staged).unwrap();
        let checked = held.metadata().unwrap();
        rename_checked(&s.staged, &checked, &s.app, &s.bak).unwrap();
        assert_eq!(std::fs::read(&s.app).unwrap(), b"checked");
        assert_eq!(std::fs::read(&s.bak).unwrap(), b"old");
        assert!(!s.staged.exists());
    }

    #[test]
    fn a_staged_file_swapped_after_the_check_is_undone_from_the_backup() {
        let s = swap_dir();
        let held = std::fs::File::open(&s.staged).unwrap();
        let checked = held.metadata().unwrap();
        plant(&s.staged);
        match rename_checked(&s.staged, &checked, &s.app, &s.bak) {
            Err(UpdateApplyError::Changed { path }) => assert_eq!(path, s.staged),
            other => panic!("expected Changed, got {other:?}"),
        }
        assert_eq!(std::fs::read(&s.app).unwrap(), b"old");
        assert!(!s.bak.exists(), "the backup went back into place");
        drop(held);
    }

    #[test]
    fn a_restore_persists_the_folder() {
        let s = swap_dir();
        std::fs::rename(&s.staged, &s.app).unwrap();
        let synced = std::cell::RefCell::new(Vec::new());
        restore_with(&s.bak, &s.app, |path| {
            synced.borrow_mut().push(path.to_path_buf())
        })
        .unwrap();
        assert_eq!(std::fs::read(&s.app).unwrap(), b"old");
        assert_eq!(synced.into_inner(), [s.app]);
    }

    // Without a backup to restore, the planted file stays and the error says so.
    #[test]
    fn a_failed_restore_is_an_io_error_on_the_appimage() {
        let s = swap_dir();
        let held = std::fs::File::open(&s.staged).unwrap();
        let checked = held.metadata().unwrap();
        plant(&s.staged);
        std::fs::remove_file(&s.bak).unwrap();
        match rename_checked(&s.staged, &checked, &s.app, &s.bak) {
            Err(UpdateApplyError::Io { path, .. }) => assert_eq!(path, s.app),
            other => panic!("expected Io, got {other:?}"),
        }
        drop(held);
    }

    #[test]
    fn a_refused_link_falls_back_to_a_copy_with_the_mode() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("tf2tools.AppImage");
        let bak = tmp.path().join("tf2tools.AppImage.bak");
        std::fs::write(&app, b"old AppImage").unwrap();
        // vfat and exFAT refuse hard links with EPERM.
        let refuse =
            |_: &Path, _: &Path| Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        back_up_with(&app, &bak, 0o100751, refuse).unwrap();
        assert_eq!(std::fs::read(&bak).unwrap(), b"old AppImage");
        let copy = std::fs::metadata(&bak).unwrap();
        assert_eq!(copy.permissions().mode() & 0o777, 0o751);
        assert_eq!(copy.nlink(), 1, "a copy, not a link");
        assert_eq!(std::fs::metadata(&app).unwrap().nlink(), 1);
    }

    // Hands out `left` bytes, then fails as a full disk would.
    struct CutShort {
        left: usize,
    }

    impl std::io::Read for CutShort {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.left == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::StorageFull));
            }
            let n = buf.len().min(self.left);
            buf[..n].fill(0x5a);
            self.left -= n;
            Ok(n)
        }
    }

    #[test]
    fn a_copy_cut_short_leaves_no_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let bak = tmp.path().join("tf2tools.AppImage.bak");
        let err = copy_new_from(&mut CutShort { left: 64 * 1024 }, &bak, 0o755).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::StorageFull);
        assert!(!bak.exists(), "a partial backup was left behind");
    }

    #[cfg(target_os = "linux")]
    const SCRATCH: &str = "TF2TOOLS_SCRATCH_NS_DIR";

    // The scratch dir, only inside a user namespace: outside one `uid_map` is the identity
    // range "0 0 4294967295", and that must never mount anything.
    #[cfg(target_os = "linux")]
    fn scratch_dir() -> Option<PathBuf> {
        let dir = PathBuf::from(std::env::var_os(SCRATCH)?);
        let map = std::fs::read_to_string("/proc/self/uid_map").unwrap();
        let fields: Vec<&str> = map.split_whitespace().collect();
        assert!(
            fields.len() == 3 && fields[2] != "4294967295",
            "refusing to mount outside a scratch user namespace: {map:?}"
        );
        Some(dir)
    }

    #[cfg(target_os = "linux")]
    fn mount(args: &[&std::ffi::OsStr]) {
        let status = std::process::Command::new("mount")
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "mount {args:?}");
    }

    // A file bind-mounted onto itself sits on its own mount, so the kernel refuses to
    // hard-link it into its directory (EXDEV).
    #[cfg(target_os = "linux")]
    fn unlinkable(path: &Path, size: usize) {
        std::fs::write(path, vec![0x5a; size]).unwrap();
        mount(&[OsStr::new("--bind"), path.as_os_str(), path.as_os_str()]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "run by real_link_and_disk_errors_in_a_scratch_namespace inside its namespace"]
    fn backup_in_a_scratch_namespace() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let Some(dir) = scratch_dir() else { return };
        mount(&[
            OsStr::new("-t"),
            OsStr::new("tmpfs"),
            OsStr::new("-o"),
            OsStr::new("size=1m"),
            OsStr::new("tf2tools-test"),
            dir.as_os_str(),
        ]);

        let app = dir.join("a.AppImage");
        let bak = dir.join("a.AppImage.bak");
        unlinkable(&app, 64 * 1024);
        let refused = std::fs::hard_link(&app, &bak).unwrap_err();
        assert_eq!(refused.kind(), std::io::ErrorKind::CrossesDevices);
        back_up(&app, &bak, 0o100751).unwrap();
        assert_eq!(std::fs::read(&bak).unwrap(), std::fs::read(&app).unwrap());
        let copy = std::fs::metadata(&bak).unwrap();
        assert_eq!(copy.permissions().mode() & 0o777, 0o751);
        assert_eq!(copy.nlink(), 1, "a copy, not a link");

        // 128 KiB used, so a 640 KiB copy runs out of room part way.
        let big = dir.join("b.AppImage");
        let big_bak = dir.join("b.AppImage.bak");
        unlinkable(&big, 640 * 1024);
        let full = back_up(&big, &big_bak, 0o100755).unwrap_err();
        assert_eq!(full.kind(), std::io::ErrorKind::StorageFull);
        assert!(!big_bak.exists(), "a partial backup was left behind");
        assert_eq!(std::fs::read(&big).unwrap(), vec![0x5a; 640 * 1024]);
    }

    // Runs the test above as root of a fresh user and mount namespace, where a tmpfs and bind
    // mounts are allowed and vanish with it. On demand: it fails where namespaces are off.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs unprivileged user namespaces; run on demand with --ignored"]
    fn real_link_and_disk_errors_in_a_scratch_namespace() {
        let ns = [
            "--user",
            "--map-root-user",
            "--mount",
            "--propagation",
            "private",
        ];
        let scratch = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("unshare")
            .args(ns)
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "update::install::tests::backup_in_a_scratch_namespace",
            ])
            .args(["--ignored", "--test-threads=1"])
            .env(SCRATCH, scratch.path())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && stdout.contains("test result: ok. 1 passed"),
            "{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
