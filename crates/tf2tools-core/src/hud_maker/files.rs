//! The game's files as the HUD Maker reads them: ordered layers (top first) of VPKs, loose
//! fonts or in-memory files, looked up by case-insensitive normalised game paths.

use std::cell::RefCell;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::changes::{check_cfg_path, check_switch_path};
use super::options::CfgPlace;
use crate::error::HudFilesError;
use crate::fonts::Platform;
use crate::fonts::keyvalues::{self, KvPair, KvSource, KvValue};
use crate::pak;

/// Entries larger than this are refused before they are read.
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// One file found through [`HudFiles`]: its bytes, the layer it came from and its spelling there.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HudFile {
    bytes: Vec<u8>,
    layer: usize,
    name: String,
}

impl HudFile {
    /// The file's contents.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The file's contents, owned.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// The index of the layer that supplied the file; 0 is the top.
    pub fn layer(&self) -> usize {
        self.layer
    }

    /// The normalised path as the layer spells it.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Where one indexed file's bytes live.
#[derive(Debug)]
enum Item {
    Memory(Vec<u8>),
    Loose(PathBuf),
    /// A file of a library HUD folder, checked again each time it is read.
    Hud {
        root: Arc<PathBuf>,
        rel: String,
    },
    Vpk {
        pak: Arc<PathBuf>,
        entry: vpk::entry::VPKEntry,
    },
}

/// One layer: lowercase key to the winning spelling and its source.
type Layer = BTreeMap<String, (String, Item)>;

/// Ordered file layers, top first, with the names each layer hid through case collisions.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HudFiles {
    /// Shared, so a switched view costs no copy of the files beneath it.
    layers: Vec<Arc<Layer>>,
    collisions: Vec<String>,
    /// Set for a HUD copy: the folder name it installs under in `tf/custom/`.
    install_folder: Option<String>,
    /// How many top layers hold the copy's own files (originals, then the copy).
    copy_layers: usize,
    /// The read-only `tf/cfg` index, keyed relative to `tf/cfg`; never a layer.
    tf_cfg: Arc<Layer>,
    /// The HUD's own switches over the layers, when this is a switched view.
    switches: Option<Arc<Switches>>,
}

/// A switch layer: whole files written over the layers, and paths taken out of them, plus
/// the settings files the options write under `tf/cfg`, by lowercase path.
#[derive(Debug)]
struct Switches {
    written: Arc<Layer>,
    removed: BTreeSet<String>,
    cfg: BTreeMap<String, CfgFile>,
}

/// A settings file an option writes under the game's `tf/cfg`: its path below `tf/cfg` and
/// its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CfgFile {
    path: String,
    bytes: Vec<u8>,
}

impl CfgFile {
    /// The file at `path` below `tf/cfg`; `None` when the path is no plain `.txt` or `.res`
    /// path at most three folders deep.
    pub fn new(path: &str, bytes: Vec<u8>) -> Option<CfgFile> {
        Some(CfgFile {
            path: check_cfg_path(path)?,
            bytes,
        })
    }

    /// The lowercase path below `tf/cfg`.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The file's bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// How a HUD copy is opened: the folder it installs as and the paths hidden from its layer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CopyView {
    install_folder: String,
    /// Lowercase normalised paths the Maker added over game files.
    hidden: BTreeSet<String>,
}

impl CopyView {
    /// A view installing as `install_folder` with `hidden` (any case) left out of the copy layer.
    pub fn new(install_folder: &str, hidden: impl IntoIterator<Item = String>) -> CopyView {
        let hidden = hidden
            .into_iter()
            .filter_map(|path| Some(normalise_game_path(&path)?.to_ascii_lowercase()))
            .collect();
        CopyView {
            install_folder: install_folder.to_owned(),
            hidden,
        }
    }
}

impl HudFiles {
    /// The game's files as Stages 1-6 read them: the `game` and `platform` VPKs that
    /// `tf/gameinfo.txt` lists, in its order (sound and missing paks skipped, loose folders
    /// ignored), then loose `tf/resource/*.ttf|*.otf`, then the loose English text files
    /// `GameText` reads. `tf_dir` is `<game>/tf`.
    pub fn open_game(tf_dir: &Path) -> Result<HudFiles, HudFilesError> {
        Self::open_over(tf_dir, Vec::new())
    }

    /// The game's files with the library HUD folder `hud_dir` as a read-only top layer, so a
    /// borrow reads that HUD as the game would with it installed. Only regular files are
    /// indexed (a symlink is skipped), `.tf2tools/` is left out and a file over 32 MiB is
    /// skipped. Nothing is ever written to the folder.
    pub fn open_hud(tf_dir: &Path, hud_dir: &Path) -> Result<HudFiles, HudFilesError> {
        refuse_link(hud_dir)?;
        Self::open_over(tf_dir, vec![loose_hud(hud_dir)])
    }

    /// Layers top first: `<hud>/.tf2tools/original`, `<hud>` minus `.tf2tools/` and hidden paths,
    /// then `open_game`'s; plus the `tf/cfg` index. Err `LinkedFolder` when `hud_dir`,
    /// `<hud>/.tf2tools` or `<hud>/.tf2tools/original` is a link.
    pub fn open_copy(
        tf_dir: &Path,
        hud_dir: &Path,
        view: &CopyView,
    ) -> Result<HudFiles, HudFilesError> {
        let meta = hud_dir.join(".tf2tools");
        let originals_dir = meta.join("original");
        for dir in [hud_dir, &meta, &originals_dir] {
            refuse_link(dir)?;
        }
        // Originals are read through `hud_dir`, so every read re-checks `.tf2tools` and
        // `original` for a link as well as the folders below them.
        let root = Arc::new(hud_dir.to_path_buf());
        let originals = match originals_dir.is_dir() {
            true => (loose_hud(&originals_dir).into_iter())
                .map(|(name, _)| {
                    let item = Item::Hud {
                        root: Arc::clone(&root),
                        rel: format!(".tf2tools/original/{name}"),
                    };
                    (name, item)
                })
                .collect(),
            false => Vec::new(),
        };
        let copy = loose_hud(hud_dir)
            .into_iter()
            .filter(|(rel, _)| {
                normalise_game_path(rel)
                    .is_some_and(|key| !view.hidden.contains(&key.to_ascii_lowercase()))
            })
            .collect();
        let mut files = Self::open_over(tf_dir, vec![originals, copy])?;
        files.install_folder = Some(view.install_folder.clone());
        files.copy_layers = 2;
        files.tf_cfg = Arc::new(tf_cfg_index(tf_dir));
        Ok(files)
    }

