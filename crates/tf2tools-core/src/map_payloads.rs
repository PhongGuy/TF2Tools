//! Finding the maps in an extracted GameBanana map download, with the files that go
//! with each one. Only `.bsp.bz2` maps are written to, decompressed in place.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use bzip2::read::BzDecoder;
use tempfile::NamedTempFile;
use walkdir::WalkDir;

use crate::error::ArchiveError;
use crate::library::{io_at, walk_failure};
use crate::store::{BSP_MAGIC, is_map_name};

/// More maps than this in one download, or none, means the layout isn't recognised.
pub const MAX_MAPS: usize = 64;

/// One map of an extracted download and the files installed with it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MapPayload {
    name: String,
    bsp: PathBuf,
    companions: Vec<(PathBuf, String)>,
    root: PathBuf,
    skipped: usize,
}

impl MapPayload {
    /// The map name: its file name without `.bsp`, in the download's case.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The `.bsp` file.
    pub fn bsp(&self) -> &Path {
        &self.bsp
    }

    /// Files installed with the map: the source, and its `/` path inside the install folder.
    pub fn companions(&self) -> &[(PathBuf, String)] {
        &self.companions
    }

    /// The folder the map's `maps/`, `scripts/` and `materials/` are read from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Files under `root` that no map of the download installs.
    pub fn skipped(&self) -> usize {
        self.skipped
    }
}

/// Every map in an extracted tree, in walk order, with its companions. `.bsp.bz2` maps are
/// decompressed first, at most [`MAX_MAPS`] of them and `cap` bytes in all; one that is
/// corrupt or over budget is left as it is and is not a map. A name repeated in another
/// case keeps the first. Returns an empty list when there is no map.
///
/// # Errors
/// [`ArchiveError::Io`] when the tree cannot be walked or a decompressed map not written.
pub fn find_maps(tree: &Path, cap: u64) -> Result<Vec<MapPayload>, ArchiveError> {
    decompress_bz2_maps(tree, cap)?;
    let files = walk_files(tree)?;

    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for path in &files {
        if let Some(name) = map_stem(path, ".bsp")
            && has_bsp_magic(path)
            && seen.insert(name.to_ascii_lowercase())
        {
            found.push((name.to_owned(), path.clone()));
        }
    }

    let mut maps: Vec<MapPayload> = found
        .into_iter()
        .map(|(name, bsp)| {
            let root = map_root(tree, &bsp);
            let companions = companions(&files, &root, &bsp, &name);
            MapPayload {
                name,
                bsp,
                companions,
                root,
                skipped: 0,
            }
        })
        .collect();

    // A file counts as skipped only when no map of the tree installs it.
    let installed: HashSet<&Path> = maps
        .iter()
        .flat_map(|m| {
            std::iter::once(m.bsp.as_path()).chain(m.companions.iter().map(|(s, _)| s.as_path()))
        })
        .collect();
    let skipped: Vec<usize> = maps
        .iter()
        .map(|m| {
            let left: Vec<&PathBuf> = files
                .iter()
                .filter(|f| f.starts_with(&m.root) && !installed.contains(f.as_path()))
                .collect();
            for file in &left {
                tracing::debug!(map = m.name, file = %file.display(), "file not installed");
            }
            left.len()
        })
        .collect();
    for (map, skipped) in maps.iter_mut().zip(skipped) {
        map.skipped = skipped;
    }
    tracing::info!(maps = maps.len(), "maps found in download");
    Ok(maps)
}

/// Every regular file under `tree`, in walk order, links not followed.
fn walk_files(tree: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    let mut files = Vec::new();
    for entry in WalkDir::new(tree).sort_by_file_name() {
        let entry = entry.map_err(|e| walk_failure(tree, e))?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

/// The stem of a file named `<stem><suffix>` (suffix matched ignoring case) when the stem
/// is a safe map name.
fn map_stem<'a>(path: &'a Path, suffix: &str) -> Option<&'a str> {
    let name = path.file_name()?.to_str()?;
    let stem = name.get(..name.len().checked_sub(suffix.len())?)?;
    (ends_with_ignore_case(name, suffix) && is_map_name(stem)).then_some(stem)
}

fn has_bsp_magic(path: &Path) -> bool {
    let mut head = [0u8; 4];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok_and(|()| &head == BSP_MAGIC)
}

/// The parent of the map's folder when that folder is `maps`, else the map's folder.
fn map_root(tree: &Path, bsp: &Path) -> PathBuf {
    let dir = bsp.parent().unwrap_or(tree);
    let in_maps = dir != tree
        && dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("maps"));
    match dir.parent() {
        Some(parent) if in_maps => parent.to_path_buf(),
        _ => dir.to_path_buf(),
    }
}

