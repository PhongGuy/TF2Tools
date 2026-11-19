//! Removing a real TF2Tools 4 install on Windows through the app's own detection and runner.
//! Run only by `.github/workflows/windows-checks.yml`, which downloads v4.8.0's installer from
//! this repo's release; it installs v4 on the machine it runs on and removes it again.

use std::ffi::{OsStr, OsString};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tf2tools_app::old_v4::system_runner;
use tf2tools_core::detect_old_v4;

const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall";
/// Set only for the stand-in v6 process, which starts the uninstaller and exits.
const STAND_IN: &str = "TF2TOOLS_V4_STAND_IN";
const STAND_IN_TEST: &str = "windows_real_v4::stand_in_v6_starts_the_v4_uninstaller";
/// How often a waiting step reports what it sees, so a hang shows where it is.
const HEARTBEAT: Duration = Duration::from_secs(30);

#[test]
#[ignore = "installs and removes TF2Tools 4; run by windows-checks.yml"]
fn a_real_v4_install_is_found_and_removed_by_its_own_uninstaller() {
    assert_eq!(
        std::env::var("TF2TOOLS_REAL_WINDOWS").as_deref(),
        Ok("1"),
        "set TF2TOOLS_REAL_WINDOWS=1 to let this test install TF2Tools 4"
    );
    let releases = PathBuf::from(std::env::var_os("TF2TOOLS_V4_DIR").expect("v4 releases"));
    assert!(detect_old_v4().is_none(), "the runner starts without v4");

    // v4.8.0's web installer rejects the package on its release, so a real install is tried
    // with v4.7.5 first; failing that, v4.8.0 is put together from its own release files.
    let (how, expected) = if install_real(&releases.join("v4.7.5")) {
        (Install::Real, "4.7.5")
    } else {
        reconstruct(&releases.join("v4.8.0"));
        (Install::Reconstructed, "4.8.0")
    };
    say(&format!("RESULT install path: {how:?} v4 {expected}"));
    let old = wait_step("detect", Duration::from_secs(120), detect_old_v4);
    say(&v4_entries());
    say(&format!(
        "v6 found v4 {} at {} with {:?}",
        old.version(),
        old.uninstaller().display(),
        old.arguments()
    ));
    // A v4 the installer left running would make its uninstaller ask to close it.
    say(&tasklist(&["/v", "/FI", "IMAGENAME eq TF2Tools.exe"]));
    keep_for_inspection(old.uninstaller());
    assert_eq!(old.version(), expected, "the installed version");
    let install_dir = old.uninstaller().parent().unwrap().to_path_buf();
    assert!(
        install_dir.join("TF2Tools.exe").is_file(),
        "v4 installed into {}",
        install_dir.display()
    );

    // v4's uninstaller asks `cmd /c tasklist ... | find "TF2Tools.exe"` whether v4 still runs,
    // and gives up with exit code 2 when it seems to. This job's PATH, inherited from Git Bash,
    // may resolve `find` to Git's own find, unlike a player's PATH where System32 comes first.
    say(&find_check(&install_dir, &runner_path()));
    say(&find_check(&install_dir, &player_path()));

    // First with the runner's PATH as a control, then as a player's PC starts v6.
    let control = remove_by(Way::RunnerPath, &old);
    say(&format!(
        "RESULT RunnerPath: folder removed {control}, entry left {}",
        v4_entry_listed()
    ));
    if detect_old_v4().is_none() {
        assert!(
            install_real(&releases.join("v4.7.5")),
            "v4.7.5 installs again"
        );
    }
    let old = wait_step("detect", Duration::from_secs(120), detect_old_v4);
    let removed = remove_by(Way::PlayerPath, &old);
    let entry_left = v4_entry_listed();
    say(&format!(
        "RESULT PlayerPath: folder removed {removed}, entry left {entry_left}"
    ));
    assert!(removed, "the app's own way removes v4 on a player's PATH");
    // The reconstructed entry's key name is derived, not read from a real install, so a key
    // the uninstaller leaves behind is reported rather than failed on.
    if how == Install::Real {
        assert!(!entry_left, "v4's uninstaller removes its own entry");
    }
}