    /// Whether the copy's own layers (originals or copy files) supply `path`; false for
    /// files that are no copy.
    pub fn in_copy(&self, path: &str) -> bool {
        self.install_folder.is_some()
            && self
                .find_pristine(path)
                .is_some_and(|(layer, _, _)| layer - self.shift() < self.copy_layers)
    }

    /// A view of these files with `written` (path, bytes) on top and `removed` hidden; shares
    /// the layers. Paths failing `check_switch_path` are skipped. Later calls replace the layer.
    pub fn with_switches(&self, written: Vec<(String, Vec<u8>)>, removed: Vec<String>) -> HudFiles {
        let written: Layer = (written.into_iter())
            .filter_map(|(path, bytes)| {
                let key = check_switch_path(&path)?;
                Some((key.clone(), (key, Item::Memory(bytes))))
            })
            .collect();
        // A written path wins over a removed one, so the two sets never overlap.
        let removed = (removed.iter())
            .filter_map(|path| check_switch_path(path))
            .filter(|key| !written.contains_key(key))
            .collect();
        HudFiles {
            layers: self.layers.clone(),
            collisions: self.collisions.clone(),
            install_folder: self.install_folder.clone(),
            copy_layers: self.copy_layers,
            tf_cfg: Arc::clone(&self.tf_cfg),
            switches: Some(Arc::new(Switches {
                written: Arc::new(written),
                removed,
                cfg: BTreeMap::new(),
            })),
        }
    }

    /// This switched view with `cfg` as the settings files the options write; they read
    /// through `tf/cfg` over the game's own. Later calls replace them.
    pub fn with_switched_cfg(mut self, cfg: Vec<CfgFile>) -> HudFiles {
        let (written, removed) = match self.switches.as_deref() {
            Some(s) => (Arc::clone(&s.written), s.removed.clone()),
            None => (Arc::default(), BTreeSet::new()),
        };
        let cfg = cfg.into_iter().map(|f| (f.path.clone(), f)).collect();
        self.switches = Some(Arc::new(Switches {
            written,
            removed,
            cfg,
        }));
        self
    }

    /// The settings files the switch layer writes under `tf/cfg`, by path.
    pub fn switched_cfg(&self) -> Vec<CfgFile> {
        (self.switches.iter())
            .flat_map(|s| s.cfg.values().cloned())
            .collect()
    }

    /// As `read`, ignoring the switch layer (removed paths included).
    pub fn read_pristine(&self, path: &str) -> Result<Option<HudFile>, HudFilesError> {
        let Some((layer, name, item)) = self.find_pristine(path) else {
            return Ok(None);
        };
        Ok(Some(HudFile {
            bytes: read_item(name, item)?,
            layer,
            name: name.clone(),
        }))
    }

    /// (written, removed) lowercase paths of the switch layer, sorted.
    pub fn switched_paths(&self) -> (Vec<String>, Vec<String>) {
        match &self.switches {
            Some(s) => (
                s.written.keys().cloned().collect(),
                s.removed.iter().cloned().collect(),
            ),
            None => (Vec::new(), Vec::new()),
        }
    }

