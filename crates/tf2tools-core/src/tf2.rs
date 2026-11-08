//! Finding and validating the TF2 game directory.

use std::path::{Path, PathBuf};

use steamlocate::SteamDir;

use crate::error::DetectError;

/// Steam's app id for Team Fortress 2.
const TF2_APP_ID: u32 = 440;

/// A validated TF2 game directory (the one containing `tf/`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Tf2Install {
    game_dir: PathBuf,
}

impl Tf2Install {
    /// Locates TF2 through the machine's Steam install (app 440). Every Steam failure,
    /// like a missing Steam or an unreadable library, reads as "not installed".
    pub fn detect() -> Result<Option<Self>, DetectError> {
        match SteamDir::locate() {
            Ok(steam) => detect_in(steam.path()),
            Err(_) => Ok(None),
        }
    }

    /// Every TF2 install across all Steam roots (native, Flatpak, Snap), in `locate_all` order,
    /// deduplicated by canonical game path. No Steam at all gives an empty list.
    pub fn detect_all() -> Result<Vec<Self>, DetectError> {
        match steamlocate::locate_all() {
            Ok(dirs) => {
                let roots: Vec<PathBuf> = dirs.iter().map(|d| d.path().to_path_buf()).collect();
                detect_all_in(&roots)
            }
            Err(_) => Ok(Vec::new()),
        }
    }

    /// Accepts `path` as a game directory if it contains a `tf` directory, creating
    /// `tf/custom` when absent.
    pub fn from_path(path: &Path) -> Result<Self, DetectError> {
        if !path.join("tf").is_dir() {
            return Err(DetectError::NotTf2 {
                path: path.to_path_buf(),
            });
        }
        let custom = path.join("tf").join("custom");
        std::fs::create_dir_all(&custom).map_err(|source| DetectError::Io {
            path: custom,
            source,
        })?;
        Ok(Self {
            game_dir: path.to_path_buf(),
        })
    }

    /// The game directory, the one containing `tf/`.
    pub fn game_dir(&self) -> &Path {
        &self.game_dir
    }

    /// `<game_dir>/tf`.
    pub fn tf_dir(&self) -> PathBuf {
        self.game_dir.join("tf")
    }

    /// Whether the game folder (`tf/`) is still where the install was found.
    pub fn is_present(&self) -> bool {
        self.tf_dir().is_dir()
    }

    /// `<game_dir>/tf/custom`, where HUDs, sounds and addons are installed.
    pub fn custom_dir(&self) -> PathBuf {
        self.tf_dir().join("custom")
    }
}

/// Looks for TF2 under one Steam root. A missing app, a steamlocate failure or a game
/// directory without `tf/` all give `Ok(None)`; only a failure creating `tf/custom` errors.
fn detect_in(steam_root: &Path) -> Result<Option<Tf2Install>, DetectError> {
    let Ok(steam) = SteamDir::from_dir(steam_root) else {
        return Ok(None);
    };
    let Ok(Some((app, library))) = steam.find_app(TF2_APP_ID) else {
        return Ok(None);
    };
    match Tf2Install::from_path(&library.resolve_app_dir(&app)) {
        Ok(install) => Ok(Some(install)),
        Err(DetectError::NotTf2 { .. }) => Ok(None),
        Err(err) => Err(err),
    }
}

/// `detect_in` over each root in order, keeping the first install per canonical game dir.
fn detect_all_in(roots: &[PathBuf]) -> Result<Vec<Tf2Install>, DetectError> {
    let mut seen = Vec::new();
    let mut found = Vec::new();
    for root in roots {
        let Some(install) = detect_in(root)? else {
            continue;
        };
        // Two roots can list the same library, or reach it through a symlink.
        let key = std::fs::canonicalize(install.game_dir())
            .unwrap_or_else(|_| install.game_dir().to_path_buf());
        if !seen.contains(&key) {
            seen.push(key);
            found.push(install);
        }
    }
    Ok(found)
}

/// The names `/proc/<pid>/comm` shows for a running TF2: native, then under Proton (Wine sets
/// comm to the Windows exe's file name). `tf.sh` is left out: it is only the launcher.
const TF2_PROCESS_NAMES: [&str; 6] = [
    "tf_linux64",
    "tf_linux",
    "hl2_linux",
    "tf_win64.exe",
    "tf.exe",
    "hl2.exe",
];

/// Whether a TF2 process is running. Linux scans `/proc`, Windows reads the process list,
/// other platforms return false.
pub fn tf2_running() -> bool {
    #[cfg(target_os = "linux")]
    {
        tf2_running_in(Path::new("/proc"))
    }
    #[cfg(windows)]
    {
        // A process list that cannot be read counts as TF2 not running.
        sysinfo::System::new().is_ok_and(|mut system| {
            system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            any_tf2_exe(system.processes().values().map(sysinfo::Process::name))
        })
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        false
    }
}

