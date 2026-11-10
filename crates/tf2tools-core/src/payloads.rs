//! Finding the installable units in an extracted add-on download: content folders
//! and VPKs. A pure filesystem walk; nothing is copied or written.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::ArchiveError;
use crate::library::walk_failure;
use crate::pak;

/// More payloads than this, or none, means the layout isn't recognised.
pub const MAX_PAYLOADS: usize = 16;

/// Subfolders that mark their parent as a game content folder.
const CONTENT_DIRS: [&str; 9] = [
    "materials",
    "models",
    "sound",
    "resource",
    "scripts",
    "particles",
    "cfg",
    "maps",
    "media",
];

/// What a payload is installed as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PayloadKind {
    /// A VPK file, with its numbered chunks when it is a `_dir.vpk`.
    Vpk,
    /// A content folder, copied without the `.vpk` files inside it.
    Folder,
}

/// One installable unit of an extracted download.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Payload {
    name: String,
    label: String,
    kind: PayloadKind,
    path: PathBuf,
    chunks: Vec<PathBuf>,
    nested: Vec<PathBuf>,
    size: u64,
}

impl Payload {
    /// The tree-relative path with `/` (`"54 FOV/X.vpk"`, `"eve hud"`); `""` is the tree root.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// A short name for the player: the last parts of `name` that tell it apart from the
    /// other payloads, joined with " / ", or "Main files" for the tree root.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Whether this is a VPK or a folder.
    pub fn kind(&self) -> PayloadKind {
        self.kind
    }

    /// Total bytes: a VPK with its chunks, or a folder's files without nested `.vpk`s.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Files a VPK payload is made of: 1, or 1 plus its `_NNN` chunks; 0 for a folder.
    pub fn vpk_parts(&self) -> usize {
        match self.kind {
            PayloadKind::Vpk => 1 + self.chunks.len(),
            PayloadKind::Folder => 0,
        }
    }

    /// Where the VPK or folder sits in the extracted tree.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// A `_dir.vpk`'s numbered data files; empty otherwise.
    pub(crate) fn chunks(&self) -> &[PathBuf] {
        &self.chunks
    }

    /// Content roots inside a folder payload that are payloads of their own, and so are
    /// left out of it; empty for a VPK.
    pub(crate) fn nested(&self) -> &[PathBuf] {
        &self.nested
    }
}

/// The payloads under `tree`, largest first, then by name. Never fails on layout:
/// none or more than `MAX_PAYLOADS` is the caller's to refuse. Past `MAX_PAYLOADS`
/// VPKs only the VPKs are listed and no folder is searched.
///
/// # Errors
/// [`ArchiveError::Io`] when a directory or file's metadata cannot be read.
pub fn find_payloads(tree: &Path) -> Result<Vec<Payload>, ArchiveError> {
    let mut vpks = find_vpks(tree)?;
    // A tree already past the limit is refused anyway, so a hostile one with thousands
    // of VPKs skips the folder walk and the VPK directory reads that collapse folders.
    if vpks.len() > MAX_PAYLOADS {
        sort_payloads(&mut vpks);
        for vpk in &mut vpks {
            vpk.label.clone_from(&vpk.name);
        }
        return Ok(vpks);
    }
    let mut folder_roots = Vec::new();
    let mut stopped = false;
    find_folders(tree, &mut folder_roots, &mut stopped)?;
    // Roots past the stop were never seen, so dropping empty or duplicate ones could make
    // a partial list look complete; every root found is listed and the tree is refused.
    if stopped {
        let mut payloads = vpks;
        payloads.extend(
            folder_roots
                .into_iter()
                .map(|FolderRoot { dir, nested, .. }| {
                    let name = tree_name(tree, &dir);
                    Payload {
                        label: name.clone(),
                        name,
                        kind: PayloadKind::Folder,
                        path: dir,
                        chunks: Vec::new(),
                        nested,
                        size: 0,
                    }
                }),
        );
        sort_payloads(&mut payloads);
        return Ok(payloads);
    }

    // A folder is kept only when it brings files that no single VPK already carries,
    // so an add-on shipped both packed and loose installs once, as the VPK.
    let entries: Vec<HashSet<String>> = if folder_roots.is_empty() {
        Vec::new()
    } else {
        vpks.iter().map(|v| vpk_entries(&v.path)).collect()
    };
    let mut payloads = vpks;
    for FolderRoot { dir, nested, .. } in folder_roots {
        let (files, size) = folder_files(&dir, &nested)?;
        let duplicated = entries.iter().any(|e| files.iter().all(|f| e.contains(f)));
        if files.is_empty() || duplicated {
            continue;
        }
        payloads.push(Payload {
            name: tree_name(tree, &dir),
            label: String::new(),
            kind: PayloadKind::Folder,
            path: dir,
            chunks: Vec::new(),
            nested,
            size,
        });
    }
    sort_payloads(&mut payloads);
    assign_labels(&mut payloads);
    Ok(payloads)
}

