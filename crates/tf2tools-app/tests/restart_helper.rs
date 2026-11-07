//! The vendored restart helper, run against stub processes: the "app" it starts is `touch`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn linux_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/gpui-pre-linux/src/linux")
}

// The bytes `LinuxPlatform::restart` ships, pulled in the same way.
const SHIPPED: &str = include_str!("../../../vendor/gpui-pre-linux/src/linux/restart.sh");

fn script() -> String {
    SHIPPED.to_owned()
}

/// Runs `script` as `LinuxPlatform::restart` does, waiting on `pid` and then running
/// `touch marker`; panics if it still runs after `within`. Returns how long it took.
fn run_helper(script: &str, pid: u32, marker: &Path, within: Duration) -> Duration {
    let start = Instant::now();
    let mut helper = Command::new("bash")
        .arg("-c")
        .arg(script)
        .arg(pid.to_string())
        .arg("touch")
        .arg(marker)
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    loop {
        if let Some(status) = helper.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            return start.elapsed();
        }
        if start.elapsed() > within {
            let _ = helper.kill();
            let _ = helper.wait();
            panic!("the helper was still waiting after {within:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn state_of(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(") ")?.1.chars().next()
}

#[test]
fn the_platform_runs_the_shipped_script() {
    let platform = std::fs::read_to_string(linux_dir().join("platform.rs")).unwrap();
    assert!(platform.contains(r#"include_str!("restart.sh")"#));
}

#[test]
fn a_zombie_old_process_counts_as_exited() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("started");
    // A child that has exited but is not waited for stays a zombie of this test process.
    let mut old = Command::new("true").spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while state_of(old.id()) != Some('Z') {
        assert!(Instant::now() < deadline, "the stub never became a zombie");
        std::thread::sleep(Duration::from_millis(10));
    }
    run_helper(&script(), old.id(), &marker, Duration::from_secs(5));
    assert!(marker.exists(), "the app was not started");
    old.wait().unwrap();
}

#[test]
fn a_live_old_process_is_waited_for_only_up_to_the_limit() {
    let script = script();
    assert!(
        script.contains("\nlimit=10\n"),
        "the helper's wait limit moved"
    );
    let quick = script.replacen("\nlimit=10\n", "\nlimit=1\n", 1);
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("started");
    let mut old = Command::new("sleep").arg("30").spawn().unwrap();
    let took = run_helper(&quick, old.id(), &marker, Duration::from_secs(5));
    assert!(took >= Duration::from_secs(1), "gave up after {took:?}");
    assert!(marker.exists(), "the app was not started");
    assert!(
        old.try_wait().unwrap().is_none(),
        "the old process should still run"
    );
    old.kill().unwrap();
    old.wait().unwrap();
}
