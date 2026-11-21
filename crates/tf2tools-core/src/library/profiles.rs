//! Profiles: named setups of five slots (HUD, Sounds, Crosshairs, Graphics, Add-ons), read
//! from v5's `profiles` and `profile_crosshair_assignments` rows plus v6's slot modes and
//! `profile_addons`. A profile refers to its items by library folder name, so a reference
//! can outlive the item it names.

use std::collections::BTreeSet;
use std::fmt;

use rusqlite::{Connection, OptionalExtension, Row};

use super::addons::{display_info, is_override};
use super::crosshairs::{SOURCE_BUILTIN, SOURCE_LIBRARY, is_weapon_script};
use super::profile_setup::CurrentSetup;
use super::profile_slots::{DraftItem, ProfileAddons, ProfileGraphics, ProfileSounds, SlotMode};
use super::profile_steps::ApplySkipReason;
use super::{
    AddonName, Assignment, Choices, CrosshairRef, ExternalHud, GbId, HudId, Library, MovementKeys,
    Preset, Provenance, SoundId, SoundKind, game_custom_dir, modules_json, new_id, now,
    parse_modules, profile_db, read_setting, save_setting,
};
use crate::crosshair_catalog::BuiltinCrosshairs;
use crate::error::{ApplyProfileError, LibraryError, ProfileError};
use crate::paths::{DataDir, LibraryKind};
use crate::tf2::Tf2Install;

/// The settings key v5 keeps the applied profile's id under.
const ACTIVE_PROFILE_KEY: &str = "active_profile_id";

const PROFILE_COLUMNS: &str = "id, name, hud_folder_name, hud_name, hud_store_hud_id, \
     hud_store_gb_id, hitsound_folder_name, hitsound_name, hitsound_store_gb_id, \
     killsound_folder_name, killsound_name, killsound_store_gb_id, created_at, updated_at, \
     hud_mode, sounds_mode, crosshairs_mode, graphics_mode, addons_mode, graphics_preset, \
     graphics_modules, addons_only";

text_id!(
    /// A profile id, v5's TEXT format (a v4 UUID for new ones).
    ProfileId
);

/// One item slot of a profile, as recorded: the library folder it names, its display
/// name and where it came from. The folder may no longer be in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileItem {
    pub(super) folder_name: String,
    pub(super) name: String,
    pub(super) provenance: Provenance,
}

impl ProfileItem {
    /// The library folder the profile refers to.
    pub fn folder_name(&self) -> &str {
        &self.folder_name
    }

    /// The display name recorded with the profile.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where the item came from; a GameBanana author always reads back empty.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// The crosshair a profile assigns to a weapon: a builtin by catalog name, or a library
/// crosshair by folder name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProfileCrosshairRef {
    /// A builtin crosshair, by its catalog name.
    Builtin(String),
    /// A library crosshair, by its library folder.
    Library {
        /// The folder under `library/crosshairs/`.
        folder_name: String,
    },
}

/// One weapon's crosshair in a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileCrosshair {
    weapon_script: String,
    crosshair: ProfileCrosshairRef,
    store_gb_id: Option<GbId>,
}

impl ProfileCrosshair {
    /// The weapon script name, such as `tf_weapon_scattergun`.
    pub fn weapon_script(&self) -> &str {
        &self.weapon_script
    }

    /// The crosshair assigned to it.
    pub fn crosshair(&self) -> &ProfileCrosshairRef {
        &self.crosshair
    }

    /// The GameBanana id of a downloaded library crosshair.
    pub fn store_gb_id(&self) -> Option<GbId> {
        self.store_gb_id
    }
}

/// A saved profile.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Profile {
    id: ProfileId,
    name: String,
    hud: SlotMode<ProfileItem>,
    sounds: SlotMode<ProfileSounds>,
    crosshairs: SlotMode<Vec<ProfileCrosshair>>,
    graphics: SlotMode<ProfileGraphics>,
    addons: SlotMode<ProfileAddons>,
    created_at: String,
    updated_at: String,
}

impl Profile {
    /// The profile id.
    pub fn id(&self) -> &ProfileId {
        &self.id
    }

    /// The display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The HUD slot.
    pub fn hud(&self) -> &SlotMode<ProfileItem> {
        &self.hud
    }

    /// The Sounds slot: the hitsound and the killsound together.
    pub fn sounds(&self) -> &SlotMode<ProfileSounds> {
        &self.sounds
    }

    /// The per-weapon crosshairs, sorted by weapon script when Set.
    pub fn crosshairs(&self) -> &SlotMode<Vec<ProfileCrosshair>> {
        &self.crosshairs
    }

    /// The Graphics slot: a mastercomfig preset and module choices.
    pub fn graphics(&self) -> &SlotMode<ProfileGraphics> {
        &self.graphics
    }

    /// The Add-ons slot: the add-ons to turn on and whether the rest go off.
    pub fn addons(&self) -> &SlotMode<ProfileAddons> {
        &self.addons
    }

    /// When the profile was created, as RFC 3339 text.
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    /// When the profile was last changed, as RFC 3339 text.
    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }
}

/// A slot is set only when its folder column holds a name; a missing display name falls
/// back to the folder, as v5 shows it.
fn item(
    folder_name: Option<String>,
    name: Option<String>,
    store_hud_id: Option<String>,
    store_gb_id: Option<i64>,
) -> Option<ProfileItem> {
    let folder_name = folder_name.filter(|f| !f.is_empty())?;
    let name = name
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| folder_name.clone());
    Some(ProfileItem {
        folder_name,
        name,
        provenance: Provenance::from_columns(store_hud_id, store_gb_id, ""),
    })
}

/// Maps one `profiles` row selected with `PROFILE_COLUMNS`; a Set crosshair or add-on list
/// is filled by `fill_lists`. A slot reads its content only when its mode is `set`.
fn profile_row(r: &Row<'_>) -> rusqlite::Result<Profile> {
    let hud = item(r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?);
    let hit = item(r.get(6)?, r.get(7)?, None, r.get(8)?);
    let kill = item(r.get(9)?, r.get(10)?, None, r.get(11)?);
    let preset: Option<String> = r.get(19)?;
    let modules: Option<String> = r.get(20)?;
    let only_these = r.get::<_, i64>(21)? != 0;
    Ok(Profile {
        id: ProfileId::new(r.get::<_, String>(0)?),
        name: r.get(1)?,
        hud: SlotMode::from_text(&r.get::<_, String>(14)?, || hud),
        sounds: SlotMode::from_text(&r.get::<_, String>(15)?, || {
            Some(ProfileSounds { hit, kill })
        }),
        crosshairs: SlotMode::from_text(&r.get::<_, String>(16)?, || Some(Vec::new())),
        graphics: SlotMode::from_text(&r.get::<_, String>(17)?, || {
            let preset = Preset::from_name(preset.as_deref().unwrap_or_default());
            Some(ProfileGraphics {
                choices: Choices::from_saved(
                    preset.unwrap_or(Preset::Medium),
                    parse_modules(modules.as_deref().unwrap_or_default()),
                ),
            })
        }),
        addons: SlotMode::from_text(&r.get::<_, String>(18)?, || {
            Some(ProfileAddons {
                on: Vec::new(),
                only_these,
            })
        }),
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
    })
}

/// A profile's add-on names, sorted; a name that is not a valid add-on is skipped.
fn addon_rows(conn: &Connection, id: &str) -> rusqlite::Result<Vec<AddonName>> {
    let mut stmt = conn.prepare(
        "SELECT addon_name FROM profile_addons WHERE profile_id = ?1 ORDER BY addon_name",
    )?;
    let names = stmt
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names.into_iter().filter_map(AddonName::new).collect())
}

