use tf2tools_app::diagnostics::log_filter;
use tracing_subscriber::filter::LevelFilter;

#[test]
fn the_filter_defaults_to_warn() {
    assert_eq!(log_filter(None).max_level_hint(), Some(LevelFilter::WARN));
    assert_eq!(
        log_filter(Some("  ")).max_level_hint(),
        Some(LevelFilter::WARN)
    );
}

#[test]
fn rust_log_wins_when_it_parses() {
    assert_eq!(
        log_filter(Some("tf2tools_core=debug")).max_level_hint(),
        Some(LevelFilter::DEBUG)
    );
    assert_eq!(
        log_filter(Some("tf2tools_core=loud")).max_level_hint(),
        Some(LevelFilter::WARN)
    );
}

#[test]
fn init_twice_is_harmless() {
    tf2tools_app::diagnostics::init();
    tf2tools_app::diagnostics::init();
}

#[test]
fn warnings_reach_the_crash_buffer() {
    tf2tools_app::diagnostics::init();
    tracing::warn!("m5p global probe");
    assert!(
        tf2tools_app::crash::recent::global()
            .snapshot()
            .iter()
            .any(|l| l.ends_with("m5p global probe")),
        "{:?}",
        tf2tools_app::crash::recent::global().snapshot()
    );
}

// Runs in a child process so RUST_LOG is set before init and never leaks into the other tests.
#[test]
#[ignore = "run by rust_log_error_still_feeds_the_crash_buffer in a child process"]
fn child_warns_under_rust_log_error() {
    assert_eq!(std::env::var("RUST_LOG").as_deref(), Ok("error"));
    tf2tools_app::diagnostics::init();
    tracing::warn!("m5p child probe");
    let lines = tf2tools_app::crash::recent::global().snapshot();
    assert!(
        lines.iter().any(|l| l.ends_with("m5p child probe")),
        "{lines:?}"
    );
}

#[test]
fn rust_log_error_still_feeds_the_crash_buffer() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "child_warns_under_rust_log_error"])
        .env("RUST_LOG", "error")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("1 passed"),
        "the child ran no test: {stdout}"
    );
}
