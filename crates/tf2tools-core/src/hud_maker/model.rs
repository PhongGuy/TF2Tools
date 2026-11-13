//! The Stage 1 HUD model: the documents the maker reads, the scheme, and the part tree the
//! panel rules resolve them into.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use super::changes::{
    AddBlock, AssetFile, Change, ChangeTarget, RemoveKey, SetKey, check_hud_path,
};
use super::document::{KvValueId, writable};
use super::fonts::{FONT_DIR, KEPT_DIR};
use super::options::OptionEdit;
use super::{
    AnimEvent, BoardMode, CodeToggled, EditRefused, HiddenBy, HudAnimations, HudFiles, KvDocument,
    KvEntry, MeterKind, PanelRule, PartId, RankLayout, Scene, SceneRule, Scheme,
};
use crate::error::HudFilesError;
use crate::fonts::Platform;
use crate::fonts::keyvalues::write_pairs;

const HUDLAYOUT: &str = "scripts/hudlayout.res";

/// A part's path in the tree: names joined by `/` as spelled by the first of any same-named
/// sibling blocks, which are one part.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub struct PartKey(String);

impl PartKey {
    /// A key from its text, such as `HudPlayerStatus/HudPlayerHealth`.
    pub fn new(text: impl Into<String>) -> PartKey {
        PartKey(text.into())
    }

    /// The key's text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn child(parent: Option<&PartKey>, segment: &str) -> PartKey {
        match parent {
            Some(parent) => PartKey(format!("{}/{segment}", parent.0)),
            None => PartKey(segment.to_owned()),
        }
    }
}

/// One line of a part's settings and where it was read: file, block path and line.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyLine {
    key: String,
    text: String,
    file: String,
    block: Vec<String>,
    line: u32,
    cond: Option<String>,
    /// For a line in a copy's `#base` file: the entry file and block an edit writes instead.
    through: Option<(String, Vec<String>)>,
}

impl KeyLine {
    /// The line `entry` at `block` (the block path the line sits in) of the located `within`.
    fn new(within: &Located<'_>, block: &[String], entry: &KvEntry) -> KeyLine {
        KeyLine {
            key: entry.key().to_owned(),
            text: entry.text().unwrap_or_default().to_owned(),
            file: within.file.to_owned(),
            block: block.to_vec(),
            line: entry.line(),
            cond: entry.cond().map(str::to_owned),
            through: within.owner.map(|_| within.written_at(block)),
        }
    }

    /// For a line read from a copy's `#base` file, the (entry file, block path) an edit of it
    /// writes; `None` for a line in the file it is written to.
    pub(super) fn through(&self) -> Option<(&str, &[String])> {
        (self.through.as_ref()).map(|(file, block)| (file.as_str(), block.as_slice()))
    }

    /// The key as spelled on this line.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The value as written.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The lowercase game path of the file the line is in.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The block path as spelled, the file's root block first.
    pub fn block(&self) -> &[String] {
        &self.block
    }

    /// The 1-based line of the key.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The line's conditional, such as `$WIN32`, if it has one.
    pub fn cond(&self) -> Option<&str> {
        self.cond.as_deref()
    }
}

/// A part's settings resolved once at load: the winning line, every line of each key in the
/// winning block and in every block (all keyed lowercase), the blocks read in load order and
/// the block absent keys are added to.
#[derive(Debug, Clone)]
struct Values {
    winners: HashMap<String, KeyLine>,
    lines: HashMap<String, Vec<KeyLine>>,
    chain: HashMap<String, Vec<KeyLine>>,
    blocks: Vec<(String, Vec<String>)>,
    own_block: (String, Vec<String>),
}

/// One panel in the part tree: a hudlayout container, a rule's own panel or a control under it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Part {
    key: PartKey,
    name: String,
    parent: Option<PartKey>,
    owner: Option<PartId>,
    root: bool,
    class: Option<String>,
    readable: bool,
    from_base: bool,
    base_incomplete: bool,
    toggled: Option<&'static CodeToggled>,
    values: Values,
}

impl Part {
    /// The part's key.
    pub fn key(&self) -> &PartKey {
        &self.key
    }

    /// The block name as spelled, without any `#n` suffix.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The enclosing part, if any.
    pub fn parent(&self) -> Option<&PartKey> {
        self.parent.as_ref()
    }

    /// The Stage 1 part this belongs to; `None` for a hudlayout container.
    pub fn owner(&self) -> Option<PartId> {
        self.owner
    }

    /// Whether this is a rule's own panel.
    pub fn is_root(&self) -> bool {
        self.root
    }

    /// The `ControlName` of its blocks, the last one in load order.
    pub fn class(&self) -> Option<&str> {
        self.class.as_deref()
    }

    /// Whether every file the part reads loaded and the HUD does not hide this scene; an
    /// unreadable part is shown but not edited.
    pub fn is_readable(&self) -> bool {
        self.readable
    }

    /// Whether a file the part reads is a `#base` stub, drawn from the merged tree: shown and
    /// pickable but never edited. Never true for a readable part.
    pub fn is_from_base(&self) -> bool {
        self.from_base && !self.readable
    }

    /// Whether a `#base` file behind a from-base part was not found, so the drawing may differ
    /// from the game's.
    pub fn base_incomplete(&self) -> bool {
        self.base_incomplete
    }

    /// The scene's code-toggled entry for this part: the scene root or a direct child of a
    /// rule's own panel named in the scene's table.
    pub fn code_toggled(&self) -> Option<&'static CodeToggled> {
        self.toggled
    }

    /// The winning line of `key` (ASCII case-insensitive): the last of the part's blocks in load
    /// order that sets it, an active `if_*` sub-block counting after its parent, and within a
    /// block the first entry whose conditional holds.
    pub fn value(&self, key: &str) -> Option<&KeyLine> {
        self.values.winners.get(&key.to_ascii_lowercase())
    }

    /// Every entry of `key` in the winning block and its `if_*` sub-blocks, in document order,
    /// including entries whose conditional fails and duplicates. Empty when no block sets it.
    pub fn lines(&self, key: &str) -> Vec<&KeyLine> {
        self.values
            .lines
            .get(&key.to_ascii_lowercase())
            .map(|lines| lines.iter().collect())
            .unwrap_or_default()
    }

    /// Every key the part resolves, as its winning line, sorted by key ignoring ASCII case.
    pub fn settings(&self) -> Vec<&KeyLine> {
        let mut rows: Vec<(&String, &KeyLine)> = self.values.winners.iter().collect();
        rows.sort_by(|a, b| a.0.cmp(b.0));
        rows.into_iter().map(|(_, line)| line).collect()
    }

    /// Every line of `key` in every block the part reads and those blocks' `if_*` sub-blocks,
    /// in load order then document order; inactive and conditional lines included.
    pub fn chain_lines(&self, key: &str) -> Vec<&KeyLine> {
        self.values
            .chain
            .get(&key.to_ascii_lowercase())
            .map(|lines| lines.iter().collect())
            .unwrap_or_default()
    }

    /// The part's last block in load order as (file, block path); a part with no block names
    /// the file it would be written to and an empty path.
    pub fn own_block(&self) -> (&str, &[String]) {
        (&self.values.own_block.0, &self.values.own_block.1)
    }

    /// Every block the part reads as (file, block path), in load order, whatever keys they
    /// hold; an `if_*` sub-block is not listed apart from its parent.
    pub fn blocks(&self) -> &[(String, Vec<String>)] {
        &self.values.blocks
    }
}

/// A scene file that is only `#base` lines, kept as its merged tree for drawing and never edited.
#[derive(Debug, Clone)]
struct Stub {
    doc: KvDocument,
    /// Some base, at any depth, was not found.
    incomplete: bool,
}

/// The read-only `#base` documents and each entry file's chain.
#[derive(Debug, Default)]
pub(super) struct BaseChains {
    /// Every file reached from an entry file, by resolver id.
    docs: BTreeMap<String, KvDocument>,
    /// Entry file to the bases merged into its root block, in load order (last base first).
    chains: BTreeMap<String, Vec<String>>,
    /// Bases that two or more entry files merge.
    shared: BTreeSet<String>,
    /// Entry files with a base that was absent.
    incomplete: BTreeSet<String>,
    /// Landing names of absent bases, sorted and deduplicated.
    missing: Vec<String>,
    /// Entry file to every copy path its chain names, found or missing, at any depth.
    named: BTreeMap<String, Vec<String>>,
}

