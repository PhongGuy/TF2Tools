//! The profile editor's pure rules: its slots, the draft each click saves and the name a new
//! profile starts with.

use tf2tools_core::{
    BuiltinCrosshairs, CrosshairEntry, CrosshairRef, DraftCrosshair, DraftSlot, HudId, MissingItem,
    MissingKind, Profile, ProfileCrosshair, ProfileCrosshairRef, ProfileDraft, SoundId, SoundKind,
};

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
    pub fn label(self) -> &'static str {
        match self {
            Self::Hud => "HUD",
            Self::Hitsound => "Hitsound",
            Self::Killsound => "Killsound",
            Self::Crosshairs => "Crosshairs",
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
    /// and ClearCrosshairs empty the list. Rename sets the name as typed (core trims it).
    pub fn draft(&self, saved: &Profile) -> ProfileDraft {
        let keep = ProfileDraft::from_profile(saved);
        match self {
            Self::SetHud(id) => keep.with_hud(DraftSlot::Item(id.clone())),
            Self::SetSound(SoundKind::Hitsound, id) => {
                keep.with_hitsound(DraftSlot::Item(id.clone()))
            }
            Self::SetSound(_, id) => keep.with_killsound(DraftSlot::Item(id.clone())),
            Self::ClearSlot(Slot::Hud) => keep.with_hud(DraftSlot::Empty),
            Self::ClearSlot(Slot::Hitsound) => keep.with_hitsound(DraftSlot::Empty),
            Self::ClearSlot(Slot::Killsound) => keep.with_killsound(DraftSlot::Empty),
            Self::ClearSlot(Slot::Crosshairs) | Self::ClearCrosshairs => {
                keep.with_crosshairs(Vec::new())
            }
            Self::Rename(name) => keep.with_name(name.as_str()),
            Self::Assign { scripts, crosshair } => {
                // Saved entries keep their place, a target's entry turning into the new Set;
                // targets the profile has no entry for follow in the order given.
                let set = |script: &str| DraftCrosshair::Set {
                    weapon_script: script.to_owned(),
                    crosshair: crosshair.clone(),
                };
                let saved_list = saved.crosshairs();
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
                keep.with_crosshairs(list)
            }
            Self::ClearWeapon(script) => keep.with_crosshairs(
                saved
                    .crosshairs()
                    .iter()
                    .filter(|c| c.weapon_script() != script)
                    .cloned()
                    .map(DraftCrosshair::Keep)
                    .collect(),
            ),
        }
    }
}

/// "Profile N" for the lowest N >= 1 that no profile's name equals, ignoring case.
pub fn next_profile_name(profiles: &[Profile]) -> String {
    let taken: Vec<String> = profiles.iter().map(|p| p.name().to_lowercase()).collect();
    let mut n = 1;
    while taken.contains(&format!("profile {n}")) {
        n += 1;
    }
    format!("Profile {n}")
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
        Slot::Hud => (profile.hud(), MissingKind::Hud),
        Slot::Hitsound => (profile.hitsound(), MissingKind::Hitsound),
        Slot::Killsound => (profile.killsound(), MissingKind::Killsound),
        Slot::Crosshairs => {
            let n = profile.crosshairs().len();
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
        SlotState::Empty if slot == Slot::Crosshairs => "Default crosshairs".to_owned(),
        SlotState::Empty => "None".to_owned(),
        SlotState::Set(text) => text.clone(),
        SlotState::Missing(name) => format!("Missing: {name}"),
    }
}

/// "1 weapon", "N weapons".
pub fn weapon_count(n: usize) -> String {
    if n == 1 {
        "1 weapon".to_owned()
    } else {
        format!("{n} weapons")
    }
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
            .unwrap_or_else(|| format!("{name} (missing)")),
        ProfileCrosshairRef::Library { folder_name } => library
            .iter()
            .find(|c| c.folder_name() == folder_name)
            .map(|c| c.name().to_owned())
            .unwrap_or_else(|| format!("{folder_name} (missing)")),
        _ => "Unknown (missing)".to_owned(),
    }
}

/// (weapon script, keep_label) for each saved crosshair, in the profile's order.
pub fn crosshair_labels(
    profile: &Profile,
    builtins: &BuiltinCrosshairs,
    library: &[CrosshairEntry],
) -> Vec<(String, String)> {
    profile
        .crosshairs()
        .iter()
        .map(|c| {
            (
                c.weapon_script().to_owned(),
                keep_label(c, builtins, library),
            )
        })
        .collect()
}
