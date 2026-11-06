//! Profiles: named sets of a HUD, a hitsound, a killsound and per-weapon crosshairs, read
//! from v5's `profiles` and `profile_crosshair_assignments` rows. A profile refers to its
//! items by library folder name, so a reference can outlive the item it names.

use std::fmt;

use rusqlite::{Connection, OptionalExtension, Row};

use super::crosshairs::{SOURCE_BUILTIN, SOURCE_LIBRARY, is_weapon_script};
use super::{
    CrosshairRef, ExternalHud, GbId, HudId, HudStatus, Library, Provenance, SoundId, SoundKind,
    game_custom_dir, new_id, now, profile_db, read_setting, save_setting,
};
use crate::crosshair_catalog::BuiltinCrosshairs;
use crate::error::{ApplyProfileError, LibraryError, ProfileError};
use crate::paths::{DataDir, LibraryKind};
use crate::tf2::Tf2Install;

/// The settings key v5 keeps the applied profile's id under.
const ACTIVE_PROFILE_KEY: &str = "active_profile_id";

const PROFILE_COLUMNS: &str = "id, name, hud_folder_name, hud_name, hud_store_hud_id, \
     hud_store_gb_id, hitsound_folder_name, hitsound_name, hitsound_store_gb_id, \
     killsound_folder_name, killsound_name, killsound_store_gb_id, created_at, updated_at";

text_id!(
    /// A profile id, v5's TEXT format (a v4 UUID for new ones).
    ProfileId
);

/// One item slot of a profile, as recorded: the library folder it names, its display
/// name and where it came from. The folder may no longer be in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileItem {
    folder_name: String,
    name: String,
    provenance: Provenance,
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
    hud: Option<ProfileItem>,
    hitsound: Option<ProfileItem>,
    killsound: Option<ProfileItem>,
    crosshairs: Vec<ProfileCrosshair>,
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

    /// The HUD slot, `None` when the profile leaves it empty.
    pub fn hud(&self) -> Option<&ProfileItem> {
        self.hud.as_ref()
    }

    /// The hitsound slot, `None` when the profile leaves it empty.
    pub fn hitsound(&self) -> Option<&ProfileItem> {
        self.hitsound.as_ref()
    }

    /// The killsound slot, `None` when the profile leaves it empty.
    pub fn killsound(&self) -> Option<&ProfileItem> {
        self.killsound.as_ref()
    }

    /// The per-weapon crosshairs, sorted by weapon script.
    pub fn crosshairs(&self) -> &[ProfileCrosshair] {
        &self.crosshairs
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

/// Maps one `profiles` row selected with `PROFILE_COLUMNS`; crosshairs are filled later.
fn profile_row(r: &Row<'_>) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: ProfileId::new(r.get::<_, String>(0)?),
        name: r.get(1)?,
        hud: item(r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?),
        hitsound: item(r.get(6)?, r.get(7)?, None, r.get(8)?),
        killsound: item(r.get(9)?, r.get(10)?, None, r.get(11)?),
        crosshairs: Vec::new(),
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
    })
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
    profile.crosshairs = crosshair_rows(conn, id)?;
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

/// One item slot of a draft.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DraftSlot<T> {
    /// No item; the slot's columns are cleared.
    Empty,
    /// The library item with this id.
    Item(T),
    /// The saved profile's slot, untouched; on create, an empty slot.
    Keep,
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

/// The contents of a profile to create or update. The crosshair list replaces the
/// profile's assignments in full.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProfileDraft {
    name: String,
    hud: DraftSlot<HudId>,
    hitsound: DraftSlot<SoundId>,
    killsound: DraftSlot<SoundId>,
    crosshairs: Vec<DraftCrosshair>,
}

