//! Crosshair library: importing a PNG or VTF crosshair, previews of library and builtin
//! crosshairs, and the per-weapon assignment table, ported from v5's `commands/crosshairs.rs`.

use std::collections::{BTreeMap, HashSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::{
    CrosshairId, IoFailure, Library, Provenance, create_dir_all, first_free_number, folder_name,
    game_custom_dir, gb_id_column, import_db, install_db, io_at, is_plain_name, library_db, new_id,
    now, paint_db, remove_dir_if_present, remove_file_if_present, save_setting,
};
use crate::crosshair_catalog::BuiltinCrosshairs;
use crate::error::{ImportError, InstallError, LibraryError, PaintError, VtfError};
use crate::paths::LibraryKind;
use crate::templates;
use crate::texture::{RgbaImage, crosshair_for_game, decode_vtf, png_to_vtf, rgba_to_vtf};
use crate::tf2::Tf2Install;
use crate::{ice, pak};

/// v5's `crosshair_source` texts.
pub(super) const SOURCE_BUILTIN: &str = "builtin";
pub(super) const SOURCE_LIBRARY: &str = "library";

/// The folder under `tf/custom` that holds every applied crosshair, as v5.
const MANAGED_DIR: &str = "tf2tools-crosshairs";
/// Where TF2 looks for the textures a weapon script names, under the managed folder.
const MATERIALS_DIR: &str = "materials/vgui/replay/thumbnails";

/// What a library crosshair was imported from, v5's `source_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CrosshairFormat {
    /// A PNG, converted to a 64x64 VTF on import.
    Png,
    /// A VTF, copied as is.
    Vtf,
    /// A drawing made in the crosshair maker, kept as `source.png` beside its VTF.
    Painted,
}

impl CrosshairFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Vtf => "vtf",
            Self::Painted => "painted",
        }
    }

    /// v5 only ever wrote `png` or `vtf` and v6 adds `painted`; anything else reads as a PNG
    /// import.
    fn from_column(text: &str) -> Self {
        match text {
            "vtf" => Self::Vtf,
            "painted" => Self::Painted,
            _ => Self::Png,
        }
    }
}

/// Where the maker saves a drawing: a new library crosshair, or over a painted one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PaintTarget {
    /// A new crosshair with its own folder.
    New,
    /// The painted crosshair with this id, replaced in place.
    Existing(CrosshairId),
}

/// A painted crosshair's drawing, read back for editing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PaintedSource {
    drawing: RgbaImage,
    recovered: bool,
}

impl PaintedSource {
    /// The drawing at its painted size, 32x32 or 64x64.
    pub fn drawing(&self) -> &RgbaImage {
        &self.drawing
    }

    /// True when source.png was unreadable and the drawing was rebuilt from the VTF.
    pub fn recovered(&self) -> bool {
        self.recovered
    }
}

/// A crosshair in the library, read from `library_crosshairs`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CrosshairEntry {
    id: CrosshairId,
    folder_name: String,
    name: String,
    format: CrosshairFormat,
    provenance: Provenance,
    created_at: String,
    wanted_name: Option<String>,
}

impl CrosshairEntry {
    /// The library id.
    pub fn id(&self) -> &CrosshairId {
        &self.id
    }

    /// The folder under `library/crosshairs/`, also the VTF and VMT file stem.
    pub fn folder_name(&self) -> &str {
        &self.folder_name
    }

    /// The display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the crosshair was imported from a PNG or a VTF, or painted in the maker.
    pub fn format(&self) -> CrosshairFormat {
        self.format
    }

    /// Where the crosshair came from; a GameBanana author always reads back empty.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// When the crosshair was imported, as v5's RFC 3339 text.
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    /// The name an import wanted when another library crosshair already had it, so this
    /// one took a numbered name instead. Only the entry `import_crosshair` returns carries it.
    pub fn wanted_name(&self) -> Option<&str> {
        self.wanted_name.as_deref()
    }
}

/// The crosshair a weapon is assigned: a bundled one by catalog name or a library item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CrosshairRef {
    /// A builtin crosshair, by its catalog name.
    Builtin(String),
    /// A library crosshair, by id.
    Library(CrosshairId),
}

/// One weapon's crosshair assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Assignment {
    weapon_script: String,
    crosshair: CrosshairRef,
}

impl Assignment {
    /// The weapon script name, such as `tf_weapon_scattergun`.
    pub fn weapon_script(&self) -> &str {
        &self.weapon_script
    }

    /// The crosshair assigned to it.
    pub fn crosshair(&self) -> &CrosshairRef {
        &self.crosshair
    }
}

/// True for `tf_weapon_` followed by one or more of `[a-z0-9_]`, the only names that are
/// accepted as assignments and joined into a script path.
pub(super) fn is_weapon_script(name: &str) -> bool {
    name.strip_prefix("tf_weapon_").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    })
}

/// What `apply_crosshairs` wrote and which assignments it left out.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApplyReport {
    applied: Vec<String>,
    skipped: Vec<SkippedAssignment>,
}

impl ApplyReport {
    /// The weapon scripts written, sorted.
    pub fn applied(&self) -> &[String] {
        &self.applied
    }

    /// The assignments left out, sorted by weapon script.
    pub fn skipped(&self) -> &[SkippedAssignment] {
        &self.skipped
    }
}

/// An assignment `apply_crosshairs` could not write, as read from the database.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SkippedAssignment {
    weapon_script: String,
    reference: String,
    reason: SkipReason,
}

impl SkippedAssignment {
    /// The weapon script column as stored.
    pub fn weapon_script(&self) -> &str {
        &self.weapon_script
    }

    /// The builtin name or library folder the assignment points at.
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Why it was left out.
    pub fn reason(&self) -> SkipReason {
        self.reason
    }
}

/// Why an assignment was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SkipReason {
    /// The weapon script is not `tf_weapon_[a-z0-9_]+`, so it cannot name a file.
    InvalidWeaponScript,
    /// The builtin crosshair is not in the catalog.
    UnknownBuiltin,
    /// The library folder is unsafe, gone or holds no VTF.
    MissingLibraryCrosshair,
    /// The game's own script for the weapon is missing, unreadable or has no crosshair block,
    /// so no full script can be written.
    NoStockScript,
}

/// What `tf/custom` holds of the managed crosshair folder and of rival weapon scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InstalledCrosshairs {
    folder_exists: bool,
    script_count: usize,
    conflicts: Vec<String>,
    in_game: Vec<CrosshairId>,
}

impl InstalledCrosshairs {
    /// Whether `tf/custom/tf2tools-crosshairs` exists.
    pub fn folder_exists(&self) -> bool {
        self.folder_exists
    }