impl BaseChains {
    /// Reads the chain of each entry file in `entries` (path and its bytes) from `files`.
    fn load(
        files: &HudFiles,
        entries: &[(String, Vec<u8>)],
        platform: Platform,
    ) -> Result<BaseChains, HudFilesError> {
        let mut chains = BaseChains::default();
        let mut users: BTreeMap<String, usize> = BTreeMap::new();
        // A missing landing inside the copy reads as a copy path; any other landing names no
        // file of the copy.
        let inside =
            (files.install_folder()).map(|f| format!("custom/{}/", f.to_ascii_lowercase()));
        for (path, bytes) in entries {
            let (_, kept, missing) = files.load_kv_missing(path, platform)?;
            if !missing.is_empty() {
                chains.incomplete.insert(path.clone());
            }
            let mut named: Vec<String> = (missing.iter())
                .filter_map(|m| m.strip_prefix(inside.as_deref()?))
                .map(str::to_owned)
                .collect();
            chains.missing.extend(missing);
            let mut merged = Vec::new();
            for file in kept.chain(bytes, platform) {
                // A `tf/cfg` id is no copy path.
                if !file.id.starts_with('<') {
                    named.push(file.id.clone());
                }
                if file.merged {
                    *users.entry(file.id.clone()).or_default() += 1;
                    merged.push(file.id.clone());
                }
                if let Ok(doc) = KvDocument::parse(&file.bytes) {
                    chains.docs.entry(file.id).or_insert(doc);
                }
            }
            // The walk ranks own first; the part resolver wants the lowest rank first.
            merged.reverse();
            if !merged.is_empty() {
                chains.chains.insert(path.clone(), merged);
            }
            named.sort();
            named.dedup();
            chains.named.insert(path.clone(), named);
        }
        chains.shared = (users.into_iter())
            .filter(|(_, n)| *n >= 2)
            .map(|(id, _)| id)
            .collect();
        chains.missing.sort();
        chains.missing.dedup();
        Ok(chains)
    }

    /// Whether a base merged into `entry` holds the block `path` (its root segment ignored),
    /// names matched ignoring case and conditionals, so a block held only as `[$WIN32]` and
    /// `[$X360]` variants counts.
    fn holds(&self, entry: &str, path: &[&str]) -> bool {
        let Some((_, below)) = path.split_first() else {
            return false;
        };
        self.roots(entry).any(|(_, root)| {
            let mut level = vec![root];
            for name in below {
                level = (level.into_iter())
                    .flat_map(|b| b.children())
                    .filter(|e| e.text().is_none() && e.key().eq_ignore_ascii_case(name))
                    .collect();
            }
            !level.is_empty()
        })
    }

    /// Whether `file`'s blocks are injected into many files rather than declared: a base two or
    /// more entries merge, unless the game also loads it as a panel's own file, as meter files
    /// that #base one another are.
    fn injects(&self, file: &str) -> bool {
        self.shared.contains(file) && !self.named.contains_key(file)
    }

    /// The root blocks merged into `entry`'s root, in load order, with their base ids.
    fn roots<'a>(&'a self, entry: &str) -> impl Iterator<Item = (&'a str, &'a KvEntry)> + 'a {
        (self.chains.get(entry).into_iter().flatten())
            .filter_map(|id| Some((id.as_str(), root_block(self.docs.get(id)?)?)))
    }
}

/// What a part's files allow, passed from a part to everything drawn under it.
#[derive(Clone, Copy)]
struct Access {
    readable: bool,
    from_base: bool,
    incomplete: bool,
}

impl Access {
    fn of(part: &Part) -> Access {
        Access {
            readable: part.readable,
            from_base: part.from_base,
            incomplete: part.base_incomplete,
        }
    }
}

/// The document at `path` in the editable set, else in the stubs.
fn doc_or_stub<'a>(
    docs: &'a BTreeMap<String, KvDocument>,
    stubs: &'a BTreeMap<String, Stub>,
    path: &str,
) -> Option<&'a KvDocument> {
    docs.get(path).or_else(|| stubs.get(path).map(|s| &s.doc))
}

/// Whether the file has a top-level `#base` or `#include` line.
fn has_directive(doc: &KvDocument) -> bool {
    doc.entries().iter().any(|e| {
        e.text().is_some()
            && (e.key().eq_ignore_ascii_case("#base") || e.key().eq_ignore_ascii_case("#include"))
    })
}

/// The merged tree of a stub at `path` as a document, or `None` when it merges to no block.
fn load_stub(
    files: &HudFiles,
    path: &str,
    platform: Platform,
) -> Result<Option<Stub>, HudFilesError> {
    let (pairs, _, incomplete) = files.load_kv_checked(path, platform)?;
    let text = write_pairs(&pairs);
    let doc = KvDocument::parse(text.as_bytes()).ok();
    Ok(doc
        .filter(|doc| root_block(doc).is_some())
        .map(|doc| Stub { doc, incomplete }))
}

/// The loaded events with `changes`' animation changes replayed: every valid whole event in
/// list order, then every valid append, so a feature's lines follow whichever body the
/// player last wrote. Returns the events, the applied indexes and the dropped count, or
/// `None` when no change is an animation.
fn replay_animations(
    loaded: &HudAnimations,
    changes: &[Change],
    platform: Platform,
) -> Option<(HudAnimations, Vec<usize>, usize)> {
    let any = (changes.iter()).any(|c| {
        matches!(
            c.target(),
            ChangeTarget::Animation(_) | ChangeTarget::AnimationAppend(_)
        )
    });
    if !any {
        return None;
    }
    let (mut anims, mut applied, mut dropped) = (loaded.clone(), Vec::new(), 0);
    // An event the HUD ships with a console command is never edited or appended to.
    let locked = |event: &str| {
        loaded
            .event(event)
            .is_some_and(AnimEvent::runs_game_commands)
    };
    for (i, change) in changes.iter().enumerate() {
        if let ChangeTarget::Animation(a) = change.target() {
            match a.check(platform).filter(|_| !locked(a.event())) {
                Some(commands) => {
                    anims.with_event(a.event(), commands);
                    applied.push(i);
                }
                None => dropped += 1,
            }
        }
    }
    for (i, change) in changes.iter().enumerate() {
        if let ChangeTarget::AnimationAppend(a) = change.target() {
            match a.check(platform).filter(|_| !locked(a.event())) {
                Some(commands) => {
                    anims.with_appended(a.event(), commands);
                    applied.push(i);
                }
                None => dropped += 1,
            }
        }
    }
    Some((anims, applied, dropped))
}

/// The client scheme's game path, read as a document beside the part files.
pub(super) const SCHEME_FILE: &str = "resource/clientscheme.res";

/// The Stage 1 files of one HUD, every scene's documents, resolved into one scene's parts.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HudModel {
    pristine: BTreeMap<String, KvDocument>,
    current: BTreeMap<String, KvDocument>,
    stubs: Arc<BTreeMap<String, Stub>>,
    /// Loaded from a HUD copy, whose parts read their `#base` chains.
    copy: bool,
    bases: Arc<BaseChains>,
    platform: Platform,
    loaded_scheme: Arc<Scheme>,
    scheme: Arc<Scheme>,
    scheme_cache: Arc<SchemeCache>,
    chat_scheme: Arc<Scheme>,
    rank_layout: Arc<RankLayout>,
    scene: Scene,
    board_mode: BoardMode,
    meter: MeterKind,
    minmode: bool,
    parts: Vec<Part>,
    index: HashMap<PartKey, usize>,
    all_problems: Vec<String>,
    problems: Vec<String>,
    hud_hides: Option<HiddenBy>,
    assets: Vec<(String, String)>,
    shipped_fonts: Arc<[String]>,
    loaded_animations: Arc<HudAnimations>,
    animations: Arc<HudAnimations>,
    replayed: Arc<[Change]>,
}

impl HudModel {
    /// Reads the HUD scene; see [`HudModel::load_scene`].
    pub fn load(files: &HudFiles, platform: Platform) -> Result<HudModel, HudFilesError> {
        HudModel::load_scene(files, platform, Scene::Hud)
    }