/// Labels each payload with its last path part, adding the nearer parent parts that differ
/// from payloads ending in the same parts. A trailing `tf` wrapper is not a part; a label
/// that still collides falls back to the full name, then gains a " (n)" counter.
fn assign_labels(payloads: &mut [Payload]) {
    let parts: Vec<Vec<&str>> = payloads
        .iter()
        .map(|p| {
            let mut parts: Vec<&str> = p.name.split('/').filter(|s| !s.is_empty()).collect();
            if parts.last().is_some_and(|l| l.eq_ignore_ascii_case("tf")) {
                parts.pop();
            }
            parts
        })
        .collect();
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let mut labels: Vec<String> = parts
        .iter()
        .enumerate()
        .map(|(i, own)| {
            let Some(last) = own.last() else {
                return "Main files".to_owned();
            };
            let mut rivals: Vec<&Vec<&str>> = parts
                .iter()
                .enumerate()
                .filter(|&(j, other)| j != i && other.last().is_some_and(|l| same(l, last)))
                .map(|(_, other)| other)
                .collect();
            // Walk up from the end, keeping a part only where some rival differs there.
            let mut kept = vec![*last];
            for back in 2..=own.len() {
                if rivals.is_empty() {
                    break;
                }
                let part = own[own.len() - back];
                let differs = |r: &&Vec<&str>| r.len() < back || !same(r[r.len() - back], part);
                if rivals.iter().any(differs) {
                    kept.push(part);
                    rivals.retain(|r| !differs(r));
                }
            }
            if rivals.is_empty() {
                kept.reverse();
                kept.join(" / ")
            } else {
                own.join(" / ")
            }
        })
        .collect();
    for i in 0..labels.len() {
        let clash = (0..labels.len()).any(|j| j != i && same(&labels[j], &labels[i]));
        if clash && !payloads[i].name.is_empty() {
            labels[i] = payloads[i].name.replace('/', " / ");
        }
    }
    // Names that differ only in case still match above, so number the later ones.
    for i in 0..labels.len() {
        let taken = |label: &str, labels: &[String]| labels[..i].iter().any(|l| same(l, label));
        if taken(&labels[i], &labels) {
            let base = labels[i].clone();
            let mut n = 2;
            while taken(&format!("{base} ({n})"), &labels) {
                n += 1;
            }
            labels[i] = format!("{base} ({n})");
        }
    }
    for (payload, label) in payloads.iter_mut().zip(labels) {
        payload.label = label;
    }
}

fn sort_payloads(payloads: &mut [Payload]) {
    payloads.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
}

/// A content folder found by the walk, with the content roots below it that were split off.
struct FolderRoot {
    dir: PathBuf,
    nested: Vec<PathBuf>,
    /// Has a `resource` folder or `info.vdf`, the shape of a HUD or a HUD's option.
    hud_like: bool,
}

/// Collects content folders, stopping one past `MAX_PAYLOADS` and setting `stopped` when a
/// directory was left unsearched. A directory with an `info.vdf` is one, kept whole. A
/// directory with a content subfolder is one too, and its other subfolders are searched
/// for content roots: TF2 loads only content at the top of a custom folder, so each becomes
/// a payload of its own, except a HUD-like one, which a GUI mod keeps for `#base`.
/// Any other directory is searched below.
fn find_folders(
    dir: &Path,
    out: &mut Vec<FolderRoot>,
    stopped: &mut bool,
) -> Result<(), ArchiveError> {
    if out.len() > MAX_PAYLOADS {
        *stopped = true;
        return Ok(());
    }
    let io = |source| ArchiveError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut others = Vec::new();
    let mut has_content = false;
    let mut has_info = false;
    let mut has_resource = false;
    for entry in fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        let file_type = entry.file_type().map_err(io)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if file_type.is_file() && name.eq_ignore_ascii_case("info.vdf") {
            has_info = true;
        } else if file_type.is_dir() {
            if is_content_name(&name) && !wraps_game_dir(&entry.path())? {
                has_content = true;
                has_resource |= name.eq_ignore_ascii_case("resource");
            } else {
                others.push(entry.path());
            }
        }
    }
    others.sort();
    let root = |hud_like| FolderRoot {
        dir: dir.to_path_buf(),
        nested: Vec::new(),
        hud_like,
    };
    if has_info {
        out.push(root(true));
    } else if has_content {
        let at = out.len();
        out.push(root(has_resource));
        for sub in others {
            let before = out.len();
            find_folders(&sub, out, stopped)?;
            // HUD-like roots stay in this folder's copy; the rest are split off from it.
            let found: Vec<FolderRoot> = out.drain(before..).collect();
            for nested in found {
                if !nested.hud_like {
                    out[at].nested.push(nested.dir.clone());
                    out.push(nested);
                }
            }
        }
    } else {
        for sub in others {
            find_folders(&sub, out, stopped)?;
        }
    }
    Ok(())
}