/// Reads the rows of a Set crosshair or add-on slot into the profile.
fn fill_lists(conn: &Connection, profile: &mut Profile) -> rusqlite::Result<()> {
    let id = profile.id.as_str();
    if let SlotMode::Set(list) = &mut profile.crosshairs {
        *list = crosshair_rows(conn, id)?;
    }
    if let SlotMode::Set(addons) = &mut profile.addons {
        addons.on = addon_rows(conn, id)?;
    }
    Ok(())
}

/// A profile's crosshair rows by weapon script; a source v6 does not know is skipped.
fn crosshair_rows(conn: &Connection, id: &str) -> rusqlite::Result<Vec<ProfileCrosshair>> {
    let mut stmt = conn.prepare(
        "SELECT weapon_script, crosshair_source, crosshair_ref, crosshair_store_gb_id
         FROM profile_crosshair_assignments WHERE profile_id = ?1 ORDER BY weapon_script",
    )?;
    let rows = stmt.query_map([id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<i64>>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (weapon_script, source, reference, gb_id) = row?;
        let crosshair = match source.as_str() {
            SOURCE_BUILTIN => ProfileCrosshairRef::Builtin(reference),
            SOURCE_LIBRARY => ProfileCrosshairRef::Library {
                folder_name: reference,
            },
            _ => continue,
        };
        out.push(ProfileCrosshair {
            weapon_script,
            crosshair,
            store_gb_id: gb_id.and_then(|id| u64::try_from(id).ok()).map(GbId::new),
        });
    }
    Ok(out)
}

/// Reads one profile with its crosshairs; `None` when no row has the id.
pub(super) fn load_profile(conn: &Connection, id: &str) -> rusqlite::Result<Option<Profile>> {
    let profile = conn
        .query_row(
            &format!("SELECT {PROFILE_COLUMNS} FROM profiles WHERE id = ?1"),
            [id],
            profile_row,
        )
        .optional()?;
    let Some(mut profile) = profile else {
        return Ok(None);
    };
    fill_lists(conn, &mut profile)?;
    Ok(Some(profile))
}

/// What `update_profile` did: the saved profile, and whether the edit ended its active state.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileUpdate {
    profile: Profile,
    deactivated: bool,
}

impl ProfileUpdate {
    /// The profile as saved.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// True when the profile was active and the edit changed what the game would get, so
    /// it must be activated again before the game matches it.
    pub fn deactivated(&self) -> bool {
        self.deactivated
    }
}

/// One weapon's crosshair in a draft.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DraftCrosshair {
    /// Assign a crosshair to a weapon script.
    Set {
        /// The weapon script name, such as `tf_weapon_scattergun`.
        weapon_script: String,
        /// The builtin or library crosshair.
        crosshair: CrosshairRef,
    },
    /// Write a saved assignment back as it is, even when its crosshair is gone.
    Keep(ProfileCrosshair),
}

/// The hitsound and killsound of a draft's Sounds slot; `None` is TF2's default.
type DraftSounds = (Option<DraftItem<SoundId>>, Option<DraftItem<SoundId>>);

/// The contents of a profile to create or update. Every slot is written on save, and a Set
/// crosshair list replaces the profile's assignments in full.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileDraft {
    name: String,
    hud: SlotMode<DraftItem<HudId>>,
    sounds: SlotMode<DraftSounds>,
    crosshairs: SlotMode<Vec<DraftCrosshair>>,
    graphics: SlotMode<Choices>,
    addons: SlotMode<(Vec<DraftItem<AddonName>>, bool)>,
}

impl ProfileDraft {
    /// A draft that takes the HUD, sounds and crosshairs out and leaves Graphics and
    /// Add-ons alone, as a profile applied before slots had modes.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            hud: SlotMode::None,
            sounds: SlotMode::None,
            crosshairs: SlotMode::None,
            graphics: SlotMode::Leave,
            addons: SlotMode::Leave,
        }
    }

    /// A draft of a saved profile: every slot in its saved mode, each item `Keep`.
    pub fn from_profile(profile: &Profile) -> Self {
        let keep = |item: &Option<ProfileItem>| item.clone().map(DraftItem::Keep);
        Self {
            name: profile.name.clone(),
            hud: profile.hud.map_ref(|item| DraftItem::Keep(item.clone())),
            sounds: profile.sounds.map_ref(|s| (keep(&s.hit), keep(&s.kill))),
            crosshairs: profile
                .crosshairs
                .map_ref(|list| list.iter().cloned().map(DraftCrosshair::Keep).collect()),
            graphics: profile.graphics.map_ref(|g| g.choices.clone()),
            addons: profile.addons.map_ref(|a| {
                let on = a.on.iter().cloned().map(DraftItem::keep_addon).collect();
                (on, a.only_these)
            }),
        }
    }

    /// A new draft of what is installed now: a managed HUD, the managed sounds and the
    /// crosshair assignments as kept references, and Graphics as found. Add-ons are the
    /// installed ones only, unless mastercomfig is not installed, where they are left alone.
    pub fn from_current(name: impl Into<String>, setup: &CurrentSetup) -> Self {
        let keep = |item: &Option<ProfileItem>| item.clone().map(DraftItem::Keep);
        let hud = match &setup.hud {
            SlotMode::Set(item) => SlotMode::Set(DraftItem::Keep(item.clone())),
            SlotMode::None | SlotMode::Leave => SlotMode::None,
        };
        let addons = match setup.graphics {
            SlotMode::None => SlotMode::Leave,
            SlotMode::Set(_) | SlotMode::Leave => {
                let on = setup.addons.iter().cloned().map(DraftItem::keep_addon);
                SlotMode::Set((on.collect(), true))
            }
        };
        Self {
            name: name.into(),
            hud,
            sounds: SlotMode::Set((keep(&setup.sounds.hit), keep(&setup.sounds.kill))),
            crosshairs: SlotMode::Set(
                setup
                    .crosshairs
                    .iter()
                    .cloned()
                    .map(DraftCrosshair::Keep)
                    .collect(),
            ),
            graphics: setup.graphics.map_ref(|g| g.choices.clone()),
            addons,
        }
    }

    /// Replaces the name; it is trimmed when saved.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Replaces the HUD slot.
    pub fn with_hud(mut self, slot: SlotMode<DraftItem<HudId>>) -> Self {
        self.hud = slot;
        self
    }

    /// Replaces the Sounds slot; under Set, each side is a sound or `None` for TF2's default.
    pub fn with_sounds(mut self, slot: SlotMode<DraftSounds>) -> Self {
        self.sounds = slot;
        self
    }

    /// Replaces the Crosshairs slot.
    pub fn with_crosshairs(mut self, slot: SlotMode<Vec<DraftCrosshair>>) -> Self {
        self.crosshairs = slot;
        self
    }

    /// Replaces the Graphics slot.
    pub fn with_graphics(mut self, slot: SlotMode<Choices>) -> Self {
        self.graphics = slot;
        self
    }

    /// Replaces the Add-ons slot: under Set, the add-ons to turn on and whether every other
    /// one goes off.
    pub fn with_addons(mut self, slot: SlotMode<(Vec<DraftItem<AddonName>>, bool)>) -> Self {
        self.addons = slot;
        self
    }
}

/// Where one slot's columns live, which library table a `Pick` is copied from, and the
/// `source` text a GameBanana `Keep` writes, as that table's rows use it.
struct SlotSpec {
    prefix: &'static str,
    table: &'static str,
    label: &'static str,
    has_hud_id: bool,
    gb_source: &'static str,
}

const HUD_SLOT: SlotSpec = SlotSpec {
    prefix: "hud",
    table: "library_huds",
    label: "HUD",
    has_hud_id: true,
    gb_source: "store_gb",
};
const HITSOUND_SLOT: SlotSpec = SlotSpec {
    prefix: "hitsound",
    table: "library_sounds",
    label: "hitsound",
    has_hud_id: false,
    gb_source: "store",
};
const KILLSOUND_SLOT: SlotSpec = SlotSpec {
    prefix: "killsound",
    table: "library_sounds",
    label: "killsound",
    has_hud_id: false,
    gb_source: "store",
};