    /// Reads every scene's files, the scheme and the option files, and resolves `scene`'s
    /// parts; a missing, unparsable or rootless part file makes its parts unreadable and the
    /// rest load. Errors when a file cannot be read for another reason or the scheme fails to
    /// load.
    pub fn load_scene(
        files: &HudFiles,
        platform: Platform,
        scene: Scene,
    ) -> Result<HudModel, HudFilesError> {
        let scheme = Arc::new(Scheme::load(files, platform)?);
        let chat_scheme = Arc::new(Scheme::load_chat(files, platform)?);
        let rank_layout = Arc::new(RankLayout::read(files, platform));
        let mut docs = BTreeMap::new();
        let mut stubs = BTreeMap::new();
        let mut problems = Vec::new();
        let copy = files.install_folder().is_some();
        let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
        let paths = SceneRule::every().iter().flat_map(|rule| rule.files());
        // The scheme is kept as a document so its edits replay like part edits; it belongs
        // to no part, so a missing one is no problem.
        let quiet = [SCHEME_FILE];
        for &path in paths.chain(&quiet) {
            if docs.contains_key(path)
                || stubs.contains_key(path)
                || problems.iter().any(|p| p == path)
            {
                continue;
            }
            // Too large, unparsable or rootless reads like a missing file: the part goes read-only.
            let read = match files.read(path) {
                Ok(file) => file,
                Err(HudFilesError::TooLarge { .. }) => None,
                Err(e) => return Err(e),
            };
            if let Some(file) = &read {
                entries.push((path.to_owned(), file.bytes().to_vec()));
            }
            let parsed = read.and_then(|file| KvDocument::parse(file.bytes()).ok());
            match parsed {
                // A copy's stub is edited like any file: its parts read its bases, and edits
                // give it its own blocks.
                Some(doc) if root_block(&doc).is_some() || (copy && has_directive(&doc)) => {
                    docs.insert(path.to_owned(), doc);
                }
                // A file of only #base lines is drawn from its merged tree; one whose bases
                // all vanish merges to nothing and is a problem like a missing file.
                Some(doc) if !quiet.contains(&path) && has_directive(&doc) => {
                    match load_stub(files, path, platform)? {
                        Some(stub) => {
                            stubs.insert(path.to_owned(), stub);
                        }
                        None => problems.push(path.to_owned()),
                    }
                }
                // A HUD without a usable scheme file still shows its parts over the game's scheme,
                // and one without an option file just offers fewer options.
                _ if quiet.contains(&path) => {}
                _ => problems.push(path.to_owned()),
            }
        }
        problems.sort();
        // The game merges each file's #base chain, stock files too (the Phlogistinator's meter
        // is only a box over the plain file). A stock stub is already its merged tree.
        if !copy {
            entries.retain(|(path, _)| docs.contains_key(path));
        }
        let bases = BaseChains::load(files, &entries, platform)?;
        // A copy's stub whose bases all vanish merges to nothing, a problem like a missing file.
        let empty: Vec<String> = (docs.iter())
            .filter(|(path, doc)| root_block(doc).is_none() && bases.roots(path).next().is_none())
            .map(|(path, _)| path.clone())
            .collect();
        for path in empty {
            docs.remove(&path);
            if !quiet.contains(&path.as_str()) {
                problems.push(path);
            }
        }
        problems.sort();
        let animations = Arc::new(HudAnimations::load(files, platform));
        // Added font files must not overwrite one the HUD already ships, so remember those names.
        let shipped_fonts = [FONT_DIR, KEPT_DIR]
            .into_iter()
            .flat_map(|dir| files.list(&format!("{dir}/")))
            .filter_map(|path| path.rsplit('/').next().map(str::to_owned))
            .collect();

        let mut model = HudModel {
            pristine: docs.clone(),
            current: docs,
            stubs: Arc::new(stubs),
            copy,
            bases: Arc::new(bases),
            platform,
            loaded_scheme: Arc::clone(&scheme),
            scheme,
            scheme_cache: Arc::default(),
            chat_scheme,
            rank_layout,
            scene,
            board_mode: BoardMode::Casual,
            meter: MeterKind::Plain,
            minmode: false,
            parts: Vec::new(),
            index: HashMap::new(),
            all_problems: problems,
            problems: Vec::new(),
            hud_hides: None,
            assets: Vec::new(),
            shipped_fonts,
            loaded_animations: Arc::clone(&animations),
            animations,
            replayed: Arc::new([]),
        };
        model.resolve_parts(true);
        Ok(model)
    }

    /// The same current documents resolved into `scene`'s parts.
    pub fn with_scene(&self, scene: Scene) -> HudModel {
        let mut model = HudModel {
            pristine: self.pristine.clone(),
            current: self.current.clone(),
            stubs: Arc::clone(&self.stubs),
            copy: self.copy,
            bases: Arc::clone(&self.bases),
            platform: self.platform,
            loaded_scheme: Arc::clone(&self.loaded_scheme),
            scheme: Arc::clone(&self.scheme),
            scheme_cache: Arc::clone(&self.scheme_cache),
            chat_scheme: Arc::clone(&self.chat_scheme),
            rank_layout: Arc::clone(&self.rank_layout),
            scene,
            board_mode: self.board_mode,
            meter: self.meter,
            minmode: self.minmode,
            parts: Vec::new(),
            index: HashMap::new(),
            all_problems: self.all_problems.clone(),
            problems: Vec::new(),
            hud_hides: None,
            assets: self.assets.clone(),
            shipped_fonts: Arc::clone(&self.shipped_fonts),
            loaded_animations: Arc::clone(&self.loaded_animations),
            animations: Arc::clone(&self.animations),
            replayed: Arc::clone(&self.replayed),
        };
        model.resolve_parts(true);
        model
    }

    /// The files of the part's blocks and every #base path their chains name, found or
    /// missing, as lowercase copy paths; sorted, deduplicated. Empty for an unknown key.
    pub fn part_files(&self, key: &PartKey) -> Vec<String> {
        let Some(part) = self.index.get(key).and_then(|&i| self.parts.get(i)) else {
            return Vec::new();
        };
        // A stub's blocks sit in its bases, so the chain read is that of the one entry reaching
        // the block's file, keeping an off slot the stub names (missing) in reach. A base many
        // entries share stays out, or every entry's slots would land on the part.
        let mut files = Vec::new();
        for (file, _) in part.blocks() {
            files.push(file.to_ascii_lowercase());
            let shared = self.bases.shared.contains(file);
            let reaching = (self.bases.named.iter())
                .filter(|(entry, named)| *entry == file || (!shared && named.contains(file)))
                .flat_map(|(_, named)| named.iter().cloned());
            files.extend(reaching);
        }
        files.sort();
        files.dedup();
        files
    }

    /// Files reached through `#base`/`#include` from scene files and the scheme, sorted.
    pub fn base_files(&self) -> Vec<&str> {
        self.bases.docs.keys().map(String::as_str).collect()
    }

    /// Landing names (see files.rs) of bases that were absent, sorted, deduplicated (copies only).
    pub fn missing_bases(&self) -> Vec<&str> {
        self.bases.missing.iter().map(String::as_str).collect()
    }

    /// The scene files that are `#base` stubs (files with no root block of their own), sorted
    /// lowercase. A stock model draws them from their merged trees; a copy edits them.
    pub fn stub_files(&self) -> Vec<&str> {
        let copy_stubs = (self.pristine.iter())
            .filter(|(_, doc)| self.copy && root_block(doc).is_none())
            .map(|(path, _)| path.as_str());
        let mut stubs: Vec<&str> = self
            .stubs
            .keys()
            .map(String::as_str)
            .chain(copy_stubs)
            .collect();
        stubs.sort_unstable();
        stubs
    }

    /// The same current documents resolved for `mode`; only the Scoreboard differs by mode.
    pub fn with_board_mode(&self, mode: BoardMode) -> HudModel {
        let mut model = self.clone();
        model.board_mode = mode;
        model.resolve_parts(true);
        model
    }

    /// The scoreboard mode the model resolves under (Casual unless set).
    pub fn board_mode(&self) -> BoardMode {
        self.board_mode
    }

    /// The same current documents with the Item meter read from `kind`'s file.
    pub fn with_meter(&self, kind: MeterKind) -> HudModel {
        let mut model = self.clone();
        model.meter = kind;
        model.resolve_parts(true);
        model
    }

    /// The meter the Item meter reads (Plain unless set).
    pub fn meter(&self) -> MeterKind {
        self.meter
    }