    /// The `tf_weapon_*.txt` files directly in the managed `scripts/` folder.
    pub fn script_count(&self) -> usize {
        self.script_count
    }

    /// Other folders under `tf/custom` whose weapon scripts may override ours, sorted.
    pub fn conflicts(&self) -> &[String] {
        &self.conflicts
    }

    /// Whether library crosshair `id` has its texture in the managed folder, which an
    /// assignment alone does not tell: it may not be installed yet, or may have moved on.
    pub fn is_in_game(&self, id: &CrosshairId) -> bool {
        self.in_game.contains(id)
    }
}

/// The game's stock weapon scripts, read from `tf/tf2_misc_dir.vpk`; None when the pak is
/// missing or unreadable, which leaves every weapon without one.
struct StockScripts {
    pak: Option<(PathBuf, vpk::VPK)>,
}

impl StockScripts {
    fn open(tf2: &Tf2Install) -> Self {
        let path = tf2.tf_dir().join("tf2_misc_dir.vpk");
        let pak = pak::open_vpk(&path).ok().map(|pak| (path, pak));
        Self { pak }
    }

    /// The full script for `weapon_script` pointing at `reference`'s texture, decrypted from
    /// `scripts/<weapon_script>.ctx` and patched; None when it is absent, unreadable or has no
    /// crosshair block.
    fn patched(&self, weapon_script: &str, reference: &str) -> Option<Vec<u8>> {
        let (path, pak) = self.pak.as_ref()?;
        let name = format!("scripts/{weapon_script}.ctx");
        let entry = pak.tree.get(&name)?;
        let encrypted = pak::read_entry(path, &name, entry).ok()?;
        templates::weapon_script(&ice::decrypt(&encrypted, ice::CTX_KEY), reference)
    }
}

/// A crosshair's texture and optional VMT to copy in as `<ref>.vtf` and `<ref>.vmt`.
struct CrosshairFiles {
    vtf: PathBuf,
    vmt: Option<PathBuf>,
}

fn crosshair_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CrosshairEntry> {
    Ok(CrosshairEntry {
        id: CrosshairId::new(r.get::<_, String>("id")?),
        folder_name: r.get("folder_name")?,
        name: r.get("name")?,
        format: CrosshairFormat::from_column(&r.get::<_, String>("source_type")?),
        provenance: Provenance::from_columns(None, r.get("store_gb_id")?, ""),
        created_at: r.get("created_at")?,
        wanted_name: None,
    })
}

