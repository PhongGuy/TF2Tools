//! A private KWin session that logs every window damage commit.
//!
//! `run_session` starts `kwin_wayland --virtual` inside its own `dbus-run-session` with a cleared
//! env, so the command under test never sees the user's display or bus.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::guard::{REAL_DATA_VAR, SESSION_VAR, assert_fresh, assert_scratch, refuse_real_args};

pub const SOCKET: &str = "tf2tools-timing";

/// KWin script: logs `DMG <pid> <unix_ms>` for every damage commit of a normal window.
pub const DAMAGE_JS: &str = r#"function hook(w) {
    if (!w.normalWindow) return;
    console.info("HOOK " + w.pid + " " + w.resourceClass);
    w.damaged.connect(function () { console.info("DMG " + w.pid + " " + Date.now()); });
}
workspace.windowAdded.connect(hook);
workspace.stackingOrder.forEach(hook);
console.info("READY");
"#;

/// Runs inside `dbus-run-session`; the command is in "$@", the scratch in $TIMING_SCRATCH.
pub const INNER_SH: &str = r#"#!/bin/sh
# Inside the private bus: start KWin on a private socket, load the damage script, run the
# command against that socket, then stop KWin. Exit codes 70-72 name the step that failed.
set -u
S="$TIMING_SCRATCH"
QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES="js.debug=true;default.debug=false" \
KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 \
  kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --socket tf2tools-timing \
  --width 1920 --height 1080 >"$S/kwin.log" 2>&1 &
KWIN=$!
fail() { echo "session: $2" >&2; kill "$KWIN" 2>/dev/null; exit "$1"; }
i=0
while [ ! -S "$XDG_RUNTIME_DIR/tf2tools-timing" ]; do
  i=$((i + 1)); [ "$i" -gt 200 ] && fail 70 "kwin socket never appeared"; sleep 0.05
done
i=0
until qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$S/damage.js" tf2tools-timing >/dev/null 2>&1; do
  i=$((i + 1)); [ "$i" -gt 100 ] && fail 71 "kwin refused the damage script"; sleep 0.05
done
qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.start >/dev/null || fail 71 "kwin did not start scripts"
i=0
while ! grep -q READY "$S/kwin.log"; do
  i=$((i + 1)); [ "$i" -gt 200 ] && fail 72 "damage script never reported READY"; sleep 0.05
done
WAYLAND_DISPLAY=tf2tools-timing "$@"
STATUS=$?
kill "$KWIN" 2>/dev/null
wait "$KWIN" 2>/dev/null
exit "$STATUS"
"#;

/// HOME, XDG_CONFIG_HOME, XDG_DATA_HOME, XDG_CACHE_HOME, XDG_RUNTIME_DIR, in that order, under
/// `scratch`.
pub fn session_env(scratch: &Path) -> Vec<(&'static str, PathBuf)> {
    [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_RUNTIME_DIR", "rt"),
    ]
    .into_iter()
    .map(|(key, dir)| (key, scratch.join(dir)))
    .collect()
}

