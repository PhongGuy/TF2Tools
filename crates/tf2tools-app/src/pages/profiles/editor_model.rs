//! The profile editor's pure rules: its slots, the draft each click saves and the name a new
//! profile starts with; and the five-slot draft the whole-setup editor edits and saves.

use gpui_kit::SharedString;
use tf2tools_core::{
    AddonEntry, AddonName, BuiltinCrosshairs, Choices, CrosshairEntry, CrosshairRef, CurrentSetup,
    DraftCrosshair, DraftItem, HudEntry, HudId, MissingItem, MissingKind, Preset, Profile,
    ProfileCrosshair, ProfileCrosshairRef, ProfileDraft, ProfileItem, SlotMode, SoundEntry,
    SoundId, SoundKind,
};

use super::model::{crosshair_list, hit, hud_item, kill, sounds_with, taken_message};
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

/// One of a profile's five slots, in page order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotKind {
    Hud,
    Sounds,
    Crosshairs,
    Graphics,
    Addons,
}

impl SlotKind {
    pub const ALL: [SlotKind; 5] = [
        Self::Hud,
        Self::Sounds,
        Self::Crosshairs,
        Self::Graphics,
        Self::Addons,
    ];

    /// "hud", "sounds", "crosshairs", "graphics", "addons": the id key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Hud => "hud",
            Self::Sounds => "sounds",
            Self::Crosshairs => "crosshairs",
            Self::Graphics => "graphics",
            Self::Addons => "addons",
        }
    }

    /// "HUD", "Sounds", "Crosshairs", "Graphics", "Add-ons".
    pub fn label(self) -> SharedString {
        match self {
            Self::Hud => "HUD".into(),
            Self::Sounds => tr!("profiles.part.sounds"),
            Self::Crosshairs => tr!("profiles.part.crosshairs"),
            Self::Graphics => tr!("profiles.part.graphics"),
            Self::Addons => tr!("profiles.part.addons"),
        }
    }
}

/// What a slot does on Apply: set its content, take it out, or leave the game alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Set,
    None,
    Leave,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Self::Set, Self::None, Self::Leave];

    /// "set", "none", "leave": the id key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::None => "none",
            Self::Leave => "leave",
        }
    }

    /// The core slot's mode; a mode this app does not know reads as Leave, as core stores it.
    pub fn of<T>(slot: &SlotMode<T>) -> Mode {
        match slot {
            SlotMode::Set(_) => Self::Set,
            SlotMode::None => Self::None,
            SlotMode::Leave | _ => Self::Leave,
        }
    }
}

/// A library item in the draft; two are equal when they name the same folder.
#[derive(Debug, Clone)]
pub struct Picked<T> {
    pub folder: String,
    pub name: String,
    pub item: DraftItem<T>,
}

impl<T> PartialEq for Picked<T> {
    fn eq(&self, other: &Self) -> bool {
        self.folder == other.folder
    }
}

impl<T> Picked<T> {
    /// A saved reference, written back as it is.
    pub fn kept(item: &ProfileItem) -> Self {
        Self {
            folder: item.folder_name().to_owned(),
            name: item.name().to_owned(),
            item: DraftItem::Keep(item.clone()),
        }
    }
}

impl Picked<HudId> {
    /// A library HUD the player picked.
    pub fn hud(entry: &HudEntry) -> Self {
        Self {
            folder: entry.folder_name().to_owned(),
            name: entry.name().to_owned(),
            item: DraftItem::Pick(entry.id().clone()),
        }
    }
}

impl Picked<SoundId> {
    /// A library sound the player picked.
    pub fn sound(entry: &SoundEntry) -> Self {
        Self {
            folder: entry.folder_name().to_owned(),
            name: entry.name().to_owned(),
            item: DraftItem::Pick(entry.id().clone()),
        }
    }
}

impl Picked<AddonName> {
    /// A library add-on the player ticked, named by its display name.
    pub fn addon(entry: &AddonEntry) -> Self {
        Self {
            folder: entry.name().as_str().to_owned(),
            name: entry.display_name().to_owned(),
            item: DraftItem::Pick(entry.name().clone()),
        }
    }

    /// A saved or installed add-on, kept by its asset name and shown as `display`.
    pub fn kept_addon(name: &AddonName, display: String) -> Self {
        Self {
            folder: name.as_str().to_owned(),
            name: display,
            item: DraftItem::keep_addon(name.clone()),
        }
    }
}

/// One weapon's crosshair; equal when script and what it shows match.
#[derive(Debug, Clone)]
pub struct WeaponPick {
    pub script: String,
    pub shows: ProfileCrosshairRef,
    pub item: DraftCrosshair,
}

impl PartialEq for WeaponPick {
    fn eq(&self, other: &Self) -> bool {
        self.script == other.script && self.shows == other.shows
    }
}

