//! HUD copies: an editable whole-folder copy of a library HUD, the files the Maker reads for
//! it, and where its base came from.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::made_huds::{
    CREDITS_FILE, HudAssets, MAKER_FILE, MAX_SLUG_CHARS, ORIGINAL_DIR, name_taken, old_folder,
    prune_pictures, read_adding, read_capped, read_row, readable_folder, recover_swap,
    replay_with_kept_copies, roll_back, write_build, write_copy_in_place,
};
use super::{
    HudEntry, HudId, IoFailure, Library, METADATA_DIR, Provenance, create_dir_all, folder_name,
    io_at, is_plain_name, library_db, new_id, now, tree_copy,
};
use crate::error::{
    CopyHudError, HudLoadError, InstallError, LibraryError, MakeHudError, UpdateCopyError,
};
use crate::fonts::Platform;
use crate::hud_maker::{
    ChangeNote, ChangeTarget, CopyBase, CopyView, HudFiles, HudModel, MakerBase, MakerFile,
    OWN_SCRIPT, both_changed, left_out,
};
use crate::paths::LibraryKind;

/// The name a left-out HUD option is listed under when no version of the HUD labels it.
const UNNAMED_OPTION: &str = "A HUD option";

/// Room kept free beyond a copy's own bytes, so a copy never fills the disk.
pub(super) const HEADROOM: u64 = 64 * 1024 * 1024;

/// A library HUD row as a copy needs it.
struct BaseRow {
    entry: HudEntry,
    store_hud_id: Option<String>,
    store_gb_id: Option<i64>,
}

impl Library {
    /// Replaces how free disk space is measured; tests use it to simulate a full disk.
    #[doc(hidden)]
    pub fn set_free_space_probe(&mut self, probe: fn(&Path) -> std::io::Result<u64>) {
        self.free_space = probe;
    }

    /// Makes an editable copy of the library HUD `base`: its folder copied byte for byte
    /// (links and `.tf2tools/` left out), a `maker.json` recording the base, and a `local` row
    /// named "<name> (copy)" or "(copy N)". The base is never changed.
    ///
    /// # Errors
    /// `NotFound`, `MadeHud`, `BaseMissing` (folder gone or a link), `NoSpace` before anything
    /// is written, `NameTaken` past "(copy 99)", `Io` or `Database` when writing fails; a
    /// failure leaves no folder, no row and nothing in the library tmp dir.
    pub fn copy_hud(&self, base: &HudId) -> Result<HudEntry, CopyHudError> {
        let row = self.base_row(base)?.ok_or_else(|| CopyHudError::NotFound {
            id: base.as_str().to_owned(),
        })?;
        if self.is_made_hud(base).map_err(copy_library)? {
            return Err(CopyHudError::MadeHud {
                id: base.as_str().to_owned(),
            });
        }
        let huds = self.dir.library_dir(LibraryKind::Huds);
        let src = huds.join(row.entry.folder_name());
        if !std::fs::symlink_metadata(&src).is_ok_and(|m| m.is_dir()) {
            return Err(CopyHudError::BaseMissing {
                id: base.as_str().to_owned(),
            });
        }
        let skip = |rel: &Path| rel == Path::new(METADATA_DIR);
        let bytes = tree_copy::tree_bytes(&src, &skip)?;
        self.check_space(bytes)
            .map_err(|(needed, free)| CopyHudError::NoSpace { needed, free })?;

        let base_name = row.entry.name().to_owned();
        let first = self
            .db
            .read(|conn| free_copy_name(conn, &base_name))
            .map_err(copy_db)?
            .ok_or_else(|| CopyHudError::NameTaken {
                name: base_name.clone(),
            })?;
        let record = self.copy_record(&row, &src)?;

        // Built under the tmp dir, whose guard removes it on every early return.
        let tmp = self.dir.tmp_dir();
        create_dir_all(&tmp)?;
        let scratch = tempfile::Builder::new()
            .tempdir_in(&tmp)
            .map_err(io_at(&tmp))?;
        let id = new_id();
        let short: String = first.chars().take(MAX_SLUG_CHARS).collect();
        let folder = folder_name(&short, &id);
        let build = scratch.path().join(&folder);
        tree_copy::copy_tree(&src, &build, &skip)?;
        let maker =
            MakerFile::new(Default::default(), Vec::new()).with_base(MakerBase::Copy(record));
        let maker_path = build.join(METADATA_DIR).join(MAKER_FILE);
        create_dir_all(&build.join(METADATA_DIR))?;
        std::fs::write(&maker_path, maker.to_json()).map_err(io_at(&maker_path))?;

        create_dir_all(&huds)?;
        let live = huds.join(&folder);
        std::fs::rename(&build, &live).map_err(io_at(&live))?;
        let inserted = self.insert_copy_row(&id, &folder, &base_name);
        if !matches!(inserted, Ok(Some(_))) {
            let _ = super::remove_dir_if_present(&live);
        }
        let entry = inserted?.ok_or(CopyHudError::NameTaken { name: base_name })?;
        Ok(self.refresh_hud_thumbnail(entry))
    }

