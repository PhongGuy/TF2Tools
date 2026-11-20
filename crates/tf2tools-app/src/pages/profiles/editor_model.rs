//! The profile editor's pure rules: its slots, the draft each click saves and the name a new
//! profile starts with.

use gpui_kit::SharedString;
use tf2tools_core::{
    BuiltinCrosshairs, CrosshairEntry, CrosshairRef, DraftCrosshair, DraftItem, HudId, MissingItem,
    MissingKind, Profile, ProfileCrosshair, ProfileCrosshairRef, ProfileDraft, SlotMode, SoundId,
    SoundKind,
};

use super::model::{crosshair_list, hit, hud_item, kill, sounds_with};
use crate::i18n::{names, tr};

/// One of the editor's four slots, in page order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    Hud,
    Hitsound,
    Killsound,
    Crosshairs,
}

impl Slot {
    pub const ALL: [Slot; 4] = [Self::Hud, Self::Hitsound, Self::Killsound, Self::Crosshairs];

    /// "hud", "hitsound", "killsound", "crosshairs": the id key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Hud => "hud",
            Self::Hitsound => "hitsound",
            Self::Killsound => "killsound",
            Self::Crosshairs => "crosshairs",
        }
    }

    /// "HUD", "Hitsound", "Killsound", "Crosshairs".
    pub fn label(self) -> SharedString {
        match self {
            Self::Hud => "HUD".into(),
            Self::Hitsound => names::sound_kind_name(SoundKind::Hitsound),
            Self::Killsound => names::sound_kind_name(SoundKind::Killsound),
            Self::Crosshairs => tr!("profiles.part.crosshairs"),
        }
    }
}

/// One click's change to a saved profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    SetHud(HudId),
    SetSound(SoundKind, SoundId),
    ClearSlot(Slot),
    Rename(String),
    /// One weapon or a bulk target.
    Assign {
        scripts: Vec<String>,
        crosshair: CrosshairRef,
    },
    ClearWeapon(String),
    ClearCrosshairs,
}

impl Edit {
    /// `ProfileDraft::from_profile(saved)` (every slot and crosshair Keep) plus this one change.
    /// Assign replaces each target's saved entry in place with a Set and appends targets the
    /// profile lacks, in `scripts` order. ClearWeapon drops that entry. ClearSlot(Crosshairs)
    /// and ClearCrosshairs make the slot None. A sound edit keeps the other sound, or TF2's
    /// default when Sounds was not Set. Rename sets the name as typed (core trims it).
    pub fn draft(&self, saved: &Profile) -> ProfileDraft {
        let keep = ProfileDraft::from_profile(saved);
        match self {
            Self::SetHud(id) => keep.with_hud(SlotMode::Set(DraftItem::Pick(id.clone()))),
            Self::SetSound(kind, id) => {
                keep.with_sounds(sounds_with(saved, *kind, Some(DraftItem::Pick(id.clone()))))
            }
            Self::ClearSlot(Slot::Hud) => keep.with_hud(SlotMode::None),
            Self::ClearSlot(Slot::Hitsound) => {
                keep.with_sounds(sounds_with(saved, SoundKind::Hitsound, None))
            }
            Self::ClearSlot(Slot::Killsound) => {
                keep.with_sounds(sounds_with(saved, SoundKind::Killsound, None))
            }
            Self::ClearSlot(Slot::Crosshairs) | Self::ClearCrosshairs => {
                keep.with_crosshairs(SlotMode::None)
            }
            Self::Rename(name) => keep.with_name(name.as_str()),
            Self::Assign { scripts, crosshair } => {
                // Saved entries keep their place, a target's entry turning into the new Set;
                // targets the profile has no entry for follow in the order given.
                let set = |script: &str| DraftCrosshair::Set {
                    weapon_script: script.to_owned(),
                    crosshair: crosshair.clone(),
                };
                let saved_list = crosshair_list(saved);
                let mut list: Vec<DraftCrosshair> = saved_list
                    .iter()
                    .map(|c| {
                        if scripts.iter().any(|s| s == c.weapon_script()) {
                            set(c.weapon_script())
                        } else {
                            DraftCrosshair::Keep(c.clone())
                        }
                    })
                    .collect();
                let lacks = |s: &&String| saved_list.iter().all(|c| c.weapon_script() != *s);
                list.extend(scripts.iter().filter(lacks).map(|s| set(s)));
                keep.with_crosshairs(SlotMode::Set(list))
            }
            Self::ClearWeapon(script) => keep.with_crosshairs(SlotMode::Set(
                crosshair_list(saved)
                    .iter()
                    .filter(|c| c.weapon_script() != script)
                    .cloned()
                    .map(DraftCrosshair::Keep)
                    .collect(),
            )),
        }
    }
}