    /// The same current documents resolved with TF2's minimal HUD setting `on`.
    pub fn with_minmode(&self, on: bool) -> HudModel {
        let mut model = self.clone();
        model.minmode = on;
        model.resolve_parts(true);
        model
    }

    /// Whether `_minmode` keys replace their bare keys.
    pub fn minmode(&self) -> bool {
        self.minmode
    }

    /// Whether any part of the current scene reads a line named `*_minmode` (an `if_*`
    /// sub-block's too), so the minimal HUD can look different from the normal one.
    pub fn has_minmode_keys(&self) -> bool {
        (self.parts.iter())
            .flat_map(|part| part.values.chain.keys())
            .any(|key| minmode_base(key).is_some())
    }

    /// The rule for the model's scene and mode.
    pub fn scene_rule(&self) -> &'static SceneRule {
        SceneRule::for_mode(self.scene, self.board_mode)
    }

    /// The scene whose parts the model resolves.
    pub fn scene(&self) -> Scene {
        self.scene
    }

    /// Whether the HUD's own files hide this scene, as measured when the scene was resolved.
    pub(crate) fn hud_hides(&self) -> Option<HiddenBy> {
        self.hud_hides
    }

    /// Rebuilds the part tree, its index and the scene's problems from the current documents;
    /// `measure` first decides whether the HUD hides the scene, else the last answer stands.
    fn resolve_parts(&mut self, measure: bool) {
        let rule = self.scene_rule();
        let reading = Reading {
            platform: self.platform,
            meter: self.meter,
            minmode: self.minmode,
        };
        self.problems = (self.all_problems.iter())
            .filter(|p| (rule.problem_files().iter()).any(|f| f.eq_ignore_ascii_case(p)))
            .cloned()
            .collect();
        // The lock reads the HUD's own files, never the player's changes, so a player who hid
        // the scene can still unhide it. Only a scene with a root can be hidden this way.
        if measure && rule.root().is_some() {
            self.index_parts(build_parts(
                &self.pristine,
                &self.stubs,
                &self.bases,
                reading,
                rule,
            ));
            self.hud_hides = self.measure_hidden_by_hud();
        }
        self.index_parts(build_parts(
            &self.current,
            &self.stubs,
            &self.bases,
            reading,
            rule,
        ));
        // A scene the HUD hides is never seen in game, so nothing in it is offered for editing.
        if self.hud_hides.is_some() {
            self.parts.iter_mut().for_each(|part| part.readable = false);
        }
    }

    fn index_parts(&mut self, parts: Vec<Part>) {
        self.parts = parts;
        self.index = self
            .parts
            .iter()
            .enumerate()
            .map(|(i, part)| (part.key.clone(), i))
            .collect();
    }

    /// Replays `changes` in order onto copies of the pristine documents and re-resolves parts.
    pub fn with_changes(&self, changes: &[Change]) -> Edited {
        // Built field by field so the current documents and parts, all replaced, are not cloned.
        let mut model = HudModel {
            pristine: self.pristine.clone(),
            current: self.pristine.clone(),
            stubs: Arc::clone(&self.stubs),
            copy: self.copy,
            bases: Arc::clone(&self.bases),
            platform: self.platform,
            loaded_scheme: Arc::clone(&self.loaded_scheme),
            scheme: Arc::clone(&self.loaded_scheme),
            scheme_cache: Arc::clone(&self.scheme_cache),
            chat_scheme: Arc::clone(&self.chat_scheme),
            rank_layout: Arc::clone(&self.rank_layout),
            scene: self.scene,
            board_mode: self.board_mode,
            meter: self.meter,
            minmode: self.minmode,
            parts: Vec::new(),
            index: HashMap::new(),
            all_problems: self.all_problems.clone(),
            problems: Vec::new(),
            hud_hides: self.hud_hides,
            assets: Vec::new(),
            shipped_fonts: Arc::clone(&self.shipped_fonts),
            loaded_animations: Arc::clone(&self.loaded_animations),
            animations: Arc::clone(&self.loaded_animations),
            replayed: Arc::new([]),
        };
        let (mut applied, mut dropped, mut touched) = (Vec::new(), 0, Vec::new());
        let platform = model.platform;
        // Options replay first so a canvas edit to the same key always wins; `applied` keeps
        // input order by recording each change's index across both passes.
        for (i, change) in changes.iter().enumerate() {
            let ChangeTarget::Option(choice) = change.target() else {
                continue;
            };
            let Some(sets) = self.compile_option(choice) else {
                dropped += 1;
                continue;
            };
            let (mut any, mut missed, mut counted) = (false, 0, false);
            for compiled in &sets {
                let file = compiled.set.as_ref().and_then(|edit| match edit {
                    OptionEdit::Set(s) => apply_set(&mut model.current, &self.bases, s, platform),
                    OptionEdit::Add(a) => {
                        apply_add_block(&mut model.current, &self.bases, a, platform)
                    }
                });
                match file {
                    Some(file) => {
                        any |= compiled.counts;
                        touched.push(file);
                    }
                    None if compiled.counts => missed += 1,
                    None => {}
                }
                counted |= compiled.counts;
            }
            // A choice with no edits (a switch off) applies; one whose every edit missed drops.
            if any || !counted {
                applied.push(i);
                dropped += missed;
            } else {
                dropped += 1;
            }
        }
        for (i, change) in changes.iter().enumerate() {
            // `None` drops the change; `Some(None)` applies it without editing a file.
            let outcome = match change.target() {
                ChangeTarget::Set(set) => {
                    apply_set(&mut model.current, &self.bases, set, platform).map(Some)
                }
                ChangeTarget::Remove(remove) => apply_remove(&mut model.current, remove, platform),
                ChangeTarget::AddBlock(add) => {
                    apply_add_block(&mut model.current, &self.bases, add, platform).map(Some)
                }
                ChangeTarget::Asset(asset) => apply_asset(&mut model.assets, asset).then_some(None),
                // A credit is a record for the credits file; it edits nothing.
                ChangeTarget::Credit(_) => Some(None),
                ChangeTarget::Option(_)
                | ChangeTarget::Animation(_)
                | ChangeTarget::AnimationAppend(_) => continue,
            };
            match outcome {
                Some(file) => {
                    applied.push(i);
                    touched.extend(file);
                }
                None => dropped += 1,
            }
        }
        if let Some((anims, done, missed)) =
            replay_animations(&self.loaded_animations, changes, platform)
        {
            model.animations = Arc::new(anims);
            applied.extend(done);
            dropped += missed;
        }
        applied.sort_unstable();
        let applied: Vec<Change> = applied
            .into_iter()
            .filter_map(|i| changes.get(i).cloned())
            .collect();
        model.replayed = applied.as_slice().into();
        touched.sort();
        touched.dedup();
        // Only an edit to the scheme file reparses it, and bytes seen before reuse their
        // scheme, so the full replay behind every drag frame stays cheap.
        if let Some(own) = (touched.iter().any(|f| f == SCHEME_FILE))
            .then(|| model.current.get(SCHEME_FILE))
            .flatten()
        {
            let loaded = model.pristine.get(SCHEME_FILE);
            model.scheme = match loaded.is_some_and(|doc| doc.as_bytes() == own.as_bytes()) {
                true => Arc::clone(&model.loaded_scheme),
                false => model.scheme_cache.get_or_build(own.as_bytes(), || {
                    model.loaded_scheme.with_own(own, model.platform)
                }),
            };
        }
        model.resolve_parts(false);
        Edited {
            model,
            applied,
            dropped,
            touched,
        }
    }

    /// The changes this model was replayed with, as applied; empty for a loaded model.
    pub(crate) fn replayed(&self) -> &[Change] {
        &self.replayed
    }

    /// Whether the player may change event `event`: refused when the HUD's own copy runs a
    /// console command.
    ///
    /// # Errors
    /// `RunsGameCommands` for such an event.
    pub fn can_change_animation(&self, event: &str) -> Result<(), EditRefused> {
        match self.loaded_animations.event(event) {
            Some(e) if e.runs_game_commands() => Err(EditRefused::RunsGameCommands),
            _ => Ok(()),
        }
    }

    /// The HUD's animation events with the changes replayed.
    pub fn animations(&self) -> &HudAnimations {
        &self.animations
    }

    /// Every part, parents before children; rule table order, then document order.
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// The part at `key`.
    pub fn part(&self, key: &PartKey) -> Option<&Part> {
        self.index.get(key).map(|&i| &self.parts[i])
    }

    /// The platform whose conditionals the model evaluates.
    pub(super) fn platform(&self) -> Platform {
        self.platform
    }

    /// The pristine document at a lowercase game path, before any change.
    pub(super) fn pristine_document(&self, path: &str) -> Option<&KvDocument> {
        self.pristine.get(path)
    }

    /// The current document at a game path, matched ignoring ASCII case.
    pub fn document(&self, path: &str) -> Option<&KvDocument> {
        self.current.get(&path.to_ascii_lowercase())
    }

    /// The root blocks `path`'s settings merge, in the order the game reads them: the file's
    /// own root block, then the root of each `#base` file merged into it.
    pub(super) fn merged_roots(&self, path: &str) -> Vec<&KvEntry> {
        let file = path.to_ascii_lowercase();
        let own = self.document(&file).and_then(root_block);
        own.into_iter()
            .chain(self.bases.roots(&file).map(|(_, root)| root))
            .collect()
    }

    /// The applied `Asset` changes as (path, source), one per path, the last applied winning,
    /// in first-applied order of path.
    pub fn assets(&self) -> &[(String, String)] {
        &self.assets
    }

    /// The file names the loaded HUD holds under the added fonts' folder or their kept copies'.
    pub(super) fn shipped_fonts(&self) -> &[String] {
        &self.shipped_fonts
    }

    /// The scheme as loaded, before any change.
    pub(super) fn loaded_scheme(&self) -> &Scheme {
        &self.loaded_scheme
    }

    /// The HUD's scheme, read from the same files.
    pub fn scheme(&self) -> &Scheme {
        &self.scheme
    }

    /// The rank panel's control tree as loaded; edits never change it.
    pub fn rank_layout(&self) -> &RankLayout {
        &self.rank_layout
    }

    /// The chat's own scheme as loaded; edits never change it.
    pub fn chat_scheme(&self) -> &Scheme {
        &self.chat_scheme
    }

    /// The game paths of this scene's part files that are missing, unparsable or have no root
    /// block, sorted.
    pub fn problems(&self) -> &[String] {
        &self.problems
    }
}

