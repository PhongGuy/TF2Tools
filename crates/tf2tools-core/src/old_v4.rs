//! Finds an old TF2Tools 4 install on Windows, so the app can offer to run its own uninstaller.
//! v4 was electron-builder's per-user NSIS: an HKCU Uninstall entry named "TF2Tools 4.x.y" whose
//! command runs `Uninstall TF2Tools.exe`. The matching is pure; only `detect_old_v4` reads Windows.

use std::path::{Path, PathBuf};

const UNINSTALLER: &str = "Uninstall TF2Tools.exe";

/// One app's entry under Windows' Uninstall key, as the registry holds it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct UninstallEntry {
    key: String,
    display_name: Option<String>,
    display_version: Option<String>,
    uninstall_string: Option<String>,
}

impl UninstallEntry {
    /// An entry under the subkey `key` with no values yet.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ..Self::default()
        }
    }

    /// Sets `DisplayName`.
    pub fn display_name(mut self, value: impl Into<String>) -> Self {
        self.display_name = Some(value.into());
        self
    }

    /// Sets `DisplayVersion`.
    pub fn display_version(mut self, value: impl Into<String>) -> Self {
        self.display_version = Some(value.into());
        self
    }

    /// Sets `UninstallString`.
    pub fn uninstall_string(mut self, value: impl Into<String>) -> Self {
        self.uninstall_string = Some(value.into());
        self
    }

    /// The subkey name.
    pub fn key(&self) -> &str {
        &self.key
    }
}

/// A TF2Tools 4 install whose own uninstaller is still on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OldV4 {
    version: String,
    uninstaller: PathBuf,
    arguments: Vec<String>,
}

impl OldV4 {
    /// The installed version, such as `4.8.0`.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// v4's own `Uninstall TF2Tools.exe`.
    pub fn uninstaller(&self) -> &Path {
        &self.uninstaller
    }

    /// The arguments v4's entry passes to it, such as `/currentuser`.
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
}

/// The first entry that is TF2Tools 4 with its uninstaller present (`exists`), else `None`.
/// An entry needs v4's name, a 4.x version and a command running `Uninstall TF2Tools.exe`, so
/// v6's own entry, other apps and an entry left behind without its files never match.
pub fn find_old_v4(entries: &[UninstallEntry], exists: impl Fn(&Path) -> bool) -> Option<OldV4> {
    entries.iter().find_map(|entry| {
        let name = entry.display_name.as_deref()?;
        let version = entry.display_version.as_deref()?.trim();
        let named = name
            .get(..9)
            .is_some_and(|head| head.eq_ignore_ascii_case("TF2Tools "));
        if !named || version.split('.').next() != Some("4") {
            return None;
        }
        let (exe, arguments) = split_command(entry.uninstall_string.as_deref()?)?;
        // The command is a Windows path, so take its last part by hand on any host.
        let file = exe.rsplit(['\\', '/']).next()?;
        if !file.eq_ignore_ascii_case(UNINSTALLER) || !exists(Path::new(&exe)) {
            return None;
        }
        Some(OldV4 {
            version: version.to_owned(),
            uninstaller: PathBuf::from(exe),
            arguments,
        })
    })
}

// Splits `"C:\path with spaces\app.exe" /a /b` into the program and its arguments; an unquoted
// command ends its program at the first `.exe`, since the path itself may hold spaces.
fn split_command(command: &str) -> Option<(String, Vec<String>)> {
    let command = command.trim();
    let (exe, rest) = if let Some(quoted) = command.strip_prefix('"') {
        quoted.split_once('"')?
    } else {
        let end = command.to_ascii_lowercase().find(".exe")? + 4;
        command.split_at(end)
    };
    let args = rest.split_whitespace().map(str::to_owned).collect();
    Some((exe.to_owned(), args))
}

/// The TF2Tools 4 install on this PC: read from HKCU's Uninstall key on Windows, never elsewhere.
pub fn detect_old_v4() -> Option<OldV4> {
    #[cfg(windows)]
    {
        find_old_v4(&windows::user_uninstall_entries(), Path::is_file)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod windows {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    use super::UninstallEntry;

    const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

    /// Every subkey under HKCU's Uninstall key; an unreadable key or value is skipped.
    pub(super) fn user_uninstall_entries() -> Vec<UninstallEntry> {
        let Ok(root) =
            RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(UNINSTALL_KEY, KEY_READ)
        else {
            return Vec::new();
        };
        root.enum_keys()
            .filter_map(Result::ok)
            .filter_map(|name| {
                let key = root.open_subkey_with_flags(&name, KEY_READ).ok()?;
                let value = |v: &str| key.get_value::<String, _>(v).ok();
                Some(UninstallEntry {
                    display_name: value("DisplayName"),
                    display_version: value("DisplayVersion"),
                    uninstall_string: value("UninstallString"),
                    key: name,
                })
            })
            .collect()
    }
}