    /// The base a copy records, or `None` for a HUD that is no copy (no `maker.json`, the
    /// default base, or a file this build cannot read).
    ///
    /// # Errors
    /// `NotFound` for no such row; `Database` when the lookup fails.
    pub fn copy_base(&self, id: &HudId) -> Result<Option<CopyBase>, LibraryError> {
        let folder =
            self.hud_folder(id)
                .map_err(library_db)?
                .ok_or_else(|| LibraryError::NotFound {
                    what: format!("HUD {id}"),
                })?;
        let live = self.dir.library_dir(LibraryKind::Huds).join(folder);
        Ok(read_maker(&readable_folder(live))
            .ok()
            .flatten()
            .and_then(|maker| match maker.base() {
                MakerBase::Copy(base) => Some(base.clone()),
                MakerBase::Default => None,
            }))
    }

    /// The files the HUD Maker reads for the made HUD `id`: the game's for a stock-based made
    /// HUD, the copy's layers over the game's for a copy.
    ///
    /// # Errors
    /// `NotFound`, `NotMade` without a `maker.json`, `Damaged` or `Newer` from the file,
    /// `Files` when the files cannot be opened, `Database` when the lookup fails.
    pub fn made_hud_files(&self, id: &HudId, tf_dir: &Path) -> Result<HudFiles, HudLoadError> {
        let folder = self
            .hud_folder(id)
            .map_err(|e| HudLoadError::Database {
                message: e.to_string(),
            })?
            .ok_or_else(|| HudLoadError::NotFound {
                id: id.as_str().to_owned(),
            })?;
        let live = readable_folder(self.dir.library_dir(LibraryKind::Huds).join(&folder));
        let maker = read_maker(&live)?.ok_or_else(|| HudLoadError::NotMade {
            id: id.as_str().to_owned(),
        })?;
        match maker.base() {
            MakerBase::Copy(base) => {
                let install = base.install_folder().unwrap_or(&folder);
                let view = CopyView::new(install, hidden_paths(&live, &maker));
                Ok(HudFiles::open_copy(tf_dir, &live, &view)?)
            }
            MakerBase::Default => Ok(HudFiles::open_game(tf_dir)?),
        }
    }

    /// Refuses `bytes` plus the headroom when the data dir's disk has less free, as
    /// `(needed, free)`. A probe that fails never blocks a copy.
    pub(super) fn check_space(&self, bytes: u64) -> Result<(), (u64, u64)> {
        let needed = bytes.saturating_add(HEADROOM);
        let free = (self.free_space)(self.dir.root()).unwrap_or(u64::MAX);
        if free < needed {
            return Err((needed, free));
        }
        Ok(())
    }

    /// The row of library HUD `id` with its store columns, when its folder is a plain name.
    fn base_row(&self, id: &HudId) -> Result<Option<BaseRow>, CopyHudError> {
        let row = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT store_hud_id, store_gb_id FROM library_huds WHERE id = ?1",
                    [id.as_str()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .map(|(store_hud_id, store_gb_id)| {
                    Ok(BaseRow {
                        entry: read_row(conn, id.as_str())?,
                        store_hud_id,
                        store_gb_id,
                    })
                })
                .transpose()
            })
            .map_err(copy_db)?;
        Ok(row.filter(|r| is_plain_name(r.entry.folder_name())))
    }

    /// What a copy of `row` (files at `src`) records about its base.
    fn copy_record(&self, row: &BaseRow, src: &Path) -> Result<CopyBase, CopyHudError> {
        let entry = &row.entry;
        let (store_id, origin, repo, hash) = match entry.provenance() {
            Provenance::Curated { hud_id } => {
                let repo: Option<String> = self
                    .db
                    .read(|conn| {
                        conn.query_row(
                            "SELECT repo FROM store_huds WHERE hud_id = ?1",
                            [hud_id],
                            |r| r.get(0),
                        )
                        .optional()
                    })
                    .map_err(copy_db)?;
                let hash = commit_suffix(entry.folder_name()).unwrap_or_default();
                (
                    format!("hud-db:{hud_id}"),
                    format!("hud-db {hud_id}"),
                    repo.unwrap_or_default(),
                    hash.to_owned(),
                )
            }
            Provenance::GameBanana { gb_id, .. } => (
                format!("gb:{gb_id}"),
                format!("GameBanana item {gb_id}"),
                String::new(),
                String::new(),
            ),
            Provenance::Local => (
                String::new(),
                "a file you imported".to_owned(),
                String::new(),
                String::new(),
            ),
        };
        // Deleting a HUD takes no write lock, so its row can vanish between reads.
        let tree = self.hud_tree_hash(entry.id()).map_err(|e| match e {
            LibraryError::Io { path, source } => CopyHudError::Io { path, source },
            LibraryError::NotFound { .. } => CopyHudError::NotFound {
                id: entry.id().as_str().to_owned(),
            },
            other @ LibraryError::InvalidWeaponScript { .. }
            | other @ LibraryError::Vpk(_)
            | other @ LibraryError::Vtf(_)
            | other @ LibraryError::Database { .. } => copy_library(other),
        })?;
        let install = self
            .install_name(
                src,
                row.store_hud_id.clone(),
                row.store_gb_id,
                entry.folder_name(),
            )
            .map_err(copy_install)?;
        Ok(CopyBase::new(entry.id().as_str(), entry.name())
            .with_author(entry.author())
            .with_origin(&origin)
            .with_store_id(&store_id)
            .with_repo(&repo)
            .with_hash(&hash)
            .with_tree(tree.hash())
            .with_base_folder(entry.folder_name())
            .with_base_updated_at(entry.updated_at())
            .with_install_folder(Some(&install)))
    }

    /// Inserts the copy's `local` row under the first free copy name, checked inside the
    /// write; `None` when every name was taken meanwhile.
    fn insert_copy_row(
        &self,
        id: &str,
        folder: &str,
        base_name: &str,
    ) -> Result<Option<HudEntry>, CopyHudError> {
        let timestamp = now();
        self.db
            .write(|conn| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let Some(name) = free_copy_name(&tx, base_name)? else {
                    return Ok(None);
                };
                tx.execute(
                    "INSERT INTO library_huds (id, folder_name, name, author, source,
                         created_at, updated_at)
                     VALUES (?1, ?2, ?3, '', 'local', ?4, ?4)",
                    params![id, folder, name, timestamp],
                )?;
                let entry = read_row(&tx, id)?;
                tx.commit()?;
                Ok(Some(entry))
            })
            .map_err(copy_db)
    }
}

