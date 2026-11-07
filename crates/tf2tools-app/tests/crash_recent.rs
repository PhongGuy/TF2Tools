use tf2tools_app::crash::recent::{LINE_BYTES, RECENT_LINES, Recent};
use tracing_subscriber::layer::SubscriberExt as _;

fn capture(recent: &Recent, f: impl FnOnce()) {
    let subscriber = tracing_subscriber::registry().with(recent.layer());
    tracing::subscriber::with_default(subscriber, f);
}

#[test]
fn only_warnings_and_errors_are_kept_oldest_first() {
    let recent = Recent::new();
    capture(&recent, || {
        tracing::info!("not kept");
        tracing::warn!(path = "huds/x", "disk is full");
        tracing::error!("store sync failed");
    });
    let lines = recent.snapshot();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].ends_with(" WARN integration::crash_recent: disk is full path=huds/x"),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].ends_with(" ERROR integration::crash_recent: store sync failed"),
        "{}",
        lines[1]
    );
    assert!(
        lines[0].starts_with("20") && lines[0].contains('T'),
        "{}",
        lines[0]
    );
}

#[test]
fn the_buffer_keeps_only_the_newest_lines_each_cut_short() {
    let recent = Recent::new();
    capture(&recent, || {
        for i in 0..RECENT_LINES + 5 {
            tracing::warn!("line {i}");
        }
        tracing::warn!("{}", "é".repeat(LINE_BYTES));
    });
    let lines = recent.snapshot();
    assert_eq!(lines.len(), RECENT_LINES);
    assert!(lines[0].ends_with(": line 6"), "{}", lines[0]);
    assert!(lines.last().unwrap().len() <= LINE_BYTES);
}

#[test]
fn the_buffer_never_hides_info_from_other_layers() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Count(Arc<AtomicUsize>);
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Count {
        fn on_event(&self, _: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let seen = Arc::new(AtomicUsize::new(0));
    let recent = Recent::new();
    let subscriber = tracing_subscriber::registry()
        .with(recent.layer())
        .with(Count(seen.clone()));
    tracing::subscriber::with_default(subscriber, || tracing::info!("for the other layer"));
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    assert!(recent.snapshot().is_empty());
}