impl Library {
    /// Every library crosshair, ordered by display name.
    pub fn list_crosshairs(&self) -> Result<Vec<CrosshairEntry>, LibraryError> {
        self.db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, folder_name, name, source_type, store_gb_id, created_at
                     FROM library_crosshairs ORDER BY name",
                )?;
                stmt.query_map([], crosshair_from_row)?.collect()
            })
            .map_err(library_db)
    }

    /// Imports a `.png` (converted to a 64x64 VTF, the PNG kept as `preview.png`) or a
    /// `.vtf` (copied, with a best-effort decoded `preview.png`) into
    /// `library/crosshairs/<slug>-<id8>/` beside its VMT, and records it. A name another
    /// crosshair holds keeps that one and takes "<name> (<n>)" (see `wanted_name`). Curated
    /// provenance is refused, since only HUDs come from the curated store.
    pub fn import_crosshair(
        &self,
        path: &Path,
        name: &str,
        provenance: Provenance,
    ) -> Result<CrosshairEntry, ImportError> {
        let store_gb_id = match &provenance {
            Provenance::Local => None,
            Provenance::GameBanana { gb_id, .. } => Some(gb_id_column(*gb_id)?),
            Provenance::Curated { .. } => return Err(ImportError::UnsupportedProvenance),
        };
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "png" | "vtf") {
            return Err(ImportError::UnsupportedFile {
                path: path.to_path_buf(),
            });
        }
        // Everything that can reject the input runs before the folder exists.
        let source = std::fs::read(path).map_err(io_at(path))?;
        let (format, vtf, preview) = if ext == "png" {
            (CrosshairFormat::Png, png_to_vtf(&source)?, Some(source))
        } else {
            let preview = decode_vtf(&source).ok().and_then(encode_png);
            (CrosshairFormat::Vtf, source, preview)
        };

        let id = new_id();
        let folder = folder_name(name, &id);
        let dest = self.dir.library_dir(LibraryKind::Crosshairs).join(&folder);
        if let Err(failure) = write_crosshair_files(&dest, &folder, &vtf, preview.as_deref()) {
            let _ = remove_dir_if_present(&dest);
            return Err(failure.into());
        }

        let mut entry = CrosshairEntry {
            id: CrosshairId::new(id),
            folder_name: folder,
            name: name.to_owned(),
            format,
            // Crosshairs keep no author column, so the entry matches what a list reads back.
            provenance: Provenance::from_columns(None, store_gb_id, ""),
            created_at: now(),
            wanted_name: None,
        };
        let source = if store_gb_id.is_some() {
            "store"
        } else {
            "local"
        };
        // The name is picked inside one immediate transaction, so two imports racing for it
        // cannot both take it. A taken name, in any case, gives way to "<name> (<n>)".
        let recorded = self.db.write(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if name_taken(&tx, name, None)? {
                let mut taken = HashSet::new();
                let mut stmt = tx.prepare("SELECT name FROM library_crosshairs")?;
                for other in stmt.query_map([], |r| r.get::<_, String>(0))? {
                    taken.insert(other?.to_lowercase());
                }
                drop(stmt);
                let numbered = |n| format!("{name} ({n})");
                let number = first_free_number(|n| !taken.contains(&numbered(n).to_lowercase()));
                entry.name = numbered(number);
                entry.wanted_name = Some(name.to_owned());
            }
            tx.execute(
                "INSERT INTO library_crosshairs (id, folder_name, name, source_type, source,
                     store_gb_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    entry.id.as_str(),
                    entry.folder_name,
                    entry.name,
                    format.as_str(),
                    source,
                    store_gb_id,
                    entry.created_at
                ],
            )?;
            tx.commit()
        });
        if let Err(e) = recorded {
            let _ = remove_dir_if_present(&dest);
            return Err(import_db(e));
        }
        Ok(entry)
    }

    /// Validates the drawing (32x32 or 64x64) and the trimmed name (not blank, not another
    /// crosshair's name in any case), writes `<folder>.vtf`, `<folder>.vmt`, `source.png` and
    /// `preview.png`, then records a `'painted'`, `'local'` row. A failure removes the new folder.
    /// `Existing` rewrites a painted crosshair's files in place, all or nothing, then renames its
    /// row (`NotFound`, `NotPainted`); a failed rename keeps the new files under the old name.
    pub fn save_painted_crosshair(
        &self,
        target: &PaintTarget,
        name: &str,
        drawing: &RgbaImage,
    ) -> Result<CrosshairEntry, PaintError> {
        if !is_canvas_size(drawing) {
            return Err(PaintError::InvalidDrawing {
                width: drawing.width(),
                height: drawing.height(),
            });
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(PaintError::EmptyName);
        }
        if let PaintTarget::Existing(id) = target {
            return self.resave_painted(id, name, drawing);
        }
        // The early check gives fast feedback; the write below checks again in its transaction.
        if self
            .db
            .read(|conn| name_taken(conn, name, None))
            .map_err(paint_db)?
        {
            return Err(PaintError::NameTaken {
                name: name.to_owned(),
            });
        }
        let source = drawing_png(drawing)?;
        let vtf = rgba_to_vtf(&exported(drawing))?;

        let id = new_id();
        let folder = folder_name(name, &id);
        let dest = self.dir.library_dir(LibraryKind::Crosshairs).join(&folder);
        let written = write_crosshair_files(&dest, &folder, &vtf, Some(&source)).and_then(|()| {
            let path = dest.join("source.png");
            std::fs::write(&path, &source).map_err(io_at(&path))
        });
        if let Err(failure) = written {
            let _ = remove_dir_if_present(&dest);
            return Err(failure.into());
        }

        let entry = CrosshairEntry {
            id: CrosshairId::new(id),
            folder_name: folder,
            name: name.to_owned(),
            format: CrosshairFormat::Painted,
            provenance: Provenance::Local,
            created_at: now(),
            wanted_name: None,
        };
        if let Err(e) = self.record_new_painted(&entry) {
            let _ = remove_dir_if_present(&dest);
            return Err(e);
        }
        Ok(entry)
    }

    /// The editable drawing of a painted crosshair: `source.png` when it decodes to 32x32 or
    /// 64x64, else the VTF (also 32x32 or 64x64) with `recovered` set. `NotFound` for an
    /// unknown id, `NotPainted` for another format, `Unreadable` when neither file serves.
    pub fn painted_source(&self, id: &CrosshairId) -> Result<PaintedSource, PaintError> {
        let row: Option<(String, String)> = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT folder_name, source_type FROM library_crosshairs WHERE id = ?1",
                    [id.as_str()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
            })
            .map_err(paint_db)?;
        let Some((folder, source_type)) = row else {
            return Err(PaintError::NotFound { id: id.to_string() });
        };
        if CrosshairFormat::from_column(&source_type) != CrosshairFormat::Painted {
            return Err(PaintError::NotPainted { id: id.to_string() });
        }
        let unreadable = || PaintError::Unreadable { id: id.to_string() };
        if !is_plain_name(&folder) {
            return Err(unreadable());
        }
        // Any read or decode failure of source.png falls back to the VTF, never to an error.
        let dir = self.dir.library_dir(LibraryKind::Crosshairs).join(&folder);
        let from_png = std::fs::read(dir.join("source.png"))
            .ok()
            .and_then(|bytes| {
                image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()
            })
            .map(|img| img.to_rgba8())
            .and_then(|img| {
                let (width, height) = (img.width(), img.height());
                RgbaImage::from_pixels(width, height, img.into_raw())
            })
            .filter(is_canvas_size);
        if let Some(drawing) = from_png {
            return Ok(PaintedSource {
                drawing,
                recovered: false,
            });
        }
        let drawing = std::fs::read(self.crosshair_vtf(&folder))
            .ok()
            .and_then(|bytes| decode_vtf(&bytes).ok())
            .filter(is_canvas_size)
            .ok_or_else(unreadable)?;
        Ok(PaintedSource {
            drawing,
            recovered: true,
        })
    }

    /// The `Existing` arm of `save_painted_crosshair`, after the drawing and name are validated.
    fn resave_painted(
        &self,
        id: &CrosshairId,
        name: &str,
        drawing: &RgbaImage,
    ) -> Result<CrosshairEntry, PaintError> {
        let row: Option<(String, String, bool)> = self
            .db
            .read(|conn| {
                let row = conn
                    .query_row(
                        "SELECT folder_name, source_type FROM library_crosshairs WHERE id = ?1",
                        [id.as_str()],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                let Some((folder, source_type)) = row else {
                    return Ok(None);
                };
                Ok(Some((
                    folder,
                    source_type,
                    name_taken(conn, name, Some(id))?,
                )))
            })
            .map_err(paint_db)?;
        let not_found = || PaintError::NotFound { id: id.to_string() };
        let Some((folder, source_type, taken)) = row else {
            return Err(not_found());
        };
        if CrosshairFormat::from_column(&source_type) != CrosshairFormat::Painted {
            return Err(PaintError::NotPainted { id: id.to_string() });
        }
        if !is_plain_name(&folder) {
            return Err(not_found());
        }
        if taken {
            return Err(PaintError::NameTaken {
                name: name.to_owned(),
            });
        }
        let source = drawing_png(drawing)?;
        let vtf = rgba_to_vtf(&exported(drawing))?;
        let vmt = templates::vmt(&folder);

        // The VTF goes last, so the texture TF2 reads changes only once everything else has.
        let dir = self.dir.library_dir(LibraryKind::Crosshairs).join(&folder);
        replace_files(&[
            (dir.join("source.png"), &source),
            (dir.join("preview.png"), &source),
            (dir.join(format!("{folder}.vmt")), vmt.as_bytes()),
            (dir.join(format!("{folder}.vtf")), &vtf),
        ])?;
        self.rename_painted(id, name)?;
        self.db
            .read(|conn| {
                conn.query_row(
                    "SELECT id, folder_name, name, source_type, store_gb_id, created_at
                     FROM library_crosshairs WHERE id = ?1",
                    [id.as_str()],
                    crosshair_from_row,
                )
                .optional()
            })
            .map_err(paint_db)?
            .ok_or_else(not_found)
    }

    /// Inside one immediate `db.write` transaction, so no other process writes between the
    /// check and the write: rescans names (`NameTaken`), then renames the row
    /// (`NotFound` when no row has `id`, as after a delete mid-save).
    fn rename_painted(&self, id: &CrosshairId, name: &str) -> Result<(), PaintError> {
        let renamed = self
            .db
            .write(|conn| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if name_taken(&tx, name, Some(id))? {
                    return Ok(None);
                }
                let changed = tx.execute(
                    "UPDATE library_crosshairs SET name = ?1 WHERE id = ?2",
                    params![name, id.as_str()],
                )?;
                tx.commit()?;
                Ok(Some(changed))
            })
            .map_err(paint_db)?;
        match renamed {
            None => Err(PaintError::NameTaken {
                name: name.to_owned(),
            }),
            Some(0) => Err(PaintError::NotFound { id: id.to_string() }),
            Some(_) => Ok(()),
        }
    }

    /// Inside one immediate `db.write` transaction, so no other process writes between the
    /// check and the write: rescans names (`NameTaken`), then inserts the row.
    fn record_new_painted(&self, entry: &CrosshairEntry) -> Result<(), PaintError> {
        let recorded = self
            .db
            .write(|conn| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if name_taken(&tx, &entry.name, None)? {
                    return Ok(false);
                }
                tx.execute(
                    "INSERT INTO library_crosshairs (id, folder_name, name, source_type, source,
                         created_at)
                     VALUES (?1, ?2, ?3, ?4, 'local', ?5)",
                    params![
                        entry.id.as_str(),
                        entry.folder_name,
                        entry.name,
                        CrosshairFormat::Painted.as_str(),
                        entry.created_at
                    ],
                )?;
                tx.commit()?;
                Ok(true)
            })
            .map_err(paint_db)?;
        if recorded {
            Ok(())
        } else {
            Err(PaintError::NameTaken {
                name: entry.name.clone(),
            })
        }
    }

    /// Removes a crosshair's row and the weapon assignments that point at it in one
    /// transaction, as v5, then its library folder. Removing an assignment clears the active
    /// profile, as changing one does. Files in `tf/custom` stay; see `uninstall_crosshair`.
    pub fn delete_crosshair(&self, id: &CrosshairId) -> Result<(), LibraryError> {
        let folder = self.crosshair_folder(id)?;
        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                let unassigned = tx.execute(
                    "DELETE FROM crosshair_assignments
                     WHERE crosshair_source = ?1 AND crosshair_ref = ?2",
                    [SOURCE_LIBRARY, folder.as_str()],
                )?;
                if unassigned > 0 {
                    save_setting(&tx, "active_profile_id", "")?;
                }
                tx.execute(
                    "DELETE FROM library_crosshairs WHERE id = ?1",
                    [id.as_str()],
                )?;
                tx.commit()
            })
            .map_err(library_db)?;
        // A row whose folder name is not a plain name has no files, so only the row goes.
        if is_plain_name(&folder) {
            remove_dir_if_present(&self.dir.library_dir(LibraryKind::Crosshairs).join(&folder))?;
        }
        Ok(())
    }

    /// The pixels to copy or export: a painted crosshair's own drawing (32x32 or 64x64), else the
    /// decoded VTF of a builtin (from `builtins`) or library crosshair. Reads only.
    pub fn crosshair_drawing(
        &self,
        crosshair: &CrosshairRef,
        builtins: &BuiltinCrosshairs,
    ) -> Result<RgbaImage, LibraryError> {
        // Any painted-source failure falls back to the VTF, whose read maps errors properly.
        if let CrosshairRef::Library(id) = crosshair
            && let Ok(source) = self.painted_source(id)
        {
            return Ok(source.drawing().clone());
        }
        self.crosshair_preview(crosshair, builtins)
    }

    /// Decodes the VTF of a builtin crosshair (from `builtins`) or a library crosshair.
    /// An unknown crosshair or a missing VTF is `NotFound`.
    pub fn crosshair_preview(
        &self,
        crosshair: &CrosshairRef,
        builtins: &BuiltinCrosshairs,
    ) -> Result<RgbaImage, LibraryError> {
        let not_found = || LibraryError::NotFound {
            what: match crosshair {
                CrosshairRef::Builtin(name) => format!("builtin crosshair {name}"),
                CrosshairRef::Library(id) => format!("the VTF of crosshair {id}"),
            },
        };
        let vtf_path = match crosshair {
            CrosshairRef::Builtin(name) => builtins
                .get(name)
                .ok_or_else(not_found)?
                .vtf_path()
                .to_path_buf(),
            CrosshairRef::Library(id) => {
                let folder = self.crosshair_folder(id)?;
                if !is_plain_name(&folder) {
                    return Err(not_found());
                }
                self.crosshair_vtf(&folder)
            }
        };
        let bytes = match std::fs::read(&vtf_path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(not_found()),
            Err(e) => return Err(io_at(&vtf_path)(e).into()),
        };
        Ok(decode_vtf(&bytes)?)
    }

    /// Every assignment, sorted by weapon script. A library assignment whose crosshair row
    /// is gone, or a source v5 never wrote, is left out.
    pub fn assignments(&self) -> Result<Vec<Assignment>, LibraryError> {
        let rows: Vec<(String, String, String, Option<String>)> = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT a.weapon_script, a.crosshair_source, a.crosshair_ref, c.id
                     FROM crosshair_assignments a
                     LEFT JOIN library_crosshairs c
                         ON a.crosshair_source = 'library' AND c.folder_name = a.crosshair_ref
                     ORDER BY a.weapon_script",
                )?;
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                    .collect()
            })
            .map_err(library_db)?;
        Ok(rows
            .into_iter()
            .filter_map(|(weapon_script, source, reference, id)| {
                let crosshair = match (source.as_str(), id) {
                    (SOURCE_BUILTIN, _) => CrosshairRef::Builtin(reference),
                    (SOURCE_LIBRARY, Some(id)) => CrosshairRef::Library(CrosshairId::new(id)),
                    _ => return None,
                };
                Some(Assignment {
                    weapon_script,
                    crosshair,
                })
            })
            .collect())
    }

    /// Assigns a crosshair to a weapon script, replacing any earlier one, and clears the
    /// active profile. Fails with `InvalidWeaponScript` or, for an unknown library
    /// crosshair, `NotFound`.
    pub fn assign(
        &self,
        weapon_script: &str,
        crosshair: &CrosshairRef,
    ) -> Result<(), LibraryError> {
        self.assign_many(&[weapon_script], crosshair)
    }

    /// Assigns `crosshair` to every script in `weapon_scripts` in one transaction and clears the
    /// active profile: every row is written or none is. A bad name (`InvalidWeaponScript`) or an
    /// unknown library crosshair (`NotFound`) fails before anything is written; an empty slice
    /// writes nothing. Files in `tf/custom` change only on `apply_crosshairs`.
    pub fn assign_many(
        &self,
        weapon_scripts: &[&str],
        crosshair: &CrosshairRef,
    ) -> Result<(), LibraryError> {
        if let Some(bad) = weapon_scripts.iter().find(|s| !is_weapon_script(s)) {
            return Err(LibraryError::InvalidWeaponScript {
                name: (*bad).to_owned(),
            });
        }
        if weapon_scripts.is_empty() {
            return Ok(());
        }
        // v5 stores a library crosshair by folder name, so the id is resolved in the same
        // transaction that writes the rows. An error drops the transaction uncommitted.
        let assigned = self
            .db
            .write(|conn| {
                let tx = conn.transaction()?;
                let target = match crosshair {
                    CrosshairRef::Builtin(name) => Some((SOURCE_BUILTIN, name.clone())),
                    CrosshairRef::Library(id) => tx
                        .query_row(
                            "SELECT folder_name FROM library_crosshairs WHERE id = ?1",
                            [id.as_str()],
                            |r| r.get::<_, String>(0),
                        )
                        .optional()?
                        .map(|folder| (SOURCE_LIBRARY, folder)),
                };
                let Some((source, reference)) = target else {
                    return Ok(false);
                };
                for weapon_script in weapon_scripts {
                    tx.execute(
                        "INSERT OR REPLACE INTO crosshair_assignments
                             (weapon_script, crosshair_source, crosshair_ref)
                         VALUES (?1, ?2, ?3)",
                        [*weapon_script, source, reference.as_str()],
                    )?;
                }
                save_setting(&tx, "active_profile_id", "")?;
                tx.commit()?;
                Ok(true)
            })
            .map_err(library_db)?;
        match (assigned, crosshair) {
            (false, CrosshairRef::Library(id)) => Err(crosshair_not_found(id)),
            _ => Ok(()),
        }
    }

    /// Clears the assignments of every script in `weapon_scripts` in one transaction, so those
    /// weapons go back to the game's own crosshair on the next `apply_crosshairs`. Clears the
    /// active profile when a row went. A bad name (`InvalidWeaponScript`) fails before anything
    /// is cleared; weapons without an assignment are ignored.
    pub fn unassign_many(&self, weapon_scripts: &[&str]) -> Result<(), LibraryError> {
        if let Some(bad) = weapon_scripts.iter().find(|s| !is_weapon_script(s)) {
            return Err(LibraryError::InvalidWeaponScript {
                name: (*bad).to_owned(),
            });
        }
        if weapon_scripts.is_empty() {
            return Ok(());
        }
        self.db
            .write(|conn| {
                let tx = conn.transaction()?;
                let mut removed = 0;
                for weapon_script in weapon_scripts {
                    removed += tx.execute(
                        "DELETE FROM crosshair_assignments WHERE weapon_script = ?1",
                        [*weapon_script],
                    )?;
                }
                if removed > 0 {
                    save_setting(&tx, "active_profile_id", "")?;
                }
                tx.commit()
            })
            .map_err(library_db)
    }

    /// The weapon scripts, sorted, that `apply_crosshairs` would change in the game: a row it
    /// would write whose installed script or texture differs, and an installed script it would
    /// remove. Rows it would skip never count. Reads only; nothing in `tf/custom` changes.
    pub fn crosshairs_not_in_game(
        &self,
        tf2: &Tf2Install,
        builtins: &BuiltinCrosshairs,
    ) -> Result<Vec<String>, LibraryError> {
        let rows: Vec<(String, String, String)> = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT weapon_script, crosshair_source, crosshair_ref
                     FROM crosshair_assignments ORDER BY weapon_script",
                )?;
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect()
            })
            .map_err(library_db)?;
        // Judge each row as apply does, then compare the script apply would write, byte for
        // byte, with the one in the managed folder, and check its texture is beside it.
        let base = tf2.custom_dir().join(MANAGED_DIR);
        let scripts = base.join("scripts");
        let materials = base.join(MATERIALS_DIR);
        let stock = StockScripts::open(tf2);
        let mut written = HashSet::new();
        let mut waiting = Vec::new();
        for (weapon_script, source, reference) in rows {
            let resolvable = is_weapon_script(&weapon_script)
                && match source.as_str() {
                    SOURCE_BUILTIN => builtins.get(&reference).is_some(),
                    SOURCE_LIBRARY => self.library_crosshair_files(&reference).is_some(),
                    _ => false,
                };
            let Some(script) = resolvable
                .then(|| stock.patched(&weapon_script, &reference))
                .flatten()
            else {
                continue;
            };
            let path = scripts.join(format!("{weapon_script}.txt"));
            let texture = materials.join(format!("{}.vtf", templates::texture_name(&reference)));
            let installed = std::fs::read(&path).ok();
            if installed.as_deref() != Some(script.as_slice()) || !texture.is_file() {
                waiting.push(weapon_script.clone());
            }
            written.insert(weapon_script);
        }
        let entries = match std::fs::read_dir(&scripts) {
            Ok(entries) => entries.collect::<Result<Vec<_>, _>>(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e),
        }
        .map_err(|e| LibraryError::from(io_at(&scripts)(e)))?;
        for entry in entries {
            let path = entry.path();
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
            if let Some(stem) = stem.filter(|_| is_weapon_script_file(&path))
                && !written.contains(&stem)
            {
                waiting.push(stem);
            }
        }
        waiting.sort();
        waiting.dedup();
        Ok(waiting)
    }

    /// Rebuilds `tf/custom/tf2tools-crosshairs/` from the assignment table: the folder is
    /// removed, then written with each applicable crosshair's VTF and VMT and one weapon
    /// script per assignment, files named by `templates::texture_name`. Each script is the
    /// game's own from `tf2_misc_dir.vpk` with only its crosshair block replaced. Unsafe or
    /// dangling rows, and weapons whose stock script cannot be read, are skipped and reported.
    /// A failure mid-write can leave the folder partial; the next apply rebuilds it.
    pub fn apply_crosshairs(
        &self,
        tf2: &Tf2Install,
        builtins: &BuiltinCrosshairs,
    ) -> Result<ApplyReport, InstallError> {
        let rows: Vec<(String, String, String)> = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT weapon_script, crosshair_source, crosshair_ref
                     FROM crosshair_assignments ORDER BY weapon_script",
                )?;
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect()
            })
            .map_err(install_db)?;

        // Every row is judged before the folder is touched, since its weapon script and
        // reference come from the database and are joined into paths below. A row also needs
        // the game's own script to patch, since TF2 takes a loose script as the whole weapon.
        let stock = StockScripts::open(tf2);
        let mut applied = Vec::new();
        let mut skipped = Vec::new();
        let mut files: BTreeMap<String, CrosshairFiles> = BTreeMap::new();
        for (weapon_script, source, reference) in rows {
            let resolved = if !is_weapon_script(&weapon_script) {
                Err(SkipReason::InvalidWeaponScript)
            } else {
                match source.as_str() {
                    SOURCE_BUILTIN => builtins
                        .get(&reference)
                        .map(|b| CrosshairFiles {
                            vtf: b.vtf_path().to_path_buf(),
                            vmt: b.vmt_path().map(Path::to_path_buf),
                        })
                        .ok_or(SkipReason::UnknownBuiltin),
                    SOURCE_LIBRARY => self
                        .library_crosshair_files(&reference)
                        .ok_or(SkipReason::MissingLibraryCrosshair),
                    // A source v5 never wrote, as `assignments` leaves it out.
                    _ => continue,
                }
            };
            let resolved = resolved.and_then(|found| {
                stock
                    .patched(&weapon_script, &reference)
                    .map(|script| (found, script))
                    .ok_or(SkipReason::NoStockScript)
            });
            match resolved {
                Ok((found, script)) => {
                    files.entry(reference.clone()).or_insert(found);
                    applied.push((weapon_script, script));
                }
                Err(reason) => skipped.push(SkippedAssignment {
                    weapon_script,
                    reference,
                    reason,
                }),
            }
        }

        let base = game_custom_dir(tf2)?.join(MANAGED_DIR);
        remove_dir_if_present(&base)?;
        if !applied.is_empty() {
            let materials = base.join(MATERIALS_DIR);
            let scripts = base.join("scripts");
            create_dir_all(&materials)?;
            create_dir_all(&scripts)?;
            for (reference, found) in &files {
                // A renamed reference's own VMT still names the unsafe path, so it gets
                // the template instead.
                let name = templates::texture_name(reference);
                let vtf = materials.join(format!("{name}.vtf"));
                let bytes = std::fs::read(&found.vtf).map_err(io_at(&found.vtf))?;
                std::fs::write(&vtf, crosshair_for_game(bytes)).map_err(io_at(&vtf))?;
                let vmt = materials.join(format!("{name}.vmt"));
                match &found.vmt {
                    Some(src) if name == reference.as_str() => std::fs::copy(src, &vmt).map(drop),
                    _ => std::fs::write(&vmt, templates::vmt(reference)),
                }
                .map_err(io_at(&vmt))?;
            }
            for (weapon_script, text) in &applied {
                let script = scripts.join(format!("{weapon_script}.txt"));
                std::fs::write(&script, text).map_err(io_at(&script))?;
            }
        }
        Ok(ApplyReport {
            applied: applied.into_iter().map(|(script, _)| script).collect(),
            skipped,
        })
    }

    /// Removes `tf/custom/tf2tools-crosshairs/`; the assignments stay. Already gone is fine.
    pub fn remove_crosshairs(&self, tf2: &Tf2Install) -> Result<(), InstallError> {
        Ok(remove_dir_if_present(
            &game_custom_dir(tf2)?.join(MANAGED_DIR),
        )?)
    }

    /// Takes crosshair `id` out of the game: the weapon scripts naming its texture, then its
    /// VMT and VTF, then the managed folder if that leaves it empty. Nothing else in the folder
    /// changes, so an assignment not yet installed stays that way. A crosshair not in the game
    /// changes nothing. Assignments and the library stay; `delete_crosshair` clears those. An
    /// unknown id is `NotInLibrary`.
    pub fn uninstall_crosshair(
        &self,
        id: &CrosshairId,
        tf2: &Tf2Install,
    ) -> Result<(), InstallError> {
        let folder = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT folder_name FROM library_crosshairs WHERE id = ?1",
                    [id.as_str()],
                    |r| r.get::<_, String>(0),
                )
                .optional()
            })
            .map_err(install_db)?
            .ok_or_else(|| InstallError::NotInLibrary {
                what: format!("crosshair {id}"),
            })?;
        let custom = game_custom_dir(tf2)?;
        if !texture_in_game(&custom, &folder) {
            return Ok(());
        }
        let base = custom.join(MANAGED_DIR);
        let scripts = base.join("scripts");
        let entries = match std::fs::read_dir(&scripts) {
            Ok(entries) => entries.collect::<Result<Vec<_>, _>>(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e),
        }
        .map_err(io_at(&scripts))?;
        for entry in entries {
            let path = entry.path();
            if path.is_file() {
                let text = std::fs::read(&path).map_err(io_at(&path))?;
                if templates::names_texture(&String::from_utf8_lossy(&text), &folder) {
                    remove_file_if_present(&path)?;
                }
            }
        }
        // The VTF goes last: it is what marks the crosshair as in the game, so a failure part
        // way leaves a delete that can be retried, and no script pointing at a missing texture.
        let materials = base.join(MATERIALS_DIR);
        let name = templates::texture_name(&folder);
        remove_file_if_present(&materials.join(format!("{name}.vmt")))?;
        remove_file_if_present(&materials.join(format!("{name}.vtf")))?;
        if !holds_files(&base)? {
            remove_dir_if_present(&base)?;
        }
        Ok(())
    }

    /// Scans `tf/custom` for the managed crosshair folder, its weapon scripts and other
    /// folders shipping weapon scripts, ported from v5's `scan_installed_crosshairs`.
    pub fn installed_crosshairs(
        &self,
        tf2: &Tf2Install,
    ) -> Result<InstalledCrosshairs, LibraryError> {
        let custom = tf2.custom_dir();
        let base = custom.join(MANAGED_DIR);
        let entries = match std::fs::read_dir(&custom) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(InstalledCrosshairs {
                    folder_exists: false,
                    script_count: 0,
                    conflicts: Vec::new(),
                    in_game: Vec::new(),
                });
            }
            Err(e) => return Err(io_at(&custom)(e).into()),
        };
        let mut conflicts = Vec::new();
        for entry in entries {
            let path = entry.map_err(io_at(&custom))?.path();
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            if name != MANAGED_DIR && path.is_dir() && weapon_scripts_in(&path)? > 0 {
                conflicts.push(name);
            }
        }
        conflicts.sort();
        let in_game = self
            .list_crosshairs()?
            .into_iter()
            .filter(|c| texture_in_game(&custom, c.folder_name()))
            .map(|c| c.id)
            .collect();
        Ok(InstalledCrosshairs {
            folder_exists: base.exists(),
            script_count: weapon_scripts_in(&base)?,
            conflicts,
            in_game,
        })
    }

    /// A library crosshair's files by folder name, as v5's import names them; `None` when
    /// the name is unsafe or the VTF is gone. A missing VMT gets the template on apply.
    fn library_crosshair_files(&self, folder: &str) -> Option<CrosshairFiles> {
        if !is_plain_name(folder) {
            return None;
        }
        let vtf = self.crosshair_vtf(folder);
        if !vtf.is_file() {
            return None;
        }
        let vmt = vtf.with_extension("vmt");
        Some(CrosshairFiles {
            vmt: vmt.is_file().then_some(vmt),
            vtf,
        })
    }

    /// A crosshair's folder name, or `NotFound` for an unknown id.
    fn crosshair_folder(&self, id: &CrosshairId) -> Result<String, LibraryError> {
        self.db
            .read(|conn| {
                conn.query_row(
                    "SELECT folder_name FROM library_crosshairs WHERE id = ?1",
                    [id.as_str()],
                    |r| r.get(0),
                )
                .optional()
            })
            .map_err(library_db)?
            .ok_or_else(|| crosshair_not_found(id))
    }

    /// `library/crosshairs/<folder>/<folder>.vtf`; the caller checks `folder` is plain.
    fn crosshair_vtf(&self, folder: &str) -> PathBuf {
        self.dir
            .library_dir(LibraryKind::Crosshairs)
            .join(folder)
            .join(format!("{folder}.vtf"))
    }
}

