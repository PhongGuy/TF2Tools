//! The Sound Store's pure view model: category pills, the card badge and counts, and the
//! sync banner's text, count and percentage.

use gpui_kit::SharedString;
use tf2tools_core::{SoundKind, StoreKind};

use crate::i18n::{names, tr};
use crate::widgets::sync_banner;
pub use crate::widgets::sync_banner::{banner_count, banner_percent};

/// One of v5's category pills above the sound grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    All,
    Hitsounds,
    Killsounds,
}

impl Category {
    /// The pills in v5's order.
    pub const ALL: [Category; 3] = [Category::All, Category::Hitsounds, Category::Killsounds];

    /// The pill's text.
    pub fn label(self) -> SharedString {
        match self {
            Category::All => tr!("sound_store.category.all"),
            Category::Hitsounds => tr!("sound_store.category.hitsounds"),
            Category::Killsounds => tr!("sound_store.category.killsounds"),
        }
    }

    /// The pill's id key.
    pub fn slug(self) -> &'static str {
        self.filter().unwrap_or("all")
    }

    /// The store category the grid filters on; `None` shows every sound.
    pub fn filter(self) -> Option<&'static str> {
        match self {
            Category::All => None,
            Category::Hitsounds => Some("hitsound"),
            Category::Killsounds => Some("killsound"),
        }
    }
}

/// v5's `categoryLabel`: "Hitsound", "Killsound", else "Sound", in the player's language.
pub fn category_label(category: Option<&str>) -> SharedString {
    match category {
        Some("hitsound") => names::sound_kind_name(SoundKind::Hitsound),
        Some("killsound") => names::sound_kind_name(SoundKind::Killsound),
        _ => tr!("sound_store.category.sound"),
    }
}

/// "Syncing sounds from GameBanana..." plus " Not all items loaded yet." when the store has rows.
pub fn banner_text(has_rows: bool) -> String {
    sync_banner::banner_text(StoreKind::Sounds, has_rows)
}
