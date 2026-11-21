//! The profile editor's pure rules: the five-slot draft it edits and saves, why Save is off,
//! the card lines, the Apply summary and the new draft's note.

use gpui_kit::SharedString;
use tf2tools_core::{
    AddonEntry, AddonName, Choices, CrosshairId, CrosshairRef, CurrentSetup, DraftCrosshair,
    DraftItem, HudEntry, HudId, MissingItem, MissingKind, Preset, Profile, ProfileCrosshair,
    ProfileCrosshairRef, ProfileDraft, ProfileItem, SlotMode, SoundEntry, SoundId, SoundKind,
};

use super::model::{crosshair_list, hit, hud_item, kill, taken_message};
use crate::i18n::{counted_plural, names, plural, separated, tr};
use crate::pages::graphics::model::preset_label;

/// "1 weapon", "N weapons".
pub fn weapon_count(n: usize) -> String {
    crate::i18n::plural("profiles.weapons", u64::try_from(n).unwrap_or(u64::MAX)).to_string()
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
        if !self.can_set(slot, mode) {
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

    /// Whether `slot` may switch to `mode`: everything but Add-ons Set while Graphics is None.
    pub fn can_set(&self, slot: SlotKind, mode: Mode) -> bool {
        !(slot == SlotKind::Addons && mode == Mode::Set && self.addons_need_graphics())
    }

    /// The draft's `kind` sound; None for TF2's default.
    pub fn sound(&self, kind: SoundKind) -> Option<&Picked<SoundId>> {
        if kind == SoundKind::Hitsound {
            self.hit.as_ref()
        } else {
            self.kill.as_ref()
        }
    }

    /// Puts `pick` in the draft's `kind` sound; None for TF2's default.
    pub fn set_sound(&mut self, kind: SoundKind, pick: Option<Picked<SoundId>>) {
        if kind == SoundKind::Hitsound {
            self.hit = pick;
        } else {
            self.kill = pick;
        }
    }

    /// Moves a Set Add-ons slot to None while Graphics is None, the combination the editor
    /// never saves; true when it moved.
    pub fn drop_forbidden_addons(&mut self) -> bool {
        let forbidden =
            self.mode(SlotKind::Addons) == Mode::Set && !self.can_set(SlotKind::Addons, Mode::Set);
        if forbidden {
            self.modes[SlotKind::Addons as usize] = Mode::None;
        }
        forbidden
    }

    /// Drops every library pick whose id the library lists no longer hold, so a Save does not
    /// fail on it again; kept items stay, since Save writes them back as they are.
    pub fn drop_missing_picks(
        &mut self,
        huds: &[HudId],
        sounds: &[SoundId],
        crosshairs: &[CrosshairId],
        addons: &[AddonName],
    ) {
        fn listed<T: PartialEq>(pick: Option<&Picked<T>>, ids: &[T]) -> bool {
            if let Some(DraftItem::Pick(id)) = pick.map(|p| &p.item) {
                ids.contains(id)
            } else {
                true
            }
        }
        if !listed(self.hud.as_ref(), huds) {
            self.hud = None;
        }
        if !listed(self.hit.as_ref(), sounds) {
            self.hit = None;
        }
        if !listed(self.kill.as_ref(), sounds) {
            self.kill = None;
        }
        self.crosshairs.retain(|p| {
            if let DraftCrosshair::Set {
                crosshair: CrosshairRef::Library(id),
                ..
            } = &p.item
            {
                crosshairs.contains(id)
            } else {
                true
            }
        });
        self.addons.retain(|a| listed(Some(a), addons));
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

/// One slot's line on a profile card; `left` (Leave) is drawn muted and italic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardLine {
    pub slot: SlotKind,
    pub text: String,
    pub left: bool,
}

/// What each slot of `profile` does, in page order: "Left as is" for Leave, TF2's own for
/// None, and the Set content in a few words.
pub fn card_lines(profile: &Profile, addon_name: &dyn Fn(&AddonName) -> String) -> [CardLine; 5] {
    SlotKind::ALL.map(|slot| {
        let mode = profile_mode(profile, slot);
        let text = match mode {
            Mode::Leave => tr!("profiles.card.leave").into(),
            Mode::None => none_text(slot),
            Mode::Set => set_text(profile, slot, addon_name),
        };
        CardLine {
            slot,
            text,
            left: mode == Mode::Leave,
        }
    })
}

fn profile_mode(profile: &Profile, slot: SlotKind) -> Mode {
    match slot {
        SlotKind::Hud => Mode::of(profile.hud()),
        SlotKind::Sounds => Mode::of(profile.sounds()),
        SlotKind::Crosshairs => Mode::of(profile.crosshairs()),
        SlotKind::Graphics => Mode::of(profile.graphics()),
        SlotKind::Addons => Mode::of(profile.addons()),
    }
}

// A None slot's line, which is also how a Set slot holding nothing reads.
fn none_text(slot: SlotKind) -> String {
    match slot {
        SlotKind::Hud => tr!("profiles.card.hud_none"),
        SlotKind::Sounds => tr!("profiles.card.sounds_none"),
        SlotKind::Crosshairs => tr!("profiles.card.crosshairs_none"),
        SlotKind::Graphics => tr!("profiles.card.graphics_none"),
        SlotKind::Addons => tr!("profiles.card.addons_none"),
    }
    .into()
}

fn set_text(
    profile: &Profile,
    slot: SlotKind,
    addon_name: &dyn Fn(&AddonName) -> String,
) -> String {
    match slot {
        SlotKind::Hud => hud_item(profile).map_or_else(|| none_text(slot), |h| h.name().to_owned()),
        SlotKind::Sounds => tr!(
            "profiles.card.sounds",
            hit = sound_or_default(hit(profile)),
            kill = sound_or_default(kill(profile))
        )
        .into(),
        SlotKind::Crosshairs => match crosshair_list(profile).len() {
            0 => none_text(slot),
            n => weapon_crosshairs(n),
        },
        SlotKind::Graphics => profile
            .graphics()
            .as_set()
            .map_or_else(|| none_text(slot), |g| graphics_text(&g.choices)),
        SlotKind::Addons => {
            let Some(addons) = profile.addons().as_set() else {
                return none_text(slot);
            };
            let names: Vec<String> = addons.on.iter().map(addon_name).collect();
            let list = separated(&names);
            match (addons.only_these, count(names.len())) {
                (true, 0) => none_text(slot),
                (true, 1) => tr!("profiles.card.addons_only_one", list = list).into(),
                (true, n) => counted_plural(
                    "profiles.card.addons_only",
                    n,
                    &n.to_string(),
                    &[("list", &list)],
                )
                .into(),
                (false, 0) => tr!("profiles.card.addons_nothing").into(),
                (false, _) => tr!("profiles.card.addons_on", list = list).into(),
            }
        }
    }
}

// A sound's name, or "TF2 default" for a side that keeps the game's own.
fn sound_or_default(item: Option<&ProfileItem>) -> String {
    item.map_or_else(
        || tr!("profiles.card.tf2_default").into(),
        |i| i.name().to_owned(),
    )
}

fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// "Ultra as is" when no module is changed from the preset, else "Low, 1 change".
pub fn graphics_text(choices: &Choices) -> String {
    let preset = preset_label(choices.preset());
    match count(choices.modules().len()) {
        0 => tr!("profiles.card.graphics_as_is", preset = preset).into(),
        n => counted_plural(
            "profiles.card.graphics",
            n,
            &n.to_string(),
            &[("preset", &preset)],
        )
        .into(),
    }
}

/// "1 weapon crosshair", "3 weapon crosshairs".
pub fn weapon_crosshairs(n: usize) -> String {
    plural("profiles.card.crosshairs", count(n)).into()
}

/// The new-draft note: what each Set slot copied from the game, then the ask to name and
/// save it. With nothing copied, the ask alone.
pub fn new_draft_note(draft: &Draft) -> String {
    let set = |slot| draft.mode(slot) == Mode::Set;
    let mut parts: Vec<String> = Vec::new();
    if let Some(hud) = draft.hud.as_ref().filter(|_| set(SlotKind::Hud)) {
        parts.push(hud.name.clone());
    }
    if set(SlotKind::Sounds) {
        match (&draft.hit, &draft.kill) {
            (Some(h), Some(k)) => parts.push(
                tr!(
                    "profiles.note.sounds",
                    hit = h.name.as_str(),
                    kill = k.name.as_str()
                )
                .into(),
            ),
            (Some(one), None) | (None, Some(one)) => parts.push(one.name.clone()),
            (None, None) => {}
        }
    }
    if set(SlotKind::Crosshairs) && !draft.crosshairs.is_empty() {
        parts.push(weapon_crosshairs(draft.crosshairs.len()));
    }
    if set(SlotKind::Graphics) {
        let preset = preset_label(draft.graphics.preset());
        parts.push(match count(draft.graphics.modules().len()) {
            0 => tr!("profiles.card.graphics_as_is", preset = preset).into(),
            n => counted_plural(
                "profiles.note.graphics",
                n,
                &n.to_string(),
                &[("preset", &preset)],
            )
            .into(),
        });
    }
    if set(SlotKind::Addons) && !draft.addons.is_empty() {
        parts.push(plural("profiles.note.addons", count(draft.addons.len())).into());
    }
    let tail = tr!("profiles.note.tail");
    if parts.is_empty() {
        return tail.into();
    }
    tr!(
        "profiles.note.copied",
        parts = separated(&parts),
        tail = tail
    )
    .into()
}

/// One part of the game Apply changes: from what is installed to what the profile puts there.
/// A row for a missing item carries it, leaves `from` empty and holds its sentence in `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryRow {
    pub key: String,
    pub label: SharedString,
    pub from: String,
    pub to: String,
    pub missing: Option<MissingItem>,
}

impl SummaryRow {
    /// "from → to", or a missing row's sentence.
    pub fn text(&self) -> String {
        match self.missing {
            Some(_) => self.to.clone(),
            None => format!("{} → {}", self.from, self.to),
        }
    }
}

/// The Apply summary: the parts that change, and the Leave slots in page order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Summary {
    pub rows: Vec<SummaryRow>,
    pub left: Vec<SlotKind>,
}