fn crosshair_not_found(id: &CrosshairId) -> LibraryError {
    LibraryError::NotFound {
        what: format!("crosshair {id}"),
    }
}

/// Writes v5's crosshair folder: `<folder>.vtf`, `<folder>.vmt` and, when given, `preview.png`.
fn write_crosshair_files(
    dest: &Path,
    folder: &str,
    vtf: &[u8],
    preview: Option<&[u8]>,
) -> Result<(), IoFailure> {
    create_dir_all(dest)?;
    let write = |file: String, bytes: &[u8]| {
        let target = dest.join(file);
        std::fs::write(&target, bytes).map_err(io_at(&target))
    };
    write(format!("{folder}.vtf"), vtf)?;
    write(format!("{folder}.vmt"), templates::vmt(folder).as_bytes())?;
    if let Some(preview) = preview {
        write("preview.png".to_owned(), preview)?;
    }
    Ok(())
}

/// Replaces each file with its bytes, all or nothing. Every new file is first written beside
/// its target as `<file>.<token>.tmp`; then, in order, the old file is copied to
/// `<file>.<token>.bak` and the temp renamed over it. Any failure puts back every file already
/// replaced and removes all temps and backups, so the folder is as it was.
fn replace_files(files: &[(PathBuf, &[u8])]) -> Result<(), IoFailure> {
    let token = new_id();
    let beside = |path: &Path, kind: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(".{token}.{kind}"));
        PathBuf::from(name)
    };
    let temps: Vec<PathBuf> = files.iter().map(|(p, _)| beside(p, "tmp")).collect();
    let backups: Vec<PathBuf> = files.iter().map(|(p, _)| beside(p, "bak")).collect();
    let discard = |paths: &[PathBuf]| {
        for path in paths {
            let _ = remove_file_if_present(path);
        }
    };

    for ((_, bytes), temp) in files.iter().zip(&temps) {
        if let Err(e) = std::fs::write(temp, bytes) {
            discard(&temps);
            return Err(io_at(temp)(e));
        }
    }

    // `backed_up[k]` says whether file k had an old copy, so a rollback restores or removes it.
    let mut backed_up = Vec::with_capacity(files.len());
    for (k, (path, _)) in files.iter().enumerate() {
        let swapped = match std::fs::copy(path, &backups[k]) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(io_at(path)(e)),
        }
        .and_then(|had_old| {
            backed_up.push(had_old);
            std::fs::rename(&temps[k], path).map_err(io_at(path))
        });
        if let Err(failure) = swapped {
            for (j, (replaced, _)) in files.iter().enumerate().take(k).rev() {
                let _ = if backed_up[j] {
                    std::fs::rename(&backups[j], replaced)
                } else {
                    std::fs::remove_file(replaced)
                };
            }
            discard(&temps);
            discard(&backups);
            return Err(failure);
        }
    }
    discard(&backups);
    Ok(())
}