/// A slot's v5 columns; all `None` writes an empty slot.
#[derive(Default)]
struct SlotValues {
    folder_name: Option<String>,
    name: Option<String>,
    source: Option<String>,
    store_gb_id: Option<i64>,
    store_hud_id: Option<String>,
}

/// One `profile_crosshair_assignments` row: weapon, source, reference, GameBanana id.
type CrosshairRow = (String, &'static str, String, Option<i64>);

/// What an item slot writes: no item clears its columns, and `None` leaves them as saved,
/// for a `Keep` of the folder the slot already holds.
fn resolve_slot<T>(
    conn: &Connection,
    spec: &SlotSpec,
    item: Option<&DraftItem<T>>,
    as_str: fn(&T) -> &str,
    saved: Option<&ProfileItem>,
) -> Result<Option<SlotValues>, ProfileError> {
    let id = match item {
        None => return Ok(Some(SlotValues::default())),
        Some(DraftItem::Keep(kept)) => {
            if saved.is_some_and(|s| s.folder_name == kept.folder_name) {
                return Ok(None);
            }
            return Ok(Some(kept_values(spec, kept)));
        }
        Some(DraftItem::Pick(id)) => as_str(id),
    };
    let hud_id = if spec.has_hud_id {
        "store_hud_id"
    } else {
        "NULL"
    };
    conn.query_row(
        &format!(
            "SELECT folder_name, name, source, store_gb_id, {hud_id} FROM {} WHERE id = ?1",
            spec.table
        ),
        [id],
        |r| {
            Ok(SlotValues {
                folder_name: r.get(0)?,
                name: r.get(1)?,
                source: r.get(2)?,
                store_gb_id: r.get(3)?,
                store_hud_id: r.get(4)?,
            })
        },
    )
    .optional()
    .map_err(profile_db)?
    .map(Some)
    .ok_or_else(|| ProfileError::ItemNotFound {
        what: format!("{} {id}", spec.label),
    })
}

/// The columns a kept reference writes when the slot held another folder: its folder,
/// name and store ids, with the `source` text its provenance gives in the library.
fn kept_values(spec: &SlotSpec, kept: &ProfileItem) -> SlotValues {
    let (source, store_hud_id, store_gb_id) = match &kept.provenance {
        Provenance::GameBanana { gb_id, .. } => {
            (spec.gb_source, None, i64::try_from(gb_id.get()).ok())
        }
        Provenance::Curated { hud_id } if spec.has_hud_id => ("store", Some(hud_id.clone()), None),
        Provenance::Local | Provenance::Curated { .. } => ("local", None, None),
    };
    SlotValues {
        folder_name: Some(kept.folder_name.clone()),
        name: Some(kept.name.clone()),
        source: Some(source.to_owned()),
        store_gb_id,
        store_hud_id,
    }
}

/// The library HUD with this id, as a profile records it.
pub(super) fn hud_item(conn: &Connection, id: &HudId) -> Result<ProfileItem, ProfileError> {
    library_item(conn, &HUD_SLOT, id.as_str())
}

/// The library sound with this id, as a profile records it.
pub(super) fn sound_item(conn: &Connection, id: &SoundId) -> Result<ProfileItem, ProfileError> {
    library_item(conn, &HITSOUND_SLOT, id.as_str())
}

/// Reads a library row the way a `Pick` would save it, then back as the profile reads it.
fn library_item(conn: &Connection, spec: &SlotSpec, id: &str) -> Result<ProfileItem, ProfileError> {
    let pick = DraftItem::Pick(id);
    let values = resolve_slot(conn, spec, Some(&pick), |id: &&str| *id, None)?.unwrap_or_default();
    item(
        values.folder_name,
        values.name,
        values.store_hud_id,
        values.store_gb_id,
    )
    .ok_or_else(|| ProfileError::ItemNotFound {
        what: format!("{} {id}", spec.label),
    })
}

/// The given assignments as profile crosshair rows, sorted by weapon script.
pub(super) fn assigned_crosshairs(
    conn: &Connection,
    assignments: &[Assignment],
) -> Result<Vec<ProfileCrosshair>, ProfileError> {
    let drafts: Vec<DraftCrosshair> = assignments
        .iter()
        .map(|a| DraftCrosshair::Set {
            weapon_script: a.weapon_script().to_owned(),
            crosshair: a.crosshair().clone(),
        })
        .collect();
    let mut out: Vec<ProfileCrosshair> = resolve_crosshairs(conn, &drafts)?
        .into_iter()
        .map(
            |(weapon_script, source, reference, gb_id)| ProfileCrosshair {
                weapon_script,
                crosshair: if source == SOURCE_BUILTIN {
                    ProfileCrosshairRef::Builtin(reference)
                } else {
                    ProfileCrosshairRef::Library {
                        folder_name: reference,
                    }
                },
                store_gb_id: gb_id.and_then(|id| u64::try_from(id).ok()).map(GbId::new),
            },
        )
        .collect();
    out.sort_by(|a, b| a.weapon_script.cmp(&b.weapon_script));
    Ok(out)
}

/// The draft's add-on names, once each and sorted. A `Pick` must be in the library and not a
/// mastercomfig override, which the add-on step would copy over; a `Keep` only has to be a
/// valid add-on name.
fn resolve_addons(
    conn: &Connection,
    items: &[DraftItem<AddonName>],
) -> Result<BTreeSet<AddonName>, ProfileError> {
    let mut names = BTreeSet::new();
    for item in items {
        let name = match item {
            DraftItem::Pick(name) => {
                let known: bool = !is_override(name)
                    && conn
                        .query_row(
                            "SELECT EXISTS (SELECT 1 FROM library_addons WHERE asset_name = ?1)",
                            [name.as_str()],
                            |r| r.get(0),
                        )
                        .map_err(profile_db)?;
                if !known {
                    return Err(ProfileError::ItemNotFound {
                        what: format!("addon {}", name.as_str()),
                    });
                }
                name.clone()
            }
            DraftItem::Keep(kept) => {
                AddonName::new(kept.folder_name.as_str()).ok_or_else(|| {
                    ProfileError::ItemNotFound {
                        what: format!("addon {}", kept.folder_name),
                    }
                })?
            }
        };
        names.insert(name);
    }
    Ok(names)
}

/// The draft's crosshair rows; a library crosshair is recorded by its folder, as v5 does.
fn resolve_crosshairs(
    conn: &Connection,
    crosshairs: &[DraftCrosshair],
) -> Result<Vec<CrosshairRow>, ProfileError> {
    let mut rows = Vec::with_capacity(crosshairs.len());
    for crosshair in crosshairs {
        let row = match crosshair {
            DraftCrosshair::Set {
                weapon_script,
                crosshair: CrosshairRef::Builtin(name),
            } => (weapon_script.clone(), SOURCE_BUILTIN, name.clone(), None),
            DraftCrosshair::Set {
                weapon_script,
                crosshair: CrosshairRef::Library(id),
            } => {
                let (folder, gb_id) = conn
                    .query_row(
                        "SELECT folder_name, store_gb_id FROM library_crosshairs WHERE id = ?1",
                        [id.as_str()],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)),
                    )
                    .optional()
                    .map_err(profile_db)?
                    .ok_or_else(|| ProfileError::ItemNotFound {
                        what: format!("crosshair {id}"),
                    })?;
                (weapon_script.clone(), SOURCE_LIBRARY, folder, gb_id)
            }
            DraftCrosshair::Keep(kept) => {
                let (source, reference) = match &kept.crosshair {
                    ProfileCrosshairRef::Builtin(name) => (SOURCE_BUILTIN, name.clone()),
                    ProfileCrosshairRef::Library { folder_name } => {
                        (SOURCE_LIBRARY, folder_name.clone())
                    }
                };
                let gb_id = kept.store_gb_id.and_then(|g| i64::try_from(g.get()).ok());
                (kept.weapon_script.clone(), source, reference, gb_id)
            }
        };
        rows.push(row);
    }
    Ok(rows)
}

