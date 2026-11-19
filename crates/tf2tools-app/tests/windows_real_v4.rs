//! Removing a real TF2Tools 4 install on Windows through the app's own detection and runner.
//! Run only by `.github/workflows/windows-checks.yml`, which downloads v4.8.0's installer from
//! this repo's release; it installs v4 on the machine it runs on and removes it again.

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
    let installer = PathBuf::from(std::env::var_os("TF2TOOLS_V4_INSTALLER").expect("installer"));
    say(&format!(
        "installer {} ({} bytes)",
        installer.display(),
        std::fs::metadata(&installer).map_or(0, |m| m.len())
    ));
    assert!(detect_old_v4().is_none(), "the runner starts without v4");

    let mut install = Command::new(&installer);
    install.arg("/S");
    let status = run_step("install", install, Duration::from_secs(300), v4_state);
    assert!(status.success(), "v4's installer exited with {status}");
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
    assert_eq!(old.version(), "4.8.0", "the release's version");
    let install_dir = old.uninstaller().parent().unwrap().to_path_buf();
    assert!(
        install_dir.join("TF2Tools.exe").is_file(),
        "v4 installed into {}",
        install_dir.display()
    );

    // A copy of this binary named TF2Tools.exe stands in for v6: v4's uninstaller looks for a
    // running TF2Tools.exe, and the stand-in exits right after starting it, as v6 closes.
    let tmp = tempfile::tempdir().unwrap();
    let stand_in = tmp.path().join("TF2Tools.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &stand_in).unwrap();
    let mut close = Command::new(&stand_in);
    close
        .args([STAND_IN_TEST, "--exact", "--ignored", "--nocapture"])
        .env(STAND_IN, "1");
    let status = run_step("stand-in v6", close, Duration::from_secs(60), v4_state);
    assert!(status.success(), "the stand-in v6 exited with {status}");

    let gone = || (!v4_entry_listed() && !install_dir.exists()).then_some(());
    wait_step("removal", Duration::from_secs(300), gone);
    assert!(detect_old_v4().is_none(), "nothing left to offer");
}

/// The stand-in v6's part: detect v4 and start its uninstaller through the app's own runner.
/// Run directly, without the stand-in variable, it does nothing.
#[test]
#[ignore = "runs only as the stand-in v6 of the test above"]
fn stand_in_v6_starts_the_v4_uninstaller() {
    if std::env::var_os(STAND_IN).is_none() {
        return;
    }
    let old = detect_old_v4().expect("the stand-in finds v4");
    system_runner()(old.uninstaller(), old.arguments()).expect("the uninstaller starts");
}

/// Prints one line and flushes, so a step's progress reaches the log even if the job is killed.
fn say(line: &str) {
    println!("{line}");
    let _ = std::io::stdout().flush();
}

/// Runs `command` as `step` for at most `limit`, reporting `probe()` every `HEARTBEAT`. Past the
/// limit it records the desktop and processes, kills the child and fails naming the step.
fn run_step(
    step: &str,
    mut command: Command,
    limit: Duration,
    probe: impl Fn() -> String,
) -> ExitStatus {
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
            return status;
        }
        if started.elapsed() >= limit {
            diagnose(step);
            let _ = child.kill();
            let _ = child.wait();
            panic!("STEP {step}: still running after {} s", limit.as_secs());
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
