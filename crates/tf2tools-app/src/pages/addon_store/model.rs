//! The Addon Store's pure model: each addon's button state, its labels and the card order.

use tf2tools_core::{AddonEntry, StoreItem};

/// What an addon card offers: a Download or Update button, or an In Library or Installed badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddonState {
    Download,
    InLibrary,
    Installed,
    Update,
}

impl AddonState {
    /// The text v5 shows on the button or badge.
    pub fn label(self) -> &'static str {
        match self {
            Self::Download => "Download",
            Self::InLibrary => "In Library",
            Self::Installed => "Installed",
            Self::Update => "Update",
        }
    }

    /// True for the states drawn as buttons (Download and Update); the others are badges.
    pub fn is_action(self) -> bool {
        matches!(self, Self::Download | Self::Update)
    }
}

/// v5's order: Update (installed, store tag differs), Installed, In Library, else Download.
pub fn addon_state(entry: Option<&AddonEntry>) -> AddonState {
    match entry {
        Some(e) if e.is_installed() && e.update_available() => AddonState::Update,
        Some(e) if e.is_installed() => AddonState::Installed,
        Some(_) => AddonState::InLibrary,
        None => AddonState::Download,
    }
}

/// v5's "{n.toLocaleString()} downloads": "40,838 downloads".
pub fn downloads_label(count: u64) -> String {
    // Group the digits in threes from the right, as the en-US locale does.
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{grouped} downloads")
}

/// The store's addons by display name, ignoring case.
pub fn ordered(mut items: Vec<StoreItem>) -> Vec<StoreItem> {
    items.sort_by_cached_key(|item| item.name().to_lowercase());
    items
}

/// The header badge: the first item's release tag.
pub fn store_release(items: &[StoreItem]) -> Option<&str> {
    items.first().and_then(StoreItem::release_tag)
}