fn write_slot(
    conn: &Connection,
    id: &str,
    spec: &SlotSpec,
    values: &SlotValues,
) -> rusqlite::Result<()> {
    let p = spec.prefix;
    let sql = format!(
        "UPDATE profiles SET {p}_folder_name = ?2, {p}_name = ?3, {p}_source = ?4, \
         {p}_store_gb_id = ?5{} WHERE id = ?1",
        if spec.has_hud_id {
            format!(", {p}_store_hud_id = ?6")
        } else {
            String::new()
        }
    );
    let base = rusqlite::params![
        id,
        values.folder_name,
        values.name,
        values.source,
        values.store_gb_id
    ];
    if spec.has_hud_id {
        let mut all = base.to_vec();
        all.push(&values.store_hud_id);
        conn.execute(&sql, all.as_slice())?;
    } else {
        conn.execute(&sql, base)?;
    }
    Ok(())
}

/// True when applying either profile gives the game the same setup. Only what apply reads
/// counts: each slot's mode, item folders, each weapon's crosshair, the Graphics choices and
/// the add-on list, not names or store ids.
fn same_setup(a: &Profile, b: &Profile) -> bool {
    fn folder(item: &ProfileItem) -> &str {
        &item.folder_name
    }
    fn sounds(s: &ProfileSounds) -> [Option<&str>; 2] {
        [s.hit.as_ref().map(folder), s.kill.as_ref().map(folder)]
    }
    fn assignments(list: &[ProfileCrosshair]) -> Vec<(&str, &ProfileCrosshairRef)> {
        list.iter()
            .map(|c| (c.weapon_script(), c.crosshair()))
            .collect()
    }
    fn addons(a: &ProfileAddons) -> (&[AddonName], bool) {
        (&a.on, a.only_these)
    }
    a.hud.map_ref(folder) == b.hud.map_ref(folder)
        && a.sounds.map_ref(sounds) == b.sounds.map_ref(sounds)
        && a.crosshairs.map_ref(|l| assignments(l)) == b.crosshairs.map_ref(|l| assignments(l))
        && a.graphics.map_ref(|g| &g.choices) == b.graphics.map_ref(|g| &g.choices)
        && a.addons.map_ref(addons) == b.addons.map_ref(addons)
}

/// Creates or updates profile `id` from a draft in one transaction. Every item is looked
/// up before the first write, so a refused draft leaves the database unchanged.
fn save_draft(
    conn: &mut Connection,
    id: &str,
    name: &str,
    draft: &ProfileDraft,
    create: bool,
) -> Result<ProfileUpdate, ProfileError> {
    let tx = conn.transaction().map_err(profile_db)?;
    // Two cards with one name cannot be told apart, so another profile's name is refused.
    let taken: bool = tx
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM profiles WHERE name = ?1 COLLATE NOCASE AND id != ?2)",
            [name, id],
            |r| r.get(0),
        )
        .map_err(profile_db)?;
    if taken {
        return Err(ProfileError::NameTaken {
            name: name.to_owned(),
        });
    }
    // The saved profile is read first, so a `Keep` of the folder a slot holds leaves it alone.
    let before = if create {
        None
    } else {
        load_profile(&tx, id).map_err(profile_db)?
    };
    let saved_hud = before.as_ref().and_then(|b| b.hud.as_set());
    let saved_sounds = before.as_ref().and_then(|b| b.sounds.as_set());
    let (hit, kill) = draft
        .sounds
        .as_set()
        .map_or((None, None), |(hit, kill)| (hit.as_ref(), kill.as_ref()));
    let slots = [
        (
            &HUD_SLOT,
            resolve_slot(&tx, &HUD_SLOT, draft.hud.as_set(), HudId::as_str, saved_hud)?,
        ),
        (
            &HITSOUND_SLOT,
            resolve_slot(
                &tx,
                &HITSOUND_SLOT,
                hit,
                SoundId::as_str,
                saved_sounds.and_then(|s| s.hit.as_ref()),
            )?,
        ),
        (
            &KILLSOUND_SLOT,
            resolve_slot(
                &tx,
                &KILLSOUND_SLOT,
                kill,
                SoundId::as_str,
                saved_sounds.and_then(|s| s.kill.as_ref()),
            )?,
        ),
    ];
    let crosshairs = match draft.crosshairs.as_set() {
        Some(list) => resolve_crosshairs(&tx, list)?,
        None => Vec::new(),
    };
    let (addons, only_these) = match draft.addons.as_set() {
        Some((items, only_these)) => (resolve_addons(&tx, items)?, *only_these),
        None => (BTreeSet::new(), false),
    };
    let graphics = draft
        .graphics
        .as_set()
        .map(|choices| (choices.preset().name(), modules_json(choices)));

    let stamp = now();
    let written = if create {
        tx.execute(
            "INSERT INTO profiles (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
            [id, name, &stamp],
        )
    } else {
        tx.execute(
            "UPDATE profiles SET name = ?2, updated_at = ?3 WHERE id = ?1",
            [id, name, &stamp],
        )
    }
    .map_err(profile_db)?;
    if written == 0 {
        return Err(not_found(&ProfileId::new(id)));
    }
    // Every mode is written on each save; a slot that is not Set keeps no content.
    tx.execute(
        "UPDATE profiles SET hud_mode = ?2, sounds_mode = ?3, crosshairs_mode = ?4,
             graphics_mode = ?5, addons_mode = ?6, graphics_preset = ?7,
             graphics_modules = ?8, addons_only = ?9
         WHERE id = ?1",
        rusqlite::params![
            id,
            draft.hud.mode_text(),
            draft.sounds.mode_text(),
            draft.crosshairs.mode_text(),
            draft.graphics.mode_text(),
            draft.addons.mode_text(),
            graphics.as_ref().map(|(preset, _)| *preset),
            graphics.as_ref().map(|(_, modules)| modules),
            only_these,
        ],
    )
    .map_err(profile_db)?;
    for (spec, values) in &slots {
        if let Some(values) = values {
            write_slot(&tx, id, spec, values).map_err(profile_db)?;
        }
    }
    tx.execute("DELETE FROM profile_addons WHERE profile_id = ?1", [id])
        .map_err(profile_db)?;
    for name in &addons {
        tx.execute(
            "INSERT OR IGNORE INTO profile_addons (profile_id, addon_name) VALUES (?1, ?2)",
            [id, name.as_str()],
        )
        .map_err(profile_db)?;
    }
    // Only the sources v6 can read and rewrite are cleared; a row of any other source
    // survives unless the draft sets its weapon, which replaces it below.
    tx.execute(
        "DELETE FROM profile_crosshair_assignments
         WHERE profile_id = ?1 AND crosshair_source IN (?2, ?3)",
        rusqlite::params![id, SOURCE_BUILTIN, SOURCE_LIBRARY],
    )
    .map_err(profile_db)?;
    for (weapon_script, source, reference, gb_id) in &crosshairs {
        // A weapon listed twice keeps its last assignment.
        tx.execute(
            "INSERT OR REPLACE INTO profile_crosshair_assignments
                 (profile_id, weapon_script, crosshair_source, crosshair_ref, crosshair_store_gb_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, weapon_script, source, reference, gb_id],
        )
        .map_err(profile_db)?;
    }
    let profile = load_profile(&tx, id)
        .map_err(profile_db)?
        .ok_or_else(|| not_found(&ProfileId::new(id)))?;
    // Active means the game matches the profile, so an edit the game would see ends it here,
    // in the save's transaction. Applying stays with the activate flow and its dialogs.
    let deactivated = before.is_some_and(|before| !same_setup(&before, &profile))
        && read_setting(&tx, ACTIVE_PROFILE_KEY)
            .map_err(profile_db)?
            .as_deref()
            == Some(id);
    if deactivated {
        save_setting(&tx, ACTIVE_PROFILE_KEY, "").map_err(profile_db)?;
    }
    tx.commit().map_err(profile_db)?;
    Ok(ProfileUpdate {
        profile,
        deactivated,
    })
}