impl ProfileDraft {
    /// A draft with every slot `Empty` and no crosshairs.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            hud: DraftSlot::Empty,
            hitsound: DraftSlot::Empty,
            killsound: DraftSlot::Empty,
            crosshairs: Vec::new(),
        }
    }

    /// A draft of a saved profile with every slot and crosshair `Keep`.
    pub fn from_profile(profile: &Profile) -> Self {
        Self {
            name: profile.name.clone(),
            hud: DraftSlot::Keep,
            hitsound: DraftSlot::Keep,
            killsound: DraftSlot::Keep,
            crosshairs: profile
                .crosshairs
                .iter()
                .cloned()
                .map(DraftCrosshair::Keep)
                .collect(),
        }
    }

    /// Replaces the name; it is trimmed when saved.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Replaces the HUD slot.
    pub fn with_hud(mut self, slot: DraftSlot<HudId>) -> Self {
        self.hud = slot;
        self
    }

    /// Replaces the hitsound slot.
    pub fn with_hitsound(mut self, slot: DraftSlot<SoundId>) -> Self {
        self.hitsound = slot;
        self
    }

    /// Replaces the killsound slot.
    pub fn with_killsound(mut self, slot: DraftSlot<SoundId>) -> Self {
        self.killsound = slot;
        self
    }

    /// Replaces the crosshair list.
    pub fn with_crosshairs(mut self, crosshairs: Vec<DraftCrosshair>) -> Self {
        self.crosshairs = crosshairs;
        self
    }
}

/// Where one slot's columns live and which library table an `Item` is copied from.
struct SlotSpec {
    prefix: &'static str,
    table: &'static str,
    label: &'static str,
    has_hud_id: bool,
}