    /// Lowercase paths the copy's own layers hold, sorted; empty for files that are no copy.
    pub fn copy_paths(&self) -> Vec<String> {
        if self.install_folder.is_none() {
            return Vec::new();
        }
        let mut paths: Vec<String> = (self.layers.iter().take(self.copy_layers))
            .flat_map(|layer| layer.keys().cloned())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// How many layers a switched view puts above the others.
    fn shift(&self) -> usize {
        usize::from(self.switches.is_some())
    }

    /// The file at `rel` under `tf/cfg` from the read-only index; `Ok(None)` when it is not
    /// indexed, `Err` when it became a link or unreadable since.
    pub fn read_tf_cfg(&self, rel: &str) -> Result<Option<HudFile>, HudFilesError> {
        let Some(key) = normalise_game_path(rel).map(|k| k.to_ascii_lowercase()) else {
            return Ok(None);
        };
        if let Some(file) = self.switches.as_ref().and_then(|s| s.cfg.get(&key)) {
            return Ok(Some(HudFile {
                bytes: file.bytes.clone(),
                layer: self.layer_count(),
                name: file.path.clone(),
            }));
        }
        let Some((name, item)) = self.tf_cfg.get(&key) else {
            return Ok(None);
        };
        Ok(Some(HudFile {
            bytes: read_item(name, item)?,
            layer: self.layer_count(),
            name: name.clone(),
        }))
    }

    /// Marks the files a copy installed as `folder`; every existing layer then counts as the copy's
    /// (in-memory tests have no game layers), so `in_copy` is true for any path they hold.
    pub fn with_install_folder(mut self, folder: &str) -> HudFiles {
        self.install_folder = Some(folder.to_owned());
        self.copy_layers = self.layers.len();
        self
    }

    /// Replaces the `tf/cfg` index with in-memory files named relative to `tf/cfg`.
    pub fn with_tf_cfg(mut self, files: Vec<(String, Vec<u8>)>) -> HudFiles {
        let entries = files
            .into_iter()
            .map(|(name, bytes)| (name, Item::Memory(bytes)));
        self.tf_cfg = Arc::new(index_layer(entries, &mut self.collisions));
        self
    }

    /// Where `#base` target `rel` in copy file `from` lands, as a copy path, by the engine's rule
    /// for a copy (so `../../custom/<install folder>/...` lands inside); `None` when it leaves the
    /// copy or the files are no copy. Whether the copy holds it is not checked.
    pub(crate) fn copy_target(&self, from: &str, rel: &str) -> Option<String> {
        let folder = self.install_folder.as_deref()?;
        locate_in_copy(folder, from, rel, |_| true, |_| false).ok()
    }

    /// The folder a copy installs under; `None` for files that are no copy.
    pub fn install_folder(&self) -> Option<&str> {
        self.install_folder.as_deref()
    }

    fn open_over(
        tf_dir: &Path,
        top_layers: Vec<Vec<(String, Item)>>,
    ) -> Result<HudFiles, HudFilesError> {
        let gameinfo = tf_dir.join("gameinfo.txt");
        let no_gameinfo = || HudFilesError::NoGameInfo {
            path: tf_dir.to_path_buf(),
        };
        let bytes = match crate::fonts::read_regular_capped(&gameinfo, keyvalues::MAX_FILE_BYTES) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Err(no_gameinfo()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(no_gameinfo()),
            Err(source) => {
                return Err(HudFilesError::Io {
                    path: gameinfo,
                    source,
                });
            }
        };
        let text = String::from_utf8_lossy(&bytes);
        let file = keyvalues::parse(&text, Platform::Linux);
        let root = tf_dir.parent().unwrap_or(tf_dir);

        let mut files = HudFiles::empty();
        // Each top layer is pushed even when empty, so a copy's layer indexes stay fixed.
        for layer in top_layers {
            files.push_layer(layer.into_iter());
        }
        for pak_path in search_path_paks(&file.pairs, root) {
            if !pak_path.exists() {
                continue;
            }
            let pak = pak::open_vpk(&pak_path)?;
            let shared = Arc::new(pak_path);
            let entries = pak.tree.into_iter().map(|(name, entry)| {
                let item = Item::Vpk {
                    pak: Arc::clone(&shared),
                    entry,
                };
                (name, item)
            });
            files.push_layer(entries);
        }
        let fonts = loose_fonts(&tf_dir.join("resource"));
        if !fonts.is_empty() {
            files.push_layer(fonts.into_iter());
        }
        let lang = loose_lang(root);
        if !lang.is_empty() {
            files.push_layer(lang.into_iter());
        }
        Ok(files)
    }

    /// In-memory layers, top first; each name is normalised, invalid names are dropped.
    pub fn from_memory(layers: Vec<Vec<(String, Vec<u8>)>>) -> HudFiles {
        let mut files = HudFiles::empty();
        for layer in layers {
            files.push_layer(
                layer
                    .into_iter()
                    .map(|(name, bytes)| (name, Item::Memory(bytes))),
            );
        }
        files
    }

    /// The file at `path` from the topmost layer holding it, matched ignoring case;
    /// `Ok(None)` when no layer has it or the path is invalid.
    pub fn read(&self, path: &str) -> Result<Option<HudFile>, HudFilesError> {
        let Some((layer, name, item)) = self.find(path) else {
            return Ok(None);
        };
        Ok(Some(HudFile {
            bytes: read_item(name, item)?,
            layer,
            name: name.clone(),
        }))
    }

    fn empty() -> HudFiles {
        HudFiles {
            layers: Vec::new(),
            collisions: Vec::new(),
            install_folder: None,
            copy_layers: 0,
            tf_cfg: Arc::default(),
            switches: None,
        }
    }

    /// Whether any layer holds `path`, matched ignoring case.
    pub fn contains(&self, path: &str) -> bool {
        self.find(path).is_some()
    }

    /// Lowercase keys under `prefix` (matched ignoring case and normalised like a path, a
    /// trailing separator kept) across all layers, sorted, deduplicated; empty if it escapes.
    pub fn list(&self, prefix: &str) -> Vec<String> {
        let Some(prefix) = normalise_prefix(prefix) else {
            return Vec::new();
        };
        let switched = self.switches.as_deref();
        let mut keys: Vec<String> = (switched.map(|s| &*s.written).into_iter())
            .chain(self.layers.iter().map(|layer| &**layer))
            .flat_map(|layer| {
                layer
                    .range(prefix.clone()..)
                    .map(|(key, _)| key)
                    .take_while(|key| key.starts_with(&prefix))
            })
            .filter(|key| !switched.is_some_and(|s| s.removed.contains(*key)))
            .cloned()
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// How many layers there are.
    pub fn layer_count(&self) -> usize {
        self.layers.len() + self.shift()
    }

    /// Names hidden by another spelling of the same path in the same layer, sorted.
    pub fn collisions(&self) -> &[String] {
        &self.collisions
    }

    /// `path` merged with its chain, and the chain's other files as read; a missing start is
    /// empty, a failing start read is `Err`. A start over the KeyValues size cap is empty, as
    /// the filesystem loader treats it.
    pub(crate) fn load_kv(
        &self,
        path: &str,
        platform: Platform,
    ) -> Result<(Vec<KvPair>, KeptFiles), HudFilesError> {
        let (pairs, kept, _) = self.load_kv_checked(path, platform)?;
        Ok((pairs, kept))
    }

    /// [`load_kv`](Self::load_kv) plus whether any `#base` or `#include` target, at any depth,
    /// was skipped because no layer held it or it could not be read.
    pub(crate) fn load_kv_checked(
        &self,
        path: &str,
        platform: Platform,
    ) -> Result<(Vec<KvPair>, KeptFiles, bool), HudFilesError> {
        let (pairs, kept, missing) = self.load_kv_missing(path, platform)?;
        Ok((pairs, kept, !missing.is_empty()))
    }

    /// [`load_kv`](Self::load_kv) plus the landing name of every `#base` or `#include` target,
    /// at any depth and in load order, that no layer held or that could not be read.
    pub(crate) fn load_kv_missing(
        &self,
        path: &str,
        platform: Platform,
    ) -> Result<(Vec<KvPair>, KeptFiles, Vec<String>), HudFilesError> {
        let empty = || KeptFiles {
            start: normalise_game_path(path)
                .unwrap_or_default()
                .to_ascii_lowercase(),
            install_folder: self.install_folder.clone(),
            files: Arc::default(),
        };
        // The size is checked before reading, so a start over the cap is never read.
        if self
            .find(path)
            .is_some_and(|(_, _, item)| item_len(item) > keyvalues::MAX_FILE_BYTES)
        {
            return Ok((Vec::new(), empty(), Vec::new()));
        }
        let Some(file) = self.read(path)? else {
            return Ok((Vec::new(), empty(), Vec::new()));
        };
        let recorder = Recorder {
            files: self,
            read: RefCell::new(BTreeMap::new()),
            missing: RefCell::new(Vec::new()),
        };
        let key = file.name.to_ascii_lowercase();
        let pairs = keyvalues::load_from(&recorder, &key, &file.bytes, platform);
        let kept = KeptFiles {
            start: key,
            install_folder: self.install_folder.clone(),
            files: Arc::new(recorder.read.take()),
        };
        Ok((pairs, kept, recorder.missing.take()))
    }

    /// The id a `#base` target `rel` in the file `from` resolves to, or its landing name when
    /// it is absent; a copy follows [`locate_in_copy`], other files today's rule.
    fn locate(&self, from: &str, rel: &str) -> Result<String, String> {
        match &self.install_folder {
            Some(folder) => locate_in_copy(
                folder,
                from,
                rel,
                |key| self.contains(key),
                |rest| self.holds_cfg(rest),
            ),
            None => locate_in_game(from, rel, |key| self.contains(key)),
        }
    }

    /// Whether `rest` (lowercase, below `tf/cfg`) is a switched settings file or indexed.
    fn holds_cfg(&self, rest: &str) -> bool {
        (self.switches.as_ref()).is_some_and(|s| s.cfg.contains_key(rest))
            || self.tf_cfg.contains_key(rest)
    }

    /// Where `#base` target `rel` of copy file `from` lands when it names a settings file:
    /// under `tf/cfg` or inside the copy, by the engine's rule for a copy; `None` anywhere
    /// else or for files that are no copy. Whether the file is there is not checked.
    pub(crate) fn cfg_landing(&self, from: &str, rel: &str) -> Option<CfgPlace> {
        let folder = self.install_folder.as_deref()?;
        let at = locate_in_copy(folder, from, rel, |_| true, |_| true).ok()?;
        Some(match at.strip_prefix(CFG_ID_PREFIX) {
            Some(rest) => CfgPlace::Game(rest.to_owned()),
            None => CfgPlace::Hud(at),
        })
    }

    /// The topmost entry for `path`: the switch layer's (layer 0), none when it was removed,
    /// else the layers'.
    fn find(&self, path: &str) -> Option<(usize, &String, &Item)> {
        if let Some(switches) = &self.switches {
            let key = normalise_game_path(path)?.to_ascii_lowercase();
            if let Some((name, item)) = switches.written.get(&key) {
                return Some((0, name, item));
            }
            if switches.removed.contains(&key) {
                return None;
            }
        }
        self.find_pristine(path)
    }

    /// The topmost entry for `path` in the layers beneath any switch layer.
    fn find_pristine(&self, path: &str) -> Option<(usize, &String, &Item)> {
        let key = normalise_game_path(path)?.to_ascii_lowercase();
        let shift = self.shift();
        self.layers
            .iter()
            .enumerate()
            .find_map(|(i, layer)| layer.get(&key).map(|(name, item)| (i + shift, name, item)))
    }

    /// Indexes one layer below the existing ones.
    fn push_layer(&mut self, entries: impl Iterator<Item = (String, Item)>) {
        let layer = index_layer(entries, &mut self.collisions);
        self.layers.push(Arc::new(layer));
    }
}

/// Every source is indexed here, so the collision rule is the same everywhere: the spelling
/// first in byte order wins its key, and the others are added to `collisions`.
fn index_layer(
    entries: impl Iterator<Item = (String, Item)>,
    collisions: &mut Vec<String>,
) -> Layer {
    let mut named: Vec<(String, Item)> = entries
        .filter_map(|(name, item)| Some((normalise_game_path(&name)?, item)))
        .collect();
    named.sort_by(|a, b| a.0.cmp(&b.0));
    let mut layer = Layer::new();
    for (name, item) in named {
        match layer.entry(name.to_ascii_lowercase()) {
            Entry::Occupied(_) => collisions.push(name),
            Entry::Vacant(slot) => {
                slot.insert((name, item));
            }
        }
    }
    collisions.sort();
    layer
}

/// The bytes of one indexed item, re-checking a HUD folder file for links and every size cap.
fn read_item(name: &str, item: &Item) -> Result<Vec<u8>, HudFilesError> {
    let too_large = |bytes: u64| HudFilesError::TooLarge {
        path: name.to_owned(),
        bytes,
    };
    let bytes = match item {
        Item::Memory(bytes) => bytes.clone(),
        Item::Loose(file) => {
            let io = |source| HudFilesError::Io {
                path: file.clone(),
                source,
            };
            let len = std::fs::metadata(file).map_err(io)?.len();
            if len > MAX_ENTRY_BYTES {
                return Err(too_large(len));
            }
            match crate::fonts::read_regular_capped(file, MAX_ENTRY_BYTES).map_err(io)? {
                Some(bytes) => bytes,
                None => return Err(too_large(len)),
            }
        }
        Item::Hud { root, rel } => {
            let io = |source| HudFilesError::Io {
                path: root.join(rel),
                source,
            };
            let file = checked_hud_file(root, rel).map_err(io)?;
            match crate::fonts::read_regular_capped(&file, MAX_HUD_FILE_BYTES).map_err(io)? {
                Some(bytes) => bytes,
                None => return Err(too_large(MAX_HUD_FILE_BYTES + 1)),
            }
        }
        Item::Vpk { pak, entry } => {
            let len =
                u64::from(entry.dir_entry.preload_length) + u64::from(entry.dir_entry.file_length);
            if len > MAX_ENTRY_BYTES {
                return Err(too_large(len));
            }
            pak::read_entry(pak, name, entry)?
        }
    };
    Ok(bytes)
}

/// `#base` targets resolve through [`HudFiles::locate`]; a cfg id reads the `tf/cfg` index.
impl KvSource for HudFiles {
    type Id = String;

    fn resolve(&self, from: &String, rel: &str) -> Option<String> {
        self.locate(from, rel).ok()
    }

    fn read(&self, id: &String) -> Option<Vec<u8>> {
        let file = match id.strip_prefix(CFG_ID_PREFIX) {
            Some(rest) => self.read_tf_cfg(rest),
            None => HudFiles::read(self, id),
        };
        let bytes = file.ok()??.into_bytes();
        (bytes.len() as u64 <= keyvalues::MAX_FILE_BYTES).then_some(bytes)
    }
}

/// The prefix of a resolver id naming a `tf/cfg` index file; no indexed name can spell it,
/// since the walks skip names holding `<` or `>`.
const CFG_ID_PREFIX: &str = "<tf>/cfg/";

/// Today's rule for files that are no copy: `rel` against the including file's folder by
/// lowercase key; escaping the game root or held by no layer is absent.
fn locate_in_game(from: &str, rel: &str, holds: impl Fn(&str) -> bool) -> Result<String, String> {
    let dir = from.rsplit_once('/').map_or("", |(dir, _)| dir);
    let joined = format!("{dir}/{rel}");
    match normalise_game_path(&joined) {
        Some(key) => {
            let key = key.to_ascii_lowercase();
            if holds(&key) { Ok(key) } else { Err(key) }
        }
        None => Err(joined.to_ascii_lowercase()),
    }
}

/// The engine's rule for a copy installed as `folder`: the including file sits at
/// `tf/custom/<folder>/<dir>` (a copy key) or `tf/cfg/<dir>` (a cfg id), and `rel` is normalised
/// from there with `tf/` as root. Landing under `custom/<folder>/` reads the layers, under `cfg/`
/// the index (`holds_cfg` takes the rest below `cfg/`); anything else is absent. The `Err` is
/// the landing path below `tf/`, lowercase, with one `../` per level above `tf/`.
fn locate_in_copy(
    folder: &str,
    from: &str,
    rel: &str,
    holds_copy: impl Fn(&str) -> bool,
    holds_cfg: impl Fn(&str) -> bool,
) -> Result<String, String> {
    let place = match from.strip_prefix(CFG_ID_PREFIX) {
        Some(rest) => format!("cfg/{rest}"),
        None => format!("custom/{folder}/{from}"),
    };
    let dir = place.rsplit_once('/').map_or("", |(dir, _)| dir);
    let joined = format!("{dir}/{rel}");
    let (above, parts) = normalise_counting_above(&joined);
    let landing = || {
        let mut name = "../".repeat(above);
        name.push_str(&parts.join("/").to_ascii_lowercase());
        name
    };
    if above > 0 {
        return Err(landing());
    }
    match parts.as_slice() {
        [custom, f, rest @ ..]
            if !rest.is_empty()
                && custom.eq_ignore_ascii_case("custom")
                && f.eq_ignore_ascii_case(folder) =>
        {
            let key = rest.join("/").to_ascii_lowercase();
            if holds_copy(&key) {
                Ok(key)
            } else {
                Err(landing())
            }
        }
        [cfg, rest @ ..] if !rest.is_empty() && cfg.eq_ignore_ascii_case("cfg") => {
            let rest = rest.join("/").to_ascii_lowercase();
            if holds_cfg(&rest) {
                Ok(format!("{CFG_ID_PREFIX}{rest}"))
            } else {
                Err(landing())
            }
        }
        _ => Err(landing()),
    }
}

/// Like [`normalise_game_path`], but a `..` past the root is counted instead of refused:
/// (levels above the root, the segments left).
fn normalise_counting_above(path: &str) -> (usize, Vec<&str>) {
    let mut above = 0;
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    above += 1;
                }
            }
            part => parts.push(part),
        }
    }
    (above, parts)
}

