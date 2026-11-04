//! The profile wizard's pure rules: its steps, v5's texts, the HUD and sound options with
//! their preselected slot, and the activity row a save writes.

use std::collections::BTreeMap;

use gpui_kit::SharedString;
use tf2tools_core::{
    Assignment, BuiltinCrosshairs, CrosshairEntry, CrosshairRef, DraftCrosshair, DraftSlot,
    HudEntry, HudId, LogLevel, NewLogEntry, Profile, ProfileCrosshair, ProfileCrosshairRef,
    ProfileItem, SoundEntry, SoundId,
};

/// One of the wizard's four steps, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WizardStep {
    Name,
    Hud,
    Sounds,
    Crosshairs,
}

impl WizardStep {
    pub const ALL: [WizardStep; 4] = [Self::Name, Self::Hud, Self::Sounds, Self::Crosshairs];

    /// The step's 1-based position, as its stepper circle shows it.
    pub fn number(self) -> usize {
        self as usize + 1
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Hud => "HUD",
            Self::Sounds => "Sounds",
            Self::Crosshairs => "Crosshairs",
        }
    }

    /// The dialog description while this step is shown.
    pub fn description(self) -> &'static str {
        match self {
            Self::Name => "Choose a name for your profile.",
            Self::Hud => "Select a HUD from your library (optional).",
            Self::Sounds => "Select hitsound and killsound (optional).",
            Self::Crosshairs => "Configure crosshair assignments (optional).",
        }
    }

    pub fn next(self) -> Option<WizardStep> {
        Self::ALL.get(self.number()).copied()
    }

    pub fn back(self) -> Option<WizardStep> {
        (self as usize).checked_sub(1).map(|i| Self::ALL[i])
    }
}

/// How the profile's crosshairs are chosen on the last step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrosshairMode {
    None,
    Copy,
    Customize,
}

/// The trimmed name is not empty.
pub fn can_proceed(name: &str) -> bool {
    !name.trim().is_empty()
}

pub fn title(editing: bool) -> &'static str {
    if editing {
        "Edit Profile"
    } else {
        "Create New Profile"
    }
}

pub fn save_label(editing: bool, saving: bool) -> &'static str {
    match (editing, saving) {
        (false, false) => "Create Profile",
        (false, true) => "Creating...",
        (true, false) => "Save Changes",
        (true, true) => "Saving...",
    }
}

/// The success toast: (title, description).
pub fn saved_toast(editing: bool, name: &str) -> (&'static str, String) {
    if editing {
        ("Profile updated", format!("\"{name}\" has been saved"))
    } else {
        (
            "Profile created",
            format!("\"{name}\" is ready to activate"),
        )
    }
}

/// The success toast for an edit that took the active profile out of step with the game.
pub fn deactivated_toast(name: &str) -> (&'static str, String) {
    (
        "Profile updated",
        format!("\"{name}\" was saved. Activate it to use the changes in game"),
    )
}

pub fn failed_title(editing: bool) -> &'static str {
    if editing {
        "Failed to update profile"
    } else {
        "Failed to create profile"
    }
}

pub fn copy_label(n: usize) -> String {
    format!("Copy current ({n})")
}

pub fn copy_note(n: usize) -> String {
    let noun = if n == 1 { "assignment" } else { "assignments" };
    format!("Will copy your {n} current crosshair {noun} into this profile.")
}

pub fn assigned_count(n: usize) -> String {
    let noun = if n == 1 { "weapon" } else { "weapons" };
    format!("{n} {noun} assigned")
}

/// The activity row for a saved profile; an update also names the profile id, as v5 did.
pub fn log_entry(editing: bool, profile: &Profile) -> NewLogEntry {
    let name = profile.name();
    if editing {
        NewLogEntry::new(
            LogLevel::Info,
            "profile",
            "update",
            format!("Updated profile: {name}"),
        )
        .with_details(format!("Profile ID: {}", profile.id().as_str()))
    } else {
        NewLogEntry::new(
            LogLevel::Info,
            "profile",
            "create",
            format!("Created profile: {name}"),
        )
    }
}

/// One row of a HUD or sound select: the draft slot it picks, its text and its pilot key.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotOption<T> {
    pub value: DraftSlot<T>,
    pub title: SharedString,
    pub key: String,
    pub field: &'static str,
}