/// The PATH the stand-in v6 runs with; both start the uninstaller the app's way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    /// This job's PATH, as inherited from Git Bash.
    RunnerPath,
    /// System32 first, as Windows gives a program started from Explorer.
    PlayerPath,
}

fn runner_path() -> OsString {
    std::env::var_os("PATH").unwrap_or_default()
}

/// Windows' own folders first, then the rest of this job's PATH.
fn player_path() -> OsString {
    let root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let first = [
        root.join("System32"),
        root.clone(),
        root.join(r"System32\Wbem"),
        root.join(r"System32\WindowsPowerShell\v1.0"),
    ];
    let rest: Vec<PathBuf> = std::env::split_paths(&runner_path()).collect();
    std::env::join_paths(first.into_iter().chain(rest)).unwrap()
}

/// Which `find` cmd picks with `path`, and what v4's running check answers from the install
/// folder: exit code 0 means it believes TF2Tools.exe is running.
fn find_check(dir: &Path, path: &OsStr) -> String {
    let run = |line: &str| {
        let mut command = Command::new("cmd");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            command.raw_arg(format!("/c {line}"));
        }
        #[cfg(not(windows))]
        command.arg(line);
        command.current_dir(dir).env("PATH", path);
        bounded_output(&mut command).map_or_else(
            || format!("{line}: no answer"),
            |out| {
                format!(
                    "{line}: exit {:?}, out {:?}, err {:?}",
                    out.status.code(),
                    String::from_utf8_lossy(&out.stdout).trim(),
                    String::from_utf8_lossy(&out.stderr).trim()
                )
            },
        )
    };
    let first = std::env::split_paths(path).next().unwrap_or_default();
    format!(
        "FIND with PATH starting {}\n  {}\n  {}",
        first.display(),
        run("where find"),
        run(
            r#"tasklist /FI "USERNAME eq %USERNAME%" /FI "IMAGENAME eq TF2Tools.exe" | find "TF2Tools.exe""#
        )
    )
}