/// A [`HudFiles`] source that keeps a copy of every file the chain reads.
struct Recorder<'a> {
    files: &'a HudFiles,
    read: RefCell<BTreeMap<String, Vec<u8>>>,
    /// The landing names of targets that could not be resolved or read, in load order.
    missing: RefCell<Vec<String>>,
}

impl KvSource for Recorder<'_> {
    type Id = String;

    fn resolve(&self, from: &String, rel: &str) -> Option<String> {
        self.files
            .locate(from, rel)
            .map_err(|landing| self.missing.borrow_mut().push(landing))
            .ok()
    }

    fn read(&self, id: &String) -> Option<Vec<u8>> {
        let Some(bytes) = KvSource::read(self.files, id) else {
            let landing = match (&self.files.install_folder, id.strip_prefix(CFG_ID_PREFIX)) {
                (_, Some(rest)) => format!("cfg/{rest}"),
                (Some(folder), None) => format!("custom/{}/{id}", folder.to_ascii_lowercase()),
                (None, None) => id.clone(),
            };
            self.missing.borrow_mut().push(landing);
            return None;
        };
        self.read.borrow_mut().insert(id.clone(), bytes.clone());
        Some(bytes)
    }
}

/// The files a KeyValues chain read at load besides its start, by lowercase key, so the chain
/// can be merged again over new start bytes without the layers.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct KeptFiles {
    start: String,
    /// The copy's install folder, so the chain resolves as [`HudFiles`] did; `None` otherwise.
    install_folder: Option<String>,
    /// Shared, so cloning a model or its scheme does not copy the bytes.
    files: Arc<BTreeMap<String, Vec<u8>>>,
}