/// "<name> (copy)", else "<name> (copy N)" for the first free N from 2 to 99.
fn free_copy_name(conn: &rusqlite::Connection, base: &str) -> rusqlite::Result<Option<String>> {
    let candidates = std::iter::once(format!("{base} (copy)"))
        .chain((2..=99).map(|n| format!("{base} (copy {n})")));
    for name in candidates {
        if !name_taken(conn, &name, None)? {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

/// A curated folder's trailing commit: 7 to 40 hex digits after the last `-`.
pub(super) fn commit_suffix(folder: &str) -> Option<&str> {
    let (_, hash) = folder.rsplit_once('-')?;
    ((7..=40).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hash)
}

/// The made HUD's `maker.json` under `folder`, `None` when there is none.
pub(super) fn read_maker(folder: &Path) -> Result<Option<MakerFile>, HudLoadError> {
    let path = folder.join(METADATA_DIR).join(MAKER_FILE);
    if !std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
        return Ok(None);
    }
    let (maker, _) = MakerFile::parse(&read_capped(&path)?)?;
    Ok(Some(maker))
}

/// The paths the Maker added over game files, hidden so the copy's layers give the pristine
/// HUD: sources with no kept original, asset paths, our animation script and the credits.
pub(super) fn hidden_paths(folder: &Path, maker: &MakerFile) -> Vec<String> {
    let originals = folder.join(METADATA_DIR).join(ORIGINAL_DIR);
    let mut hidden: Vec<String> = maker
        .sources()
        .keys()
        .filter(|path| !is_regular_file_under(&originals, path))
        .cloned()
        .collect();
    hidden.extend(maker.changes().iter().filter_map(|c| match c.target() {
        ChangeTarget::Asset(asset) => Some(asset.path().to_owned()),
        ChangeTarget::Set(_)
        | ChangeTarget::Remove(_)
        | ChangeTarget::AddBlock(_)
        | ChangeTarget::Option(_)
        | ChangeTarget::Credit(_)
        | ChangeTarget::Animation(_)
        | ChangeTarget::AnimationAppend(_) => None,
    }));
    hidden.extend(read_adding(folder));
    hidden.push(OWN_SCRIPT.to_owned());
    hidden.push(CREDITS_FILE.to_owned());
    hidden
}

/// True when the normalised game path `rel` names a regular file under `root` reached with
/// no link on the way.
pub(super) fn is_regular_file_under(root: &Path, rel: &str) -> bool {
    let mut path = root.to_path_buf();
    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_symlink()) {
        return false;
    }
    let rel = Path::new(rel);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return false;
    }
    for part in rel.components() {
        path.push(part);
        match std::fs::symlink_metadata(&path) {
            Ok(m) if m.is_symlink() => return false,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file())
}

fn copy_db(e: rusqlite::Error) -> CopyHudError {
    CopyHudError::Database {
        message: e.to_string(),
    }
}

fn copy_library(e: LibraryError) -> CopyHudError {
    CopyHudError::Database {
        message: e.to_string(),
    }
}

/// `install_name` only reads the HUD's files and the store table, so it fails with `Io` or
/// `Database`; the other variants never reach here.
fn copy_install(e: InstallError) -> CopyHudError {
    match e {
        InstallError::Io { path, source } => CopyHudError::Io { path, source },
        other @ InstallError::NotInLibrary { .. }
        | other @ InstallError::Database { .. }
        | other @ InstallError::ExternalHuds(_)
        | other @ InstallError::GameMissing { .. } => CopyHudError::Database {
            message: other.to_string(),
        },
    }
}

impl From<IoFailure> for CopyHudError {
    fn from(IoFailure { path, source }: IoFailure) -> Self {
        Self::Io { path, source }
    }
}

/// Whether a copy's base has a newer version from the same source.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CopyUpdate {
    /// Nothing newer from the copy's source.
    UpToDate,
    /// A library HUD holds the newer version.
    InLibrary {
        /// The library HUD to update from.
        from: HudId,
        /// Its name.
        name: String,
    },
    /// The HUD store has a newer commit that is not downloaded yet.
    InStore,
}

