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
}