/// The last scheme rebuilt from edited clientscheme bytes, shared by every model replayed from
/// one load.
#[derive(Debug, Default)]
struct SchemeCache(Mutex<Option<(Vec<u8>, Arc<Scheme>)>>);

impl SchemeCache {
    /// The scheme last built from `bytes`, else `build()`'s, which replaces it.
    fn get_or_build(&self, bytes: &[u8], build: impl FnOnce() -> Scheme) -> Arc<Scheme> {
        // A panic elsewhere cannot leave the slot half written, so a poisoned lock is still sound.
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((seen, scheme)) = slot.as_ref()
            && seen.as_slice() == bytes
        {
            return Arc::clone(scheme);
        }
        let scheme = Arc::new(build());
        *slot = Some((bytes.to_vec(), Arc::clone(&scheme)));
        scheme
    }
}

/// Applies one set to the documents and returns the file it edited, or `None` when it does not
/// resolve: a refused path, an unread file, a missing block or unwritable text. In a copy, a
/// block the file lacks but its `#base` chain holds is created first.
pub(super) fn apply_set(
    docs: &mut BTreeMap<String, KvDocument>,
    bases: &BaseChains,
    set: &SetKey,
    platform: Platform,
) -> Option<String> {
    let file = check_hud_path(set.file())?;
    let doc = docs.get_mut(&file)?;
    if !writable(set.key(), set.value(), set.cond()) {
        return None;
    }
    let block: Vec<&str> = set.block().iter().map(String::as_str).collect();
    if live_blocks(doc, &block, platform).is_empty() && bases.holds(&file, &block) {
        create_blocks(doc, &block, platform);
    }
    // The first direct entry of the key with the same conditional (any case, as the engine
    // reads it) is set in place; with none, or for an appending set, a line is added.
    let same_cond = |c: Option<&str>| match (c, set.cond()) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (a, b) => a == b,
    };
    // The last live block is applied last, so a line there wins over every earlier duplicate.
    let target = *live_blocks(doc, &block, platform).last()?;
    let existing = (target.children().iter())
        .find(|e| {
            !set.appends()
                && e.text().is_some()
                && e.key().eq_ignore_ascii_case(set.key())
                && same_cond(e.cond())
        })
        .map(KvEntry::value_id);
    let target = target.value_id();
    let done = match existing {
        Some(id) => doc.set_text(id, set.value()),
        None => doc.insert_text_into(target, set.key(), set.value(), set.cond()),
    };
    done.then_some(file)
}

/// Creates each missing block on `path` in `doc`, unconditional so it applies on every platform:
/// a root block when the file has none, then each child inside the last live parent.
fn create_blocks(doc: &mut KvDocument, path: &[&str], platform: Platform) {
    if root_block(doc).is_none() && !path.first().is_some_and(|root| doc.append_root_block(root)) {
        return;
    }
    for depth in 1..path.len() {
        if !live_blocks(doc, &path[..=depth], platform).is_empty() {
            continue;
        }
        let Some(parent) = live_blocks(doc, &path[..depth], platform)
            .last()
            .map(|b| b.value_id())
        else {
            return;
        };
        if !doc.insert_block_into(parent, path[depth], None, &[]) {
            return;
        }
    }
}

/// Deletes a key's lines from every live block on the remove's path, since an earlier
/// duplicate's line would otherwise show through. `None` when it does not resolve (a refused
/// path, an unread file or no live block); `Some(None)` when it found no line, else
/// `Some(file)`.
fn apply_remove(
    docs: &mut BTreeMap<String, KvDocument>,
    remove: &RemoveKey,
    platform: Platform,
) -> Option<Option<String>> {
    let file = check_hud_path(remove.file())?;
    let doc = docs.get_mut(&file)?;
    let block: Vec<&str> = remove.block().iter().map(String::as_str).collect();
    // Without a conditional every direct line and every line in a direct `if_*` sub-block
    // goes; with one only the direct lines carrying it, compared as the engine reads it.
    let is_line = |e: &KvEntry| e.text().is_some() && e.key().eq_ignore_ascii_case(remove.key());
    let line_in = |target: &KvEntry| -> Option<KvValueId> {
        let direct = target.children().iter().find(|e| {
            is_line(e)
                && match remove.cond() {
                    None => true,
                    Some(c) => e.cond().is_some_and(|ec| ec.eq_ignore_ascii_case(c)),
                }
        });
        let nested = || {
            (remove.cond().is_none())
                .then(|| {
                    target
                        .children()
                        .iter()
                        .filter(|e| e.text().is_none() && is_if_block(e))
                        .flat_map(|e| e.children())
                        .find(|e| is_line(e))
                })
                .flatten()
        };
        direct.or_else(nested).map(KvEntry::value_id)
    };
    let next_line = |doc: &KvDocument| -> Option<Option<KvValueId>> {
        let targets = live_blocks(doc, &block, platform);
        (!targets.is_empty()).then(|| targets.into_iter().find_map(line_in))
    };
    next_line(doc)?;
    // Each removal shifts the remaining ids, so the next line is found afresh every time.
    let mut removed = false;
    while let Some(Some(id)) = next_line(doc) {
        if !doc.remove_entry(id) {
            break;
        }
        removed = true;
    }
    Some(removed.then_some(file))
}

/// Inserts a new block and returns its file, or `None` when it does not resolve: a refused
/// path, an unread file, a missing parent, an empty name, a sibling block of that name or
/// unwritable text.
fn apply_add_block(
    docs: &mut BTreeMap<String, KvDocument>,
    bases: &BaseChains,
    add: &AddBlock,
    platform: Platform,
) -> Option<String> {
    let file = check_hud_path(add.file())?;
    let doc = docs.get_mut(&file)?;
    if add.name().is_empty() {
        return None;
    }
    let parent: Vec<&str> = add.parent().iter().map(String::as_str).collect();
    // In a copy, a parent only the `#base` chain holds is created first, as a set does.
    if live_blocks(doc, &parent, platform).is_empty() && bases.holds(&file, &parent) {
        create_blocks(doc, &parent, platform);
    }
    let targets = live_blocks(doc, &parent, platform);
    let target = targets.last()?.value_id();
    // A newer HUD file may already have the block, in any live duplicate of the parent; adding
    // it again would duplicate it.
    let exists = (targets.iter().flat_map(|t| t.children()))
        .any(|e| e.text().is_none() && e.key().eq_ignore_ascii_case(add.name()));
    if exists {
        return None;
    }
    let lines: Vec<(&str, &str, Option<&str>)> = (add.lines().iter())
        .map(|l| (l.key(), l.value(), l.cond()))
        .collect();
    doc.insert_block_into(target, add.name(), add.cond(), &lines)
        .then_some(file)
}