impl KeptFiles {
    /// The chain as `HudFiles::load_kv` merges it when the start holds `bytes` and every other
    /// file is as it was at load; a file the load did not read is absent.
    pub(crate) fn load(&self, bytes: &[u8], platform: Platform) -> Vec<KvPair> {
        if bytes.len() as u64 > keyvalues::MAX_FILE_BYTES {
            return Vec::new();
        }
        keyvalues::load_from(self, &self.start, bytes, platform)
    }
}

/// One file a chain reached: its resolver id, its bytes, and whether it merges into the
/// start's root block (reached only through `#base` lines) rather than through an `#include`.
pub(crate) struct ChainFile {
    pub(crate) id: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) merged: bool,
}

impl KeptFiles {
    /// Every file the chain reaches from a start holding `bytes`, walked as `load_from` walks
    /// it (directive order, depth first, each file once, the same depth cap), so the merged
    /// files come out in the order the merge ranks them: own first, then each base followed by
    /// its own bases.
    pub(crate) fn chain(&self, bytes: &[u8], platform: Platform) -> Vec<ChainFile> {
        let mut visited = std::collections::HashSet::from([self.start.clone()]);
        let mut out = Vec::new();
        self.walk(
            &self.start,
            bytes,
            (platform, 0, true),
            (&mut visited, &mut out),
        );
        out
    }