impl Summary {
    fn row(&mut self, key: &'static str, label: SharedString, from: String, to: String) {
        self.rows.push(SummaryRow {
            key: key.to_owned(),
            label,
            from,
            to,
            missing: None,
        });
    }
}

/// Shows each item Apply will skip as its slot's row: "<name> is missing; keeps <what stays>".
/// A HUD or sound replaces its slot's row; a crosshair or add-on gets a row of its own beside
/// its slot's, since the rest of that slot still changes.
pub fn mark_missing(summary: &mut Summary, missing: &[MissingItem], current: &CurrentSetup) {
    for item in missing {
        let (key, label, kept) = missing_slot(item, current);
        let to = match kept {
            Some(kept) => tr!(
                "profiles.summary.missing_keeps",
                name = item.name(),
                kept = kept
            ),
            None => tr!("profiles.summary.missing_none", name = item.name()),
        };
        let row = SummaryRow {
            key,
            label,
            from: String::new(),
            to: to.into(),
            missing: Some(item.clone()),
        };
        let rows = &mut summary.rows;
        if let Some(same) = rows.iter_mut().find(|r| r.key == row.key) {
            *same = row;
        } else {
            let at = rows
                .iter()
                .position(|r| row_rank(&r.key) > row_rank(&row.key))
                .unwrap_or(rows.len());
            rows.insert(at, row);
        }
    }
}