/// True when a row other than `except` has `name` in any case; shared by both checks.
/// Folding runs in Rust, since SQLite's `lower()` folds ASCII only.
fn name_taken(
    conn: &rusqlite::Connection,
    name: &str,
    except: Option<&CrosshairId>,
) -> rusqlite::Result<bool> {
    let wanted = name.to_lowercase();
    let mut stmt = conn.prepare("SELECT id, name FROM library_crosshairs")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let other: String = row.get(1)?;
        if except.is_none_or(|e| e.as_str() != id) && other.to_lowercase() == wanted {
            return Ok(true);
        }
    }
    Ok(false)
}

/// True for the two canvas sizes the maker can show, 32x32 and 64x64.
fn is_canvas_size(image: &RgbaImage) -> bool {
    matches!((image.width(), image.height()), (32, 32) | (64, 64))
}

/// The drawing as TF2 gets it: 64x64, a 32x32 drawing with each texel doubled to a 2x2 block.
fn exported(drawing: &RgbaImage) -> RgbaImage {
    if drawing.width() == 64 {
        return drawing.clone();
    }
    let side = drawing.width() as usize;
    let mut out = Vec::with_capacity(side * side * 16);
    for y in 0..side * 2 {
        for x in 0..side * 2 {
            let at = ((y / 2) * side + x / 2) * 4;
            out.extend_from_slice(&drawing.pixels()[at..at + 4]);
        }
    }
    RgbaImage::from_pixels(64, 64, out).unwrap_or_else(|| drawing.clone())
}