/// The whole env `dbus-run-session` starts with: `session_env`, PATH=/usr/bin:/bin,
/// LANG=C.UTF-8, TIMING_SCRATCH=`scratch`, TF2TOOLS_REAL_DATA=`real`. Nothing else.
pub fn inner_env(scratch: &Path, real: &Path) -> Vec<(&'static str, OsString)> {
    let mut env: Vec<(&'static str, OsString)> = session_env(scratch)
        .into_iter()
        .map(|(key, path)| (key, path.into_os_string()))
        .collect();
    env.extend([
        ("PATH", OsString::from("/usr/bin:/bin")),
        ("LANG", OsString::from("C.UTF-8")),
        (SESSION_VAR, scratch.as_os_str().to_owned()),
        (REAL_DATA_VAR, real.as_os_str().to_owned()),
    ]);
    env
}

/// Refuses a scratch whose `rt/tf2tools-timing` socket path is 100 bytes or longer ("too long").
pub fn check_socket(scratch: &Path) -> Result<(), String> {
    // A unix socket address holds 108 bytes; keep a margin rather than fail inside KWin.
    let socket = scratch.join("rt").join(SOCKET);
    let len = socket.as_os_str().len();
    if len >= 100 {
        return Err(format!(
            "socket path is too long ({len} bytes): {}",
            socket.display()
        ));
    }
    Ok(())
}

/// `(pid, unix_ms)` from a line holding `DMG <pid> <ms>`.
pub fn parse_damage(line: &str) -> Option<(u32, u64)> {
    let mut words = line.split_whitespace().skip_while(|w| *w != "DMG").skip(1);
    let pid = words.next()?.parse().ok()?;
    let ms = words.next()?.parse().ok()?;
    Some((pid, ms))
}

pub fn extract_damage(log: &str) -> Vec<(u32, u64)> {
    log.lines().filter_map(parse_damage).collect()
}

/// Runs `command` inside a private session; writes one `DMG <pid> <unix_ms>` line per damage to
/// `out`; returns the command's exit code.
pub fn run_session(
    scratch: &Path,
    out: &Path,
    command: &[OsString],
    real: &Path,
) -> Result<i32, String> {
    assert_scratch(scratch, real)?;
    assert_fresh(scratch)?;
    check_socket(scratch)?;
    refuse_real_args(command, real)?;

    let io = |what: &str, e: std::io::Error| format!("{what}: {e}");
    for (_, dir) in session_env(scratch) {
        std::fs::create_dir_all(&dir).map_err(|e| io(&dir.display().to_string(), e))?;
    }
    // Wayland refuses a runtime dir that others can read.
    let rt = scratch.join("rt");
    std::fs::set_permissions(&rt, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| io("chmod rt", e))?;
    std::fs::write(scratch.join("damage.js"), DAMAGE_JS).map_err(|e| io("damage.js", e))?;
    let inner = scratch.join("inner.sh");
    std::fs::write(&inner, INNER_SH).map_err(|e| io("inner.sh", e))?;

    // The cleared env drops DISPLAY, WAYLAND_DISPLAY and the user's bus address before
    // dbus-run-session starts a bus of its own.
    let status = Command::new("timeout")
        .args(["1800", "dbus-run-session", "--", "sh"])
        .arg(&inner)
        .args(command)
        .env_clear()
        .envs(inner_env(scratch, real))
        .status()
        .map_err(|e| io("spawn dbus-run-session", e))?;

    let log_path = scratch.join("kwin.log");
    let log = std::fs::read(&log_path).map_err(|e| io("read kwin.log", e))?;
    let damage = extract_damage(&String::from_utf8_lossy(&log));
    let lines: String = damage
        .iter()
        .map(|(pid, ms)| format!("DMG {pid} {ms}\n"))
        .collect();
    std::fs::write(out, lines).map_err(|e| io(&out.display().to_string(), e))?;
    eprintln!(
        "session damages={} kwin_log={}",
        damage.len(),
        log_path.display()
    );
    // A signal death has no code; report it the way a shell would.
    Ok(status.code().unwrap_or(128))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::{REAL_DATA_VAR, SESSION_VAR};
    use std::ffi::OsString;

    #[test]
    fn damage_lines_parse_from_the_kwin_log() {
        assert_eq!(
            parse_damage("js: DMG 3011477 1792145238302"),
            Some((3011477, 1792145238302))
        );
        assert_eq!(parse_damage("DMG 52 1000"), Some((52, 1000)));
        assert_eq!(parse_damage("js: HOOK 52 vkcube"), None);
        assert_eq!(parse_damage("js: DMG 52"), None);
        assert_eq!(
            parse_damage("kwin_screencast: Failed to connect PipeWire context"),
            None
        );
    }

    #[test]
    fn extract_damage_keeps_only_damage_lines() {
        let log = "js: HOOK 5 tf2tools\njs: DMG 5 100\nnoise\njs: DMG 5 117\n";
        assert_eq!(extract_damage(log), vec![(5, 100), (5, 117)]);
    }

    #[test]
    fn the_session_env_keeps_every_home_in_the_scratch() {
        let s = Path::new("/scratch/sess");
        let env = session_env(s);
        let keys: Vec<&str> = env.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            keys,
            [
                "HOME",
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_CACHE_HOME",
                "XDG_RUNTIME_DIR"
            ]
        );
        assert!(env.iter().all(|(_, v)| v.starts_with(s)));
    }

    #[test]
    fn the_socket_path_fits_a_unix_address() {
        assert!(check_socket(Path::new("/home/u/.cache/tf2tools-scratch-m2f/sess-v5")).is_ok());
        let long = PathBuf::from("/").join("x".repeat(120));
        assert!(check_socket(&long).unwrap_err().contains("too long"));
    }

    #[test]
    fn the_scripts_hook_damage_and_run_the_command() {
        assert!(DAMAGE_JS.contains("w.damaged.connect"));
        assert!(DAMAGE_JS.contains("console.info(\"READY\")"));
        assert!(INNER_SH.contains("--socket tf2tools-timing"));
        assert!(INNER_SH.contains("--width 1920 --height 1080"));
        assert!(INNER_SH.contains("WAYLAND_DISPLAY=tf2tools-timing \"$@\""));
    }

    #[test]
    fn the_inner_env_exports_the_real_dir_and_the_scratch() {
        let real = Path::new("/home/u/.local/share/com.tf2tools.app");
        let env = inner_env(Path::new("/scratch/sess"), real);
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get(REAL_DATA_VAR), Some(OsString::from(real)));
        assert_eq!(get(SESSION_VAR), Some(OsString::from("/scratch/sess")));
        assert_eq!(get("PATH"), Some(OsString::from("/usr/bin:/bin")));
        assert_eq!(get("HOME"), Some(OsString::from("/scratch/sess/home")));
        assert_eq!(get("DISPLAY"), None);
        assert_eq!(get("DBUS_SESSION_BUS_ADDRESS"), None);
    }

    /// Run: env CARGO_PROFILE_DEV_DEBUG=0
    ///      cargo test -p tf2tools-app --example app_timing -j 4 -- --ignored --nocapture
    #[test]
    #[ignore = "starts a private KWin session; run by hand"]
    fn a_private_session_sees_vkcube_draw() {
        let tmp = tempfile::tempdir().unwrap();
        let scratch = tmp.path().join("sess");
        let out = tmp.path().join("dmg.log");
        let cmd: Vec<OsString> = ["vkcube", "--wsi", "wayland", "--c", "60"]
            .map(OsString::from)
            .to_vec();
        let real = PathBuf::from("/nonexistent-real-data/com.tf2tools.app");
        assert_eq!(run_session(&scratch, &out, &cmd, &real).unwrap(), 0);
        let lines = std::fs::read_to_string(&out).unwrap();
        assert!(lines.lines().count() >= 30, "{lines}");
        assert!(lines.lines().all(|l| l.starts_with("DMG ")), "{lines}");
    }
}
