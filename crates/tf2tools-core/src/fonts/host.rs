//! The real font hosts: fontconfig's command-line tools on Linux, the font registry and GDI on
//! Windows. Every system font is read through `font_names`, so odd files are skipped, never fatal.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use super::check::{FontHost, FontHostError};
use super::names::font_names;
use super::winapi;

/// fc-list answers in well under a second on a large font set; past this it is hung.
const FC_LIST_TIMEOUT: Duration = Duration::from_secs(20);
/// fc-cache rescans one small directory, but a cold cache on a slow disk can take a while.
const FC_CACHE_TIMEOUT: Duration = Duration::from_secs(120);
/// Far above the few hundred KiB a system with thousands of fonts prints.
const FC_LIST_MAX_OUTPUT: u64 = 32 * 1024 * 1024;
const FC_CACHE_MAX_OUTPUT: u64 = 1024 * 1024;
/// Every family and full name of a font, each followed by a tab, one font per line; fontconfig
/// prints names unescaped, so a comma inside a family is part of the name.
const FC_LIST_FORMAT: &str = "%{[]family{%{family}\t}}%{[]fullname{%{fullname}\t}}\n";
/// How long the output may take to end once the child exited, even right at the deadline.
const READER_GRACE: Duration = Duration::from_secs(1);

/// The real host for this platform: `FcHost` on Linux, `RegistryHost` on Windows, else `None`.
pub fn system_font_host() -> Option<Arc<dyn FontHost>> {
    // `cfg!` rather than `#[cfg]`, so both hosts type-check on every platform.
    if cfg!(target_os = "linux") {
        Some(Arc::new(FcHost))
    } else if cfg!(windows) {
        Some(Arc::new(RegistryHost))
    } else {
        None
    }
}

/// fontconfig through `fc-list` and `fc-cache`; fonts are found by scanning the user font dir,
/// so `register` has nothing to do.
pub(crate) struct FcHost;

impl FontHost for FcHost {
    fn installed_names(&self) -> Result<Vec<String>, FontHostError> {
        let mut command = Command::new("fc-list");
        command.args(["--format", FC_LIST_FORMAT]);
        let output = run_bounded(command, FC_LIST_TIMEOUT, FC_LIST_MAX_OUTPUT)?;
        Ok(parse_fc_list(&String::from_utf8_lossy(&output)))
    }

    fn fonts_dir(&self) -> Result<PathBuf, FontHostError> {
        dirs::data_dir()
            .map(|dir| dir.join("fonts").join("tf2tools"))
            .ok_or_else(|| FontHostError::new("no user data directory for fonts"))
    }

    fn register(&self, _: &Path, _: &str, _: bool) -> Result<(), FontHostError> {
        Ok(())
    }

    fn refresh(&self) -> Result<(), FontHostError> {
        let mut command = Command::new("fc-cache");
        command.arg("-f").arg(self.fonts_dir()?);
        run_bounded(command, FC_CACHE_TIMEOUT, FC_CACHE_MAX_OUTPUT).map(drop)
    }
}

/// Runs `command` and returns its stdout. Failing to start, a non-zero exit, running past
/// `timeout` or printing more than `max_output` bytes is an error. Giving up kills the child
/// and, on Unix, its whole process group, so no grandchild outlives the call.
fn run_bounded(
    mut command: Command,
    timeout: Duration,
    max_output: u64,
) -> Result<Vec<u8>, FontHostError> {
    let program = command.get_program().to_string_lossy().into_owned();
    let fail = |what: String| FontHostError::new(format!("{program}: {what}"));
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| fail(format!("could not start: {e}")))?;
    let pid = child.id();

    // A reader thread drains stdout so a chatty child never blocks on a full pipe; it stops one
    // byte past the cap and drops the pipe, which ends a runaway child with a broken pipe.
    let (sender, receiver) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let read = stdout.take(max_output + 1).read_to_end(&mut output);
            let _ = sender.send(read.map(|_| output));
        });
    }

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            outcome => {
                // The group goes first, while the unreaped leader keeps its id from reuse.
                kill_group(pid);
                let _ = child.kill();
                let _ = child.wait();
                return Err(match outcome {
                    Err(e) => fail(format!("could not wait: {e}")),
                    _ => fail(format!("no answer within {} s", timeout.as_secs())),
                });
            }
        }
    };
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .max(READER_GRACE);
    let output = match receiver.recv_timeout(remaining) {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Err(fail(format!("could not read output: {e}"))),
        Err(_) => {
            // Something in the group still holds stdout; end it rather than leave it running.
            kill_group(pid);
            return Err(fail("output did not end".to_owned()));
        }
    };
    if output.len() as u64 > max_output {
        return Err(fail(format!("printed more than {max_output} bytes")));
    }
    if !status.success() {
        return Err(fail(format!("exited with {status}")));
    }
    Ok(output)
}

