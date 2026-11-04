//! The Sound Store's pure view model: category pills, the card badge and counts, and the
//! sync banner's text, count and percentage.

use tf2tools_core::StoreKind;

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
    pub fn label(self) -> &'static str {
        match self {
            Category::All => "All",
            Category::Hitsounds => "Hitsounds",
            Category::Killsounds => "Killsounds",
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

/// v5's `categoryLabel`: "Hitsound", "Killsound", else "Sound".
pub fn category_label(category: Option<&str>) -> &'static str {
    match category {
        Some("hitsound") => "Hitsound",
        Some("killsound") => "Killsound",
        _ => "Sound",
    }
}

/// "Syncing sounds from GameBanana..." plus " Not all items loaded yet." when the store has rows.
pub fn banner_text(has_rows: bool) -> String {
    sync_banner::banner_text(StoreKind::Sounds, has_rows)
}