// A missing item's row key, its slot label, and what stays installed in its place.
fn missing_slot(
    item: &MissingItem,
    current: &CurrentSetup,
) -> (String, SharedString, Option<String>) {
    let name = |i: &Option<ProfileItem>| i.as_ref().map(|i| i.name().to_owned());
    match item.kind() {
        MissingKind::Hud => {
            let kept = match &current.hud {
                SlotMode::Set(h) => Some(h.name().to_owned()),
                SlotMode::None => None,
                SlotMode::Leave | _ => Some(tr!("profiles.summary.missing_other_hud").into()),
            };
            ("hud".into(), "HUD".into(), kept)
        }
        MissingKind::Hitsound => (
            "hitsound".into(),
            names::sound_kind_name(SoundKind::Hitsound),
            name(&current.sounds.hit),
        ),
        MissingKind::Killsound => (
            "killsound".into(),
            names::sound_kind_name(SoundKind::Killsound),
            name(&current.sounds.kill),
        ),
        MissingKind::Crosshair { weapon_script } => {
            let kept = current
                .crosshairs
                .iter()
                .find(|c| c.weapon_script() == weapon_script)
                .and_then(|c| match c.crosshair() {
                    ProfileCrosshairRef::Builtin(name) => Some(name.clone()),
                    ProfileCrosshairRef::Library { folder_name } => Some(folder_name.clone()),
                    _ => None,
                });
            (
                format!("crosshair:{weapon_script}"),
                SlotKind::Crosshairs.label(),
                kept,
            )
        }
        MissingKind::Addon { name } => (
            format!("addon:{}", name.as_str()),
            SlotKind::Addons.label(),
            current
                .addons
                .contains(name)
                .then(|| name.as_str().to_owned()),
        ),
        _ => (
            format!("item:{}", item.name()),
            tr!("profiles.part.item"),
            None,
        ),
    }
}

