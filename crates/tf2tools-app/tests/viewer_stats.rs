//! The viewer's stats switch, its percentiles and the filter that lets its lines through.

use std::time::Duration;

use tf2tools_app::diagnostics::log_filter_with;
use tf2tools_app::viewer::stats::{self, Percentiles};
use tracing_subscriber::filter::LevelFilter;

#[test]
fn only_1_switches_the_stats_on() {
    assert!(stats::is_on(Some("1")));
    for off in [None, Some("0"), Some(""), Some("yes")] {
        assert!(!stats::is_on(off), "{off:?}");
    }
}

#[test]
fn percentiles_use_the_nearest_rank() {
    let mut samples: Vec<Duration> = (1..=100).rev().map(Duration::from_millis).collect();
    assert_eq!(
        stats::percentiles(&mut samples),
        Some(Percentiles {
            p50: Duration::from_millis(50),
            p99: Duration::from_millis(99),
            max: Duration::from_millis(100),
        })
    );
    let mut one = [Duration::from_millis(7)];
    let seven = Duration::from_millis(7);
    assert_eq!(
        stats::percentiles(&mut one),
        Some(Percentiles {
            p50: seven,
            p99: seven,
            max: seven
        })
    );
    assert_eq!(stats::percentiles(&mut []), None);
}

#[test]
fn the_stats_switch_lets_the_viewer_lines_through() {
    assert_eq!(
        log_filter_with(None, false).max_level_hint(),
        Some(LevelFilter::WARN)
    );
    assert_eq!(
        log_filter_with(None, true).max_level_hint(),
        Some(LevelFilter::INFO)
    );
    assert_eq!(
        log_filter_with(Some("error"), true).max_level_hint(),
        Some(LevelFilter::INFO)
    );
    assert_eq!(stats::TARGET, "tf2tools::viewer");
}
