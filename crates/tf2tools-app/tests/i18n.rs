//! Tests that switch the process-wide locale. They live apart from tests/main.rs, whose English
//! assertions would race a switch, and each one holds a `LocaleGuard` for its whole body.

// Tests and their helpers may unwrap and panic; the workspace denies it in shipped code.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::wildcard_enum_match_arm)]

use std::sync::MutexGuard;

#[path = "support/mod.rs"]
mod support;

#[path = "i18n/apply.rs"]
mod apply;

#[path = "i18n/formats.rs"]
mod formats;

#[path = "i18n/library.rs"]
mod library;

#[path = "i18n/messages.rs"]
mod messages;

#[path = "i18n/settings.rs"]
mod settings;

#[path = "i18n/stores.rs"]
mod stores;

#[path = "i18n/updater.rs"]
mod updater;

/// The locale lock for one test; English is back when it drops, panic included.
pub struct LocaleGuard {
    _lock: MutexGuard<'static, ()>,
}

impl LocaleGuard {
    /// Takes the lock and leaves the locale as it is (English).
    pub fn hold() -> Self {
        Self {
            _lock: tf2tools_app::i18n::test_lock(),
        }
    }

    /// Takes the lock and switches to `code`.
    pub fn switch(code: &str) -> Self {
        let guard = Self::hold();
        tf2tools_app::i18n::apply(code);
        guard
    }
}

impl Drop for LocaleGuard {
    // Runs before the lock field drops, so the next test always starts in English.
    fn drop(&mut self) {
        tf2tools_app::i18n::apply("en");
    }
}