    /// One file's directives, then each target's own, as `chain` describes.
    fn walk(
        &self,
        id: &String,
        bytes: &[u8],
        (platform, depth, merged): (Platform, usize, bool),
        (visited, out): (&mut std::collections::HashSet<String>, &mut Vec<ChainFile>),
    ) {
        if depth >= keyvalues::MAX_INCLUDE_DEPTH {
            return;
        }
        let file = keyvalues::parse(&String::from_utf8_lossy(bytes), platform);
        for directive in &file.directives {
            let (is_base, target) = match directive {
                keyvalues::KvDirective::Base(t) => (true, t),
                keyvalues::KvDirective::Include(t) => (false, t),
            };
            let Some(found) = KvSource::resolve(self, id, target) else {
                continue;
            };
            if !visited.insert(found.clone()) {
                continue;
            }
            let Some(bytes) = KvSource::read(self, &found) else {
                continue;
            };
            let merged = merged && is_base;
            out.push(ChainFile {
                id: found.clone(),
                bytes: bytes.clone(),
                merged,
            });
            self.walk(
                &found,
                &bytes,
                (platform, depth + 1, merged),
                (visited, out),
            );
        }
    }
}

/// Only the kept names, so a scheme's debug output does not dump file bytes.
impl std::fmt::Debug for KeptFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeptFiles")
            .field("start", &self.start)
            .field("install_folder", &self.install_folder)
            .field("files", &self.files.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl KvSource for KeptFiles {
    type Id = String;

    /// The files are kept under the ids the [`HudFiles`] resolver gave, so the same rule runs
    /// against the kept map.
    fn resolve(&self, from: &String, rel: &str) -> Option<String> {
        let holds = |key: &str| self.files.contains_key(key);
        match &self.install_folder {
            Some(folder) => locate_in_copy(folder, from, rel, holds, |rest| {
                self.files.contains_key(&format!("{CFG_ID_PREFIX}{rest}"))
            }),
            None => locate_in_game(from, rel, holds),
        }
        .ok()
    }

    fn read(&self, id: &String) -> Option<Vec<u8>> {
        self.files.get(id).cloned()
    }
}

/// The `_dir.vpk` files that `SearchPaths` lists under a `game` or `platform` id, in order,
/// resolved against the game folder; sound paks are left out.
fn search_path_paks(pairs: &[KvPair], root: &Path) -> Vec<PathBuf> {
    let search_paths = ["GameInfo", "FileSystem", "SearchPaths"]
        .iter()
        .try_fold(pairs, |pairs, name| block(pairs, name));
    let Some(search_paths) = search_paths else {
        return Vec::new();
    };
    search_paths
        .iter()
        .filter(|pair| {
            pair.key
                .split('+')
                .any(|id| id.eq_ignore_ascii_case("game") || id.eq_ignore_ascii_case("platform"))
        })
        .filter_map(|pair| match &pair.value {
            KvValue::Text(value) => Some(value.as_str()),
            KvValue::Block(_) => None,
        })
        .filter_map(|value| {
            let value = value
                .replace("|all_source_engine_paths|", "")
                .replace("|gameinfo_path|", "tf/");
            let stem = value
                .len()
                .checked_sub(4)
                .filter(|&at| value.is_char_boundary(at))
                .filter(|&at| value[at..].eq_ignore_ascii_case(".vpk"))
                .map(|at| &value[..at])?;
            let file = stem.rsplit('/').next().unwrap_or(stem);
            if file.to_ascii_lowercase().contains("_sound_") {
                return None;
            }
            Some(root.join(format!("{stem}_dir.vpk")))
        })
        .collect()
}

/// The first block named `name` (ignoring case) among `pairs`.
fn block<'a>(pairs: &'a [KvPair], name: &str) -> Option<&'a [KvPair]> {
    pairs.iter().find_map(|pair| match &pair.value {
        KvValue::Block(children) if pair.key.eq_ignore_ascii_case(name) => {
            Some(children.as_slice())
        }
        _ => None,
    })
}

/// Loose `.ttf` and `.otf` files directly in `resource_dir`, named `resource/<file>`.
fn loose_fonts(resource_dir: &Path) -> Vec<(String, Item)> {
    let Ok(dir) = std::fs::read_dir(resource_dir) else {
        return Vec::new();
    };
    dir.filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let (_, ext) = name.rsplit_once('.')?;
            let font = ext.eq_ignore_ascii_case("ttf") || ext.eq_ignore_ascii_case("otf");
            font.then(|| (format!("resource/{name}"), Item::Loose(entry.path())))
        })
        .collect()
}

/// Largest file `open_hud` indexes from a library HUD folder.
const MAX_HUD_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// The regular files of a HUD folder, named relative to it with `/`; the app's own
/// `.tf2tools/` metadata, symlinks and oversized files stay out.
fn loose_hud(hud_dir: &Path) -> Vec<(String, Item)> {
    let root = Arc::new(hud_dir.to_path_buf());
    walkdir::WalkDir::new(hud_dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !(e.depth() == 1 && e.file_name() == ".tf2tools"))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.metadata().is_ok_and(|m| m.len() <= MAX_HUD_FILE_BYTES))
        .filter_map(|e| {
            let rel = e.path().strip_prefix(hud_dir).ok()?;
            let parts: Option<Vec<&str>> = rel.iter().map(|p| p.to_str()).collect();
            let rel = parts?.join("/");
            if !indexable_name(&rel) {
                return None;
            }
            let item = Item::Hud {
                root: Arc::clone(&root),
                rel: rel.clone(),
            };
            Some((rel, item))
        })
        .collect()
}

/// Most files the `tf/cfg` index holds, and the deepest a file may sit below `tf/cfg`.
const MAX_CFG_FILES: usize = 4096;
const MAX_CFG_DEPTH: usize = 6;