/// Whether the store has a newer version of an unedited store HUD than the library holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DownloadUpdate {
    /// The store lists a different commit than the downloaded one.
    Newer,
    /// The download is the store's current commit.
    Current,
    /// No versions to compare: not a curated download, no commit in its folder name, no
    /// store row, or an edited HUD, which `copy_update` covers.
    Unknown,
}

/// A library HUD's tree hash and how many files were read to work it out.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TreeHash {
    hash: String,
    files_read: usize,
}

impl TreeHash {
    /// SHA-256 hex over the sorted lines of lowercase path and file SHA-256.
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// How many files were read rather than taken from the cache.
    pub fn files_read(&self) -> usize {
        self.files_read
    }
}

/// The tree cache's file under the data dir.
const TREE_CACHE_FILE: &str = "hud_tree_cache.json";

/// How many library folders the tree cache remembers.
const TREE_CACHE_FOLDERS: usize = 512;

/// One file's cached hash, valid while its size and modified time stay the same.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedFile {
    size: u64,
    mtime_ns: u64,
    sha: String,
}

/// Per-file hashes by library folder, each folder stamped with when it was last used.
#[derive(Debug, Clone, Default)]
pub(crate) struct TreeCache {
    folders: BTreeMap<String, (u64, BTreeMap<String, CachedFile>)>,
}

impl TreeCache {
    /// Marks `folder` the most recently used, adding it when new.
    pub(crate) fn touch(&mut self, folder: &str) {
        let next = self
            .folders
            .values()
            .map(|(used, _)| *used)
            .max()
            .unwrap_or(0)
            + 1;
        self.folders.entry(folder.to_owned()).or_default().0 = next;
    }

    /// Drops the least recently used folders until at most `keep` remain.
    pub(crate) fn trim(&mut self, keep: usize) {
        while self.folders.len() > keep {
            let oldest = (self.folders.iter())
                .min_by_key(|(_, (used, _))| *used)
                .map(|(name, _)| name.clone());
            let Some(oldest) = oldest else { break };
            self.folders.remove(&oldest);
        }
    }

    /// How many folders are cached.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.folders.len()
    }

    /// Whether `folder` is cached.
    #[cfg(test)]
    pub(crate) fn contains(&self, folder: &str) -> bool {
        self.folders.contains_key(folder)
    }

    /// The cache in `bytes`; anything unreadable is an empty cache.
    fn parse(bytes: &[u8]) -> TreeCache {
        let Ok(Value::Object(root)) = serde_json::from_slice::<Value>(bytes) else {
            return TreeCache::default();
        };
        let mut cache = TreeCache::default();
        for (folder, entry) in root {
            let used = entry.get("used").and_then(Value::as_u64).unwrap_or(0);
            let mut files = BTreeMap::new();
            for (rel, file) in entry
                .get("files")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                let parts = file.as_array().map(Vec::as_slice);
                if let Some([size, mtime, sha]) = parts
                    && let (Some(size), Some(mtime_ns), Some(sha)) =
                        (size.as_u64(), mtime.as_u64(), sha.as_str())
                {
                    let sha = sha.to_owned();
                    files.insert(
                        rel.clone(),
                        CachedFile {
                            size,
                            mtime_ns,
                            sha,
                        },
                    );
                }
            }
            cache.folders.insert(folder, (used, files));
        }
        cache
    }

    fn to_json(&self) -> Vec<u8> {
        let root: serde_json::Map<String, Value> = (self.folders.iter())
            .map(|(folder, (used, files))| {
                let files: serde_json::Map<String, Value> = (files.iter())
                    .map(|(rel, f)| (rel.clone(), json!([f.size, f.mtime_ns, f.sha])))
                    .collect();
                (folder.clone(), json!({ "used": used, "files": files }))
            })
            .collect();
        Value::Object(root).to_string().into_bytes()
    }
}

impl Library {
    /// Whether the copy `id` has a newer base from its own source: a curated copy only from
    /// the store's current commit, any copy from a reinstall of its base's library folder
    /// with other bytes. A HUD that is no copy is `UpToDate`.
    ///
    /// # Errors
    /// `NotFound` for no such row; `Io` or `Database` when reading fails.
    pub fn copy_update(&self, id: &HudId) -> Result<CopyUpdate, LibraryError> {
        let Some(base) = self.copy_base(id)? else {
            return Ok(CopyUpdate::UpToDate);
        };
        let mut candidates = self.list_huds()?;
        candidates.retain(|c| c.id() != id);
        candidates.sort_by(|a, b| b.updated_at().cmp(a.updated_at()));
        for candidate in &candidates {
            if self.same_source(&base, candidate)? {
                return Ok(CopyUpdate::InLibrary {
                    from: candidate.id().clone(),
                    name: candidate.name().to_owned(),
                });
            }
        }
        let newer_in_store = match self.store_commit(&base)? {
            Some(current) if base.hash().is_empty() => {
                // A copy whose commit was never known learns it from a download of the
                // store's current commit with its own bytes, and remembers it.
                if self.download_matches(&base, &current, &candidates)? {
                    self.remember_commit(id, &current)?;
                    false
                } else {
                    true
                }
            }
            Some(current) => !commits_match(&current, base.hash()),
            None => false,
        };
        Ok(if newer_in_store {
            CopyUpdate::InStore
        } else {
            CopyUpdate::UpToDate
        })
    }