/// Sends `SIGKILL` to the process group `pid` leads (spawned with `process_group(0)`), so a
/// grandchild holding stdout dies with it. Elsewhere there is no group and this does nothing.
fn kill_group(pid: u32) {
    #[cfg(unix)]
    if let Ok(pgid) = libc::pid_t::try_from(pid) {
        // SAFETY: `kill` takes no pointers; a negative pid names a process group.
        #[allow(unsafe_code)]
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// Every name in `fc-list --format FC_LIST_FORMAT` output: tabs and newlines separate names,
/// which are otherwise taken as printed; empty names drop.
pub(crate) fn parse_fc_list(text: &str) -> Vec<String> {
    text.split(['\t', '\n'])
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The Windows font registry (HKLM and HKCU) plus the two font directories, with per-user
/// installs written to HKCU and loaded through GDI.
pub(crate) struct RegistryHost;

impl FontHost for RegistryHost {
    fn installed_names(&self) -> Result<Vec<String>, FontHostError> {
        let machine = winapi::font_values(false)
            .map_err(|e| FontHostError::new(format!("reading the machine font registry: {e}")))?;
        // A broken per-user key only hides per-user fonts; the machine set still answers.
        let user = winapi::font_values(true).unwrap_or_else(|e| {
            tracing::warn!("reading the user font registry: {e}");
            Vec::new()
        });
        let values: Vec<(Hive, String)> = machine
            .into_iter()
            .map(|v| (Hive::Machine, v))
            .chain(user.into_iter().map(|v| (Hive::User, v)))
            .collect();
        let system_dir = std::env::var_os("WINDIR")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
            .join("Fonts");
        let user_dir = self.fonts_dir()?;

        // Fonts copied in without a registry value still load, so both directories count.
        let listed = [&system_dir, &user_dir].into_iter().flat_map(|dir| {
            std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
        });
        let mut seen = HashSet::new();
        let mut names = Vec::new();
        for path in resolve_registry_values(&values, &system_dir, &user_dir)
            .into_iter()
            .chain(listed)
        {
            if seen.insert(path.to_string_lossy().to_lowercase())
                && let Ok(found) = font_names(&path)
            {
                names.extend(found.names);
            }
        }
        Ok(names)
    }

    fn fonts_dir(&self) -> Result<PathBuf, FontHostError> {
        dirs::data_local_dir()
            .map(|dir| dir.join("Microsoft").join("Windows").join("Fonts"))
            .ok_or_else(|| FontHostError::new("no local app data directory for fonts"))
    }

    fn register(
        &self,
        installed_file: &Path,
        full_name: &str,
        open_type: bool,
    ) -> Result<(), FontHostError> {
        // Another font's value under the same name is never replaced; ours is reused.
        let base = registry_value_name(full_name, open_type);
        let value = free_value_name(&base, installed_file.as_os_str(), winapi::user_font_value)
            .map_err(|e| FontHostError::new(format!("reading font registry value {base}: {e}")))?
            .ok_or_else(|| {
                FontHostError::new(format!("every registry name for {base} is taken"))
            })?;
        winapi::set_user_font_value(&value, installed_file)
            .map_err(|e| FontHostError::new(format!("writing font registry value {value}: {e}")))?;
        winapi::add_font_resource(installed_file)
            .map_err(|e| FontHostError::new(format!("loading {}: {e}", installed_file.display())))
    }

    fn refresh(&self) -> Result<(), FontHostError> {
        winapi::broadcast_font_change()
            .map_err(|e| FontHostError::new(format!("announcing the font change: {e}")))
    }
}

/// Which registry hive a `Fonts` value came from, deciding the directory relative data is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hive {
    Machine,
    User,
}

/// Registry value data to font files: absolute (`X:\`, `X:/` or `\\`) as is, anything else
/// joined to `system_dir` (Machine) or `user_dir` (User); empty data is dropped.
pub(crate) fn resolve_registry_values(
    values: &[(Hive, String)],
    system_dir: &Path,
    user_dir: &Path,
) -> Vec<PathBuf> {
    values
        .iter()
        .filter(|(_, data)| !data.is_empty())
        .map(|(hive, data)| {
            if is_windows_absolute(data) {
                PathBuf::from(data)
            } else {
                match hive {
                    Hive::Machine => system_dir.join(data),
                    Hive::User => user_dir.join(data),
                }
            }
        })
        .collect()
}

/// Judged on the text, since `Path::is_absolute` off Windows calls `C:\x` relative.
fn is_windows_absolute(data: &str) -> bool {
    let bytes = data.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    drive || data.starts_with(r"\\")
}

/// `"<full name> (TrueType)"`, or `(OpenType)` for CFF files.
pub(crate) fn registry_value_name(full_name: &str, open_type: bool) -> String {
    let format = if open_type { "OpenType" } else { "TrueType" };
    format!("{full_name} ({format})")
}

/// How many numbered alternatives `free_value_name` tries after `base`.
const MAX_VALUE_SUFFIX: u32 = 99;

/// The HKCU `Fonts` value name to write `data` under without replacing another font's entry:
/// `base` when free or already holding `data` (in any case), else the first such `"<base> #n"`;
/// `None` when every candidate is taken. `lookup` returns a value's current data, `None` when
/// absent.
pub(crate) fn free_value_name(
    base: &str,
    data: &OsStr,
    lookup: impl Fn(&str) -> std::io::Result<Option<OsString>>,
) -> std::io::Result<Option<String>> {
    let candidates = std::iter::once(base.to_owned())
        .chain((2..=MAX_VALUE_SUFFIX).map(|n| format!("{base} #{n}")));
    for name in candidates {
        match lookup(&name)? {
            None => return Ok(Some(name)),
            Some(existing) if same_windows_path(&existing, data) => return Ok(Some(name)),
            Some(_) => {}
        }
    }
    Ok(None)
}

/// Whether two Windows paths name the same file: by `to_lowercase()` when both are Unicode,
/// byte for byte otherwise.
fn same_windows_path(a: &OsStr, b: &OsStr) -> bool {
    // NTFS names are case-insensitive; a path that is not Unicode has no case to fold.
    match (a.to_str(), b.to_str()) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fc_list_names_split_only_on_tabs_and_newlines() {
        // Real fontconfig output for FC_LIST_FORMAT: each value ends in a tab, and commas and
        // backslashes inside a name are printed as is.
        let text = "DejaVu Sans\tDejaVu Sans Condensed\tDejaVu Sans Bold\t\n\
                    Foo, Inc Sans\tFoo, Inc Sans Regular\t\n\
                    Back\\slash\t\n\
                    \n\
                    \t\n\
                    Lato\t\n";
        assert_eq!(
            parse_fc_list(text),
            [
                "DejaVu Sans",
                "DejaVu Sans Condensed",
                "DejaVu Sans Bold",
                "Foo, Inc Sans",
                "Foo, Inc Sans Regular",
                "Back\\slash",
                "Lato",
            ]
        );
    }

    #[test]
    fn registry_values_resolve_per_hive() {
        let system = Path::new("/win/Fonts");
        let user = Path::new("/user/Fonts");
        let values = [
            (Hive::Machine, "arial.ttf".to_owned()),
            (Hive::User, "Lato.ttf".to_owned()),
            (
                Hive::User,
                r"C:\Users\me\AppData\Local\Microsoft\Windows\Fonts\x.otf".to_owned(),
            ),
            (Hive::Machine, r"\\server\share\y.ttf".to_owned()),
            (Hive::Machine, "D:/fonts/z.ttf".to_owned()),
            (Hive::User, String::new()),
        ];
        assert_eq!(
            resolve_registry_values(&values, system, user),
            [
                system.join("arial.ttf"),
                user.join("Lato.ttf"),
                PathBuf::from(r"C:\Users\me\AppData\Local\Microsoft\Windows\Fonts\x.otf"),
                PathBuf::from(r"\\server\share\y.ttf"),
                PathBuf::from("D:/fonts/z.ttf"),
            ]
        );
    }

    #[test]
    fn a_taken_registry_value_name_is_never_reused() {
        let taken = |name: &str| -> std::io::Result<Option<OsString>> {
            Ok(match name {
                "Surface (TrueType)" => Some(r"C:\other\surface.ttf".into()),
                "Surface (TrueType) #2" => Some(OsString::new()),
                "Mine (TrueType)" => Some(r"C:\fonts\mine.ttf".into()),
                _ => None,
            })
        };
        assert_eq!(
            free_value_name(
                "Surface (TrueType)",
                OsStr::new(r"C:\fonts\surface.ttf"),
                taken
            )
            .unwrap(),
            Some("Surface (TrueType) #3".to_owned())
        );
        assert_eq!(
            free_value_name("Mine (TrueType)", OsStr::new(r"C:\fonts\mine.ttf"), taken).unwrap(),
            Some("Mine (TrueType)".to_owned())
        );
        let all_taken = |_: &str| Ok(Some(OsString::new()));
        assert_eq!(
            free_value_name("X (TrueType)", OsStr::new("x"), all_taken).unwrap(),
            None
        );
    }

    #[test]
    fn our_registry_value_is_recognised_in_any_case() {
        let ours = |name: &str| -> std::io::Result<Option<OsString>> {
            Ok((name == "Surface (TrueType)")
                .then(|| r"C:\Users\Me\AppData\Local\Microsoft\Windows\Fonts\surface.ttf".into()))
        };
        let data = OsStr::new(r"c:\users\me\appdata\local\microsoft\windows\fonts\SURFACE.TTF");
        assert_eq!(
            free_value_name("Surface (TrueType)", data, ours).unwrap(),
            Some("Surface (TrueType)".to_owned())
        );
    }

    /// A lookup holding `data` under "Cafe (TrueType)" only.
    fn holding(data: OsString) -> impl Fn(&str) -> std::io::Result<Option<OsString>> {
        move |name| Ok((name == "Cafe (TrueType)").then(|| data.clone()))
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_registry_data_matches_only_itself() {
        use std::os::unix::ffi::OsStrExt;
        let ours = OsStr::from_bytes(b"C:\\fonts\\caf\xe9.ttf");
        let other = OsStr::from_bytes(b"C:\\fonts\\caf\xe8.ttf");
        assert_eq!(
            free_value_name("Cafe (TrueType)", ours, holding(ours.to_owned())).unwrap(),
            Some("Cafe (TrueType)".to_owned())
        );
        assert_eq!(
            free_value_name("Cafe (TrueType)", ours, holding(other.to_owned())).unwrap(),
            Some("Cafe (TrueType) #2".to_owned())
        );
    }

    #[cfg(windows)]
    #[test]
    fn unpaired_surrogate_registry_data_matches_only_itself() {
        use std::os::windows::ffi::OsStringExt;
        // C:\<lone surrogate>.ttf, once with D800 and once with D801.
        let path = |surrogate: u16| {
            OsString::from_wide(&[0x43, 0x3a, 0x5c, surrogate, 0x2e, 0x74, 0x74, 0x66])
        };
        assert_eq!(
            free_value_name("Cafe (TrueType)", &path(0xd800), holding(path(0xd800))).unwrap(),
            Some("Cafe (TrueType)".to_owned())
        );
        assert_eq!(
            free_value_name("Cafe (TrueType)", &path(0xd800), holding(path(0xd801))).unwrap(),
            Some("Cafe (TrueType) #2".to_owned())
        );
    }

    #[test]
    fn registry_value_names_carry_the_outline_format() {
        assert_eq!(
            registry_value_name("Surface SemiBold", false),
            "Surface SemiBold (TrueType)"
        );
        assert_eq!(
            registry_value_name("code-pro-bold", true),
            "code-pro-bold (OpenType)"
        );
    }

    #[cfg(unix)]
    fn sh(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[cfg(unix)]
    #[test]
    fn a_bounded_run_returns_stdout() {
        let out = run_bounded(sh("printf 'a\\tb\\n'"), Duration::from_secs(10), 64).unwrap();
        assert_eq!(out, b"a\tb\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_bounded_run_fails_on_spawn_exit_size_and_time() {
        let missing = run_bounded(
            Command::new("tf2tools-no-such-program"),
            Duration::from_secs(10),
            64,
        )
        .unwrap_err();
        assert!(missing.reason().contains("could not start"), "{missing}");

        let failed = run_bounded(sh("exit 3"), Duration::from_secs(10), 64).unwrap_err();
        assert!(failed.reason().contains("exited"), "{failed}");

        let chatty = run_bounded(sh("yes"), Duration::from_secs(10), 1024).unwrap_err();
        assert!(chatty.reason().contains("more than 1024 bytes"), "{chatty}");

        let started = Instant::now();
        let hung = run_bounded(sh("exec sleep 30"), Duration::from_millis(200), 64).unwrap_err();
        assert!(hung.reason().contains("no answer"), "{hung}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    /// A `sh` in `dir` running `script` with the pid file path as `$1`.
    #[cfg(target_os = "linux")]
    fn sh_with_pid_file(script: &str, dir: &Path) -> (Command, PathBuf) {
        let pid_file = dir.join("grandchild");
        let mut command = Command::new("sh");
        command
            .current_dir(dir)
            .args(["-c", script, "sh"])
            .arg(&pid_file);
        (command, pid_file)
    }

    #[cfg(target_os = "linux")]
    fn grandchild_pid(pid_file: &Path) -> u32 {
        std::fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }

    /// True once `pid` is gone or a zombie, polling for up to 5 s.
    #[cfg(target_os = "linux")]
    fn exits_soon(pid: u32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .and_then(|stat| {
                    stat.rsplit_once(')')
                        .and_then(|(_, rest)| rest.trim_start().chars().next())
                });
            if matches!(state, None | Some('Z' | 'X')) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_timeout_kills_the_grandchildren_too() {
        let tmp = tempfile::tempdir().unwrap();
        let (command, pid_file) = sh_with_pid_file("sleep 30 & echo $! > \"$1\"; wait", tmp.path());
        let started = Instant::now();
        let hung = run_bounded(command, Duration::from_millis(300), 64).unwrap_err();
        assert!(hung.reason().contains("no answer"), "{hung}");
        assert!(started.elapsed() < Duration::from_secs(5));
        let pid = grandchild_pid(&pid_file);
        assert!(exits_soon(pid), "grandchild {pid} outlived the timeout");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_grandchild_holding_stdout_is_killed_once_the_child_exits() {
        let tmp = tempfile::tempdir().unwrap();
        let (command, pid_file) =
            sh_with_pid_file("sleep 30 & echo $! > \"$1\"; echo hi", tmp.path());
        let started = Instant::now();
        let held = run_bounded(command, Duration::from_millis(300), 64).unwrap_err();
        assert!(held.reason().contains("output did not end"), "{held}");
        assert!(started.elapsed() < Duration::from_secs(5));
        let pid = grandchild_pid(&pid_file);
        assert!(exits_soon(pid), "grandchild {pid} kept stdout open");
    }
}
