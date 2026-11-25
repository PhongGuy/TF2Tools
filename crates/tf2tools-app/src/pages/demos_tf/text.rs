//! The Demos.tf list's wording, kept free of UI so tests read it without a window.

use gpui_kit::SharedString;
use tf2tools_core::{DemoFormat, DemosError, DemosRange};

use crate::i18n::{format, tr};

/// A demo's row title: its server and both teams, or the server alone when a team is unnamed.
pub fn demo_title(server: &str, red: &str, blue: &str) -> String {
    if red.is_empty() || blue.is_empty() {
        server.to_owned()
    } else {
        format!("{server} - {red} vs {blue}")
    }
}

/// The name a demo's format shows under.
pub fn format_label(format: DemoFormat) -> SharedString {
    match format {
        DemoFormat::Highlander => tr!("demos_tf.format.hl"),
        DemoFormat::Prolander => tr!("demos_tf.format.prolander"),
        DemoFormat::Sixes => tr!("demos_tf.format.sixes"),
        DemoFormat::Fours => tr!("demos_tf.format.fours"),
        DemoFormat::Threes => tr!("demos_tf.format.threes"),
        DemoFormat::Twos => tr!("demos_tf.format.twos"),
        DemoFormat::Ones => tr!("demos_tf.format.ones"),
        DemoFormat::Ultiduo => tr!("demos_tf.format.ultiduo"),
        DemoFormat::BBall => tr!("demos_tf.format.bball"),
        // Every known variant is named for the lint; a future one reads as Other.
        DemoFormat::Other | _ => tr!("demos_tf.format.misc"),
    }
}

/// A range stop's label under the slider: one fixed text per stop.
pub fn range_label(range: DemosRange) -> SharedString {
    match range {
        DemosRange::Day1 => tr!("demos_tf.range.day"),
        DemosRange::Week1 => tr!("demos_tf.range.week"),
        DemosRange::Month1 => tr!("demos_tf.range.month"),
        DemosRange::Months3 => tr!("demos_tf.range.months3"),
        DemosRange::Year1 => tr!("demos_tf.range.year"),
        DemosRange::All | _ => tr!("demos_tf.range.all"),
    }
}

/// Why a page of the list failed to load, worded as what the player can do about it.
pub fn list_error(error: &DemosError) -> SharedString {
    match error {
        DemosError::Network { .. } => tr!("logs_tf.download.offline"),
        DemosError::RateLimited { .. } => tr!("logs_tf.demo.busy"),
        // Every other failure is demos.tf's, so it shares the down text; the wildcard too.
        DemosError::Http { .. }
        | DemosError::Gone
        | DemosError::Cursor
        | DemosError::TooLarge { .. }
        | DemosError::Unreadable
        | DemosError::ForeignUrl { .. }
        | DemosError::Write { .. }
        | _ => tr!("logs_tf.demo.down"),
    }
}

/// A row's Watch button: Private for a demo without a file, the download's progress while
/// one runs (a whole percent once its size is known), Watch otherwise.
pub fn watch_label(public: bool, downloading: Option<Option<u8>>) -> SharedString {
    match (public, downloading) {
        (false, _) => tr!("demos_tf.private"),
        (true, None) => tr!("demos_tf.watch"),
        (true, Some(None)) => tr!("demos_tf.downloading"),
        (true, Some(Some(done))) => format::percent(u64::from(done)),
    }
}