    /// Whether the store has a newer version of library HUD `id`, a curated download whose
    /// folder name ends in the commit it was downloaded at.
    ///
    /// # Errors
    /// `NotFound` for no such row; `Io` or `Database` when reading fails.
    pub fn download_update(&self, id: &HudId) -> Result<DownloadUpdate, LibraryError> {
        let hud = (self.list_huds()?.into_iter())
            .find(|h| h.id() == id)
            .ok_or_else(|| LibraryError::NotFound {
                what: format!("HUD {id}"),
            })?;
        let Provenance::Curated { hud_id } = hud.provenance() else {
            return Ok(DownloadUpdate::Unknown);
        };
        if self.is_made_hud(id)? {
            return Ok(DownloadUpdate::Unknown);
        }
        let (Some(downloaded), Some(current)) =
            (commit_suffix(hud.folder_name()), self.store_hash(hud_id)?)
        else {
            return Ok(DownloadUpdate::Unknown);
        };
        Ok(match commits_match(&current, downloaded) {
            true => DownloadUpdate::Current,
            false => DownloadUpdate::Newer,
        })
    }

    /// The tree hash of library HUD `id`'s folder: regular files outside `.tf2tools/`, links
    /// skipped. Files whose size and modified time match the cache are not read again.
    ///
    /// # Errors
    /// `NotFound` for no such row; `Io` when a file cannot be read; `Database`.
    pub fn hud_tree_hash(&self, id: &HudId) -> Result<TreeHash, LibraryError> {
        let folder =
            self.hud_folder(id)
                .map_err(library_db)?
                .ok_or_else(|| LibraryError::NotFound {
                    what: format!("HUD {id}"),
                })?;
        let dir = self.dir.library_dir(LibraryKind::Huds).join(&folder);
        let cache_path = self.dir.root().join(TREE_CACHE_FILE);
        let mut cache = std::fs::read(&cache_path)
            .map(|b| TreeCache::parse(&b))
            .unwrap_or_default();
        let cached = cache
            .folders
            .get(&folder)
            .map(|(_, f)| f.clone())
            .unwrap_or_default();

        let skip = |rel: &Path| rel == Path::new(METADATA_DIR);
        let mut fresh = BTreeMap::new();
        let mut lines = Vec::new();
        let mut files_read = 0;
        for rel in tree_copy::tree_files(&dir, &skip)? {
            let path = dir.join(&rel);
            let meta = std::fs::symlink_metadata(&path).map_err(io_at(&path))?;
            let mtime_ns = mtime_ns(&meta);
            let key = rel
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            let file = match cached.get(&key) {
                Some(hit) if hit.size == meta.len() && hit.mtime_ns == mtime_ns => hit.clone(),
                _ => {
                    files_read += 1;
                    CachedFile {
                        size: meta.len(),
                        mtime_ns,
                        sha: sha256_file(&path)?,
                    }
                }
            };
            lines.push(format!("{}\0{}\n", key.to_lowercase(), file.sha));
            fresh.insert(key, file);
        }
        lines.sort();
        let hash = sha256_hex(lines.concat().as_bytes());

        cache.folders.insert(folder.clone(), (0, fresh));
        cache.touch(&folder);
        let known: BTreeSet<String> = self
            .list_huds()?
            .iter()
            .map(|h| h.folder_name().to_owned())
            .collect();
        cache.folders.retain(|name, _| known.contains(name));
        cache.trim(TREE_CACHE_FOLDERS);
        // The cache only saves work, so a failed write is not the caller's failure.
        let _ = write_atomic(&cache_path, &cache.to_json());
        Ok(TreeHash { hash, files_read })
    }

    /// True when `candidate` is a newer version of `copy`'s base from the same source: the
    /// store's current commit of a curated base, or a reinstall of the base's library folder
    /// with a later time and other bytes. A made HUD never is.
    pub(crate) fn same_source(
        &self,
        copy: &CopyBase,
        candidate: &HudEntry,
    ) -> Result<bool, LibraryError> {
        if self.is_made_hud(candidate.id())? {
            return Ok(false);
        }
        if let (Some(store_hud), Provenance::Curated { hud_id }) = (
            copy.store_id().strip_prefix("hud-db:"),
            candidate.provenance(),
        ) && store_hud == hud_id
            && let Some(current) = self.store_commit(copy)?
            && let Some(commit) = commit_suffix(candidate.folder_name())
            && commits_match(&current, commit)
            && !commits_match(&current, copy.hash())
        {
            // With no recorded commit, the download is newer only when its bytes differ.
            if !copy.hash().is_empty() || self.hud_tree_hash(candidate.id())?.hash() != copy.tree()
            {
                return Ok(true);
            }
        }
        let reinstall = candidate.folder_name() == copy.base_folder()
            && candidate.updated_at() > copy.base_updated_at();
        Ok(reinstall && self.hud_tree_hash(candidate.id())?.hash() != copy.tree())
    }