impl WeaponPick {
    /// A saved assignment, written back as it is.
    pub fn kept(saved: &ProfileCrosshair) -> Self {
        Self {
            script: saved.weapon_script().to_owned(),
            shows: saved.crosshair().clone(),
            item: DraftCrosshair::Keep(saved.clone()),
        }
    }
}

/// The editor's working copy of a profile. Every slot keeps its content whatever its mode, so
/// switching back to Set restores it; only a Set slot's content is compared or saved.
///
/// No `PartialEq`: a draft is compared only through `differs_from`, which ignores content a
/// slot's mode does not save.
#[derive(Debug, Clone)]
pub struct Draft {
    pub name: String,
    pub hud: Option<Picked<HudId>>,
    /// None: TF2's default.
    pub hit: Option<Picked<SoundId>>,
    pub kill: Option<Picked<SoundId>>,
    /// Sorted by script.
    pub crosshairs: Vec<WeaponPick>,
    pub graphics: Choices,
    /// Sorted by folder.
    pub addons: Vec<Picked<AddonName>>,
    pub only_these: bool,
    modes: [Mode; 5],
}

impl Draft {
    // Every slot empty, as a slot that never had content starts when switched to Set.
    fn empty(name: String, modes: [Mode; 5]) -> Self {
        Self {
            name,
            hud: None,
            hit: None,
            kill: None,
            crosshairs: Vec::new(),
            graphics: Choices::new(Preset::Medium),
            addons: Vec::new(),
            only_these: false,
            modes,
        }
    }

    /// A saved profile: each slot in its saved mode, its Set content kept as saved.
    pub fn from_profile(profile: &Profile, addon_name: &dyn Fn(&AddonName) -> String) -> Self {
        let modes = [
            Mode::of(profile.hud()),
            Mode::of(profile.sounds()),
            Mode::of(profile.crosshairs()),
            Mode::of(profile.graphics()),
            Mode::of(profile.addons()),
        ];
        let mut d = Self::empty(profile.name().to_owned(), modes);
        d.hud = profile.hud().as_set().map(Picked::kept);
        if let Some(sounds) = profile.sounds().as_set() {
            d.hit = sounds.hit.as_ref().map(Picked::kept);
            d.kill = sounds.kill.as_ref().map(Picked::kept);
        }
        if let Some(list) = profile.crosshairs().as_set() {
            d.crosshairs = list.iter().map(WeaponPick::kept).collect();
        }
        if let Some(graphics) = profile.graphics().as_set() {
            d.graphics = graphics.choices.clone();
        }
        if let Some(addons) = profile.addons().as_set() {
            d.addons = kept_addons(&addons.on, addon_name);
            d.only_these = addons.only_these;
        }
        d.sort();
        d
    }

    /// The new-draft fill: name empty; HUD Set only for a managed HUD, else None; Sounds and
    /// Crosshairs Set; Graphics copied (Set, None or Leave); Add-ons Set to the installed ones
    /// with Only these, or Leave when Graphics is None.
    pub fn from_current(setup: &CurrentSetup, addon_name: &dyn Fn(&AddonName) -> String) -> Self {
        let graphics = Mode::of(&setup.graphics);
        let addons = if graphics == Mode::None {
            Mode::Leave
        } else {
            Mode::Set
        };
        let hud = setup.hud.as_set().map(Picked::kept);
        let hud_mode = if hud.is_some() { Mode::Set } else { Mode::None };
        let modes = [hud_mode, Mode::Set, Mode::Set, graphics, addons];
        let mut d = Self::empty(String::new(), modes);
        d.hud = hud;
        d.hit = setup.sounds.hit.as_ref().map(Picked::kept);
        d.kill = setup.sounds.kill.as_ref().map(Picked::kept);
        d.crosshairs = setup.crosshairs.iter().map(WeaponPick::kept).collect();
        if let Some(g) = setup.graphics.as_set() {
            d.graphics = g.choices.clone();
        }
        if addons == Mode::Set {
            d.addons = kept_addons(&setup.addons, addon_name);
            d.only_these = true;
        }
        d.sort();
        d
    }

    fn sort(&mut self) {
        self.crosshairs.sort_by(|a, b| a.script.cmp(&b.script));
        self.addons.sort_by(|a, b| a.folder.cmp(&b.folder));
    }

    pub fn mode(&self, slot: SlotKind) -> Mode {
        self.modes[slot as usize]
    }

    /// Keeps the slot's content. Refuses (false) Add-ons Set while Graphics is None; Graphics
    /// None moves a Set Add-ons slot to None.
    pub fn set_mode(&mut self, slot: SlotKind, mode: Mode) -> bool {
        if slot == SlotKind::Addons && mode == Mode::Set && self.addons_need_graphics() {
            return false;
        }
        if slot == SlotKind::Graphics
            && mode == Mode::None
            && self.mode(SlotKind::Addons) == Mode::Set
        {
            self.modes[SlotKind::Addons as usize] = Mode::None;
        }
        self.modes[slot as usize] = mode;
        true
    }