/// The trimmed name, after checking it and, under a Set crosshair slot, every assigned
/// weapon script.
pub(super) fn validated_name(draft: &ProfileDraft) -> Result<&str, ProfileError> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err(ProfileError::EmptyName);
    }
    for crosshair in draft.crosshairs.as_set().into_iter().flatten() {
        if let DraftCrosshair::Set { weapon_script, .. } = crosshair
            && !is_weapon_script(weapon_script)
        {
            return Err(ProfileError::InvalidWeaponScript {
                name: weapon_script.clone(),
            });
        }
    }
    Ok(name)
}

/// Which slot of a profile a missing item fills.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MissingKind {
    /// The HUD slot.
    Hud,
    /// The hitsound slot.
    Hitsound,
    /// The killsound slot.
    Killsound,
    /// One weapon's crosshair.
    Crosshair {
        /// The weapon script the crosshair is assigned to.
        weapon_script: String,
    },
    /// One ticked add-on of the Add-ons slot.
    Addon {
        /// The add-on's asset name.
        name: AddonName,
    },
}

/// A profile reference that resolves to nothing in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MissingItem {
    kind: MissingKind,
    name: String,
    provenance: Provenance,
}

impl MissingItem {
    /// The slot the item fills.
    pub fn kind(&self) -> &MissingKind {
        &self.kind
    }

    /// The recorded display name; for a crosshair, its reference.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where the item came from, which is where it can be downloaded again.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// A profile with every reference resolved against the library, per step: Leave runs no
/// step, None uninstalls, and a missing item reads as None (its step is skipped anyway).
/// Set crosshairs keep every row, a resolved one under its new folder. `profile` is the
/// row as re-read after the rewrites.
pub(super) struct Resolved {
    pub(super) hud: SlotMode<HudId>,
    pub(super) hitsound: SlotMode<SoundId>,
    pub(super) killsound: SlotMode<SoundId>,
    pub(super) crosshairs: SlotMode<Vec<ProfileCrosshair>>,
    pub(super) missing: Vec<MissingItem>,
    pub(super) profile: Profile,
}

/// The library id of the row with `folder`, when that row exists and its folder is a
/// directory on disk.
fn row_on_disk(
    conn: &Connection,
    dir: &DataDir,
    kind: LibraryKind,
    table: &str,
    folder: &str,
) -> rusqlite::Result<Option<String>> {
    let id: Option<String> = conn
        .query_row(
            &format!("SELECT id FROM {table} WHERE folder_name = ?1"),
            [folder],
            |r| r.get(0),
        )
        .optional()?;
    Ok(id.filter(|_| dir.library_dir(kind).join(folder).is_dir()))
}

/// The newest row sharing the item's store id whose folder is on disk, as (id, folder).
/// A local item has no store id, so it never matches.
fn by_provenance(
    conn: &Connection,
    dir: &DataDir,
    kind: LibraryKind,
    table: &str,
    provenance: &Provenance,
) -> rusqlite::Result<Option<(String, String)>> {
    let (column, value): (&str, rusqlite::types::Value) = match provenance {
        Provenance::GameBanana { gb_id, .. } => match i64::try_from(gb_id.get()) {
            Ok(id) => ("store_gb_id", id.into()),
            Err(_) => return Ok(None),
        },
        Provenance::Curated { hud_id } if kind == LibraryKind::Huds => {
            ("store_hud_id", hud_id.clone().into())
        }
        Provenance::Local | Provenance::Curated { .. } => return Ok(None),
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT id, folder_name FROM {table} WHERE {column} = ?1 \
         ORDER BY created_at DESC, folder_name ASC"
    ))?;
    let rows = stmt.query_map([value], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?;
    for row in rows {
        let (id, folder): (String, String) = row?;
        if dir.library_dir(kind).join(&folder).is_dir() {
            return Ok(Some((id, folder)));
        }
    }
    Ok(None)
}

/// Resolves one item: by its folder, else by provenance. `Ok(Some((id, Some(folder))))`
/// means a provenance match the caller must write back.
fn resolve_item(
    conn: &Connection,
    dir: &DataDir,
    kind: LibraryKind,
    table: &str,
    folder: &str,
    provenance: &Provenance,
) -> rusqlite::Result<Option<(String, Option<String>)>> {
    if let Some(id) = row_on_disk(conn, dir, kind, table, folder)? {
        return Ok(Some((id, None)));
    }
    Ok(by_provenance(conn, dir, kind, table, provenance)?.map(|(id, f)| (id, Some(f))))
}

/// Why a profile could not be resolved: its row is gone, or SQLite failed.
pub(super) enum ResolveError {
    NotFound(String),
    Database(String),
}

fn resolve_db(e: rusqlite::Error) -> ResolveError {
    ResolveError::Database(e.to_string())
}

impl From<ResolveError> for ProfileError {
    fn from(e: ResolveError) -> Self {
        match e {
            ResolveError::NotFound(id) => ProfileError::NotFound { id },
            ResolveError::Database(message) => ProfileError::Database { message },
        }
    }
}

impl From<ResolveError> for ApplyProfileError {
    fn from(e: ResolveError) -> Self {
        match e {
            ResolveError::NotFound(id) => ApplyProfileError::NotFound { id },
            ResolveError::Database(message) => ApplyProfileError::Database { message },
        }
    }
}

/// Resolves every reference of profile `id` in one transaction, writing provenance matches
/// back to the profile and stamping it once if any was written. With `builtins`, a builtin
/// crosshair the catalog lacks is missing.
pub(super) fn resolve_profile(
    conn: &mut Connection,
    dir: &DataDir,
    id: &ProfileId,
    builtins: Option<&BuiltinCrosshairs>,
) -> Result<Resolved, ResolveError> {
    let tx = conn.transaction().map_err(resolve_db)?;
    let profile = load_profile(&tx, id.as_str())
        .map_err(resolve_db)?
        .ok_or_else(|| ResolveError::NotFound(id.to_string()))?;
    let stamp = now();
    let mut missing = Vec::new();
    let mut rewritten = false;

    // Each item slot resolves to a library id, rewriting its folder column on a
    // provenance match, or joins the missing list.
    let mut slot = |item: Option<&ProfileItem>,
                    spec: &SlotSpec,
                    kind: LibraryKind,
                    missing_kind: MissingKind|
     -> Result<Option<String>, ResolveError> {
        let Some(item) = item else {
            return Ok(None);
        };
        match resolve_item(
            &tx,
            dir,
            kind,
            spec.table,
            &item.folder_name,
            &item.provenance,
        )
        .map_err(resolve_db)?
        {
            Some((lib_id, rewrite)) => {
                if let Some(folder) = rewrite {
                    tx.execute(
                        &format!(
                            "UPDATE profiles SET {}_folder_name = ?2 WHERE id = ?1",
                            spec.prefix
                        ),
                        [id.as_str(), &folder],
                    )
                    .map_err(resolve_db)?;
                    rewritten = true;
                }
                Ok(Some(lib_id))
            }
            None => {
                missing.push(MissingItem {
                    kind: missing_kind,
                    name: item.name.clone(),
                    provenance: item.provenance.clone(),
                });
                Ok(None)
            }
        }
    };
    // Only Set content is resolved; Leave and None pass through as the step to run.
    // A Set sound side left empty means TF2's default, so that side uninstalls.
    let found = |id: Option<String>| id.map_or(SlotMode::None, SlotMode::Set);
    let hud = match &profile.hud {
        SlotMode::Set(item) => found(slot(
            Some(item),
            &HUD_SLOT,
            LibraryKind::Huds,
            MissingKind::Hud,
        )?),
        SlotMode::Leave => SlotMode::Leave,
        SlotMode::None => SlotMode::None,
    };
    let (hitsound, killsound) = match &profile.sounds {
        SlotMode::Set(sounds) => (
            found(slot(
                sounds.hit.as_ref(),
                &HITSOUND_SLOT,
                LibraryKind::Sounds,
                MissingKind::Hitsound,
            )?),
            found(slot(
                sounds.kill.as_ref(),
                &KILLSOUND_SLOT,
                LibraryKind::Sounds,
                MissingKind::Killsound,
            )?),
        ),
        SlotMode::Leave => (SlotMode::Leave, SlotMode::Leave),
        SlotMode::None => (SlotMode::None, SlotMode::None),
    };

    let crosshairs_mode = profile.crosshairs.map_ref(|_| ());
    let saved = match profile.crosshairs {
        SlotMode::Set(list) => list,
        SlotMode::Leave | SlotMode::None => Vec::new(),
    };
    let mut crosshairs = Vec::with_capacity(saved.len());
    for mut crosshair in saved {
        let provenance =
            crosshair
                .store_gb_id
                .map_or(Provenance::Local, |gb_id| Provenance::GameBanana {
                    gb_id,
                    author: String::new(),
                });
        let (reference, found) = match crosshair.crosshair.clone() {
            ProfileCrosshairRef::Builtin(name) => {
                let known = builtins.is_none_or(|catalog| catalog.get(&name).is_some());
                (name, known)
            }
            ProfileCrosshairRef::Library { folder_name } => {
                let resolved = resolve_item(
                    &tx,
                    dir,
                    LibraryKind::Crosshairs,
                    "library_crosshairs",
                    &folder_name,
                    &provenance,
                )
                .map_err(resolve_db)?;
                if let Some((_, Some(folder))) = &resolved {
                    tx.execute(
                        "UPDATE profile_crosshair_assignments SET crosshair_ref = ?3
                         WHERE profile_id = ?1 AND weapon_script = ?2",
                        [id.as_str(), &crosshair.weapon_script, folder],
                    )
                    .map_err(resolve_db)?;
                    rewritten = true;
                    crosshair.crosshair = ProfileCrosshairRef::Library {
                        folder_name: folder.clone(),
                    };
                }
                (folder_name, resolved.is_some())
            }
        };
        if !found {
            missing.push(MissingItem {
                kind: MissingKind::Crosshair {
                    weapon_script: crosshair.weapon_script.clone(),
                },
                name: reference,
                provenance,
            });
        }
        crosshairs.push(crosshair);
    }
    // A ticked add-on needs its library row and its VPK in the library.
    if let SlotMode::Set(addons) = &profile.addons {
        for name in &addons.on {
            let recorded: bool = tx
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM library_addons WHERE asset_name = ?1)",
                    [name.as_str()],
                    |r| r.get(0),
                )
                .map_err(resolve_db)?;
            let on_disk = dir
                .library_dir(LibraryKind::Addons)
                .join(name.as_str())
                .is_file();
            if !(recorded && on_disk) {
                missing.push(MissingItem {
                    kind: MissingKind::Addon { name: name.clone() },
                    name: display_info(name).0,
                    provenance: Provenance::Local,
                });
            }
        }
    }
    // However many references were rewritten, the profile changed once.
    if rewritten {
        tx.execute(
            "UPDATE profiles SET updated_at = ?2 WHERE id = ?1",
            [id.as_str(), &stamp],
        )
        .map_err(resolve_db)?;
    }
    let profile = load_profile(&tx, id.as_str())
        .map_err(resolve_db)?
        .ok_or_else(|| ResolveError::NotFound(id.to_string()))?;
    tx.commit().map_err(resolve_db)?;
    Ok(Resolved {
        hud: hud.map_ref(|id| HudId::new(id.clone())),
        hitsound: hitsound.map_ref(|id| SoundId::new(id.clone())),
        killsound: killsound.map_ref(|id| SoundId::new(id.clone())),
        crosshairs: crosshairs_mode.map_ref(|()| crosshairs),
        missing,
        profile,
    })
}