    /// Whether a library row of the copy's store HUD at commit `current` holds exactly the
    /// copy's base bytes.
    fn download_matches(
        &self,
        base: &CopyBase,
        current: &str,
        candidates: &[HudEntry],
    ) -> Result<bool, LibraryError> {
        let Some(store_hud) = base.store_id().strip_prefix("hud-db:") else {
            return Ok(false);
        };
        for candidate in candidates {
            let same_hud = matches!(candidate.provenance(), Provenance::Curated { hud_id } if hud_id == store_hud);
            let at_current =
                commit_suffix(candidate.folder_name()).is_some_and(|c| commits_match(current, c));
            if same_hud && at_current && self.hud_tree_hash(candidate.id())?.hash() == base.tree() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Records `commit` as the copy `id`'s base commit in its `maker.json`.
    fn remember_commit(&self, id: &HudId, commit: &str) -> Result<(), LibraryError> {
        let folder =
            self.hud_folder(id)
                .map_err(library_db)?
                .ok_or_else(|| LibraryError::NotFound {
                    what: format!("HUD {id}"),
                })?;
        let live = self.dir.library_dir(LibraryKind::Huds).join(folder);
        let Ok(Some(maker)) = read_maker(&live) else {
            return Ok(());
        };
        let MakerBase::Copy(base) = maker.base().clone() else {
            return Ok(());
        };
        let maker = maker.with_base(MakerBase::Copy(base.with_hash(commit)));
        let path = live.join(METADATA_DIR).join(MAKER_FILE);
        write_atomic(&path, &maker.to_json()).map_err(io_at(&path))?;
        Ok(())
    }

    /// The store's current commit for a curated copy's base, when the store lists it.
    fn store_commit(&self, copy: &CopyBase) -> Result<Option<String>, LibraryError> {
        match copy.store_id().strip_prefix("hud-db:") {
            Some(store_hud) => self.store_hash(store_hud),
            None => Ok(None),
        }
    }

    /// The commit the curated store lists for `store_hud`, when it lists one.
    fn store_hash(&self, store_hud: &str) -> Result<Option<String>, LibraryError> {
        let hash: Option<String> = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT hash FROM store_huds WHERE hud_id = ?1",
                    [store_hud],
                    |r| r.get(0),
                )
                .optional()
            })
            .map_err(library_db)?;
        Ok(hash.filter(|h| !h.is_empty()))
    }
}

/// Two commits match when both are given and one starts with the other, ignoring case.
fn commits_match(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_ascii_lowercase(), b.to_ascii_lowercase());
    !a.is_empty() && !b.is_empty() && (a.starts_with(&b) || b.starts_with(&a))
}

pub(super) fn sha256_file(path: &Path) -> Result<String, IoFailure> {
    let mut file = std::fs::File::open(path).map_err(io_at(path))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf).map_err(io_at(path))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// A file's modified time in nanoseconds; 0 when the system has none to give.
pub(super) fn mtime_ns(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
        .unwrap_or(0)
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes `bytes` beside `path` and renames them over it.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// A copy updated to a newer base: its row, the changes that no longer fit, and the kept
/// changes whose target the update also changed.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CopyUpdated {
    entry: HudEntry,
    left_out: Vec<ChangeNote>,
    both_changed: Vec<ChangeNote>,
}

impl CopyUpdated {
    /// The copy's row after the update.
    pub fn entry(&self) -> &HudEntry {
        &self.entry
    }

    /// The player's changes that no longer fit the new base.
    pub fn left_out(&self) -> &[ChangeNote] {
        &self.left_out
    }

    /// The kept changes whose target the update also changed.
    pub fn both_changed(&self) -> &[ChangeNote] {
        &self.both_changed
    }
}