    /// True while Graphics is None, so Add-ons cannot be Set.
    pub fn addons_need_graphics(&self) -> bool {
        self.mode(SlotKind::Graphics) == Mode::None
    }

    /// Dirty: the trimmed names differ, a mode differs, or a Set slot's content differs.
    pub fn differs_from(&self, baseline: &Draft) -> bool {
        if self.name.trim() != baseline.name.trim() || self.modes != baseline.modes {
            return true;
        }
        SlotKind::ALL
            .into_iter()
            .filter(|&slot| self.mode(slot) == Mode::Set)
            .any(|slot| match slot {
                SlotKind::Hud => self.hud != baseline.hud,
                SlotKind::Sounds => self.hit != baseline.hit || self.kill != baseline.kill,
                SlotKind::Crosshairs => sorted_crosshairs(self) != sorted_crosshairs(baseline),
                SlotKind::Graphics => self.graphics != baseline.graphics,
                SlotKind::Addons => {
                    self.only_these != baseline.only_these
                        || sorted_addons(self) != sorted_addons(baseline)
                }
            })
    }

    /// Name as typed; Set slots from their content (HUD Set without a pick saves as None).
    pub fn to_profile_draft(&self) -> ProfileDraft {
        // Each slot's core mode: its content when Set, else None or Leave as the mode says.
        fn slot<T>(mode: Mode, content: impl FnOnce() -> T) -> SlotMode<T> {
            match mode {
                Mode::Set => SlotMode::Set(content()),
                Mode::None => SlotMode::None,
                Mode::Leave => SlotMode::Leave,
            }
        }
        let item = |p: &Option<Picked<SoundId>>| p.as_ref().map(|p| p.item.clone());
        let hud = match (self.mode(SlotKind::Hud), &self.hud) {
            (Mode::Set, Some(h)) => SlotMode::Set(h.item.clone()),
            (Mode::Leave, _) => SlotMode::Leave,
            (Mode::Set | Mode::None, _) => SlotMode::None,
        };
        ProfileDraft::new(self.name.as_str())
            .with_hud(hud)
            .with_sounds(slot(self.mode(SlotKind::Sounds), || {
                (item(&self.hit), item(&self.kill))
            }))
            .with_crosshairs(slot(self.mode(SlotKind::Crosshairs), || {
                self.crosshairs.iter().map(|c| c.item.clone()).collect()
            }))
            .with_graphics(slot(self.mode(SlotKind::Graphics), || {
                self.graphics.clone()
            }))
            .with_addons(slot(self.mode(SlotKind::Addons), || {
                let on = self.addons.iter().map(|a| a.item.clone()).collect();
                (on, self.only_these)
            }))
    }
}

fn kept_addons(
    names: &[AddonName],
    addon_name: &dyn Fn(&AddonName) -> String,
) -> Vec<Picked<AddonName>> {
    names
        .iter()
        .map(|n| Picked::kept_addon(n, addon_name(n)))
        .collect()
}

fn sorted_crosshairs(d: &Draft) -> Vec<&WeaponPick> {
    let mut list: Vec<&WeaponPick> = d.crosshairs.iter().collect();
    list.sort_by(|a, b| a.script.cmp(&b.script));
    list
}

fn sorted_addons(d: &Draft) -> Vec<&str> {
    let mut list: Vec<&str> = d.addons.iter().map(|a| a.folder.as_str()).collect();
    list.sort_unstable();
    list
}

/// Why Save is off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveBlock {
    BlankName,
    NameTaken(String),
    NoHud,
    Unchanged,
}

impl SaveBlock {
    /// The line under Save; Unchanged says nothing.
    pub fn message(&self) -> Option<SharedString> {
        match self {
            Self::BlankName => Some(tr!("profiles.editor.name_blank")),
            Self::NameTaken(name) => Some(taken_message(name).into()),
            Self::NoHud => Some(tr!("profiles.editor.pick_hud")),
            Self::Unchanged => None,
        }
    }
}

/// First of BlankName, NameTaken(`taken`), NoHud, Unchanged (Unchanged only when `saved`).
pub fn save_block(
    draft: &Draft,
    baseline: &Draft,
    saved: bool,
    taken: Option<String>,
) -> Option<SaveBlock> {
    if draft.name.trim().is_empty() {
        Some(SaveBlock::BlankName)
    } else if let Some(name) = taken {
        Some(SaveBlock::NameTaken(name))
    } else if draft.mode(SlotKind::Hud) == Mode::Set && draft.hud.is_none() {
        Some(SaveBlock::NoHud)
    } else if saved && !draft.differs_from(baseline) {
        Some(SaveBlock::Unchanged)
    } else {
        None
    }
}