/// The regular files under `tf/cfg`, named relative to it, within the KeyValues cap; read
/// through `tf_dir` so every read re-checks `cfg` and each folder below it for a link.
fn tf_cfg_index(tf_dir: &Path) -> Layer {
    let cfg = tf_dir.join("cfg");
    if !std::fs::symlink_metadata(&cfg).is_ok_and(|m| m.is_dir()) {
        return Layer::new();
    }
    let root = Arc::new(tf_dir.to_path_buf());
    let entries = walkdir::WalkDir::new(&cfg)
        .follow_links(false)
        .max_depth(MAX_CFG_DEPTH)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.metadata()
                .is_ok_and(|m| m.len() <= keyvalues::MAX_FILE_BYTES)
        })
        .filter_map(|e| {
            let rel = e.path().strip_prefix(&cfg).ok()?;
            let parts: Option<Vec<&str>> = rel.iter().map(|p| p.to_str()).collect();
            let rel = parts?.join("/");
            let item = Item::Hud {
                root: Arc::clone(&root),
                rel: format!("cfg/{rel}"),
            };
            indexable_name(&rel).then_some((rel, item))
        })
        .take(MAX_CFG_FILES);
    index_layer(entries, &mut Vec::new())
}

/// Whether a walked name may be indexed: `<` and `>` (invalid on Windows) are kept free for
/// the resolver's cfg ids, so no indexed name can spell one.
fn indexable_name(rel: &str) -> bool {
    !rel.contains(['<', '>'])
}

/// `LinkedFolder` when `dir` is itself a link, which could point anywhere.
fn refuse_link(dir: &Path) -> Result<(), HudFilesError> {
    if std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_symlink()) {
        return Err(HudFilesError::LinkedFolder {
            path: dir.to_path_buf(),
        });
    }
    Ok(())
}

/// The path of `rel` under `root` when no part of the way is a link and the end is a regular
/// file within the size cap; checked at every read, since the folder can change after indexing.
fn checked_hud_file(root: &Path, rel: &str) -> std::io::Result<PathBuf> {
    let refuse = |why: &str| {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            why.to_owned(),
        ))
    };
    let mut path = root.to_path_buf();
    for part in rel.split('/') {
        path.push(part);
        if std::fs::symlink_metadata(&path)?.is_symlink() {
            return refuse("a link");
        }
    }
    let meta = std::fs::symlink_metadata(&path)?;
    if !meta.is_file() {
        return refuse("not a regular file");
    }
    if meta.len() > MAX_HUD_FILE_BYTES {
        return refuse("too large");
    }
    Ok(path)
}

/// The game's loose English text files, which no VPK carries; bottom layer, so a packed copy
/// wins as it does in the game's search order.
fn loose_lang(game_root: &Path) -> Vec<(String, Item)> {
    [("tf", "tf_english.txt"), ("hl2", "gameui_english.txt")]
        .into_iter()
        .filter_map(|(dir, name)| {
            let path = game_root.join(dir).join("resource").join(name);
            // Like the fonts, a symlink is never followed out of the game folder.
            let regular = std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file());
            regular.then(|| (format!("resource/{name}"), Item::Loose(path)))
        })
        .collect()
}

/// An indexed file's size without reading it; a loose file whose metadata fails counts as 0,
/// so the read that follows reports the failure.
fn item_len(item: &Item) -> u64 {
    match item {
        Item::Memory(bytes) => bytes.len() as u64,
        Item::Loose(file) => std::fs::metadata(file).map_or(0, |m| m.len()),
        Item::Hud { root, rel } => std::fs::metadata(root.join(rel)).map_or(0, |m| m.len()),
        Item::Vpk { entry, .. } => {
            u64::from(entry.dir_entry.preload_length) + u64::from(entry.dir_entry.file_length)
        }
    }
}

/// A `list` prefix as a lowercase key prefix: normalised like a path, with a `/` kept when the
/// prefix names a folder (it ends in a separator, `.` or `..`); `None` when it escapes the root.
fn normalise_prefix(prefix: &str) -> Option<String> {
    let prefix = prefix.replace('\\', "/");
    let last = prefix.rsplit('/').next().unwrap_or("");
    let folder = matches!(last, "" | "." | "..");
    match normalise_game_path(&prefix) {
        Some(path) if folder => Some(format!("{}/", path.to_ascii_lowercase())),
        Some(path) => Some(path.to_ascii_lowercase()),
        // Only a prefix of separators and `.` normalises to nothing without escaping.
        None => prefix
            .split('/')
            .all(|part| matches!(part, "" | "."))
            .then(String::new),
    }
}