fn is_content_name(name: &str) -> bool {
    CONTENT_DIRS.iter().any(|c| name.eq_ignore_ascii_case(c))
}

/// Whether a content-named folder holds a `tf` folder with content folders of its own, as
/// in `Sound/tf/sound/...`: then it only wraps a game layout and the real root is further
/// down. A `tf` without them is ordinary content such as `sound/tf/a.wav`.
fn wraps_game_dir(dir: &Path) -> Result<bool, ArchiveError> {
    for tf in subdirs(dir)? {
        if tf.file_name().is_some_and(|n| n.eq_ignore_ascii_case("tf"))
            && subdirs(&tf)?.iter().any(|d| {
                d.file_name()
                    .is_some_and(|n| is_content_name(&n.to_string_lossy()))
            })
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The directories directly inside `dir`.
fn subdirs(dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    let io = |source| ArchiveError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut dirs = Vec::new();
    for entry in fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        if entry.file_type().map_err(io)?.is_dir() {
            dirs.push(entry.path());
        }
    }
    Ok(dirs)
}

/// Every `.vpk` under `tree` as a payload, with `<base>_NNN.vpk` chunks folded
/// into the `<base>_dir.vpk` beside them.
fn find_vpks(tree: &Path) -> Result<Vec<Payload>, ArchiveError> {
    let mut files = Vec::new();
    for entry in WalkDir::new(tree).sort_by_file_name() {
        let entry = entry.map_err(|e| walk_failure(tree, e))?;
        if entry.file_type().is_file() && has_vpk_extension(entry.path()) {
            let size = entry.metadata().map_err(|e| walk_failure(tree, e))?.len();
            files.push((entry.into_path(), size));
        }
    }

    // Index each directory's `_dir.vpk` files by their lowercased base name.
    let dir_vpks: HashMap<(PathBuf, String), usize> = files
        .iter()
        .enumerate()
        .filter_map(|(i, (path, _))| {
            let base = lower_stem(path).strip_suffix("_dir")?.to_owned();
            Some(((parent(path), base), i))
        })
        .collect();

    let mut chunks: HashMap<usize, Vec<(PathBuf, u64)>> = HashMap::new();
    let mut heads = Vec::new();
    for (i, (path, size)) in files.iter().enumerate() {
        let owner = chunk_base(&lower_stem(path))
            .and_then(|base| dir_vpks.get(&(parent(path), base.to_owned())));
        match owner {
            Some(&head) => chunks.entry(head).or_default().push((path.clone(), *size)),
            None => heads.push(i),
        }
    }

    Ok(heads
        .into_iter()
        .map(|i| {
            let (path, size) = &files[i];
            let parts = chunks.remove(&i).unwrap_or_default();
            Payload {
                name: tree_name(tree, path),
                label: String::new(),
                kind: PayloadKind::Vpk,
                path: path.clone(),
                size: size + parts.iter().map(|(_, s)| s).sum::<u64>(),
                chunks: parts.into_iter().map(|(p, _)| p).collect(),
                nested: Vec::new(),
            }
        })
        .collect())
}

/// A folder's non-`.vpk` files outside `nested`, as lowercased `/` paths relative to it,
/// and their total size.
fn folder_files(dir: &Path, nested: &[PathBuf]) -> Result<(HashSet<String>, u64), ArchiveError> {
    let mut files = HashSet::new();
    let mut size = 0;
    let walk = WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| !nested.iter().any(|n| n == e.path()));
    for entry in walk {
        let entry = entry.map_err(|e| walk_failure(dir, e))?;
        if !entry.file_type().is_file() || has_vpk_extension(entry.path()) {
            continue;
        }
        size += entry.metadata().map_err(|e| walk_failure(dir, e))?.len();
        files.insert(tree_name(dir, entry.path()).to_ascii_lowercase());
    }
    Ok((files, size))
}

/// A VPK's entry paths, lowercased with `/`; an unreadable VPK has none.
fn vpk_entries(path: &Path) -> HashSet<String> {
    pak::open_vpk(path)
        .map(|pak| {
            pak.tree
                .keys()
                .map(|k| k.replace('\\', "/").to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

/// `base` of a `base_NNN` stem, the shape of a multi-chunk VPK's data file.
fn chunk_base(stem: &str) -> Option<&str> {
    let (base, digits) = stem.rsplit_once('_')?;
    (digits.len() == 3 && digits.bytes().all(|b| b.is_ascii_digit())).then_some(base)
}

fn has_vpk_extension(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("vpk"))
}

fn lower_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn parent(path: &Path) -> PathBuf {
    path.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// `path` relative to `root`, joined with `/`; `""` for the root itself.
fn tree_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