/// "Profile N" in the player's language for the lowest N >= 1 that no profile's name equals,
/// ignoring case.
pub fn next_profile_name(profiles: &[Profile]) -> String {
    let taken: Vec<String> = profiles.iter().map(|p| p.name().to_lowercase()).collect();
    (1..)
        .map(|n: u64| tr!("profiles.default_name", n = n).to_string())
        .find(|name| !taken.contains(&name.to_lowercase()))
        .unwrap_or_default()
}

/// What a slot shows: nothing, its item, or an item the library no longer has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotState {
    Empty,
    Set(String),
    Missing(String),
}

/// HUD and sounds: the item's name, Missing when `missing` holds that slot's kind, else Empty.
/// Crosshairs: Set(weapon_count(n)) or Empty; a missing crosshair shows on its weapon row.
pub fn slot_state(profile: &Profile, slot: Slot, missing: &[MissingItem]) -> SlotState {
    let (item, kind) = match slot {
        Slot::Hud => (hud_item(profile), MissingKind::Hud),
        Slot::Hitsound => (hit(profile), MissingKind::Hitsound),
        Slot::Killsound => (kill(profile), MissingKind::Killsound),
        Slot::Crosshairs => {
            let n = crosshair_list(profile).len();
            return if n == 0 {
                SlotState::Empty
            } else {
                SlotState::Set(weapon_count(n))
            };
        }
    };
    match item {
        None => SlotState::Empty,
        Some(item) if missing.iter().any(|m| *m.kind() == kind) => {
            SlotState::Missing(item.name().to_owned())
        }
        Some(item) => SlotState::Set(item.name().to_owned()),
    }
}

/// Empty: "None" ("Default crosshairs" for Crosshairs); Set: the text; Missing: "Missing: <name>".
pub fn slot_text(state: &SlotState, slot: Slot) -> String {
    match state {
        SlotState::Empty if slot == Slot::Crosshairs => {
            tr!("profiles.editor.slot.default_crosshairs").into()
        }
        SlotState::Empty => tr!("profiles.editor.slot.none").into(),
        SlotState::Set(text) => text.clone(),
        SlotState::Missing(name) => tr!("profiles.editor.slot.missing", name = name).into(),
    }
}

/// "1 weapon", "N weapons".
pub fn weapon_count(n: usize) -> String {
    crate::i18n::plural("profiles.weapons", u64::try_from(n).unwrap_or(u64::MAX)).to_string()
}

/// A kept crosshair's label: the builtin's display name, the library crosshair's name by
/// folder, or "{name or folder} (missing)".
pub fn keep_label(
    saved: &ProfileCrosshair,
    builtins: &BuiltinCrosshairs,
    library: &[CrosshairEntry],
) -> String {
    match saved.crosshair() {
        ProfileCrosshairRef::Builtin(name) => builtins
            .get(name)
            .map(|b| b.display_name().to_owned())
            .unwrap_or_else(|| missing(name)),
        ProfileCrosshairRef::Library { folder_name } => library
            .iter()
            .find(|c| c.folder_name() == folder_name)
            .map(|c| c.name().to_owned())
            .unwrap_or_else(|| missing(folder_name)),
        _ => missing(&tr!("profiles.editor.unknown")),
    }
}

// "<name> (missing)" for a kept crosshair the game or library no longer has.
fn missing(name: &str) -> String {
    tr!("profiles.editor.missing_crosshair", name = name).into()
}

/// (weapon script, keep_label) for each saved crosshair, in the profile's order.
pub fn crosshair_labels(
    profile: &Profile,
    builtins: &BuiltinCrosshairs,
    library: &[CrosshairEntry],
) -> Vec<(String, String)> {
    crosshair_list(profile)
        .iter()
        .map(|c| {
            (
                c.weapon_script().to_owned(),
                keep_label(c, builtins, library),
            )
        })
        .collect()
}
