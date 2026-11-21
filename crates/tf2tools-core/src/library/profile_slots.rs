//! A profile's five slots (HUD, Sounds, Crosshairs, Graphics, Add-ons): each leaves the
//! game alone, takes its content out, or sets it. Stored as `leave`, `none` or `set` in the
//! slot's `<slot>_mode` column; any other text reads as Leave, so the slot is never touched.

use super::profiles::ProfileItem;
use super::{AddonName, Choices, Provenance, display_info};

/// What one profile slot does when the profile is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SlotMode<T> {
    /// Leave whatever the game has in this slot.
    Leave,
    /// Take this slot's content out of the game.
    None,
    /// Put this content in the game.
    Set(T),
}

impl<T> SlotMode<T> {
    /// The content when the slot is Set.
    pub fn as_set(&self) -> Option<&T> {
        match self {
            SlotMode::Set(content) => Some(content),
            SlotMode::Leave | SlotMode::None => Option::None,
        }
    }

    /// The same mode with its content passed through `f`.
    pub(super) fn map_ref<'a, U>(&'a self, f: impl FnOnce(&'a T) -> U) -> SlotMode<U> {
        match self {
            SlotMode::Leave => SlotMode::Leave,
            SlotMode::None => SlotMode::None,
            SlotMode::Set(content) => SlotMode::Set(f(content)),
        }
    }

    /// The database text for this mode.
    pub(super) fn mode_text(&self) -> &'static str {
        match self {
            SlotMode::Leave => "leave",
            SlotMode::None => "none",
            SlotMode::Set(_) => "set",
        }
    }

    /// Reads a stored mode; a Set whose content `set` cannot rebuild reads as None.
    pub(super) fn from_text(text: &str, set: impl FnOnce() -> Option<T>) -> Self {
        match text {
            "set" => set().map_or(SlotMode::None, SlotMode::Set),
            "none" => SlotMode::None,
            _ => SlotMode::Leave,
        }
    }
}

/// One item of a draft: a library item to look up, or a saved reference written back as is.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DraftItem<T> {
    /// The library item with this id.
    Pick(T),
    /// A reference as the profile records it, even when the library no longer has it.
    Keep(ProfileItem),
}

impl DraftItem<AddonName> {
    /// Keeps an add-on by its asset name, with its display name and no store provenance.
    pub fn keep_addon(name: AddonName) -> Self {
        let (display, _) = display_info(&name);
        DraftItem::Keep(ProfileItem {
            folder_name: name.as_str().to_owned(),
            name: display,
            provenance: Provenance::Local,
        })
    }
}

/// The Sounds slot when Set: each side is a sound, or `None` for TF2's default.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileSounds {
    /// The hitsound.
    pub hit: Option<ProfileItem>,
    /// The killsound.
    pub kill: Option<ProfileItem>,
}

/// The Graphics slot when Set: a mastercomfig preset and module choices.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileGraphics {
    /// The preset and the modules set away from it.
    pub choices: Choices,
}

/// The Add-ons slot when Set: the add-ons turned on and whether every other one goes off.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileAddons {
    /// The add-ons to turn on, sorted by asset name.
    pub on: Vec<AddonName>,
    /// When true, every other library add-on is turned off.
    pub only_these: bool,
}
