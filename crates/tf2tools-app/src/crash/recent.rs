//! The newest warning and error lines, kept in memory so a crash report can show what led up
//! to the panic.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use chrono::SecondsFormat;
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer as _;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Context;

pub const RECENT_LINES: usize = 40;
pub const LINE_BYTES: usize = 300;

/// A shared ring of the newest WARN and ERROR lines, oldest first.
#[derive(Clone, Default)]
pub struct Recent {
    lines: Arc<Mutex<VecDeque<String>>>,
}

impl Recent {
    pub fn new() -> Self {
        Self::default()
    }

    /// A layer that appends "{RFC3339 secs Z} {LEVEL} {target}: {message}[ key=value]..." for
    /// WARN and ERROR events, each line cut to LINE_BYTES on a char boundary.
    /// Owns a clone of the buffer: `+ use<S>` keeps `&self` out of the type (edition 2024
    /// would capture it, and `with_default` needs a `'static` subscriber).
    pub fn layer<S>(&self) -> impl tracing_subscriber::Layer<S> + use<S>
    where
        S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    {
        // A per-layer filter: a bare layer rejecting INFO would silence every other layer too.
        Ring(Arc::clone(&self.lines)).with_filter(LevelFilter::WARN)
    }

    /// The lines, oldest first; empty when the lock is busy or poisoned, so a panic inside the
    /// layer never deadlocks the hook that reads it.
    pub fn snapshot(&self) -> Vec<String> {
        match self.lines.try_lock() {
            Ok(lines) => lines.iter().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }
}

/// The process's buffer, the one `diagnostics::init` feeds and the panic hook reads.
pub fn global() -> &'static Recent {
    static GLOBAL: OnceLock<Recent> = OnceLock::new();
    GLOBAL.get_or_init(Recent::new)
}

struct Ring(Arc<Mutex<VecDeque<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Ring {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        let mut fields = Fields::default();
        event.record(&mut fields);
        let mut line = format!(
            "{} {} {}: {}{}",
            chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            meta.level(),
            meta.target(),
            fields.message,
            fields.rest,
        );
        // Shrunk after the cut, so the ring holds 40 short lines, not 40 huge buffers.
        line.truncate(line.floor_char_boundary(LINE_BYTES));
        line.shrink_to_fit();
        let mut lines = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        lines.push_back(line);
        while lines.len() > RECENT_LINES {
            lines.pop_front();
        }
    }
}

/// The event's `message` bare and every other field as " key=value".
#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            let _ = write!(self.rest, " {}={value}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.rest, " {}={value:?}", field.name());
        }
    }
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::layer::SubscriberExt as _;

    use super::*;

    #[test]
    fn a_huge_warning_keeps_only_a_short_line_in_memory() {
        let recent = Recent::new();
        let subscriber = tracing_subscriber::registry().with(recent.layer());
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!("{}", "x".repeat(100_000));
        });
        let lines = recent.lines.lock().unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].capacity() <= LINE_BYTES, "{}", lines[0].capacity());
    }
}