/// Starts v4's uninstaller the app's way with `way`'s PATH and waits up to two minutes for its
/// folder to go, while a watcher reports the temp copy `Un_A.exe` (command line, parent, exit
/// code, lifetime). True when the folder went.
fn remove_by(way: Way, old: &tf2tools_core::OldV4) -> bool {
    let install_dir = old.uninstaller().parent().unwrap().to_path_buf();
    let step = format!("remove {way:?}");
    say(&format!("STEP {step}: start"));
    let watcher = watch_temp_uninstaller();
    let started = Instant::now();
    // A copy of this binary named TF2Tools.exe stands in for v6, since v4's uninstaller looks
    // for a running TF2Tools.exe and v6 exits right after starting it.
    let tmp = tempfile::tempdir().unwrap();
    let stand_in = tmp.path().join("TF2Tools.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &stand_in).unwrap();
    let mut close = Command::new(&stand_in);
    close
        .args([STAND_IN_TEST, "--exact", "--ignored", "--nocapture"])
        .env(STAND_IN, "1")
        .env(
            "PATH",
            match way {
                Way::RunnerPath => runner_path(),
                Way::PlayerPath => player_path(),
            },
        );
    let status = run_step("stand-in v6", close, Duration::from_secs(60), v4_state);
    assert!(status.success(), "the stand-in v6 exited with {status}");
    let mut seen = std::collections::BTreeMap::new();
    let limit = Duration::from_secs(120);
    let removed = loop {
        if !install_dir.exists() {
            break true;
        }
        if started.elapsed() >= limit {
            diagnose(&step);
            break false;
        }
        for name in installer_processes() {
            let at = started.elapsed().as_secs();
            seen.entry(name).or_insert((at, at)).1 = at;
        }
        std::thread::sleep(Duration::from_secs(1));
    };
    say(&format!(
        "{step}: after {} s folder removed {removed}; processes seen (first s, last s): {seen:?}",
        started.elapsed().as_secs()
    ));
    let watched = watcher
        .recv_timeout(Duration::from_secs(60))
        .ok()
        .and_then(Result::ok)
        .map_or_else(
            || "watcher did not finish".to_owned(),
            |out| String::from_utf8_lossy(&out.stdout).into_owned(),
        );
    say(&watched);
    removed
}

/// Waits (up to 30 s) for NSIS's temp copy `Un_A.exe` to start, then reports its command line,
/// path and parent, and waits up to 150 s more for its exit code and lifetime.
fn watch_temp_uninstaller() -> mpsc::Receiver<std::io::Result<Output>> {
    let script = r#"
$deadline = (Get-Date).AddSeconds(30)
$p = $null
while (-not $p -and (Get-Date) -lt $deadline) {
  $p = @(Get-CimInstance Win32_Process -Filter "Name='Un_A.exe'")[0]
  if (-not $p) { Start-Sleep -Milliseconds 50 }
}
if (-not $p) { 'WATCH no Un_A.exe within 30 s'; exit }
"WATCH pid $($p.ProcessId) parent $($p.ParentProcessId) path $($p.ExecutablePath)"
"WATCH cmdline $($p.CommandLine)"
try {
  $h = [System.Diagnostics.Process]::GetProcessById($p.ProcessId)
  $null = $h.Handle
  $clock = [Diagnostics.Stopwatch]::StartNew()
  if ($h.WaitForExit(150000)) {
    "WATCH exit code $($h.ExitCode) after $([int]$clock.Elapsed.TotalSeconds) s of watching"
  } else { 'WATCH still running after 150 s' }
} catch { "WATCH exited before its handle was taken: $_" }
"#;
    let mut command = Command::new("powershell");
    command
        .args(["-NoProfile", "-Command", script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (sender, receiver) = mpsc::channel();
    match command.spawn() {
        Ok(child) => {
            std::thread::spawn(move || {
                let _ = sender.send(child.wait_with_output());
            });
        }
        Err(e) => {
            let _ = sender.send(Err(e));
        }
    }
    receiver
}

/// Names of running processes that look like v4's app or an NSIS installer or its temp copy.
fn installer_processes() -> Vec<String> {
    tasklist(&["/FO", "CSV", "/NH"])
        .lines()
        .filter_map(|line| line.split(',').next())
        .map(|name| name.trim_matches('"').to_owned())
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            ["un_", "au_", "uninstall", "tf2tools", "setup"]
                .iter()
                .any(|hint| lower.contains(hint))
        })
        .collect()
}

/// How v4 got onto the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Install {
    /// Its own web installer ran.
    Real,
    /// Its package and uninstaller were placed and its entry written by this test.
    Reconstructed,
}

/// Runs the release's web installer silently beside its package; false when it never yields
/// a v4 v6 can find (a checksum dialog shows up as the step running out of time).
fn install_real(release: &Path) -> bool {
    let stub = only_file(release, "exe");
    say(&format!(
        "installer {} ({} bytes)",
        stub.display(),
        std::fs::metadata(&stub).map_or(0, |m| m.len())
    ));
    let mut install = Command::new(&stub);
    install.arg("/S");
    let Some(status) = try_step("real install", install, Duration::from_secs(180), v4_state) else {
        return false;
    };
    say(&format!("real install exited {status}"));
    status.success() && detect_old_v4().is_some()
}

