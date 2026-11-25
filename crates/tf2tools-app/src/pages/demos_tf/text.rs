//! The Demos.tf list's wording, kept free of UI so tests read it without a window.

use gpui_kit::SharedString;
use tf2tools_core::{DemoFormat, DemosError};

use crate::i18n::tr;

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
