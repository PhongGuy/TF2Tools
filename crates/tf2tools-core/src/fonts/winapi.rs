//! The Win32 calls behind the registry font host. Only std, `winreg` and `windows-sys` are
//! used, so a scratch crate can type-check this file for the Windows target; on every other
//! platform each call fails with `Unsupported`.
#![cfg_attr(not(windows), allow(dead_code))]

use std::ffi::OsString;
use std::io;
use std::path::Path;

#[cfg(windows)]
const FONTS_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";

/// The string data of every value under the `Fonts` key of HKCU (`user`) or HKLM.
/// A missing key reads as empty; unreadable and non-string values are skipped.
#[cfg(windows)]
pub(crate) fn font_values(user: bool) -> io::Result<Vec<String>> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::types::FromRegValue;
    let hive = RegKey::predef(if user {
        HKEY_CURRENT_USER
    } else {
        HKEY_LOCAL_MACHINE
    });
    let key = match hive.open_subkey_with_flags(FONTS_KEY, KEY_READ) {
        Ok(key) => key,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for value in key.enum_values() {
        // One unreadable value (a non-UTF-16 name, say) hides only itself.
        let Ok((_name, data)) = value else { continue };
        if let Ok(text) = String::from_reg_value(&data) {
            out.push(text);
        }
    }
    Ok(out)
}

/// Writes the HKCU `Fonts` value `name` with `path` as its string data.
#[cfg(windows)]
pub(crate) fn set_user_font_value(name: &str, path: &Path) -> io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    let (key, _) =
        RegKey::predef(HKEY_CURRENT_USER).create_subkey_with_flags(FONTS_KEY, KEY_SET_VALUE)?;
    key.set_value(name, &path.as_os_str())
}

/// The data of the HKCU `Fonts` value `name`: `None` when absent, an empty string when it
/// exists but is not a readable string (so it still counts as taken). Read as `OsString`, so a
/// path that is not valid UTF-16 comes back exactly.
#[cfg(windows)]
pub(crate) fn user_font_value(name: &str) -> io::Result<Option<OsString>> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    let key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(FONTS_KEY, KEY_READ) {
        Ok(key) => key,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    match key.get_value::<OsString, _>(name) {
        Ok(data) => Ok(Some(data)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Ok(Some(OsString::new())),
    }
}

/// Makes the font at `path` available to this session (`AddFontResourceW`).
#[cfg(windows)]
pub(crate) fn add_font_resource(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
    #[allow(unsafe_code)]
    let added = unsafe { windows_sys::Win32::Graphics::Gdi::AddFontResourceW(wide.as_ptr()) };
    if added == 0 {
        return Err(io::Error::other(format!(
            "AddFontResourceW added no font from {}",
            path.display()
        )));
    }
    Ok(())
}

/// Tells running programs the font set changed, waiting at most one second per window.
#[cfg(windows)]
pub(crate) fn broadcast_font_change() -> io::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_FONTCHANGE,
    };
    let mut result = 0usize;
    // SAFETY: the message carries no pointers and `result` outlives the call.
    #[allow(unsafe_code)]
    let sent = unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_FONTCHANGE,
            0,
            0,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        )
    };
    if sent == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "the font registry exists only on Windows",
    )
}

#[cfg(not(windows))]
pub(crate) fn font_values(_user: bool) -> io::Result<Vec<String>> {
    Err(unsupported())
}

#[cfg(not(windows))]
pub(crate) fn set_user_font_value(_name: &str, _path: &Path) -> io::Result<()> {
    Err(unsupported())
}

#[cfg(not(windows))]
pub(crate) fn user_font_value(_name: &str) -> io::Result<Option<OsString>> {
    Err(unsupported())
}

#[cfg(not(windows))]
pub(crate) fn add_font_resource(_path: &Path) -> io::Result<()> {
    Err(unsupported())
}

#[cfg(not(windows))]
pub(crate) fn broadcast_font_change() -> io::Result<()> {
    Err(unsupported())
}