/// One step of applying a profile, in the order the steps run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ApplyStep {
    /// Installing the HUD, or uninstalling the current one.
    Hud,
    /// Installing the hitsound, or uninstalling the current one.
    Hitsound,
    /// Installing the killsound, or uninstalling the current one.
    Killsound,
    /// Replacing the weapon assignments and rebuilding the crosshair folder.
    Crosshairs,
    /// Writing the mastercomfig preset, or removing a managed mastercomfig.
    Graphics,
    /// Turning library add-ons on and off.
    Addons,
}

/// What applying a profile did: the steps that succeeded, the missing items it skipped
/// and each failed step with its error message.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApplyProfileReport {
    profile: Profile,
    applied: Vec<ApplyStep>,
    skipped: Vec<MissingItem>,
    failed: Vec<(ApplyStep, String)>,
    skipped_steps: Vec<(ApplyStep, ApplySkipReason)>,
    pending_modules: Option<Choices>,
    binds_restored: bool,
    movement: Option<MovementKeys>,
    movement_off: bool,
    hud_blocked_name: Option<String>,
}

impl ApplyProfileReport {
    /// The applied profile as saved once its references were resolved and rewritten, so
    /// `Library::mark_profile_active` accepts it.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// The steps that succeeded, in run order.
    pub fn applied(&self) -> &[ApplyStep] {
        &self.applied
    }

    /// The missing items left out of the apply.
    pub fn skipped(&self) -> &[MissingItem] {
        &self.skipped
    }

    /// The steps that failed, each with its error's message.
    pub fn failed(&self) -> &[(ApplyStep, String)] {
        &self.failed
    }

    /// The name the profile's HUD expected when a player's folder or file held it, so the
    /// HUD went under its library folder name and parts of it may not show.
    pub fn hud_blocked_name(&self) -> Option<&str> {
        self.hud_blocked_name.as_deref()
    }

    /// The steps that did not run although their slot asked for a change, each with why.
    pub fn skipped_steps(&self) -> &[(ApplyStep, ApplySkipReason)] {
        &self.skipped_steps
    }

    /// The module choices the app still has to apply: Some under Graphics Set once the
    /// preset step succeeded. The profile is marked active only after the app applies them.
    pub fn pending_modules(&self) -> Option<&Choices> {
        self.pending_modules.as_ref()
    }

    /// Whether removing mastercomfig put `config.cfg`'s movement binds back on the stock
    /// commands.
    pub fn binds_restored(&self) -> bool {
        self.binds_restored
    }

    /// The movement keys `addons.cfg` binds when the Add-ons step turned Null-Canceling
    /// Movement on.
    pub fn movement(&self) -> Option<&MovementKeys> {
        self.movement.as_ref()
    }

    /// Whether the Add-ons step turned Null-Canceling Movement off, which puts the movement
    /// keys back on the stock commands.
    pub fn movement_off(&self) -> bool {
        self.movement_off
    }
}

fn apply_db(e: rusqlite::Error) -> ApplyProfileError {
    ApplyProfileError::Database {
        message: e.to_string(),
    }
}

fn not_found(id: &ProfileId) -> ProfileError {
    ProfileError::NotFound { id: id.to_string() }
}