/// Places v4 as its installer would: the package's files and the stub's own uninstaller in the
/// per-user install folder, then the Uninstall entry electron-builder writes.
fn reconstruct(release: &Path) {
    let dir = Path::new(&std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA"))
        .join("Programs")
        .join("tf2tools");
    let out = format!("-o{}", dir.display());
    let mut unpack = Command::new("7z");
    unpack.args(["x", "-y", &out]).arg(only_file(release, "7z"));
    let status = run_step("unpack package", unpack, Duration::from_secs(300), v4_state);
    assert!(status.success(), "7z unpacked the package: {status}");
    let mut uninstaller = Command::new("7z");
    uninstaller
        .args(["e", "-y", &out])
        .arg(only_file(release, "exe"))
        .arg(r"$R0\Uninstall TF2Tools.exe");
    let status = run_step(
        "take uninstaller",
        uninstaller,
        Duration::from_secs(60),
        v4_state,
    );
    assert!(status.success(), "7z took the uninstaller: {status}");
    let exe = dir.join("Uninstall TF2Tools.exe");
    assert!(exe.is_file(), "the stub holds {}", exe.display());

    // electron-builder names the key by UUID v5 of the default app id, com.electron.tf2tools.
    let key = format!(r"{UNINSTALL_KEY}\4fef478b-7619-503a-a35c-72a914c5ac20");
    let quoted = format!("\"{}\" /currentuser", exe.display());
    let values = [
        ("DisplayName", "REG_SZ", "TF2Tools 4.8.0".to_owned()),
        ("DisplayVersion", "REG_SZ", "4.8.0".to_owned()),
        ("Publisher", "REG_SZ", "PhongGuy".to_owned()),
        ("UninstallString", "REG_SZ", quoted.clone()),
        ("QuietUninstallString", "REG_SZ", format!("{quoted} /S")),
        (
            "DisplayIcon",
            "REG_SZ",
            format!("{},0", dir.join("TF2Tools.exe").display()),
        ),
        ("NoModify", "REG_DWORD", "1".to_owned()),
        ("NoRepair", "REG_DWORD", "1".to_owned()),
    ];
    for (name, kind, data) in values {
        let added = bounded_output(
            Command::new("reg").args(["add", &key, "/v", name, "/t", kind, "/d", &data, "/f"]),
        )
        .is_some_and(|out| out.status.success());
        assert!(added, "reg add {name}");
    }
}

/// The one file in `dir` with extension `ext`.
fn only_file(dir: &Path, ext: &str) -> PathBuf {
    let found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)))
        .collect();
    assert_eq!(found.len(), 1, "one .{ext} in {}: {found:?}", dir.display());
    found.into_iter().next().unwrap()
}

/// The stand-in v6's part: detect v4 and start its uninstaller as the app does, through its
/// runner with the silent arguments.
/// Run directly, without the stand-in variable, it does nothing.
#[test]
#[ignore = "runs only as the stand-in v6 of the test above"]
fn stand_in_v6_starts_the_v4_uninstaller() {
    if std::env::var_os(STAND_IN).is_none() {
        return;
    }
    let old = detect_old_v4().expect("the stand-in finds v4");
    system_runner()(old.uninstaller(), &old.silent_arguments()).expect("the uninstaller starts");
}

/// Prints one line and flushes, so a step's progress reaches the log even if the job is killed.
fn say(line: &str) {
    println!("{line}");
    let _ = std::io::stdout().flush();
}