// Where a row sits: the summary's slot order, a missing crosshair or add-on after its slot.
fn row_rank(key: &str) -> u8 {
    match key.split(':').next().unwrap_or(key) {
        "hud" => 0,
        "hitsound" => 1,
        "killsound" | "sounds" => 2,
        "crosshairs" | "crosshair" => 3,
        "graphics" => 4,
        "addons" | "addon" => 5,
        _ => 6,
    }
}

/// What applying `profile` over `current` changes, one row per part that differs, in the
/// order hud, hitsound, killsound, sounds, crosshairs, graphics, addons.
pub fn summary(
    profile: &Profile,
    current: &CurrentSetup,
    addon_name: &dyn Fn(&AddonName) -> String,
) -> Summary {
    let mut s = Summary::default();
    for slot in SlotKind::ALL {
        if profile_mode(profile, slot) == Mode::Leave {
            s.left.push(slot);
            continue;
        }
        match slot {
            SlotKind::Hud => hud_rows(&mut s, profile, current),
            SlotKind::Sounds => sound_rows(&mut s, profile, current),
            SlotKind::Crosshairs => crosshair_rows(&mut s, profile, current),
            SlotKind::Graphics => graphics_rows(&mut s, profile, current),
            SlotKind::Addons => addon_rows(&mut s, profile, current, addon_name),
        }
    }
    s
}

// An unmanaged current HUD always counts as a change, since Apply replaces or removes it.
fn hud_rows(s: &mut Summary, profile: &Profile, current: &CurrentSetup) {
    let to = hud_item(profile);
    let (from, same) = match &current.hud {
        SlotMode::Set(h) => (
            h.name().to_owned(),
            to.is_some_and(|t| t.folder_name() == h.folder_name()),
        ),
        SlotMode::None => (none_text(SlotKind::Hud), to.is_none()),
        SlotMode::Leave | _ => (tr!("profiles.summary.other_hud").into(), false),
    };
    if !same {
        let to = to.map_or_else(|| none_text(SlotKind::Hud), |h| h.name().to_owned());
        s.row("hud", "HUD".into(), from, to);
    }
}

// Set compares each sound by folder; None is one row taking out whatever is installed.
fn sound_rows(s: &mut Summary, profile: &Profile, current: &CurrentSetup) {
    let installed = &current.sounds;
    let Some(sounds) = profile.sounds().as_set() else {
        let names: Vec<&str> = [&installed.hit, &installed.kill]
            .into_iter()
            .flatten()
            .map(|i| i.name())
            .collect();
        if !names.is_empty() {
            let to = tr!("profiles.card.tf2_default").into();
            s.row("sounds", SlotKind::Sounds.label(), separated(&names), to);
        }
        return;
    };
    let sides = [
        ("hitsound", SoundKind::Hitsound, &sounds.hit, &installed.hit),
        (
            "killsound",
            SoundKind::Killsound,
            &sounds.kill,
            &installed.kill,
        ),
    ];
    for (key, kind, to, from) in sides {
        let folder = |i: &Option<ProfileItem>| i.as_ref().map(|i| i.folder_name().to_owned());
        if folder(to) != folder(from) {
            let (from, to) = (
                sound_or_default(from.as_ref()),
                sound_or_default(to.as_ref()),
            );
            s.row(key, names::sound_kind_name(kind), from, to);
        }
    }
}