impl Library {
    /// Every profile, ordered by name, then id.
    pub fn profiles(&self) -> Result<Vec<Profile>, ProfileError> {
        self.db
            .read(|conn| {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {PROFILE_COLUMNS} FROM profiles ORDER BY name, id"
                ))?;
                let mut profiles = stmt
                    .query_map([], profile_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                for profile in &mut profiles {
                    fill_lists(conn, profile)?;
                }
                Ok(profiles)
            })
            .map_err(profile_db)
    }

    /// One profile by id; an unknown id is `NotFound`.
    pub fn profile(&self, id: &ProfileId) -> Result<Profile, ProfileError> {
        self.db
            .read(|conn| load_profile(conn, id.as_str()))
            .map_err(profile_db)?
            .ok_or_else(|| not_found(id))
    }

    /// Creates a profile from a draft, with a new id; a `Keep` item is written as recorded.
    /// Fails with `EmptyName`, `NameTaken` (another profile's name, any case),
    /// `InvalidWeaponScript` or `ItemNotFound` before writing.
    pub fn create_profile(&self, draft: &ProfileDraft) -> Result<Profile, ProfileError> {
        let name = validated_name(draft)?;
        let id = new_id();
        self.db
            .write(|conn| Ok(save_draft(conn, &id, name, draft, true)))
            .map_err(profile_db)?
            .map(|saved| saved.profile)
    }

    /// Rewrites a profile from a draft: a `Keep` of the folder a slot holds leaves its columns
    /// as saved, and a Set crosshair list replaces the old builtin and library rows; a row of
    /// another source stays unless the list sets its weapon. An edit of the active profile
    /// that changes what the game would get ends its active state; a rename does not, and no
    /// game file is touched either way.
    /// Fails like `create_profile`, or with `NotFound`.
    pub fn update_profile(
        &self,
        id: &ProfileId,
        draft: &ProfileDraft,
    ) -> Result<ProfileUpdate, ProfileError> {
        let name = validated_name(draft)?;
        self.db
            .write(|conn| Ok(save_draft(conn, id.as_str(), name, draft, false)))
            .map_err(profile_db)?
    }

    /// Deletes a profile and its crosshair rows, and clears the active id when it named
    /// this profile. Installed files are left alone. An unknown id is `NotFound`.
    pub fn delete_profile(&self, id: &ProfileId) -> Result<(), ProfileError> {
        let deleted = self
            .db
            .write(|conn| {
                // The writer enforces foreign keys, so the crosshair rows cascade.
                let tx = conn.transaction()?;
                let deleted = tx.execute("DELETE FROM profiles WHERE id = ?1", [id.as_str()])?;
                if read_setting(&tx, ACTIVE_PROFILE_KEY)?.as_deref() == Some(id.as_str()) {
                    save_setting(&tx, ACTIVE_PROFILE_KEY, "")?;
                }
                tx.commit()?;
                Ok(deleted)
            })
            .map_err(profile_db)?;
        if deleted == 0 {
            return Err(not_found(id));
        }
        Ok(())
    }

    /// Resolves every library reference of the profile (folder on disk, then provenance,
    /// rewriting the folder on a provenance match) and returns those that do not resolve,
    /// in slot order: HUD, hitsound, killsound, crosshairs by weapon script, then add-ons.
    /// Builtin crosshairs are not checked here. An unknown id is `NotFound`.
    pub fn missing_items(&self, id: &ProfileId) -> Result<Vec<MissingItem>, ProfileError> {
        self.db
            .write(|conn| Ok(resolve_profile(conn, &self.dir, id, None)))
            .map_err(profile_db)?
            .map(|resolved| resolved.missing)
            .map_err(ProfileError::from)
    }

    /// `missing_items` as an apply sees them: a builtin crosshair `builtins` lacks is missing
    /// too. An unknown id is `NotFound`.
    pub fn missing_items_for_apply(
        &self,
        id: &ProfileId,
        builtins: &BuiltinCrosshairs,
    ) -> Result<Vec<MissingItem>, ProfileError> {
        self.db
            .write(|conn| Ok(resolve_profile(conn, &self.dir, id, Some(builtins))))
            .map_err(profile_db)?
            .map(|resolved| resolved.missing)
            .map_err(ProfileError::from)
    }

    /// Applies a profile to `tf2`: resolves it (a builtin the catalog lacks is missing),
    /// then, slot by slot, installs a Set item, uninstalls a None one and leaves a Leave one
    /// alone, best effort. Missing items fail with `Missing` before any file is touched unless
    /// `skip_missing`, which leaves what is installed for those slots alone; unless the HUD is
    /// skipped or left, a HUD the library cannot put back then fails with `ExternalHuds` the
    /// same way. Graphics runs next, then Add-ons last; see `ApplySkipReason` and `pending_modules`.
    /// The profile becomes active when no step failed, no step was skipped for a reason and no
    /// module choices are pending; missing items skipped by `skip_missing` do not prevent it.
    pub fn apply_profile(
        &self,
        id: &ProfileId,
        tf2: &Tf2Install,
        builtins: &BuiltinCrosshairs,
        skip_missing: bool,
    ) -> Result<ApplyProfileReport, ApplyProfileError> {
        self.apply_profile_discarding(id, tf2, builtins, skip_missing, &[])
    }

    /// `apply_profile`, except that the HUD step may remove `discard`. It fails with
    /// `ExternalHuds`, before any step runs, when any other HUD the library cannot put back
    /// is in `tf/custom`.
    pub fn apply_profile_discarding(
        &self,
        id: &ProfileId,
        tf2: &Tf2Install,
        builtins: &BuiltinCrosshairs,
        skip_missing: bool,
        discard: &[ExternalHud],
    ) -> Result<ApplyProfileReport, ApplyProfileError> {
        let resolved = self
            .db
            .write(|conn| Ok(resolve_profile(conn, &self.dir, id, Some(builtins))))
            .map_err(apply_db)?
            .map_err(ApplyProfileError::from)?;
        if !resolved.missing.is_empty() && !skip_missing {
            return Err(ApplyProfileError::Missing(resolved.missing));
        }
        // Asked before any step, so a refused apply leaves sounds and crosshairs alone too.
        // A skipped HUD removes nothing, so there is nothing to ask about.
        // A HUD left as is removes nothing either.
        let skipped = |kind: MissingKind| resolved.missing.iter().any(|m| m.kind == kind);
        let run_hud = !matches!(resolved.hud, SlotMode::Leave) && !skipped(MissingKind::Hud);
        // Held from the external check through the HUD step, so no copy changes between them.
        let hud_guards = run_hud.then(|| (super::live_link::hud_swap_lock(), self.copies_read()));
        if run_hud {
            let unnamed =
                self.unnamed_huds(&game_custom_dir(tf2)?, discard)
                    .map_err(|e| match e {
                        LibraryError::Io { path, source } => ApplyProfileError::Io { path, source },
                        other @ LibraryError::NotFound { .. }
                        | other @ LibraryError::InvalidWeaponScript { .. }
                        | other @ LibraryError::Vpk(_)
                        | other @ LibraryError::Vtf(_)
                        | other @ LibraryError::Database { .. } => ApplyProfileError::Database {
                            message: other.to_string(),
                        },
                    })?;
            if !unnamed.is_empty() {
                return Err(ApplyProfileError::ExternalHuds(unnamed));
            }
        }

        // Cleared up front so a partial apply never leaves this profile marked active;
        // the crosshair step and a run where every step fails clear nothing themselves.
        self.db
            .write(|conn| save_setting(conn, ACTIVE_PROFILE_KEY, ""))
            .map_err(apply_db)?;

        let mut applied = Vec::new();
        let mut failed = Vec::new();
        let mut record = |step, result: Result<(), String>| match result {
            Ok(()) => applied.push(step),
            Err(message) => failed.push((step, message)),
        };
        // A skipped or Leave slot runs no step, so whatever is installed there stays; a
        // None slot still uninstalls, since the profile asks for nothing there.
        let mut hud_blocked_name = None;
        if run_hud {
            // Leave runs nothing here as well, so no change to `run_hud` can uninstall for it.
            let hud = match &resolved.hud {
                SlotMode::Set(hud) => Some(
                    self.install_hud_discarding_locked(hud, tf2, discard)
                        .map(|done| {
                            hud_blocked_name = done.blocked_name().map(str::to_owned);
                        }),
                ),
                SlotMode::None => Some(self.uninstall_hud_discarding_locked(tf2, discard)),
                SlotMode::Leave => None,
            };
            if let Some(hud) = hud {
                record(ApplyStep::Hud, hud.map_err(|e| e.to_string()));
            }
        }
        drop(hud_guards);
        for (step, kind, missing, sound) in [
            (
                ApplyStep::Hitsound,
                SoundKind::Hitsound,
                MissingKind::Hitsound,
                &resolved.hitsound,
            ),
            (
                ApplyStep::Killsound,
                SoundKind::Killsound,
                MissingKind::Killsound,
                &resolved.killsound,
            ),
        ] {
            if skipped(missing) {
                continue;
            }
            let result = match sound {
                SlotMode::Leave => continue,
                SlotMode::Set(sound) => self.install_sound(sound, kind, tf2),
                SlotMode::None => self.uninstall_sound(kind, tf2),
            };
            record(step, result.map_err(|e| e.to_string()));
        }
        // None passes an empty list, which clears every weapon.
        let crosshairs = match &resolved.crosshairs {
            SlotMode::Set(list) => Some(list.as_slice()),
            SlotMode::None => Some(&[][..]),
            SlotMode::Leave => None,
        };
        if let Some(crosshairs) = crosshairs {
            record(
                ApplyStep::Crosshairs,
                self.apply_profile_crosshairs(crosshairs, &resolved.missing, tf2, builtins),
            );
        }

        let graphics = self.apply_profile_graphics(resolved.profile.graphics(), tf2);
        if let Some(outcome) = graphics.outcome {
            record(ApplyStep::Graphics, outcome);
        }
        // Add-ons run after Graphics, so their turn-ons see mastercomfig as it now stands.
        let addons = self.apply_profile_addons(resolved.profile.addons(), &resolved.missing, tf2);
        if addons.ran && addons.failures.is_empty() && addons.skip.is_none() {
            applied.push(ApplyStep::Addons);
        }
        failed.extend(addons.failures.into_iter().map(|e| (ApplyStep::Addons, e)));
        let skipped_steps: Vec<_> = [
            graphics.skip.map(|reason| (ApplyStep::Graphics, reason)),
            addons.skip.map(|reason| (ApplyStep::Addons, reason)),
        ]
        .into_iter()
        .flatten()
        .collect();

        // Missing items the caller chose to skip do not block the mark; the report names them.
        // Pending module choices still have to reach the game, so the app marks those later.
        let matches_game =
            failed.is_empty() && skipped_steps.is_empty() && graphics.pending_modules.is_none();
        if matches_game {
            self.db
                .write(|conn| save_setting(conn, ACTIVE_PROFILE_KEY, id.as_str()))
                .map_err(apply_db)?;
        }
        Ok(ApplyProfileReport {
            profile: resolved.profile,
            applied,
            skipped: resolved.missing,
            failed,
            skipped_steps,
            pending_modules: graphics.pending_modules,
            binds_restored: graphics.binds_restored,
            movement: addons.movement,
            movement_off: addons.movement_off,
            hud_blocked_name,
        })
    }

    /// The crosshair step: the profile's rows replace the assignment table in one
    /// transaction, except that a weapon whose crosshair is missing keeps the assignment
    /// it had; then the managed folder is rebuilt from the table.
    fn apply_profile_crosshairs(
        &self,
        crosshairs: &[ProfileCrosshair],
        missing: &[MissingItem],
        tf2: &Tf2Install,
        builtins: &BuiltinCrosshairs,
    ) -> Result<(), String> {
        let skipped: Vec<&str> = missing
            .iter()
            .filter_map(|m| match &m.kind {
                MissingKind::Crosshair { weapon_script } => Some(weapon_script.as_str()),
                MissingKind::Hud
                | MissingKind::Hitsound
                | MissingKind::Killsound
                | MissingKind::Addon { .. } => None,
            })
            .collect();
        let remaining = self
            .db
            .write(|conn| {
                let tx = conn.transaction()?;
                // Clear every weapon but the skipped ones, which keep their old row.
                let assigned: Vec<String> = {
                    let mut stmt = tx.prepare("SELECT weapon_script FROM crosshair_assignments")?;
                    stmt.query_map([], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?
                };
                for weapon in assigned.iter().filter(|w| !skipped.contains(&w.as_str())) {
                    tx.execute(
                        "DELETE FROM crosshair_assignments WHERE weapon_script = ?1",
                        [weapon],
                    )?;
                }
                for crosshair in crosshairs {
                    if skipped.contains(&crosshair.weapon_script.as_str()) {
                        continue;
                    }
                    let (source, reference) = match &crosshair.crosshair {
                        ProfileCrosshairRef::Builtin(name) => (SOURCE_BUILTIN, name),
                        ProfileCrosshairRef::Library { folder_name } => {
                            (SOURCE_LIBRARY, folder_name)
                        }
                    };
                    tx.execute(
                        "INSERT OR REPLACE INTO crosshair_assignments
                             (weapon_script, crosshair_source, crosshair_ref)
                         VALUES (?1, ?2, ?3)",
                        [crosshair.weapon_script.as_str(), source, reference.as_str()],
                    )?;
                }
                let remaining: i64 =
                    tx.query_row("SELECT COUNT(*) FROM crosshair_assignments", [], |r| {
                        r.get(0)
                    })?;
                tx.commit()?;
                Ok(remaining)
            })
            .map_err(|e| format!("crosshair assignments: {e}"))?;
        if remaining == 0 {
            self.remove_crosshairs(tf2)
        } else {
            self.apply_crosshairs(tf2, builtins).map(drop)
        }
        .map_err(|e| e.to_string())
    }

    /// Marks `profile` active only while its saved `updated_at` equals `profile.updated_at()`:
    /// `Ok(true)` when marked, `Ok(false)` when it was edited since, `NotFound` when gone.
    pub fn mark_profile_active(&self, profile: &Profile) -> Result<bool, ProfileError> {
        let marked = self
            .db
            .write(|conn| {
                let tx = conn.transaction()?;
                let saved: Option<String> = tx
                    .query_row(
                        "SELECT updated_at FROM profiles WHERE id = ?1",
                        [profile.id.as_str()],
                        |r| r.get(0),
                    )
                    .optional()?;
                let marked = saved.map(|at| at == profile.updated_at);
                if marked == Some(true) {
                    save_setting(&tx, ACTIVE_PROFILE_KEY, profile.id.as_str())?;
                }
                tx.commit()?;
                Ok(marked)
            })
            .map_err(profile_db)?;
        marked.ok_or_else(|| not_found(&profile.id))
    }

    /// The profile last applied with no failed or skipped step, when its row still exists.
    pub fn active_profile(&self) -> Result<Option<ProfileId>, ProfileError> {
        self.db
            .read(|conn| {
                let Some(id) = read_setting(conn, ACTIVE_PROFILE_KEY)? else {
                    return Ok(None);
                };
                let exists: bool = conn.query_row(
                    "SELECT EXISTS (SELECT 1 FROM profiles WHERE id = ?1)",
                    [&id],
                    |r| r.get(0),
                )?;
                Ok(exists.then(|| ProfileId::new(id)))
            })
            .map_err(profile_db)
    }
}