/// `\` to `/`, empty and `.` segments dropped, `..` popped; `None` if empty or escaping the root.
pub fn normalise_game_path(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_kv_checked_flags_a_base_that_is_not_there() {
        let layer = vec![
            (
                "resource/ui/a.res".to_owned(),
                b"#base \"b.res\"\n#base \"gone.res\"\n".to_vec(),
            ),
            (
                "resource/ui/b.res".to_owned(),
                b"\"R\" { X { xpos 1 } }".to_vec(),
            ),
        ];
        let files = HudFiles::from_memory(vec![layer]);
        let (pairs, _, missing) = files
            .load_kv_checked("resource/ui/a.res", Platform::Linux)
            .unwrap();
        assert!(missing && !pairs.is_empty());
        let (_, _, missing) = files
            .load_kv_checked("resource/ui/b.res", Platform::Linux)
            .unwrap();
        assert!(!missing);
    }

    fn find<'a>(pairs: &'a [KvPair], path: &[&str]) -> Option<&'a str> {
        let (first, rest) = path.split_first()?;
        let pair = pairs.iter().find(|p| p.key.eq_ignore_ascii_case(first))?;
        match (&pair.value, rest.is_empty()) {
            (KvValue::Text(text), true) => Some(text.as_str()),
            (KvValue::Block(children), false) => find(children, rest),
            _ => None,
        }
    }

    fn copy(folder: &str, hud: &[(&str, &str)], cfg: &[(&str, &str)]) -> HudFiles {
        let owned = |files: &[(&str, &str)]| -> Vec<(String, Vec<u8>)> {
            files
                .iter()
                .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
                .collect()
        };
        HudFiles::from_memory(vec![owned(hud)])
            .with_install_folder(folder)
            .with_tf_cfg(owned(cfg))
    }

    fn load(files: &HudFiles, path: &str) -> (Vec<KvPair>, Vec<String>) {
        let (pairs, _, missing) = files.load_kv_missing(path, Platform::Linux).unwrap();
        (pairs, missing)
    }

    #[test]
    fn m0rehud_climbs_to_tf_cfg_and_back_into_the_copy() {
        let files = copy(
            "m0rehud",
            &[
                (
                    "resource/ui/hudplayerhealth.res",
                    "#base \"../../../../cfg/m0rehud/m0re_font.txt\"\n\"R\" { A { xpos 1 } }",
                ),
                ("resource/ui/variant.res", "\"V\" { A { wide 7 } }"),
            ],
            &[(
                "m0rehud/m0re_font.txt",
                "#base \"../../custom/m0rehud/resource/ui/variant.res\"\n",
            )],
        );
        let (pairs, missing) = load(&files, "resource/ui/hudplayerhealth.res");
        assert!(missing.is_empty(), "{missing:?}");
        assert_eq!(find(&pairs, &["R", "A", "wide"]), Some("7"));
        assert_eq!(find(&pairs, &["R", "A", "xpos"]), Some("1"));
    }

    #[test]
    fn hexhud_climbs_six_levels_from_a_deep_file() {
        let deep = "resource/ui/customizations/crosshairs/hitmarker_size.res";
        let files = copy(
            "hexhud",
            &[(
                deep,
                "#base \"../../../../../../cfg/hexhud/hex.txt\"\n\"R\" { A { xpos 1 } }",
            )],
            &[("hexhud/hex.txt", "\"C\" { A { tall 9 } }")],
        );
        let (pairs, missing) = load(&files, deep);
        assert!(missing.is_empty());
        assert_eq!(find(&pairs, &["R", "A", "tall"]), Some("9"));
    }

    #[test]
    fn jumphud_climbs_three_levels_from_resource() {
        let files = copy(
            "jumphud",
            &[(
                "resource/clientscheme.res",
                "#base \"../../../cfg/jumphud/jh.txt\"\n\"Scheme\" { Colors { A \"1 1 1 255\" } }",
            )],
            &[("jumphud/jh.txt", "\"S\" { Colors { B \"2 2 2 255\" } }")],
        );
        let (pairs, _) = load(&files, "resource/clientscheme.res");
        assert_eq!(find(&pairs, &["Scheme", "Colors", "B"]), Some("2 2 2 255"));
    }

    #[test]
    fn jofrehud_reaches_back_from_tf_cfg_with_one_level() {
        let files = copy(
            "jofrehud-main",
            &[
                (
                    "resource/ui/a.res",
                    "#base \"../../../../cfg/jofre_menu.txt\"\n\"R\" { A { xpos 1 } }",
                ),
                ("new/x.res", "\"X\" { A { wide 3 } }"),
            ],
            &[(
                "jofre_menu.txt",
                "#base \"../custom/JofreHUD-main/new/x.res\"\n",
            )],
        );
        let (pairs, missing) = load(&files, "resource/ui/a.res");
        assert!(missing.is_empty());
        assert_eq!(find(&pairs, &["R", "A", "wide"]), Some("3"));
    }

    #[test]
    fn a_landing_outside_the_copy_and_tf_cfg_is_absent_and_recorded() {
        let files = copy(
            "m0rehud",
            &[(
                "resource/ui/a.res",
                "#base \"../../../../../../cfg/m0rehud/x.txt\"\n#base \"../../../../cosmoshud/resource/y.res\"\n#base \"gone.res\"\n\"R\" { A { xpos 1 } }",
            )],
            &[("m0rehud/x.txt", "\"C\" { A { tall 9 } }")],
        );
        let (pairs, missing) = load(&files, "resource/ui/a.res");
        assert_eq!(find(&pairs, &["R", "A", "tall"]), None);
        assert_eq!(
            missing,
            [
                "../../cfg/m0rehud/x.txt",
                "cosmoshud/resource/y.res",
                "custom/m0rehud/resource/ui/gone.res"
            ]
        );
    }

    #[test]
    fn a_hud_cfg_folder_and_tf_cfg_stay_apart() {
        let files = copy(
            "myhud",
            &[
                (
                    "resource/ui/a.res",
                    "#base \"../../cfg/menu.txt\"\n\"R\" { A { xpos 1 } }",
                ),
                (
                    "resource/ui/b.res",
                    "#base \"../../../../cfg/menu.txt\"\n\"R\" { A { xpos 1 } }",
                ),
                ("cfg/menu.txt", "\"H\" { A { wide 1 } }"),
            ],
            &[("menu.txt", "\"T\" { A { wide 2 } }")],
        );
        assert_eq!(
            find(&load(&files, "resource/ui/a.res").0, &["R", "A", "wide"]),
            Some("1")
        );
        assert_eq!(
            find(&load(&files, "resource/ui/b.res").0, &["R", "A", "wide"]),
            Some("2")
        );
    }

    #[test]
    fn kept_files_resolve_like_the_files() {
        let files = copy(
            "m0rehud",
            &[
                (
                    "resource/ui/hudplayerhealth.res",
                    "#base \"../../../../cfg/m0rehud/f.txt\"\n\"R\" { A { xpos 1 } }",
                ),
                ("resource/ui/variant.res", "\"V\" { A { wide 7 } }"),
            ],
            &[(
                "m0rehud/f.txt",
                "#base \"../../custom/m0rehud/resource/ui/variant.res\"\n",
            )],
        );
        let (_, kept) = files
            .load_kv("resource/ui/hudplayerhealth.res", Platform::Linux)
            .unwrap();
        let again = kept.load(
            b"#base \"../../../../cfg/m0rehud/f.txt\"\n\"R\" { A { xpos 2 } }",
            Platform::Linux,
        );
        assert_eq!(find(&again, &["R", "A", "wide"]), Some("7"));
        assert_eq!(find(&again, &["R", "A", "xpos"]), Some("2"));
    }

    #[test]
    fn stock_files_keep_todays_rule() {
        let files = HudFiles::from_memory(vec![vec![(
            "resource/ui/a.res".to_owned(),
            b"#base \"../../../../cfg/x.txt\"\n\"R\" { A { xpos 1 } }".to_vec(),
        )]]);
        let (_, _, missing) = files
            .load_kv_checked("resource/ui/a.res", Platform::Linux)
            .unwrap();
        assert!(missing);
    }
}