/// The drawing as `source.png` at its own size; an in-memory encode failure is a VTF encode error.
fn drawing_png(drawing: &RgbaImage) -> Result<Vec<u8>, PaintError> {
    encode_png(drawing.clone()).ok_or_else(|| {
        PaintError::Vtf(VtfError::Encode {
            message: "the drawing could not be encoded as a PNG".to_owned(),
        })
    })
}

/// Whether the managed folder holds a crosshair-only weapon script from before 6.0.2: a
/// `tf_weapon_*.txt` with no `printname` key, which every stock script has. Such a script
/// stops the weapon from firing, and an apply replaces it. Reads only the managed folder; an
/// unreadable folder or file counts as nothing to repair.
pub fn crosshair_scripts_need_repair(tf2: &Tf2Install) -> bool {
    let scripts = tf2.custom_dir().join(MANAGED_DIR).join("scripts");
    let Ok(entries) = std::fs::read_dir(&scripts) else {
        return false;
    };
    entries.flatten().map(|e| e.path()).any(|path| {
        is_weapon_script_file(&path)
            && std::fs::read(&path).is_ok_and(|text| {
                !String::from_utf8_lossy(&text)
                    .to_ascii_lowercase()
                    .contains("printname")
            })
    })
}

/// Counts the `tf_weapon_*.txt` files (extension in any case) directly in `<folder>/scripts/`,
/// the files TF2 reads as weapon scripts. A missing folder has none.
fn weapon_scripts_in(folder: &Path) -> Result<usize, IoFailure> {
    let scripts = folder.join("scripts");
    let entries = match std::fs::read_dir(&scripts) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(io_at(&scripts)(e)),
    };
    let mut count = 0;
    for entry in entries {
        let path = entry.map_err(io_at(&scripts))?.path();
        count += usize::from(is_weapon_script_file(&path));
    }
    Ok(count)
}