impl Library {
    /// Updates the copy `id` to the newer base `from`: the new base's files are copied anew,
    /// the copy's changes replayed over them and written as a save writes them, and the copy
    /// swapped in through `<folder>.old`. A failure leaves the copy as it was.
    ///
    /// # Errors
    /// `NotFound`, `NotACopy`, `SourceGone`, `MadeHud`, `NotSameSource`, `NoSpace` before
    /// anything is written; `Load` or `Save` when reading or writing fails.
    pub fn update_copy(
        &self,
        id: &HudId,
        from: &HudId,
        tf_dir: &Path,
        platform: Platform,
    ) -> Result<CopyUpdated, UpdateCopyError> {
        // Held from the first read, so a save checking this copy's base waits for the new one.
        let _writes = self.lock_writes();
        let text = |id: &HudId| id.as_str().to_owned();
        let folder = (self.hud_folder(id).map_err(save_db)?)
            .ok_or_else(|| UpdateCopyError::NotFound { id: text(id) })?;
        // A swap a crash interrupted is finished first, as a save does.
        recover_swap(&self.dir.library_dir(LibraryKind::Huds).join(&folder))
            .map_err(MakeHudError::from)?;
        // Deleting a HUD takes no write lock, so the copy or `from` can vanish meanwhile.
        let gone = || UpdateCopyError::SourceGone { id: text(from) };
        let copy = self
            .copy_base(id)
            .map_err(|e| save_library(e, || UpdateCopyError::NotFound { id: text(id) }))?
            .ok_or_else(|| UpdateCopyError::NotACopy { id: text(id) })?;
        let row = (self.base_row(from).map_err(save_copy)?)
            .ok_or_else(|| UpdateCopyError::SourceGone { id: text(from) })?;
        if self.is_made_hud(from).map_err(|e| save_library(e, gone))? {
            return Err(UpdateCopyError::MadeHud { id: text(from) });
        }
        if !self
            .same_source(&copy, &row.entry)
            .map_err(|e| save_library(e, gone))?
        {
            return Err(UpdateCopyError::NotSameSource { id: text(from) });
        }
        let huds = self.dir.library_dir(LibraryKind::Huds);
        let src = huds.join(row.entry.folder_name());
        if !std::fs::symlink_metadata(&src).is_ok_and(|m| m.is_dir()) {
            return Err(UpdateCopyError::SourceGone { id: text(from) });
        }
        let skip_meta = |rel: &Path| rel == Path::new(METADATA_DIR);
        let bytes = tree_copy::tree_bytes(&src, &skip_meta).map_err(MakeHudError::from)?;
        self.check_space(bytes)
            .map_err(|(needed, free)| UpdateCopyError::NoSpace { needed, free })?;

        let live = huds.join(&folder);
        let previous =
            read_maker(&live)?.ok_or_else(|| UpdateCopyError::NotACopy { id: text(id) })?;
        let record = self.copy_record(&row, &src).map_err(save_copy)?;
        // The copy's base as it reads now, to tell which kept changes the update also touched.
        // Both bases are switched by the same choices, so a switch's own effect never reads as
        // a change the update made.
        let old_base = self.made_hud_files(id, tf_dir)?;
        let old_options = self.hud_options(&old_base, Some(previous.base()), platform);
        let old_files = (old_options.switch(&old_base, previous.changes()))
            .map_err(HudLoadError::from)?
            .into_files();
        let old_model = HudModel::load(&old_files, platform).map_err(HudLoadError::from)?;

        // Stage: the new base's files, then the copy's own `.tf2tools/` except its originals.
        let tmp = self.dir.tmp_dir();
        create_dir_all(&tmp).map_err(MakeHudError::from)?;
        let scratch = tempfile::Builder::new()
            .tempdir_in(&tmp)
            .map_err(io_at(&tmp))
            .map_err(MakeHudError::from)?;
        let stage = scratch.path().join(&folder);
        tree_copy::copy_tree(&src, &stage, &skip_meta).map_err(MakeHudError::from)?;
        let skip_originals = |rel: &Path| rel == Path::new(ORIGINAL_DIR);
        let live_meta = live.join(METADATA_DIR);
        if std::fs::symlink_metadata(&live_meta).is_ok_and(|m| m.is_dir()) {
            tree_copy::copy_tree(&live_meta, &stage.join(METADATA_DIR), &skip_originals)
                .map_err(MakeHudError::from)?;
        }

        // Replay over the staged base exactly as reopening does, then write as a save does.
        let install = record.install_folder().unwrap_or(&folder).to_owned();
        let base = HudFiles::open_copy(tf_dir, &stage, &CopyView::new(&install, Vec::new()))
            .map_err(HudLoadError::from)?;
        let options = self.hud_options(&base, Some(previous.base()), platform);
        let switched = (options.switch(&base, previous.changes())).map_err(HudLoadError::from)?;
        // A choice the new base no longer offers is named by the old base's label; one the old
        // base did not offer either is still counted, under a plain name.
        let gone: Vec<ChangeNote> = (switched.left_out(previous.changes()).into_iter())
            .filter_map(|change| match change.target() {
                ChangeTarget::Option(choice) => {
                    Some(old_options.label(choice.id()).unwrap_or(UNNAMED_OPTION))
                }
                ChangeTarget::Set(_)
                | ChangeTarget::Remove(_)
                | ChangeTarget::AddBlock(_)
                | ChangeTarget::Asset(_)
                | ChangeTarget::Credit(_)
                | ChangeTarget::Animation(_)
                | ChangeTarget::AnimationAppend(_) => None,
            })
            .map(ChangeNote::option)
            .collect();
        let kept = switched.kept(previous.changes());
        let files = switched.into_files();
        let pristine = HudModel::load(&files, platform).map_err(HudLoadError::from)?;
        let mut assets = HudAssets::new();
        let (edited, _) = replay_with_kept_copies(&pristine, &kept, &stage, &mut assets);
        let base = MakerBase::Copy(record);
        let build = scratch.path().join("build");
        write_build(
            &build, &folder, &pristine, &edited, &assets, &files, platform, &base,
        )?;
        // The staged base holds nothing the Maker added, so nothing is removed as stale.
        let fresh = MakerFile::new(Default::default(), Vec::new());
        write_copy_in_place(
            &stage,
            &build,
            &scratch.path().join("bytes"),
            &files,
            &fresh,
        )?;
        prune_pictures(&stage, edited.applied())?;
        let mut left_out = left_out(&kept, edited.applied());
        left_out.extend(gone);
        let both_changed =
            both_changed(&old_files, &old_model, &files, &pristine, edited.applied());

        // Swap through `<folder>.old` so the copy is never half replaced.
        let name = self.copy_name(id)?;
        let old = old_folder(&live);
        std::fs::rename(&live, &old)
            .map_err(io_at(&old))
            .map_err(MakeHudError::from)?;
        let swapped = std::fs::rename(&stage, &live)
            .map_err(io_at(&live))
            .map_err(MakeHudError::from)
            .and_then(|()| {
                self.update_made_row(id, &name)?
                    .ok_or(MakeHudError::NameTaken { name: name.clone() })
            });
        let entry = match swapped {
            Ok(entry) => {
                let _ = super::remove_dir_if_present(&old);
                entry
            }
            Err(e) => {
                roll_back(&live, &old, &scratch.path().join("failed"));
                return Err(e.into());
            }
        };
        Ok(CopyUpdated {
            entry: self.refresh_hud_thumbnail(entry),
            left_out,
            both_changed,
        })
    }