/// Runs `command` as `step` for at most `limit`, reporting `probe()` every `HEARTBEAT`. Past the
/// limit it records the desktop and processes, kills the child and returns `None`.
fn try_step(
    step: &str,
    mut command: Command,
    limit: Duration,
    probe: impl Fn() -> String,
) -> Option<ExitStatus> {
    say(&format!("STEP {step}: start"));
    let mut child = command.spawn().unwrap();
    let started = Instant::now();
    let mut beat = started;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            say(&format!(
                "STEP {step}: exited {status} after {} s",
                started.elapsed().as_secs()
            ));
            return Some(status);
        }
        if started.elapsed() >= limit {
            diagnose(step);
            let _ = child.kill();
            let _ = child.wait();
            say(&format!(
                "STEP {step}: still running after {} s",
                limit.as_secs()
            ));
            return None;
        }
        if beat.elapsed() >= HEARTBEAT {
            beat = Instant::now();
            say(&format!(
                "STEP {step}: {} s, {}",
                started.elapsed().as_secs(),
                probe()
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// [`try_step`] that fails the test when the step runs out of time.
fn run_step(
    step: &str,
    command: Command,
    limit: Duration,
    probe: impl Fn() -> String,
) -> ExitStatus {
    try_step(step, command, limit, probe)
        .unwrap_or_else(|| panic!("STEP {step}: still running after {} s", limit.as_secs()))
}

/// Polls `check` as `step` every half second for at most `limit`, failing like `run_step`.
fn wait_step<T>(step: &str, limit: Duration, mut check: impl FnMut() -> Option<T>) -> T {
    say(&format!("STEP {step}: waiting"));
    let started = Instant::now();
    let mut beat = started;
    loop {
        if let Some(found) = check() {
            say(&format!(
                "STEP {step}: done after {} s",
                started.elapsed().as_secs()
            ));
            return found;
        }
        if started.elapsed() >= limit {
            diagnose(step);
            panic!("STEP {step}: nothing after {} s", limit.as_secs());
        }
        if beat.elapsed() >= HEARTBEAT {
            beat = Instant::now();
            say(&format!(
                "STEP {step}: {} s, {}",
                started.elapsed().as_secs(),
                v4_state()
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// One line on where v4 stands: its Uninstall entry and the TF2Tools.exe processes.
fn v4_state() -> String {
    let running = tasklist(&["/FI", "IMAGENAME eq TF2Tools.exe", "/FO", "CSV", "/NH"]);
    format!(
        "entry listed {}, TF2Tools.exe processes: {}",
        v4_entry_listed(),
        running.lines().filter(|l| l.contains("TF2Tools")).count()
    )
}

/// Every HKCU Uninstall key mentioning TF2Tools, with all its values, as `reg query` prints them.
fn v4_entries() -> String {
    let Some(out) = bounded_output(Command::new("reg").args(["query", UNINSTALL_KEY, "/s"])) else {
        return "reg query did not answer".to_owned();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut blocks = Vec::new();
    for line in text.lines() {
        if line.starts_with("HKEY_") {
            blocks.push(String::new());
        }
        if let Some(block) = blocks.last_mut() {
            block.push_str(line);
            block.push('\n');
        }
    }
    blocks.retain(|block| block.contains("TF2Tools"));
    blocks.concat()
}

fn v4_entry_listed() -> bool {
    v4_entries().contains("TF2Tools 4.")
}

fn tasklist(args: &[&str]) -> String {
    bounded_output(Command::new("tasklist").args(args)).map_or_else(
        || "tasklist did not answer".to_owned(),
        |out| String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// `command`'s output, or `None` when it does not finish within 30 s; the diagnostics must
/// never become the hang they report on.
fn bounded_output(command: &mut Command) -> Option<Output> {
    let child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    receiver.recv_timeout(Duration::from_secs(30)).ok()?.ok()
}

/// Copies v4's uninstaller into the check output, so the workflow can show its script.
fn keep_for_inspection(uninstaller: &Path) {
    if let Some(dir) = std::env::var_os("TF2TOOLS_CHECK_OUT") {
        let _ = std::fs::copy(uninstaller, Path::new(&dir).join("v4-uninstaller.exe"));
    }
}

/// Records what a stuck step may be waiting on: the desktop (a dialog), every process with its
/// window title, and the Uninstall entries.
fn diagnose(step: &str) {
    say(&format!("STEP {step}: diagnostics"));
    if let Some(dir) = std::env::var_os("TF2TOOLS_CHECK_OUT") {
        let shot = Path::new(&dir).join(format!("{}.png", step.replace(' ', "-")));
        let script = "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; \
            $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \
            $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height; \
            [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size); \
            $bmp.Save($env:SHOT)";
        let shot_taken = bounded_output(
            Command::new("powershell")
                .args(["-NoProfile", "-Command", script])
                .env("SHOT", &shot),
        );
        say(&format!(
            "screenshot {}: {:?}",
            shot.display(),
            shot_taken.map(|o| o.status)
        ));
    }
    say(&tasklist(&["/v"]));
    say(&v4_entries());
}