const HUD_SLOT: SlotSpec = SlotSpec {
    prefix: "hud",
    table: "library_huds",
    label: "HUD",
    has_hud_id: true,
};
const HITSOUND_SLOT: SlotSpec = SlotSpec {
    prefix: "hitsound",
    table: "library_sounds",
    label: "hitsound",
    has_hud_id: false,
};
const KILLSOUND_SLOT: SlotSpec = SlotSpec {
    prefix: "killsound",
    table: "library_sounds",
    label: "killsound",
    has_hud_id: false,
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

/// What a slot writes: `None` leaves its columns untouched (a `Keep` on update).
fn resolve_slot(
    conn: &Connection,
    spec: &SlotSpec,
    slot: DraftSlot<&str>,
    create: bool,
) -> Result<Option<SlotValues>, ProfileError> {
    let id = match slot {
        DraftSlot::Keep if !create => return Ok(None),
        DraftSlot::Keep | DraftSlot::Empty => return Ok(Some(SlotValues::default())),
        DraftSlot::Item(id) => id,
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
/// counts: slot folders and each weapon's crosshair, not names or store ids.
fn same_setup(a: &Profile, b: &Profile) -> bool {
    fn slots(p: &Profile) -> [Option<&str>; 3] {
        [p.hud(), p.hitsound(), p.killsound()].map(|slot| slot.map(ProfileItem::folder_name))
    }
    fn assignment(c: &ProfileCrosshair) -> (&str, &ProfileCrosshairRef) {
        (c.weapon_script(), c.crosshair())
    }
    slots(a) == slots(b)
        && a.crosshairs
            .iter()
            .map(assignment)
            .eq(b.crosshairs.iter().map(assignment))
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
    let slots = [
        (
            &HUD_SLOT,
            resolve_slot(&tx, &HUD_SLOT, slot_ids(&draft.hud, HudId::as_str), create)?,
        ),
        (
            &HITSOUND_SLOT,
            resolve_slot(
                &tx,
                &HITSOUND_SLOT,
                slot_ids(&draft.hitsound, SoundId::as_str),
                create,
            )?,
        ),
        (
            &KILLSOUND_SLOT,
            resolve_slot(
                &tx,
                &KILLSOUND_SLOT,
                slot_ids(&draft.killsound, SoundId::as_str),
                create,
            )?,
        ),
    ];
    let crosshairs = resolve_crosshairs(&tx, &draft.crosshairs)?;
    let before = if create {
        None
    } else {
        load_profile(&tx, id).map_err(profile_db)?
    };

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
    for (spec, values) in &slots {
        if let Some(values) = values {
            write_slot(&tx, id, spec, values).map_err(profile_db)?;
        }
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

fn slot_ids<T>(slot: &DraftSlot<T>, as_str: fn(&T) -> &str) -> DraftSlot<&str> {
    match slot {
        DraftSlot::Empty => DraftSlot::Empty,
        DraftSlot::Item(id) => DraftSlot::Item(as_str(id)),
        DraftSlot::Keep => DraftSlot::Keep,
    }
}

/// The trimmed name, after checking it and every assigned weapon script.
fn validated_name(draft: &ProfileDraft) -> Result<&str, ProfileError> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err(ProfileError::EmptyName);
    }
    for crosshair in &draft.crosshairs {
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

/// A profile with every reference resolved against the library. A slot is `None` when it
/// is empty or missing; `crosshairs` keeps every row, a resolved one under its new folder.
pub(super) struct Resolved {
    pub(super) hud: Option<HudId>,
    pub(super) hitsound: Option<SoundId>,
    pub(super) killsound: Option<SoundId>,
    pub(super) crosshairs: Vec<ProfileCrosshair>,
    pub(super) missing: Vec<MissingItem>,
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
        _ => return Ok(None),
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
    let hud = slot(
        profile.hud(),
        &HUD_SLOT,
        LibraryKind::Huds,
        MissingKind::Hud,
    )?;
    let hitsound = slot(
        profile.hitsound(),
        &HITSOUND_SLOT,
        LibraryKind::Sounds,
        MissingKind::Hitsound,
    )?;
    let killsound = slot(
        profile.killsound(),
        &KILLSOUND_SLOT,
        LibraryKind::Sounds,
        MissingKind::Killsound,
    )?;

    let mut crosshairs = Vec::with_capacity(profile.crosshairs.len());
    for mut crosshair in profile.crosshairs {
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
    // However many references were rewritten, the profile changed once.
    if rewritten {
        tx.execute(
            "UPDATE profiles SET updated_at = ?2 WHERE id = ?1",
            [id.as_str(), &stamp],
        )
        .map_err(resolve_db)?;
    }
    tx.commit().map_err(resolve_db)?;
    Ok(Resolved {
        hud: hud.map(HudId::new),
        hitsound: hitsound.map(SoundId::new),
        killsound: killsound.map(SoundId::new),
        crosshairs,
        missing,
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
}

/// What applying a profile did: the steps that succeeded, the missing items it skipped
/// and each failed step with its error message.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApplyProfileReport {
    applied: Vec<ApplyStep>,
    skipped: Vec<MissingItem>,
    failed: Vec<(ApplyStep, String)>,
    hud_blocked_name: Option<String>,
}

impl ApplyProfileReport {
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
                    profile.crosshairs = crosshair_rows(conn, profile.id.as_str())?;
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

    /// Creates a profile from a draft, with a new id; `Keep` slots are left empty.
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

    /// Rewrites a profile from a draft: `Keep` slots stay as saved and the crosshair list
    /// replaces the old builtin and library rows; a row of another source stays unless the
    /// list sets its weapon. An edit of the active profile that changes what the game would
    /// get ends its active state; a rename does not, and no game file is touched either way.
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

    /// Creates a profile named `name` from what is installed now: the HUD from `hud_status`
    /// (only `Managed`), each sound slot from `installed_sounds` (only a managed one), and the
    /// crosshairs from `assignments()`. Anything else leaves that slot empty.
    pub fn save_current_profile(
        &self,
        name: &str,
        tf2: &Tf2Install,
    ) -> Result<Profile, ProfileError> {
        // Scan into a draft and go through `create_profile`, so validation and the row
        // write have a single path. A blank name is refused before the scan can fail.
        validated_name(&ProfileDraft::new(name))?;
        let hud = match self.hud_status(tf2)? {
            HudStatus::Managed { id, .. } => DraftSlot::Item(id),
            _ => DraftSlot::Empty,
        };
        let sounds = self.installed_sounds(tf2)?;
        let sound = |kind| match sounds.slot(kind).managed() {
            Some(id) => DraftSlot::Item(id.clone()),
            None => DraftSlot::Empty,
        };
        let crosshairs = self
            .assignments()?
            .into_iter()
            .map(|a| DraftCrosshair::Set {
                weapon_script: a.weapon_script().to_owned(),
                crosshair: a.crosshair().clone(),
            })
            .collect();
        let draft = ProfileDraft::new(name)
            .with_hud(hud)
            .with_hitsound(sound(SoundKind::Hitsound))
            .with_killsound(sound(SoundKind::Killsound))
            .with_crosshairs(crosshairs);
        self.create_profile(&draft)
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
    /// in slot order: HUD, hitsound, killsound, then crosshairs by weapon script.
    /// Builtin crosshairs are not checked here. An unknown id is `NotFound`.
    pub fn missing_items(&self, id: &ProfileId) -> Result<Vec<MissingItem>, ProfileError> {
        self.db
            .write(|conn| Ok(resolve_profile(conn, &self.dir, id, None)))
            .map_err(profile_db)?
            .map(|resolved| resolved.missing)
            .map_err(ProfileError::from)
    }

    /// Applies a profile to `tf2`: resolves it (a builtin the catalog lacks is missing),
    /// then installs or uninstalls the HUD, each sound and the crosshairs, best effort.
    /// Missing items fail with `Missing` before any file is touched unless `skip_missing`,
    /// which leaves what is installed for those slots alone; a HUD the library cannot put
    /// back then fails with `ExternalHuds` the same way. The profile becomes active only
    /// when nothing was skipped and no step failed, so Active means the game matches it.
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
        let unnamed = self
            .unnamed_huds(&game_custom_dir(tf2)?, discard)
            .map_err(|e| match e {
                LibraryError::Io { path, source } => ApplyProfileError::Io { path, source },
                other => ApplyProfileError::Database {
                    message: other.to_string(),
                },
            })?;
        if !unnamed.is_empty() {
            return Err(ApplyProfileError::ExternalHuds(unnamed));
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
        // A skipped slot runs no step, so whatever is installed there stays; an empty
        // slot still uninstalls, since the profile asks for nothing there.
        let skipped = |kind: MissingKind| resolved.missing.iter().any(|m| m.kind == kind);
        let mut hud_blocked_name = None;
        if !skipped(MissingKind::Hud) {
            let hud = match &resolved.hud {
                Some(hud) => self.install_hud_discarding(hud, tf2, discard).map(|done| {
                    hud_blocked_name = done.blocked_name().map(str::to_owned);
                }),
                None => self.uninstall_hud_discarding(tf2, discard),
            };
            record(ApplyStep::Hud, hud.map_err(|e| e.to_string()));
        }
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
                Some(sound) => self.install_sound(sound, kind, tf2),
                None => self.uninstall_sound(kind, tf2),
            };
            record(step, result.map_err(|e| e.to_string()));
        }
        record(
            ApplyStep::Crosshairs,
            self.apply_profile_crosshairs(&resolved.crosshairs, &resolved.missing, tf2, builtins),
        );

        if failed.is_empty() && resolved.missing.is_empty() {
            self.db
                .write(|conn| save_setting(conn, ACTIVE_PROFILE_KEY, id.as_str()))
                .map_err(apply_db)?;
        }
        Ok(ApplyProfileReport {
            applied,
            skipped: resolved.missing,
            failed,
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
                _ => None,
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

    /// The profile last applied in full with nothing skipped, when its row still exists.
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