/// The allow-listed files that go with a map, in walk order: its `.nav`, the root's
/// MvM populations and its menu and loading-screen art.
fn companions(files: &[PathBuf], root: &Path, bsp: &Path, name: &str) -> Vec<(PathBuf, String)> {
    let nav = format!("{name}.nav");
    files
        .iter()
        .filter_map(|file| {
            if file.parent() == bsp.parent() && file_name_is(file, &nav) {
                return Some((file.clone(), format!("maps/{name}.nav")));
            }
            let parts: Vec<&str> = file
                .strip_prefix(root)
                .ok()?
                .iter()
                .map(|p| p.to_str())
                .collect::<Option<_>>()?;
            let is =
                |i: usize, want: &str| parts.get(i).is_some_and(|p| p.eq_ignore_ascii_case(want));
            let dest = match parts.as_slice() {
                [_, _, pop]
                    if is(0, "scripts")
                        && is(1, "population")
                        && ends_with_ignore_case(pop, ".pop") =>
                {
                    format!("scripts/population/{pop}")
                }
                [_, _, _, rest @ ..]
                    if !rest.is_empty() && is(0, "materials") && is(1, "vgui") && is(2, "maps") =>
                {
                    format!("materials/vgui/maps/{}", rest.join("/"))
                }
                _ => return None,
            };
            Some((file.clone(), dest))
        })
        .collect()
}

fn file_name_is(path: &Path, want: &str) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case(want))
}

/// True for a name longer than `suffix` that ends in it, ignoring case.
pub(crate) fn ends_with_ignore_case(name: &str, suffix: &str) -> bool {
    name.len() > suffix.len()
        && name
            .get(name.len() - suffix.len()..)
            .is_some_and(|end| end.eq_ignore_ascii_case(suffix))
}

/// Decompresses each `<stem>.bsp.bz2` with a map-name stem to `<stem>.bsp` beside it and
/// removes the `.bz2`, within one `cap` budget and [`MAX_MAPS`] files. Each goes through a
/// temporary file, so a corrupt or oversized one leaves only its `.bz2` behind.
fn decompress_bz2_maps(tree: &Path, cap: u64) -> Result<(), ArchiveError> {
    let files = walk_files(tree)?;
    let mut taken: HashSet<PathBuf> = files.iter().map(|f| lowercase_path(f)).collect();
    let mut used = 0u64;
    let mut count = 0usize;
    for bz2 in &files {
        let Some(stem) = map_stem(bz2, ".bsp.bz2") else {
            continue;
        };
        let bsp = bz2.with_file_name(format!("{stem}.bsp"));
        if count >= MAX_MAPS || taken.contains(&lowercase_path(&bsp)) {
            tracing::debug!(file = %bz2.display(), "compressed map not unpacked");
            continue;
        }
        let remaining = cap.saturating_sub(used);
        let Some((part, size)) = decompress(bz2, remaining)? else {
            continue;
        };
        // A failed persist drops the temporary file, which deletes it.
        part.persist(&bsp).map_err(|e| io_at(&bsp)(e.error))?;
        fs::remove_file(bz2).map_err(io_at(bz2))?;
        used += size;
        count += 1;
        taken.insert(lowercase_path(&bsp));
    }
    Ok(())
}

/// Decompresses `bz2` into a temporary file beside it, or `None` when its bytes are not
/// bzip2 or come to more than `limit`. Only a decode error is a skip; a disk error is `Io`.
fn decompress(bz2: &Path, limit: u64) -> Result<Option<(NamedTempFile, u64)>, ArchiveError> {
    let dir = bz2.parent().unwrap_or(bz2);
    let source = File::open(bz2).map_err(io_at(bz2))?;
    let mut part = NamedTempFile::new_in(dir).map_err(io_at(dir))?;
    let mut reader = BzDecoder::new(source).take(limit.saturating_add(1));
    match io::copy(&mut reader, &mut part) {
        Ok(size) if size <= limit => Ok(Some((part, size))),
        Ok(_) => {
            tracing::debug!(file = %bz2.display(), "compressed map too large");
            Ok(None)
        }
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::InvalidInput | io::ErrorKind::UnexpectedEof
            ) =>
        {
            tracing::debug!(file = %bz2.display(), error = %e, "compressed map corrupt");
            Ok(None)
        }
        Err(e) => Err(io_at(part.path())(e).into()),
    }
}

fn lowercase_path(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}
