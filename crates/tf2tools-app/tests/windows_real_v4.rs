//! Removing a real TF2Tools 4 install on Windows through the app's own detection and runner.
//! Run only by `.github/workflows/windows-checks.yml`, which downloads v4.8.0's installer from
//! this repo's release; it installs v4 on the machine it runs on and removes it again.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use tf2tools_app::old_v4::system_runner;
use tf2tools_core::detect_old_v4;

const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall";
/// Set only for the stand-in v6 process, which starts the uninstaller and exits.
const STAND_IN: &str = "TF2TOOLS_V4_STAND_IN";
const STAND_IN_TEST: &str = "windows_real_v4::stand_in_v6_starts_the_v4_uninstaller";
const WAIT: Duration = Duration::from_secs(300);

#[test]
#[ignore = "installs and removes TF2Tools 4; run by windows-checks.yml"]
fn a_real_v4_install_is_found_and_removed_by_its_own_uninstaller() {
    assert_eq!(
        std::env::var("TF2TOOLS_REAL_WINDOWS").as_deref(),
        Ok("1"),
        "set TF2TOOLS_REAL_WINDOWS=1 to let this test install TF2Tools 4"
    );
    let installer = PathBuf::from(std::env::var_os("TF2TOOLS_V4_INSTALLER").expect("installer"));
    assert!(detect_old_v4().is_none(), "the runner starts without v4");

    let started = Instant::now();
    let status = Command::new(&installer).arg("/S").status().unwrap();
    assert!(status.success(), "v4's installer exited with {status}");
    let old = wait_for(detect_old_v4).unwrap_or_else(|| fail("v6 never found the installed v4"));
    println!(
        "TIMING v4 silent install: {} s",
        started.elapsed().as_secs()
    );
    println!("{}", v4_entries());
    println!(
        "v6 found v4 {} at {} with {:?}",
        old.version(),
        old.uninstaller().display(),
        old.arguments()
    );
    // A v4 the installer left running would make its uninstaller ask to close it.
    let running = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq TF2Tools.exe"])
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&running.stdout));
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
    let status = Command::new(&stand_in)
        .args([STAND_IN_TEST, "--exact", "--ignored", "--nocapture"])
        .env(STAND_IN, "1")
        .status()
        .unwrap();
    assert!(status.success(), "the stand-in v6 exited with {status}");
    let closed = Instant::now();

    let removed = wait_for(|| (!v4_entry_listed() && !install_dir.exists()).then_some(()));
    if removed.is_none() {
        fail(&format!(
            "v4 was not removed: entry listed {}, folder present {}",
            v4_entry_listed(),
            install_dir.exists()
        ));
    }
    println!(
        "TIMING v4 removal after v6 closed: {} s",
        closed.elapsed().as_secs()
    );
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

/// Polls `check` every half second until it answers or `WAIT` passes.
fn wait_for<T>(mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(found) = check() {
            return Some(found);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Every HKCU Uninstall key mentioning TF2Tools, with all its values, as `reg query` prints them.
fn v4_entries() -> String {
    let out = Command::new("reg")
        .args(["query", UNINSTALL_KEY, "/s"])
        .output()
        .unwrap();
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

/// Panics with `why` after saving what the desktop shows, since a blocked uninstaller is
/// usually waiting on a dialog.
fn fail(why: &str) -> ! {
    if let Some(dir) = std::env::var_os("TF2TOOLS_CHECK_OUT") {
        let shot = Path::new(&dir).join("desktop.png");
        let script = "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; \
            $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \
            $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height; \
            [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size); \
            $bmp.Save($env:SHOT)";
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .env("SHOT", &shot)
            .status();
    }
    if let Ok(out) = Command::new("tasklist").output() {
        println!("{}", String::from_utf8_lossy(&out.stdout));
    }
    println!("{}", v4_entries());
    panic!("{why}");
}