/// Records an asset whose path passes the path rule and whose source stays in `.tf2tools/`,
/// replacing an earlier one for the same path; false when either check fails.
fn apply_asset(assets: &mut Vec<(String, String)>, asset: &AssetFile) -> bool {
    let Some(path) = check_hud_path(asset.path()) else {
        return false;
    };
    let Some(source) = check_hud_path(asset.source()).filter(|s| s.starts_with(".tf2tools/"))
    else {
        return false;
    };
    match assets.iter_mut().find(|(p, _)| *p == path) {
        Some(slot) => slot.1 = source,
        None => assets.push((path, source)),
    }
    true
}

/// Whether a block is an `if_*` sub-block, its prefix compared ignoring ASCII case.
fn is_if_block(entry: &KvEntry) -> bool {
    entry
        .key()
        .get(..3)
        .is_some_and(|p| p.eq_ignore_ascii_case("if_"))
}

/// A model after replaying changes: what applied, how many dropped and which files changed.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Edited {
    model: HudModel,
    applied: Vec<Change>,
    dropped: usize,
    touched: Vec<String>,
}

impl Edited {
    /// The edited model.
    pub fn model(&self) -> &HudModel {
        &self.model
    }

    /// The edited model, owned.
    pub fn into_model(self) -> HudModel {
        self.model
    }

    /// The changes that resolved, in replay order.
    pub fn applied(&self) -> &[Change] {
        &self.applied
    }

    /// How many changes did not resolve and were left out.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    /// The game paths an applied change edited, sorted.
    pub fn touched(&self) -> &[String] {
        &self.touched
    }
}

/// A file's root block: its first top-level block entry.
pub(super) fn root_block(doc: &KvDocument) -> Option<&KvEntry> {
    doc.entries().iter().find(|e| e.text().is_none())
}

/// Every block under `parent` named `name` (ASCII case-insensitive) whose conditional holds,
/// in file order: the game applies each to the one panel in turn, so later ones win per key.
fn named_blocks<'a>(
    parent: &'a KvEntry,
    name: &str,
    platform: Platform,
) -> impl Iterator<Item = &'a KvEntry> {
    (parent.children().iter()).filter(move |e| {
        e.text().is_none() && e.holds(platform) && e.key().eq_ignore_ascii_case(name)
    })
}

/// The blocks on `path` (names matched ignoring ASCII case) whose conditionals hold, in file
/// order. The last is where a write takes effect, as the game applies it last.
pub(super) fn live_blocks<'a>(
    doc: &'a KvDocument,
    path: &[&str],
    platform: Platform,
) -> Vec<&'a KvEntry> {
    let Some((first, rest)) = path.split_first() else {
        return Vec::new();
    };
    let mut level: Vec<&KvEntry> = (doc.entries().iter())
        .filter(|e| e.text().is_none() && e.holds(platform) && e.key().eq_ignore_ascii_case(first))
        .collect();
    for name in rest {
        level = (level.into_iter())
            .flat_map(|block| named_blocks(block, name, platform))
            .collect();
    }
    level
}

/// A block's `ControlName`: the first such text entry whose conditional holds.
fn control_name(block: &KvEntry, platform: Platform) -> Option<&str> {
    block
        .children()
        .iter()
        .filter(|e| e.holds(platform) && e.key().eq_ignore_ascii_case("ControlName"))
        .find_map(KvEntry::text)
}

/// One block a part reads, with its file and its block path as spelled.
#[derive(Clone)]
struct Located<'a> {
    file: &'a str,
    path: Vec<String>,
    entry: &'a KvEntry,
    /// For a block read from a `#base` file: the entry file it merges into and that file's
    /// root name, if it has a root block.
    owner: Option<(&'a str, Option<&'a str>)>,
}

impl<'a> Located<'a> {
    fn own(file: &'a str, path: Vec<String>, entry: &'a KvEntry) -> Located<'a> {
        Located {
            file,
            path,
            entry,
            owner: None,
        }
    }

    /// The block's child blocks, each located under it in the same file.
    fn child_blocks(&self) -> impl Iterator<Item = Located<'a>> + '_ {
        (self.entry.children().iter())
            .filter(|e| e.text().is_none())
            .map(|e| Located {
                file: self.file,
                path: child_path(&self.path, e.key()),
                entry: e,
                owner: self.owner,
            })
    }

    /// Where an edit at `path` (this block's path or one below it) lands: this file, or for a
    /// base block the same path in the entry file, its root renamed to the entry's root.
    fn written_at(&self, path: &[String]) -> (String, Vec<String>) {
        match self.owner {
            None => (self.file.to_owned(), path.to_vec()),
            Some((entry, root)) => {
                let mut path = path.to_vec();
                if let (Some(root), Some(first)) = (root, path.first_mut()) {
                    *first = root.to_owned();
                }
                (entry.to_owned(), path)
            }
        }
    }
}

fn child_path(path: &[String], name: &str) -> Vec<String> {
    let mut path = path.to_vec();
    path.push(name.to_owned());
    path
}

/// Keys the game sets back to their default on every block it applies, so an earlier duplicate's
/// line never shows through a later block that omits them.
const RESET_PER_BLOCK: [&str; 7] = [
    "visible",
    "enabled",
    "pin_to_sibling",
    "pin_corner_to_sibling",
    "pin_to_sibling_corner",
    "scaleImage",
    "tileImage",
];