/// A `tf_weapon_*.txt` file (extension in any case), the name TF2 reads as a weapon script.
fn is_weapon_script_file(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("tf_weapon_"))
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
        && path.is_file()
}

/// Whether library folder `folder` has its texture in the managed folder under `custom`. A
/// name that is not plain is never installed.
fn texture_in_game(custom: &Path, folder: &str) -> bool {
    let name = templates::texture_name(folder);
    let materials = custom.join(MANAGED_DIR).join(MATERIALS_DIR);
    is_plain_name(folder) && materials.join(format!("{name}.vtf")).is_file()
}

/// Whether `dir` or a folder below it holds anything but empty folders.
fn holds_files(dir: &Path) -> Result<bool, IoFailure> {
    for entry in std::fs::read_dir(dir).map_err(io_at(dir))? {
        let entry = entry.map_err(io_at(dir))?;
        let is_dir = entry.file_type().map_err(io_at(dir))?.is_dir();
        if !is_dir || holds_files(&entry.path())? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Encodes decoded pixels as a PNG for v5's `preview.png`; `None` when encoding fails.
fn encode_png(img: RgbaImage) -> Option<Vec<u8>> {
    let (width, height) = (img.width(), img.height());
    let buffer = image::RgbaImage::from_raw(width, height, img.into_pixels())?;
    let mut out = Cursor::new(Vec::new());
    buffer.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::paths::DataDir;

    fn library() -> (tempfile::TempDir, DataDir, Library) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::at(tmp.path().join("data"));
        let lib = Library::new(Database::open(&dir).unwrap(), dir.clone());
        (tmp, dir, lib)
    }

    fn insert(dir: &DataDir, id: &str, name: &str, source_type: &str) {
        rusqlite::Connection::open(dir.db_path())
            .unwrap()
            .execute(
                "INSERT INTO library_crosshairs (id, folder_name, name, source_type, source, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'local', '2026-10-21T00:00:00.000Z')",
                [id, &format!("f-{id}"), name, source_type],
            )
            .unwrap();
    }

    // A rival save that lands after the early check is still caught inside the write.
    #[test]
    fn the_in_transaction_check_rejects_a_name_taken_after_the_early_check() {
        let (_tmp, dir, lib) = library();
        insert(&dir, "rival", "ÉCLAIR", "png");
        let entry = CrosshairEntry {
            id: CrosshairId::new("mine"),
            folder_name: "f-mine".into(),
            name: "Éclair".into(),
            format: CrosshairFormat::Painted,
            provenance: Provenance::Local,
            created_at: now(),
            wanted_name: None,
        };
        let err = lib.record_new_painted(&entry).unwrap_err();
        assert!(
            matches!(&err, PaintError::NameTaken { name } if name == "Éclair"),
            "{err:?}"
        );
        assert_eq!(lib.list_crosshairs().unwrap().len(), 1);
    }

    // A rival that takes the name after the early check is still caught inside the rename.
    #[test]
    fn the_in_transaction_check_rejects_a_rename_to_a_name_taken_meanwhile() {
        let (_tmp, dir, lib) = library();
        insert(&dir, "old", "Dot", "painted");
        insert(&dir, "rival", "ÉCLAIR", "png");
        let err = lib
            .rename_painted(&CrosshairId::new("old"), "éclair")
            .unwrap_err();
        assert!(matches!(&err, PaintError::NameTaken { .. }), "{err:?}");
        lib.rename_painted(&CrosshairId::new("old"), "DOT").unwrap();
        let names: Vec<String> = lib
            .list_crosshairs()
            .unwrap()
            .iter()
            .map(|e| e.name().to_owned())
            .collect();
        assert_eq!(names, ["DOT", "ÉCLAIR"]);
    }

    // A crosshair deleted after its files were rewritten has no row left to rename.
    #[test]
    fn a_rename_of_a_crosshair_deleted_meanwhile_is_not_found() {
        let (_tmp, _dir, lib) = library();
        let err = lib
            .rename_painted(&CrosshairId::new("gone"), "Dot")
            .unwrap_err();
        assert!(
            matches!(&err, PaintError::NotFound { id } if id == "gone"),
            "{err:?}"
        );
    }

    /// Runs `f` while another connection, standing in for another process, holds an open
    /// write that adds a png crosshair named `name`, committed a moment after `f` starts.
    fn while_another_process_adds<T>(dir: &DataDir, name: &str, f: impl FnOnce() -> T) -> T {
        let other = rusqlite::Connection::open(dir.db_path()).unwrap();
        other.execute_batch("BEGIN IMMEDIATE").unwrap();
        other
            .execute(
                "INSERT INTO library_crosshairs (id, folder_name, name, source_type, source, created_at)
                 VALUES ('other', 'f-other', ?1, 'png', 'local', '2026-10-21T00:00:00.000Z')",
                [name],
            )
            .unwrap();
        std::thread::scope(|s| {
            s.spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                other.execute_batch("COMMIT").unwrap();
            });
            f()
        })
    }

    // An immediate transaction waits for the other writer and then sees its name; a deferred
    // one would read before that commit and fail to write on a stale snapshot.
    #[test]
    fn a_save_waits_for_another_process_and_sees_its_name() {
        let (_tmp, dir, lib) = library();
        let entry = CrosshairEntry {
            id: CrosshairId::new("mine"),
            folder_name: "f-mine".into(),
            name: "Éclair".into(),
            format: CrosshairFormat::Painted,
            provenance: Provenance::Local,
            created_at: now(),
            wanted_name: None,
        };
        let err = while_another_process_adds(&dir, "ÉCLAIR", || lib.record_new_painted(&entry))
            .unwrap_err();
        assert!(matches!(&err, PaintError::NameTaken { .. }), "{err:?}");
    }

    #[test]
    fn a_rename_waits_for_another_process_and_sees_its_name() {
        let (_tmp, dir, lib) = library();
        insert(&dir, "old", "Dot", "painted");
        let err = while_another_process_adds(&dir, "ÉCLAIR", || {
            lib.rename_painted(&CrosshairId::new("old"), "éclair")
        })
        .unwrap_err();
        assert!(matches!(&err, PaintError::NameTaken { .. }), "{err:?}");
        assert_eq!(lib.list_crosshairs().unwrap()[0].name(), "Dot");
    }
}
