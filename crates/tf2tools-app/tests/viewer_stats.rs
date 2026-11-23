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

#[test]
fn the_platform_frame_lines_reach_the_viewer_target() {
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt as _;

    // Records each event's target and message.
    struct Seen(Arc<Mutex<Vec<(String, String)>>>);
    struct Message(String);
    impl tracing::field::Visit for Message {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0 = format!("{value:?}");
            }
        }
    }
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Seen {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut message = Message(String::new());
            event.record(&mut message);
            let target = event.metadata().target().to_owned();
            self.0.lock().unwrap().push((target, message.0));
        }
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Seen(Arc::clone(&seen)));
    let bridge = tf2tools_app::diagnostics::PlatformStatsBridge;
    let line = |target: &'static str| {
        log::Record::builder()
            .target(target)
            .level(log::Level::Info)
            .args(format_args!("wayland-frame: probe"))
            .build()
    };
    tracing::subscriber::with_default(subscriber, || {
        log::Log::log(
            &bridge,
            &line(tf2tools_app::diagnostics::PLATFORM_STATS_TARGET),
        );
        log::Log::log(&bridge, &line("gpui_linux::other"));
    });
    assert_eq!(
        tf2tools_app::diagnostics::PLATFORM_STATS_TARGET,
        "tf2tools_viewer_stats"
    );
    assert_eq!(
        *seen.lock().unwrap(),
        [(stats::TARGET.to_owned(), "wayland-frame: probe".to_owned())]
    );
}