/// Merges a part's blocks (in load order) into its values. Each block is a layer, followed by
/// its `if_*` sub-blocks named in `conditions` outside hudlayout; later layers override, and in
/// one layer the first entry of a key whose conditional holds counts. A duplicate block (same
/// file and path) first clears the `RESET_PER_BLOCK` keys an earlier duplicate set. In minimal
/// mode each layer is first rewritten as `minmode_layer` says.
fn resolve(
    blocks: &[Located<'_>],
    conditions: &[&str],
    fallback_file: &str,
    reading: Reading,
) -> Values {
    let Reading {
        platform, minmode, ..
    } = reading;
    let mut won: HashMap<String, (KeyLine, usize)> = HashMap::new();
    for (i, block) in blocks.iter().enumerate() {
        let duplicate_of =
            |j: usize| blocks[j].file == block.file && same_path(&blocks[j].path, &block.path);
        won.retain(|key, (_, j)| {
            !(RESET_PER_BLOCK.iter().any(|k| k.eq_ignore_ascii_case(key)) && duplicate_of(*j))
        });
        let active = block.file != HUDLAYOUT;
        let subs = block.entry.children().iter().filter(|c| {
            active
                && c.text().is_none()
                && c.holds(platform)
                && conditions.iter().any(|n| c.key().eq_ignore_ascii_case(n))
        });
        let layers = std::iter::once((block.entry, block.path.clone()))
            .chain(subs.map(|c| (c, child_path(&block.path, c.key()))));
        for (layer, path) in layers {
            let read = minmode_layer(layer.children(), platform, minmode);
            // A `_minmode` entry also stands under its own name, so its All settings row stays;
            // of two partners the last is the one the engine keeps.
            let own_names = (layer.children().iter().rev())
                .filter(|e| minmode && e.text().is_some() && e.holds(platform))
                .filter(|e| minmode_base(e.key()).is_some())
                .map(|e| (e.key().to_ascii_lowercase(), e));
            let mut seen = HashSet::new();
            for (key, entry) in read.iter().cloned().chain(own_names) {
                if seen.insert(key.clone()) {
                    won.insert(key, (KeyLine::new(block, &path, entry), i));
                }
            }
        }
    }
    let read_as = minmode.then_some(platform);
    let lines = won
        .iter()
        .map(|(key, (_, i))| (key.clone(), block_lines(&blocks[*i], Some(key), read_as)))
        .collect();
    // Every block's lines, active or not, so a removal can reach the blocks that lost.
    let mut chain: HashMap<String, Vec<KeyLine>> = HashMap::new();
    for block in blocks {
        for line in block_lines(block, None, None) {
            chain
                .entry(line.key.to_ascii_lowercase())
                .or_default()
                .push(line);
        }
    }
    let own_block = match blocks.last() {
        Some(last) => last.written_at(&last.path),
        None => (fallback_file.to_owned(), Vec::new()),
    };
    Values {
        winners: won.into_iter().map(|(k, (line, _))| (k, line)).collect(),
        lines,
        chain,
        blocks: listed_once(blocks),
        own_block,
    }
}

/// Each block's (file, path) once: duplicates share a path, matched ignoring ASCII case.
fn listed_once(blocks: &[Located<'_>]) -> Vec<(String, Vec<String>)> {
    let mut listed: Vec<(String, Vec<String>)> = Vec::new();
    for b in blocks {
        if !listed
            .iter()
            .any(|(f, p)| f == b.file && same_path(p, &b.path))
        {
            listed.push((b.file.to_owned(), b.path.clone()));
        }
    }
    listed
}

/// Whether two block paths name the same block, ignoring ASCII case.
fn same_path(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

/// Every entry of `key` (of any key when `None`) in `block` and its `if_*` sub-blocks, active
/// or not, in document order. With `minmode` set to the platform read, a layer holding a
/// `<key>_minmode` partner lists instead the entries that count as `key` after its rewrite.
fn block_lines(block: &Located<'_>, key: Option<&str>, minmode: Option<Platform>) -> Vec<KeyLine> {
    let rewritten = |layer| {
        key.zip(minmode)
            .and_then(|(k, p)| minmode_lines(layer, k, p))
    };
    // A rewritten layer keeps only the entries it lists; any other lists every entry of the key.
    let keep = |listed: Option<&[&KvEntry]>, e: &KvEntry| match listed {
        Some(listed) => listed.iter().any(|l| std::ptr::eq(*l, e)),
        None => key.is_none_or(|k| e.key().eq_ignore_ascii_case(k)),
    };
    let own = rewritten(block.entry);
    let mut lines = Vec::new();
    for entry in block.entry.children() {
        if entry.text().is_some() {
            if keep(own.as_deref(), entry) {
                lines.push(KeyLine::new(block, &block.path, entry));
            }
        } else if is_if_block(entry) {
            let path = child_path(&block.path, entry.key());
            let sub = rewritten(entry);
            lines.extend(
                entry
                    .children()
                    .iter()
                    .filter(|e| e.text().is_some() && keep(sub.as_deref(), e))
                    .map(|e| KeyLine::new(block, &path, e)),
            );
        }
    }
    lines
}

/// The entries of `layer` that count as `key` in minimal mode, when the layer holds a
/// `<key>_minmode` partner; `None` when it holds none and reads as in the normal HUD.
fn minmode_lines<'e>(
    layer: &'e KvEntry,
    key: &str,
    platform: Platform,
) -> Option<Vec<&'e KvEntry>> {
    let key = key.to_ascii_lowercase();
    let listed: Vec<&KvEntry> = (minmode_layer(layer.children(), platform, true).into_iter())
        .filter(|(as_key, _)| *as_key == key)
        .map(|(_, e)| e)
        .collect();
    (listed.iter())
        .any(|e| minmode_base(e.key()).is_some())
        .then_some(listed)
}

/// A layer's text entries whose conditional holds, each with the lowercase key it counts as.
/// In minimal mode the engine walks them in order: each `<k>_minmode` deletes the first
/// remaining `<k>` entry and is renamed `<k>`, so of two partners the last one wins.
fn minmode_layer(
    entries: &[KvEntry],
    platform: Platform,
    minmode: bool,
) -> Vec<(String, &KvEntry)> {
    let mut read: Vec<(String, &KvEntry)> = (entries.iter())
        .filter(|e| e.text().is_some() && e.holds(platform))
        .map(|e| (e.key().to_ascii_lowercase(), e))
        .collect();
    if !minmode {
        return read;
    }
    let mut i = 0;
    while i < read.len() {
        if let Some(bare) = minmode_base(read[i].1.key()) {
            if let Some(j) = read.iter().position(|(as_key, _)| *as_key == bare) {
                read.remove(j);
                i -= usize::from(j < i);
            }
            read[i].0 = bare;
        }
        i += 1;
    }
    read
}

/// The lowercase bare key of a name ending exactly in `_minmode` (ASCII case-insensitive).
pub(super) fn minmode_base(key: &str) -> Option<String> {
    const SUFFIX: &str = "_minmode";
    let at = key.len().checked_sub(SUFFIX.len())?;
    (key.get(at..)?.eq_ignore_ascii_case(SUFFIX)).then(|| key[..at].to_ascii_lowercase())
}

/// The root blocks a file's parts read, in load order: each merged base's root, then the
/// file's own if it has one.
fn file_roots<'a>(
    docs: &'a BTreeMap<String, KvDocument>,
    stubs: &'a BTreeMap<String, Stub>,
    bases: &'a BaseChains,
    path: &'a str,
) -> Vec<Located<'a>> {
    let Some(doc) = doc_or_stub(docs, stubs, path) else {
        return Vec::new();
    };
    // A copy's stub has no root of its own; its parts are its bases' blocks.
    let own = root_block(doc);
    let base_roots = bases.roots(path).map(|(id, root)| Located {
        file: id,
        path: vec![root.key().to_owned()],
        entry: root,
        owner: Some((path, own.map(KvEntry::key))),
    });
    let own = own.map(|own| Located::own(path, vec![own.key().to_owned()], own));
    base_roots.chain(own).collect()
}

/// Walks the scene's rule table into the part tree: each rule's container once, its own panel,
/// then the controls in its children file, depth first; then marks the code-toggled parts.
fn build_parts(
    docs: &BTreeMap<String, KvDocument>,
    stubs: &BTreeMap<String, Stub>,
    bases: &BaseChains,
    reading: Reading,
    scene: &SceneRule,
) -> Vec<Part> {
    let Reading {
        platform, meter, ..
    } = reading;
    let block_in = |path: &'static str, name: &str| -> Vec<Located> {
        file_roots(docs, stubs, bases, path)
            .into_iter()
            .flat_map(|root| {
                named_blocks(root.entry, name, platform).map(move |entry| Located {
                    path: child_path(&root.path, entry.key()),
                    entry,
                    ..root.clone()
                })
            })
            .collect()
    };
    let incomplete = |path: &str| {
        stubs.get(path).is_some_and(|s| s.incomplete) || bases.incomplete.contains(path)
    };
    let mut parts: Vec<Part> = Vec::new();
    // A scene that turns a condition on (MvM's `if_mvm`) applies it to every file it reads;
    // otherwise each panel rule names its own.
    let scene_conditions = scene.conditions();
    for rule in scene.rules() {
        let conditions = if scene_conditions.is_empty() {
            rule.conditions()
        } else {
            scene_conditions
        };
        let container = rule.parent().map(|name| {
            let key = PartKey::new(name);
            if !parts.iter().any(|p| p.key == key) {
                let blocks = block_in(HUDLAYOUT, name);
                parts.push(Part {
                    key: key.clone(),
                    name: name.to_owned(),
                    parent: None,
                    owner: None,
                    root: false,
                    class: blocks
                        .first()
                        .and_then(|b| control_name(b.entry, platform))
                        .map(str::to_owned),
                    readable: docs.contains_key(HUDLAYOUT),
                    from_base: stubs.contains_key(HUDLAYOUT),
                    base_incomplete: incomplete(HUDLAYOUT),
                    toggled: None,
                    values: resolve(&blocks, &[], HUDLAYOUT, reading),
                });
            }
            key
        });

        // The Item meter reads the chosen weapon's file in place of the plain one.
        let children_file = if rule.id() == PartId::ItemMeter {
            meter.file()
        } else {
            rule.children_file()
        };
        // The panel's blocks in load order: geometry first, then its own block unless that
        // block is already one of the geometry blocks.
        let roots = file_roots(docs, stubs, bases, children_file);
        let mut blocks: Vec<Located> = rule
            .geometry()
            .iter()
            .flat_map(|(path, name)| block_in(path, name))
            .collect();
        let own_is_geometry = rule
            .geometry()
            .iter()
            .any(|(path, name)| *path == children_file && name.eq_ignore_ascii_case(rule.panel()));
        if !own_is_geometry {
            blocks.extend(block_in(children_file, rule.panel()));
        }
        let readable = docs.contains_key(children_file)
            && rule
                .geometry()
                .iter()
                .all(|(path, _)| docs.contains_key(*path));
        // A rule reads its children file and its geometry files; any stub among them makes the
        // panel from-base unless another of them is missing outright.
        let reads: Vec<&str> = std::iter::once(children_file)
            .chain(rule.geometry().iter().map(|(path, _)| *path))
            .collect();
        let all_found = reads
            .iter()
            .all(|p| docs.contains_key(*p) || stubs.contains_key(*p));
        let from_base = all_found && reads.iter().any(|p| stubs.contains_key(*p));
        let base_incomplete = reads.iter().any(|p| incomplete(p));
        let root_key = PartKey::child(container.as_ref(), rule.panel());
        parts.push(Part {
            key: root_key.clone(),
            name: rule.panel().to_owned(),
            parent: container,
            owner: Some(rule.id()),
            root: true,
            class: blocks
                .iter()
                .rev()
                .find_map(|b| control_name(b.entry, platform))
                .map(str::to_owned),
            readable,
            from_base,
            base_incomplete,
            toggled: None,
            values: resolve(&blocks, conditions, children_file, reading),
        });

        if !roots.is_empty() && rule.has_children() {
            let children = (roots.iter())
                .flat_map(Located::child_blocks)
                .filter(|b| !b.entry.key().eq_ignore_ascii_case(rule.panel()))
                .collect();
            let access = Access {
                readable,
                from_base,
                incomplete: base_incomplete,
            };
            let parent = (&root_key, access);
            let made = |name: &str| scene.code_class(name);
            push_controls(
                &mut parts,
                children,
                (children_file, bases),
                parent,
                (Some(rule), conditions),
                reading,
                &made,
            );
        }
    }
    add_nested(&mut parts, docs, stubs, bases, reading, scene);
    // Code toggles a rule's own panel and its direct children by name; a scene that says so
    // matches a block at any depth.
    let roots: HashSet<PartKey> = (parts.iter().filter(|p| p.root))
        .map(|p| p.key.clone())
        .collect();
    for part in &mut parts {
        let near = part.root || part.parent.as_ref().is_some_and(|p| roots.contains(p));
        if near || scene.toggles_any_depth() {
            part.toggled = scene.toggled(&part.name);
        }
    }
    parts
}