    /// The copy's current name, kept through an update.
    fn copy_name(&self, id: &HudId) -> Result<String, UpdateCopyError> {
        let entry = self
            .db
            .read(|conn| read_row(conn, id.as_str()))
            .map_err(save_db)?;
        Ok(entry.name().to_owned())
    }
}

fn save_db(e: rusqlite::Error) -> UpdateCopyError {
    UpdateCopyError::Save(MakeHudError::Database {
        message: e.to_string(),
    })
}

/// `NotFound` becomes `gone()`, the error for whichever HUD that step read.
fn save_library(e: LibraryError, gone: impl FnOnce() -> UpdateCopyError) -> UpdateCopyError {
    match e {
        LibraryError::Io { path, source } => MakeHudError::Io { path, source }.into(),
        LibraryError::NotFound { .. } => gone(),
        other @ LibraryError::InvalidWeaponScript { .. }
        | other @ LibraryError::Vpk(_)
        | other @ LibraryError::Vtf(_)
        | other @ LibraryError::Database { .. } => MakeHudError::Database {
            message: other.to_string(),
        }
        .into(),
    }
}

/// Its callers, `base_row` and `copy_record`, read the source HUD, so `NotFound` means it is
/// gone; the space and name checks are `copy_hud`'s own and never reach here.
fn save_copy(e: CopyHudError) -> UpdateCopyError {
    match e {
        CopyHudError::Io { path, source } => MakeHudError::Io { path, source }.into(),
        CopyHudError::NotFound { id } => UpdateCopyError::SourceGone { id },
        other @ CopyHudError::BaseMissing { .. }
        | other @ CopyHudError::MadeHud { .. }
        | other @ CopyHudError::NameTaken { .. }
        | other @ CopyHudError::NoSpace { .. }
        | other @ CopyHudError::Database { .. } => MakeHudError::Database {
            message: other.to_string(),
        }
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CopyHudError, HudId, Library, LibraryError, TreeCache, UpdateCopyError, save_copy,
        save_library,
    };
    use crate::db::Database;
    use crate::paths::{DataDir, LibraryKind};

    /// A library holding one local HUD `hud-1` in folder `somehud`.
    fn library_with_hud() -> (tempfile::TempDir, Library) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::at(tmp.path().join("data"));
        let lib = Library::new(Database::open(&dir).unwrap(), dir.clone());
        rusqlite::Connection::open(dir.db_path())
            .unwrap()
            .execute(
                "INSERT INTO library_huds (id, folder_name, name, author, source, created_at,
                     updated_at)
                 VALUES ('hud-1', 'somehud', 'somehud', '', 'local', '2026-11-08T00:00:00Z',
                     '2026-11-08T00:00:00Z')",
                [],
            )
            .unwrap();
        let folder = dir.library_dir(LibraryKind::Huds).join("somehud");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("info.vdf"), "\"somehud\" {}").unwrap();
        (tmp, lib)
    }

    #[test]
    fn a_hud_deleted_mid_copy_is_not_found() {
        let (_tmp, lib) = library_with_hud();
        let id = HudId::new("hud-1");
        let row = lib.base_row(&id).unwrap().unwrap();
        lib.delete_hud(&id).unwrap();
        let src = lib.dir.library_dir(LibraryKind::Huds).join("somehud");
        let err = lib.copy_record(&row, &src).unwrap_err();
        assert!(
            matches!(&err, CopyHudError::NotFound { id } if id == "hud-1"),
            "{err:?}"
        );
    }

    #[test]
    fn a_hud_gone_while_checking_an_update_takes_the_step_s_gone_error() {
        let gone = LibraryError::NotFound {
            what: "HUD hud-1".into(),
        };
        let err = save_library(gone, || UpdateCopyError::SourceGone { id: "hud-1".into() });
        assert!(
            matches!(&err, UpdateCopyError::SourceGone { id } if id == "hud-1"),
            "{err:?}"
        );
    }

    #[test]
    fn an_update_source_gone_mid_update_is_source_gone() {
        let err = save_copy(CopyHudError::NotFound { id: "hud-1".into() });
        assert!(
            matches!(&err, UpdateCopyError::SourceGone { id } if id == "hud-1"),
            "{err:?}"
        );
    }

    #[test]
    fn the_cache_keeps_the_512_most_recently_used_folders() {
        let mut cache = TreeCache::default();
        for n in 0..513 {
            cache.touch(&format!("hud-{n}"));
        }
        cache.touch("hud-0");
        cache.trim(512);
        assert_eq!(cache.len(), 512);
        assert!(cache.contains("hud-0") && !cache.contains("hud-1"));
    }
}
