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
    kind: PayloadKind,
    path: PathBuf,
    chunks: Vec<PathBuf>,
    size: u64,
}

impl Payload {
    /// The tree-relative path with `/` (`"54 FOV/X.vpk"`, `"eve hud"`); `""` is the tree root.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `name`, or "Main files" for the tree root.
    pub fn label(&self) -> &str {
        if self.name.is_empty() {
            "Main files"
        } else {
            &self.name
        }
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
        return Ok(vpks);
    }
    let mut folder_dirs = Vec::new();
    find_folders(tree, &mut folder_dirs)?;

    // A folder is kept only when it brings files that no single VPK already carries,
    // so an add-on shipped both packed and loose installs once, as the VPK.
    let entries: Vec<HashSet<String>> = if folder_dirs.is_empty() {
        Vec::new()
    } else {
        vpks.iter().map(|v| vpk_entries(&v.path)).collect()
    };
    let mut payloads = vpks;
    for dir in folder_dirs {
        let (files, size) = folder_files(&dir)?;
        let duplicated = entries.iter().any(|e| files.iter().all(|f| e.contains(f)));
        if files.is_empty() || duplicated {
            continue;
        }
        payloads.push(Payload {
            name: tree_name(tree, &dir),
            kind: PayloadKind::Folder,
            path: dir,
            chunks: Vec::new(),
            size,
        });
    }
    sort_payloads(&mut payloads);
    Ok(payloads)
}

fn sort_payloads(payloads: &mut [Payload]) {
    payloads.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
}

/// Collects content folders: a directory with an `info.vdf` or a content subfolder
/// is one and is not descended into; any other directory is searched below.
fn find_folders(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ArchiveError> {
    let io = |source| ArchiveError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut subdirs = Vec::new();
    let mut is_payload = false;
    for entry in fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        let file_type = entry.file_type().map_err(io)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if file_type.is_file() && name.eq_ignore_ascii_case("info.vdf") {
            is_payload = true;
        } else if file_type.is_dir() {
            if CONTENT_DIRS.iter().any(|c| name.eq_ignore_ascii_case(c)) {
                is_payload = true;
            }
            subdirs.push(entry.path());
        }
    }
    if is_payload {
        out.push(dir.to_path_buf());
        return Ok(());
    }
    subdirs.sort();
    for sub in subdirs {
        find_folders(&sub, out)?;
    }
    Ok(())
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
                kind: PayloadKind::Vpk,
                path: path.clone(),
                size: size + parts.iter().map(|(_, s)| s).sum::<u64>(),
                chunks: parts.into_iter().map(|(p, _)| p).collect(),
            }
        })
        .collect())
}

/// A folder's non-`.vpk` files as lowercased `/` paths relative to it, and their total size.
fn folder_files(dir: &Path) -> Result<(HashSet<String>, u64), ArchiveError> {
    let mut files = HashSet::new();
    let mut size = 0;
    for entry in WalkDir::new(dir) {
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