/// Field "hud": "None (no HUD)" first, then the library in order, then a saved folder the library lacks as
/// "{name} (missing)"; with the slot to preselect (Item for a present folder, Keep for a
/// missing one, Empty without a saved item).
pub fn hud_options(
    library: &[HudEntry],
    saved: Option<&ProfileItem>,
) -> (Vec<SlotOption<HudId>>, DraftSlot<HudId>) {
    let entries = library
        .iter()
        .map(|h| (h.id().clone(), h.folder_name(), h.name()));
    slot_options("hud", "None (no HUD)".into(), entries, saved)
}

/// As `hud_options` for `field` "hitsound" or "killsound", the first title "None (no {field})".
pub fn sound_options(
    library: &[SoundEntry],
    saved: Option<&ProfileItem>,
    field: &'static str,
) -> (Vec<SlotOption<SoundId>>, DraftSlot<SoundId>) {
    let entries = library
        .iter()
        .map(|s| (s.id().clone(), s.folder_name(), s.name()));
    slot_options(field, format!("None (no {field})"), entries, saved)
}

// Build the list once for either kind: the saved item preselects its library entry by folder,
// or, when the folder is gone, a trailing "(missing)" row that keeps the saved slot as is.
fn slot_options<'a, T: Clone>(
    field: &'static str,
    none_title: String,
    entries: impl Iterator<Item = (T, &'a str, &'a str)>,
    saved: Option<&ProfileItem>,
) -> (Vec<SlotOption<T>>, DraftSlot<T>) {
    let mut options = vec![SlotOption {
        value: DraftSlot::Empty,
        title: none_title.into(),
        key: "none".to_owned(),
        field,
    }];
    let mut initial = DraftSlot::Empty;
    for (id, folder, name) in entries {
        if saved.is_some_and(|s| s.folder_name() == folder) {
            initial = DraftSlot::Item(id.clone());
        }
        options.push(SlotOption {
            value: DraftSlot::Item(id),
            title: SharedString::from(name.to_owned()),
            key: folder.to_owned(),
            field,
        });
    }
    if let Some(saved) = saved
        && matches!(initial, DraftSlot::Empty)
    {
        initial = DraftSlot::Keep;
        options.push(SlotOption {
            value: DraftSlot::Keep,
            title: format!("{} (missing)", saved.name()).into(),
            key: "missing".to_owned(),
            field,
        });
    }
    (options, initial)
}

/// One weapon's crosshair in Customize mode: newly picked, or the saved one left untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomCrosshair {
    Set(CrosshairRef),
    Keep(ProfileCrosshair),
}

/// Every saved crosshair as Keep, by weapon script.
pub fn custom_from_profile(profile: &Profile) -> BTreeMap<String, CustomCrosshair> {
    profile
        .crosshairs()
        .iter()
        .map(|c| {
            (
                c.weapon_script().to_owned(),
                CustomCrosshair::Keep(c.clone()),
            )
        })
        .collect()
}

/// Customize when the profile has crosshairs; None otherwise and when creating.
pub fn initial_mode(profile: Option<&Profile>) -> CrosshairMode {
    match profile {
        Some(p) if !p.crosshairs().is_empty() => CrosshairMode::Customize,
        _ => CrosshairMode::None,
    }
}

/// The draft's crosshair list: none; each live assignment as Set; or the custom map in script order.
pub fn draft_crosshairs(
    mode: CrosshairMode,
    live: &[Assignment],
    custom: &BTreeMap<String, CustomCrosshair>,
) -> Vec<DraftCrosshair> {
    match mode {
        CrosshairMode::None => Vec::new(),
        CrosshairMode::Copy => live
            .iter()
            .map(|a| DraftCrosshair::Set {
                weapon_script: a.weapon_script().to_owned(),
                crosshair: a.crosshair().clone(),
            })
            .collect(),
        CrosshairMode::Customize => custom
            .iter()
            .map(|(script, c)| match c {
                CustomCrosshair::Set(crosshair) => DraftCrosshair::Set {
                    weapon_script: script.clone(),
                    crosshair: crosshair.clone(),
                },
                CustomCrosshair::Keep(saved) => DraftCrosshair::Keep(saved.clone()),
            })
            .collect(),
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