/// Whether a Windows process list holds a TF2 exe. Windows file names ignore case, so
/// `TF_WIN64.EXE` counts; only the `.exe` names apply, as Windows never runs the others.
#[cfg_attr(not(windows), allow(dead_code))]
fn any_tf2_exe<S: AsRef<std::ffi::OsStr>>(names: impl IntoIterator<Item = S>) -> bool {
    names.into_iter().any(|name| {
        name.as_ref().to_str().is_some_and(|name| {
            TF2_PROCESS_NAMES
                .iter()
                .any(|exe| exe.ends_with(".exe") && exe.eq_ignore_ascii_case(name))
        })
    })
}

/// Whether a TF2 process is running under `proc_root`, on any platform: numeric
/// directories only, `comm` trimmed of its newline and matched exactly; unreadable entries
/// are skipped and a missing root reads false.
pub fn tf2_running_in(proc_root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(proc_root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let is_pid = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.parse::<u32>().is_ok());
        is_pid
            && std::fs::read_to_string(entry.path().join("comm"))
                .is_ok_and(|comm| TF2_PROCESS_NAMES.contains(&comm.trim_end_matches('\n')))
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{TF2_PROCESS_NAMES, any_tf2_exe, detect_all_in, detect_in, tf2_running_in};

    /// A fake Steam root whose `libraryfolders.vdf` lists `libraries`; each library holds
    /// the given `(app id, install dir)` manifests and their `steamapps/common/<dir>` folders.
    fn fake_steam(root: &Path, libraries: &[(&Path, &[(u32, &str)])]) {
        fs::create_dir_all(root.join("steamapps")).unwrap();
        let mut vdf = String::from("\"libraryfolders\"\n{\n");
        for (i, (lib, apps)) in libraries.iter().enumerate() {
            vdf.push_str(&format!(
                "\t\"{i}\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n",
                // Steam escapes the backslashes of a Windows path in this file.
                lib.display().to_string().replace('\\', "\\\\")
            ));
            fs::create_dir_all(lib.join("steamapps/common")).unwrap();
            for (id, dir) in *apps {
                let acf = format!(
                    "\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{dir}\"\n\t\"installdir\"\t\t\"{dir}\"\n}}\n"
                );
                fs::write(lib.join(format!("steamapps/appmanifest_{id}.acf")), acf).unwrap();
                fs::create_dir_all(lib.join("steamapps/common").join(dir)).unwrap();
            }
        }
        vdf.push_str("}\n");
        fs::write(root.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
    }

    #[test]
    fn finds_tf2_in_the_root_library_and_creates_custom() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Steam");
        fake_steam(&root, &[(&root, &[(440, "Team Fortress 2")])]);
        let game = root.join("steamapps/common/Team Fortress 2");
        fs::create_dir_all(game.join("tf")).unwrap();
        let install = detect_in(&root).unwrap().expect("found");
        assert_eq!(install.game_dir(), game);
        assert!(install.custom_dir().is_dir());
    }

    #[test]
    fn finds_tf2_in_a_secondary_library() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Steam");
        let second = tmp.path().join("Games");
        fake_steam(
            &root,
            &[
                (&root, &[(4000, "GarrysMod")]),
                (&second, &[(440, "Team Fortress 2")]),
            ],
        );
        fs::create_dir_all(second.join("steamapps/common/Team Fortress 2/tf")).unwrap();
        let install = detect_in(&root).unwrap().expect("found");
        assert_eq!(
            install.game_dir(),
            second.join("steamapps/common/Team Fortress 2")
        );
    }

    #[test]
    fn app_440_without_tf_dir_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Steam");
        fake_steam(&root, &[(&root, &[(440, "Team Fortress 2")])]);
        assert_eq!(detect_in(&root).unwrap(), None);
    }

    #[test]
    fn no_tf2_or_no_steam_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Steam");
        fake_steam(&root, &[(&root, &[(4000, "GarrysMod")])]);
        assert_eq!(detect_in(&root).unwrap(), None);
        assert_eq!(detect_in(&tmp.path().join("no-steam")).unwrap(), None);
    }

    const TF2: &str = "steamapps/common/Team Fortress 2";

    #[test]
    fn detect_all_lists_each_root_with_tf2_in_root_order() {
        let tmp = tempfile::tempdir().unwrap();
        let flatpak = tmp.path().join("flatpak/Steam");
        let empty = tmp.path().join("empty/Steam");
        let native = tmp.path().join("native/Steam");
        fake_steam(&flatpak, &[(&flatpak, &[(440, "Team Fortress 2")])]);
        fake_steam(&empty, &[(&empty, &[(4000, "GarrysMod")])]);
        fake_steam(&native, &[(&native, &[(440, "Team Fortress 2")])]);
        for root in [&flatpak, &native] {
            fs::create_dir_all(root.join(TF2).join("tf")).unwrap();
        }
        let roots = [
            flatpak.clone(),
            empty,
            native.clone(),
            tmp.path().join("missing"),
        ];
        let dirs: Vec<_> = detect_all_in(&roots)
            .unwrap()
            .iter()
            .map(|i| i.game_dir().to_path_buf())
            .collect();
        assert_eq!(dirs, vec![flatpak.join(TF2), native.join(TF2)]);
    }

    #[test]
    fn detect_all_keeps_one_install_for_a_library_two_roots_share() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a/Steam");
        let b = tmp.path().join("b/Steam");
        fake_steam(&a, &[(&a, &[(440, "Team Fortress 2")])]);
        fake_steam(&b, &[(&b, &[]), (&a, &[(440, "Team Fortress 2")])]);
        fs::create_dir_all(a.join(TF2).join("tf")).unwrap();
        let found = detect_all_in(&[a.clone(), b]).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].game_dir(), a.join(TF2));
    }

    #[cfg(unix)]
    #[test]
    fn detect_all_keeps_one_install_reached_through_a_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = tmp.path().join("lib");
        let alias = tmp.path().join("alias");
        fs::create_dir_all(&lib).unwrap();
        std::os::unix::fs::symlink(&lib, &alias).unwrap();
        let a = tmp.path().join("a/Steam");
        let b = tmp.path().join("b/Steam");
        fake_steam(&a, &[(&lib, &[(440, "Team Fortress 2")])]);
        fake_steam(&b, &[(&alias, &[(440, "Team Fortress 2")])]);
        fs::create_dir_all(lib.join(TF2).join("tf")).unwrap();
        let found = detect_all_in(&[a, b]).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].game_dir(), lib.join(TF2));
    }

    #[test]
    fn detect_all_with_no_roots_is_empty() {
        assert!(detect_all_in(&[]).unwrap().is_empty());
    }

    fn fake_proc(entries: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (pid, comm) in entries {
            let dir = root.path().join(pid);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
        }
        root
    }

    #[test]
    fn a_native_tf2_counts_as_running() {
        for name in ["tf_linux64", "tf_linux", "hl2_linux"] {
            let root = fake_proc(&[("1", "systemd"), ("4242", name)]);
            assert!(tf2_running_in(root.path()), "{name}");
        }
    }

    #[test]
    fn tf2_under_proton_counts_as_running() {
        for name in ["tf_win64.exe", "tf.exe", "hl2.exe"] {
            let root = fake_proc(&[("1", "systemd"), ("777", "wineserver"), ("778", name)]);
            assert!(tf2_running_in(root.path()), "{name}");
        }
    }

    #[test]
    fn the_launch_script_and_other_processes_do_not_count() {
        let root = fake_proc(&[
            ("1", "systemd"),
            ("10", "steam"),
            ("11", "tf.sh"),
            ("12", "reaper"),
            ("13", "TF_WIN64.EXE"),
            ("14", "tf_linux64x"),
        ]);
        assert!(!tf2_running_in(root.path()));
    }

    #[test]
    fn non_pid_entries_and_unreadable_comm_are_ignored() {
        let root = fake_proc(&[("self", "tf_linux64"), ("thread-self", "hl2.exe")]);
        fs::create_dir_all(root.path().join("99")).unwrap();
        fs::write(root.path().join("100"), "tf_linux64\n").unwrap();
        assert!(!tf2_running_in(root.path()));
    }

    #[test]
    fn a_missing_proc_root_is_not_running() {
        let root = tempfile::tempdir().unwrap();
        assert!(!tf2_running_in(&root.path().join("nope")));
    }

    #[test]
    fn a_windows_process_list_with_a_tf2_exe_counts_in_any_case() {
        for name in ["tf_win64.exe", "TF.EXE", "Hl2.Exe"] {
            assert!(any_tf2_exe(["System", "steam.exe", name]), "{name}");
        }
    }

    #[test]
    fn a_windows_process_list_without_a_tf2_exe_does_not_count() {
        assert!(!any_tf2_exe(Vec::<&str>::new()));
        assert!(!any_tf2_exe([
            "steam.exe",
            "tf_linux64",
            "hl2_linux",
            "tf.sh",
            "tf_win64",
            "tf_win64.exe.bak",
        ]));
    }

    // The kernel truncates comm to 15 bytes, so a longer name could never match a process.
    #[test]
    fn every_process_name_fits_in_comm() {
        for name in TF2_PROCESS_NAMES {
            assert!(name.len() <= 15, "{name} is {} bytes", name.len());
        }
    }
}