fn crosshair_rows(s: &mut Summary, profile: &Profile, current: &CurrentSetup) {
    fn pairs(list: &[ProfileCrosshair]) -> Vec<(&str, &ProfileCrosshairRef)> {
        let mut pairs: Vec<_> = list
            .iter()
            .map(|c| (c.weapon_script(), c.crosshair()))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        pairs
    }
    fn side(n: usize) -> String {
        match n {
            0 => tr!("profiles.summary.crosshairs_default").into(),
            n => weapon_count(n),
        }
    }
    let to = crosshair_list(profile);
    if pairs(to) != pairs(&current.crosshairs) {
        let (from, to) = (side(current.crosshairs.len()), side(to.len()));
        s.row("crosshairs", SlotKind::Crosshairs.label(), from, to);
    }
}

fn graphics_rows(s: &mut Summary, profile: &Profile, current: &CurrentSetup) {
    let from = match &current.graphics {
        SlotMode::Set(g) => graphics_text(&g.choices),
        SlotMode::None => tr!("profiles.summary.no_mastercomfig").into(),
        SlotMode::Leave | _ => tr!("profiles.summary.outside_mastercomfig").into(),
    };
    let to = match profile.graphics().as_set() {
        Some(g) if current.graphics.as_set() == Some(g) => return,
        Some(g) => graphics_text(&g.choices),
        None if matches!(current.graphics, SlotMode::None) => return,
        None => tr!("profiles.summary.mastercomfig_removed").into(),
    };
    s.row("graphics", SlotKind::Graphics.label(), from, to);
}

// Set turns on what is ticked and missing, and with Only these turns off the rest; None
// turns off every installed add-on. Null-Canceling Movement either way rebinds the keys.
fn addon_rows(
    s: &mut Summary,
    profile: &Profile,
    current: &CurrentSetup,
    addon_name: &dyn Fn(&AddonName) -> String,
) {
    let installed = &current.addons;
    let (on, off): (Vec<&AddonName>, Vec<&AddonName>) = match profile.addons().as_set() {
        Some(set) => (
            set.on.iter().filter(|a| !installed.contains(a)).collect(),
            installed
                .iter()
                .filter(|a| set.only_these && !set.on.contains(a))
                .collect(),
        ),
        None => (Vec::new(), installed.iter().collect()),
    };
    let list =
        |names: &[&AddonName]| separated(&names.iter().map(|a| addon_name(a)).collect::<Vec<_>>());
    let to: String = match (on.is_empty(), off.is_empty()) {
        (true, true) => return,
        (false, true) => tr!("profiles.summary.turns_on", list = list(&on)).into(),
        (true, false) => tr!("profiles.summary.turns_off_only", list = list(&off)).into(),
        (false, false) => tr!(
            "profiles.summary.both",
            on = tr!("profiles.summary.turns_on", list = list(&on)),
            off = tr!("profiles.summary.turns_off", list = list(&off))
        )
        .into(),
    };
    let to = if on.iter().chain(&off).any(|a| a.is_null_movement()) {
        tr!("profiles.summary.movement", text = to).into()
    } else {
        to
    };
    let n = count(installed.len());
    let from = counted_plural("profiles.summary.addons_on", n, &n.to_string(), &[]).into();
    s.row("addons", SlotKind::Addons.label(), from, to);
}

/// "Stays as it is: Crosshairs." for the Leave slots, or nothing when every slot acts.
pub fn left_line(left: &[SlotKind]) -> Option<String> {
    if left.is_empty() {
        return None;
    }
    let labels: Vec<SharedString> = left.iter().map(|s| s.label()).collect();
    Some(tr!("profiles.summary.left", list = separated(&labels)).into())
}
