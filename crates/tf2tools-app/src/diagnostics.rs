//! The stderr diagnostics subscriber: developer-facing `tracing` output, separate from the
//! user-facing activity log.

use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

use crate::crash::recent;

/// The stderr filter: `rust_log` when it is set, non-blank and parses, else `warn`.
pub fn log_filter(rust_log: Option<&str>) -> EnvFilter {
    rust_log
        .filter(|spec| !spec.trim().is_empty())
        .and_then(|spec| EnvFilter::try_new(spec).ok())
        .unwrap_or_else(|| EnvFilter::new("warn"))
}

/// `log_filter(rust_log)`, plus the viewer's frame lines at info when `viewer_stats` is on.
pub fn log_filter_with(rust_log: Option<&str>, viewer_stats: bool) -> EnvFilter {
    let filter = log_filter(rust_log);
    if !viewer_stats {
        return filter;
    }
    match format!("{}=info", crate::viewer::stats::TARGET).parse() {
        Ok(directive) => filter.add_directive(directive),
        Err(_) => filter,
    }
}

/// Installs a fmt layer to stderr filtered by `log_filter_with(RUST_LOG, ..)` beside the crash report's
/// recent-lines layer; a second call is a no-op.
pub fn init() {
    // Per-layer filters, so `RUST_LOG=error` on stderr still leaves warnings for a crash report.
    let filter = log_filter_with(
        std::env::var("RUST_LOG").ok().as_deref(),
        crate::viewer::stats::enabled(),
    );
    let stderr = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(filter);
    // A subscriber already installed (a second call, or a test harness) wins; that is fine.
    let _ = tracing_subscriber::registry()
        .with(stderr)
        .with(recent::global().layer())
        .try_init();
    if crate::viewer::stats::enabled() {
        install_platform_stats_bridge();
    }
}

/// The `log` target of the vendored platform crate's frame lines.
pub const PLATFORM_STATS_TARGET: &str = "tf2tools_viewer_stats";

/// A `log` sink that re-emits the platform crate's frame lines as `tracing` events on the viewer's
/// stats target; every other `log` record is dropped, as it was before this bridge.
pub struct PlatformStatsBridge;

impl log::Log for PlatformStatsBridge {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == PLATFORM_STATS_TARGET
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            tracing::info!(target: crate::viewer::stats::TARGET, "{}", record.args());
        }
    }

    fn flush(&self) {}
}

// `log` stays off unless the stats are, so the platform crate's lines cost nothing by default.
fn install_platform_stats_bridge() {
    static BRIDGE: PlatformStatsBridge = PlatformStatsBridge;
    if log::set_logger(&BRIDGE).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}
