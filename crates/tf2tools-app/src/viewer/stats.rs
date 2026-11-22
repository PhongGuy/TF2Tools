//! The viewer's opt-in frame timing: the switch, its log target and percentile summaries.

use std::sync::OnceLock;
use std::time::Duration;

/// The `tracing` target of the per-frame lines.
pub const TARGET: &str = "tf2tools::viewer";

/// Whether an environment value turns the stats on: only `1` does.
pub fn is_on(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Whether `TF2TOOLS_VIEWER_STATS` is `1`, read once for the process.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| is_on(std::env::var("TF2TOOLS_VIEWER_STATS").ok().as_deref()))
}

/// The median, 99th percentile and maximum of a set of timings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Percentiles {
    pub p50: Duration,
    pub p99: Duration,
    pub max: Duration,
}

/// Sorts `samples` and returns their nearest-rank percentiles; `None` when there are none.
pub fn percentiles(samples: &mut [Duration]) -> Option<Percentiles> {
    samples.sort_unstable();
    let last = samples.last().copied()?;
    // Nearest rank: the sample at ceil(p * n), counted from one.
    let rank = |percent: usize| samples[(samples.len() * percent).div_ceil(100) - 1];
    Some(Percentiles {
        p50: rank(50),
        p99: rank(99),
        max: last,
    })
}