/// Hangs each nested file's root children under the scene's first part of that name, read with
/// the scene's conditions or else the owning rule's. A child the parent's own file also declares
/// is one part: the nested file's blocks apply first, the parent file's last, and edits go there.
/// The file's controls are readable when the parent part and the file both are.
fn add_nested(
    parts: &mut Vec<Part>,
    docs: &BTreeMap<String, KvDocument>,
    stubs: &BTreeMap<String, Stub>,
    bases: &BaseChains,
    reading: Reading,
    scene: &SceneRule,
) {
    for &(panel, path) in scene.nested() {
        let Some(at) = parts
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(panel))
        else {
            continue;
        };
        let roots = file_roots(docs, stubs, bases, path);
        if roots.is_empty() {
            continue;
        }
        let key = parts[at].key.clone();
        let rule = parts[at].owner.map(PanelRule::get);
        let conditions = match scene.conditions() {
            [] => rule.map_or(&[][..], PanelRule::conditions),
            scene_conditions => scene_conditions,
        };
        let mut access = Access::of(&parts[at]);
        access.readable &= docs.contains_key(path);
        access.from_base |= stubs.contains_key(path);
        access.incomplete |= stubs.get(path).is_some_and(|s| s.incomplete);
        access.incomplete |= bases.incomplete.contains(path);
        let (parent_file, parent_path) = parts[at].values.own_block.clone();

        // Rebuild the parent's subtree from both files in load order, so push_controls merges
        // a shared name; the subtree sits right after the parent, where it is spliced back.
        let mut children: Vec<Located> = roots.iter().flat_map(Located::child_blocks).collect();
        children.extend(own_children(
            docs,
            stubs,
            bases,
            &parent_file,
            &parent_path,
            reading.platform,
        ));
        let below = format!("{}/", key.as_str());
        parts.retain(|p| !p.key.as_str().starts_with(&below));
        let mut subtree = Vec::new();
        push_controls(
            &mut subtree,
            children,
            (&parent_file, bases),
            (&key, access),
            (rule, conditions),
            reading,
            &|_| None,
        );
        // A part the nested file alone declares stays ownerless, as before the merge.
        for part in &mut subtree {
            if !(part.values.blocks.iter()).any(|(file, _)| *file == parent_file) {
                part.owner = None;
            }
        }
        parts.splice(at + 1..at + 1, subtree);
    }
}

/// The child blocks of the block at `path` (its root name ignored) in `file` and its bases.
fn own_children<'a>(
    docs: &'a BTreeMap<String, KvDocument>,
    stubs: &'a BTreeMap<String, Stub>,
    bases: &'a BaseChains,
    file: &'a str,
    path: &[String],
    platform: Platform,
) -> Vec<Located<'a>> {
    let mut level = file_roots(docs, stubs, bases, file);
    for name in path.iter().skip(1) {
        level = (level.iter())
            .flat_map(|block| {
                named_blocks(block.entry, name, platform).map(move |entry| Located {
                    path: child_path(&block.path, entry.key()),
                    entry,
                    ..block.clone()
                })
            })
            .collect();
    }
    level.iter().flat_map(Located::child_blocks).collect()
}

/// Adds each name among `blocks` (located child blocks, in load order) as one child part of
/// `parent`, then its own controls. Same-named sibling blocks are one panel to the game, so
/// they merge in load order; the name is a control when any of its blocks has a `ControlName`.
/// A control is readable only when its parent is, since its box hangs off the parent's.
/// `made` names the class of a block the game's code creates; deeper levels get none. A name
/// whose every block a shared base injects is no control unless `made` knows it.
fn push_controls(
    parts: &mut Vec<Part>,
    blocks: Vec<Located<'_>>,
    (file, bases): (&str, &BaseChains),
    (parent, access): (&PartKey, Access),
    (rule, conditions): (Option<&PanelRule>, &[&str]),
    reading: Reading,
    made: &CodeMade<'_>,
) {
    let platform = reading.platform;
    let blocks: Vec<Located> = (blocks.into_iter())
        .filter(|b| b.entry.holds(platform))
        .collect();
    let mut seen: HashSet<String> = HashSet::new();
    for (i, first) in blocks.iter().enumerate() {
        let name = first.entry.key();
        if !seen.insert(name.to_ascii_lowercase()) {
            continue;
        }
        let located: Vec<Located> = (blocks[i..].iter())
            .filter(|b| b.entry.key().eq_ignore_ascii_case(name))
            .cloned()
            .collect();
        let injected = located.iter().all(|b| bases.injects(b.file));
        let class = (located.iter())
            .find_map(|b| control_name(b.entry, platform))
            .filter(|_| !injected);
        let Some(class) = class.or_else(|| made(name)) else {
            continue;
        };
        let key = PartKey::child(Some(parent), name);
        parts.push(Part {
            key: key.clone(),
            name: name.to_owned(),
            parent: Some(parent.clone()),
            owner: rule.map(PanelRule::id),
            root: false,
            class: Some(class.to_owned()),
            readable: access.readable,
            from_base: access.from_base,
            base_incomplete: access.incomplete,
            toggled: None,
            values: resolve(&located, conditions, file, reading),
        });
        push_controls(
            parts,
            located.iter().flat_map(Located::child_blocks).collect(),
            (file, bases),
            (&key, access),
            (rule, conditions),
            reading,
            &|_| None,
        );
    }
}

/// How the model reads its documents: the platform's conditionals, the file the Item meter
/// reads and whether TF2's minimal HUD swaps `_minmode` keys in.
#[derive(Debug, Clone, Copy)]
struct Reading {
    platform: Platform,
    meter: MeterKind,
    minmode: bool,
}

/// The class the game's code gives a block named without a `ControlName`, if it makes one.
type CodeMade<'a> = dyn Fn(&str) -> Option<&'static str> + 'a;
