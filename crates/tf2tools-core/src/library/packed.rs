//! Packed library items: a tree stored as one zstd zip, read as if it were the loose folder.
//! This is the only module that knows the packed format.

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::hud_copy::{sha256_file, sha256_hex, sha256_reader};
use super::{IoFailure, METADATA_DIR, create_dir_all, io_at, tree_copy};
use crate::archive::check_entry_name;

/// Where a packed HUD keeps its tree, relative to its library folder.
pub(crate) const HUD_ZIP: &str = ".tf2tools/files.zip";

/// How many times a refused rename is tried again, and how long to wait between tries.
const RENAME_RETRIES: u32 = 5;
const RENAME_PAUSE: Duration = Duration::from_millis(200);

/// A library item's tree, either loose files in its folder or one zip.
pub(crate) enum ItemSource {
    /// The item's folder; its top-level `.tf2tools` is the app's, not the item's.
    Loose(PathBuf),
    /// The item's zip and the files it holds.
    Packed(Packed),
}

/// A zip's path and its regular-file entries in `Path` order.
pub(crate) struct Packed {
    zip: PathBuf,
    entries: Vec<Entry>,
}

/// One file in a packed item: its relative path, its name in the zip and its size.
struct Entry {
    rel: PathBuf,
    name: String,
    size: u64,
}

impl ItemSource {
    /// `Packed` when `dir/zip_rel` is a regular file (its central directory read and every
    /// name checked with `archive::check_entry_name` and `enclosed_name`), else `Loose(dir)`.
    pub(crate) fn open(dir: &Path, zip_rel: &str) -> Result<ItemSource, IoFailure> {
        let zip = dir.join(zip_rel);
        if !std::fs::symlink_metadata(&zip).is_ok_and(|m| m.is_file()) {
            return Ok(ItemSource::Loose(dir.to_path_buf()));
        }
        let mut archive = open_zip(&zip)?;
        let mut entries = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let entry = archive.by_index(i).map_err(damaged(&zip))?;
            let name = entry.name().map_err(damaged(&zip))?.into_owned();
            let rel = checked_rel(&zip, &name, entry.enclosed_name())?;
            if entry.is_file() {
                let size = entry.size();
                entries.push(Entry { rel, name, size });
            }
        }
        entries.sort_by(|a, b| a.rel.cmp(&b.rel));
        Ok(ItemSource::Packed(Packed { zip, entries }))
    }

    /// Whether the tree is read from a zip.
    pub(crate) fn is_packed(&self) -> bool {
        matches!(self, ItemSource::Packed(_))
    }

    /// Folders and regular files, relative, in walk order; a packed item's folders are the
    /// ancestors of its files. `skip` as `tree_copy::entries` takes it.
    #[cfg_attr(not(test), expect(dead_code, reason = "first used in Task 3"))]
    pub(crate) fn entries(
        &self,
        skip: &dyn Fn(&Path) -> bool,
    ) -> Result<Vec<(PathBuf, bool)>, IoFailure> {
        let packed = match self {
            ItemSource::Loose(dir) => return tree_copy::entries(dir, skip),
            ItemSource::Packed(packed) => packed,
        };
        // A folder sorts before what it holds in `Path` order, so one sorted set of files
        // and their ancestors is the walk's pre-order.
        let mut found = std::collections::BTreeMap::new();
        for entry in packed.kept(skip) {
            for folder in entry.rel.ancestors().skip(1) {
                if !folder.as_os_str().is_empty() {
                    found.insert(folder.to_path_buf(), true);
                }
            }
            found.insert(entry.rel.clone(), false);
        }
        Ok(found.into_iter().collect())
    }

    /// Regular files with their (uncompressed) sizes, in walk order.
    #[cfg_attr(not(test), expect(dead_code, reason = "first used in Task 3"))]
    pub(crate) fn files(
        &self,
        skip: &dyn Fn(&Path) -> bool,
    ) -> Result<Vec<(PathBuf, u64)>, IoFailure> {
        match self {
            ItemSource::Loose(dir) => loose_files(dir, skip),
            ItemSource::Packed(packed) => {
                Ok(packed.kept(skip).map(|e| (e.rel.clone(), e.size)).collect())
            }
        }
    }

    /// The bytes of the file at `rel`.
    #[cfg_attr(not(test), expect(dead_code, reason = "first used in Task 3"))]
    pub(crate) fn read(&self, rel: &Path) -> Result<Vec<u8>, IoFailure> {
        match self {
            ItemSource::Loose(dir) => {
                let path = dir.join(rel);
                std::fs::read(&path).map_err(io_at(&path))
            }
            ItemSource::Packed(packed) => {
                let name = packed.name_of(rel)?;
                let mut archive = open_zip(&packed.zip)?;
                let mut entry = archive.by_name(name).map_err(damaged(&packed.zip))?;
                let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
                entry.read_to_end(&mut bytes).map_err(io_at(&packed.zip))?;
                Ok(bytes)
            }
        }
    }

    /// SHA-256 hex of the file at `rel`.
    #[cfg_attr(not(test), expect(dead_code, reason = "first used in Task 3"))]
    pub(crate) fn sha256(&self, rel: &Path) -> Result<String, IoFailure> {
        match self {
            ItemSource::Loose(dir) => sha256_file(&dir.join(rel)),
            ItemSource::Packed(packed) => {
                let name = packed.name_of(rel)?;
                let mut archive = open_zip(&packed.zip)?;
                let entry = archive.by_name(name).map_err(damaged(&packed.zip))?;
                sha256_reader(&packed.zip, entry)
            }
        }
    }

    /// Writes the files `skip` keeps under `dest`, creating folders; returns bytes written.
    pub(crate) fn extract_into(
        &self,
        dest: &Path,
        skip: &dyn Fn(&Path) -> bool,
    ) -> Result<u64, IoFailure> {
        let packed = match self {
            ItemSource::Loose(dir) => return tree_copy::copy_tree(dir, dest, skip),
            ItemSource::Packed(packed) => packed,
        };
        create_dir_all(dest)?;
        let mut archive = open_zip(&packed.zip)?;
        let mut written = 0u64;
        for entry in packed.kept(skip) {
            let target = dest.join(&entry.rel);
            if let Some(parent) = target.parent() {
                create_dir_all(parent)?;
            }
            let mut from = archive.by_name(&entry.name).map_err(damaged(&packed.zip))?;
            let mut to = File::create(&target).map_err(io_at(&target))?;
            // A read failure (a CRC mismatch among them) is the zip's; a write failure the target's.
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = from.read(&mut buf).map_err(io_at(&packed.zip))?;
                if n == 0 {
                    break;
                }
                to.write_all(&buf[..n]).map_err(io_at(&target))?;
                written = written.saturating_add(n as u64);
            }
        }
        Ok(written)
    }

    /// Whether `dir` holds exactly these files (both sides filtered by `skip`) with the same bytes.
    #[cfg_attr(not(test), expect(dead_code, reason = "first used in Task 3"))]
    pub(crate) fn same_as(
        &self,
        dir: &Path,
        skip: &dyn Fn(&Path) -> bool,
    ) -> Result<bool, IoFailure> {
        let mine = self.files(skip)?;
        if mine != loose_files(dir, skip)? {
            return Ok(false);
        }
        // Lists and sizes agree, so only the bytes are left to compare, one zip open for all.
        let mut archive = match self {
            ItemSource::Packed(packed) => Some((open_zip(&packed.zip)?, packed)),
            ItemSource::Loose(_) => None,
        };
        for (rel, _) in &mine {
            let theirs_path = dir.join(rel);
            let theirs = std::fs::read(&theirs_path).map_err(io_at(&theirs_path))?;
            let ours = match &mut archive {
                Some((archive, packed)) => {
                    let name = packed.name_of(rel)?;
                    let mut entry = archive.by_name(name).map_err(damaged(&packed.zip))?;
                    let mut bytes = Vec::with_capacity(theirs.len());
                    entry.read_to_end(&mut bytes).map_err(io_at(&packed.zip))?;
                    bytes
                }
                None => self.read(rel)?,
            };
            if ours != theirs {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `tree_hash_of` over every file outside the top-level `.tf2tools`.
    #[cfg(test)]
    pub(crate) fn tree_hash(&self) -> Result<String, IoFailure> {
        let mut lines = Vec::new();
        for (rel, _) in self.files(&outside_metadata)? {
            lines.push(tree_line(&tree_key(&rel), &self.sha256(&rel)?));
        }
        Ok(tree_hash_of(lines))
    }

    /// The zip's length, or the loose files' lengths outside the top-level `.tf2tools`.
    #[cfg_attr(not(test), expect(dead_code, reason = "Library::space in plan 2"))]
    pub(crate) fn packed_bytes(&self) -> Result<u64, IoFailure> {
        match self {
            ItemSource::Loose(dir) => loose_bytes(dir),
            ItemSource::Packed(packed) => Ok(std::fs::metadata(&packed.zip)
                .map_err(io_at(&packed.zip))?
                .len()),
        }
    }

    /// The entries' uncompressed sizes, or the same loose sum.
    pub(crate) fn unpacked_bytes(&self) -> Result<u64, IoFailure> {
        match self {
            ItemSource::Loose(dir) => loose_bytes(dir),
            ItemSource::Packed(packed) => Ok(packed
                .entries
                .iter()
                .fold(0u64, |sum, e| sum.saturating_add(e.size))),
        }
    }
}

impl Packed {
    /// The entries no ancestor-or-self path of which `skip` names, as a walk would skip them.
    fn kept<'a>(&'a self, skip: &'a dyn Fn(&Path) -> bool) -> impl Iterator<Item = &'a Entry> {
        self.entries.iter().filter(move |e| {
            !e.rel
                .ancestors()
                .filter(|a| !a.as_os_str().is_empty())
                .any(skip)
        })
    }

    /// The zip name of the file at `rel`; a file the zip does not hold is `NotFound`.
    fn name_of(&self, rel: &Path) -> Result<&str, IoFailure> {
        match self.entries.binary_search_by(|e| e.rel.as_path().cmp(rel)) {
            Ok(i) => Ok(&self.entries[i].name),
            Err(_) => Err(IoFailure {
                path: self.zip.join(rel),
                source: io::Error::from(io::ErrorKind::NotFound),
            }),
        }
    }
}

/// Packs `src` (regular files `skip` keeps, `Path` order) into `dest_zip` through `<dest_zip>.tmp`:
/// zstd level 3, synced, read back (entry count, every entry's CRC and name), then renamed over.
/// Returns the zip's length. A failure removes the `.tmp` and leaves any old zip as it was.
pub(crate) fn pack(
    src: &Path,
    skip: &dyn Fn(&Path) -> bool,
    dest_zip: &Path,
) -> Result<u64, IoFailure> {
    pack_with(src, skip, dest_zip, &mut |from, to| {
        std::fs::rename(from, to)
    })
}

/// `pack` with the final rename supplied, so a refused rename can be simulated.
pub(crate) fn pack_with(
    src: &Path,
    skip: &dyn Fn(&Path) -> bool,
    dest_zip: &Path,
    rename: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<u64, IoFailure> {
    // Every name is settled before anything is written, so a non-UTF-8 name costs nothing.
    let mut files = Vec::new();
    for (rel, _) in loose_files(src, skip)? {
        let name =
            zip_name(&rel).ok_or_else(|| invalid(&src.join(&rel), "a file name is not UTF-8"))?;
        files.push((rel, name));
    }
    if let Some(parent) = dest_zip.parent() {
        create_dir_all(parent)?;
    }
    let mut tmp = dest_zip.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let placed = write_zip(src, &files, &tmp)
        .and_then(|()| check_zip(&tmp, files.len()))
        .and_then(|()| rename_retrying(&tmp, dest_zip, rename));
    if let Err(e) = placed {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(std::fs::metadata(dest_zip).map_err(io_at(dest_zip))?.len())
}

/// Writes `files` from `src` into a new zip at `tmp` and syncs it to disk.
fn write_zip(src: &Path, files: &[(PathBuf, String)], tmp: &Path) -> Result<(), IoFailure> {
    let out = File::create(tmp).map_err(io_at(tmp))?;
    let mut zip = ZipWriter::new(BufWriter::new(out));
    for (rel, name) in files {
        let path = src.join(rel);
        let mut from = File::open(&path).map_err(io_at(&path))?;
        let len = from.metadata().map_err(io_at(&path))?.len();
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Zstd)
            .compression_level(Some(3))
            .large_file(len >= u64::from(u32::MAX));
        zip.start_file(name.as_str(), options)
            .map_err(zip_failure(tmp))?;
        io::copy(&mut from, &mut zip).map_err(io_at(&path))?;
    }
    let out = zip.finish().map_err(zip_failure(tmp))?;
    let out = out.into_inner().map_err(|e| IoFailure {
        path: tmp.to_path_buf(),
        source: e.into_error(),
    })?;
    out.sync_all().map_err(io_at(tmp))
}

/// Reads the written zip back to its end: zip fails a read whose CRC does not match.
fn check_zip(tmp: &Path, count: usize) -> Result<(), IoFailure> {
    let mut archive = open_zip(tmp)?;
    if archive.len() != count {
        return Err(invalid(tmp, "the written zip lost entries"));
    }
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(damaged(tmp))?;
        let name = entry.name().map_err(damaged(tmp))?.into_owned();
        checked_rel(tmp, &name, entry.enclosed_name())?;
        io::copy(&mut entry, &mut io::sink()).map_err(io_at(tmp))?;
    }
    Ok(())
}

/// The relative path of the entry `name`, when `open` accepts it: `check_entry_name` passes
/// and the zip crate found it enclosed. `pack` checks the same, so it never places a zip
/// `open` would refuse.
fn checked_rel(zip: &Path, name: &str, enclosed: Option<PathBuf>) -> Result<PathBuf, IoFailure> {
    check_entry_name(zip, name).map_err(|e| invalid(zip, e))?;
    enclosed.ok_or_else(|| invalid(zip, format!("unsafe entry name {name:?}")))
}

/// Renames `tmp` over `dest`, trying again while something (an antivirus scan, an open
/// handle on Windows) refuses it; a missing `tmp` is not worth a retry.
fn rename_retrying(
    tmp: &Path,
    dest: &Path,
    rename: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), IoFailure> {
    let mut tries = 0;
    loop {
        match rename(tmp, dest) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound || tries == RENAME_RETRIES => {
                return Err(io_at(dest)(e));
            }
            Err(_) => {
                tries += 1;
                std::thread::sleep(RENAME_PAUSE);
            }
        }
    }
}

/// One tree-hash line: lowercase `/`-separated path, NUL, file SHA-256 hex, newline.
#[cfg_attr(not(test), expect(dead_code, reason = "first used in Task 4"))]
pub(crate) fn tree_line(key: &str, sha: &str) -> String {
    format!("{}\0{sha}\n", key.to_lowercase())
}

/// SHA-256 hex of the sorted, concatenated lines.
#[cfg_attr(not(test), expect(dead_code, reason = "first used in Task 4"))]
pub(crate) fn tree_hash_of(mut lines: Vec<String>) -> String {
    lines.sort();
    sha256_hex(lines.concat().as_bytes())
}

/// The `/`-separated key a tree-hash line names `rel` by.
#[cfg(test)]
fn tree_key(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether `rel` is the item's top-level metadata folder.
#[cfg(test)]
fn outside_metadata(rel: &Path) -> bool {
    rel == Path::new(METADATA_DIR)
}

/// The loose regular files under `dir` with their lengths, in walk order.
fn loose_files(dir: &Path, skip: &dyn Fn(&Path) -> bool) -> Result<Vec<(PathBuf, u64)>, IoFailure> {
    let mut files = Vec::new();
    for rel in tree_copy::tree_files(dir, skip)? {
        let path = dir.join(&rel);
        let len = std::fs::symlink_metadata(&path)
            .map_err(io_at(&path))?
            .len();
        files.push((rel, len));
    }
    Ok(files)
}

/// The loose files' lengths outside the top-level `.tf2tools`.
fn loose_bytes(dir: &Path) -> Result<u64, IoFailure> {
    tree_copy::tree_bytes(dir, &|rel: &Path| rel == Path::new(METADATA_DIR))
}

/// The zip name of `rel`: its plain components joined by `/`, or `None` when one is not UTF-8.
fn zip_name(rel: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in rel.components() {
        let Component::Normal(part) = component else {
            return None;
        };
        parts.push(part.to_str()?);
    }
    Some(parts.join("/"))
}

fn open_zip(zip: &Path) -> Result<ZipArchive<File>, IoFailure> {
    let file = File::open(zip).map_err(io_at(zip))?;
    ZipArchive::new(file).map_err(damaged(zip))
}

/// A zip that cannot be read as one is damaged data at its path.
fn damaged(zip: &Path) -> impl FnOnce(ZipError) -> IoFailure + use<> {
    let zip = zip.to_path_buf();
    move |e| invalid(&zip, e)
}

/// A write-side zip error keeps an I/O failure's own kind (a full disk stays one).
fn zip_failure(zip: &Path) -> impl FnOnce(ZipError) -> IoFailure + use<> {
    let path = zip.to_path_buf();
    move |e| IoFailure {
        path,
        source: io::Error::from(e),
    }
}

fn invalid(path: &Path, error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> IoFailure {
    IoFailure {
        path: path.to_path_buf(),
        source: io::Error::new(io::ErrorKind::InvalidData, error),
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::{Path, PathBuf};

    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn top_meta(rel: &Path) -> bool {
        rel == Path::new(METADATA_DIR)
    }

    fn nothing(_: &Path) -> bool {
        false
    }

    /// A HUD folder with text, a large binary, a `-` name that sorts before `/`, and metadata.
    fn hud(root: &Path) -> PathBuf {
        let dir = root.join("myhud");
        write(&dir.join("info.vdf"), b"\"myhud\" {}");
        write(
            &dir.join("resource/ui/hudlayout.res"),
            b"\"Resource/HudLayout.res\" {}",
        );
        write(
            &dir.join("materials/vgui/bg.vtf"),
            &[0u8, 1, 2, 3].repeat(4096),
        );
        write(&dir.join("a-c.txt"), b"dash");
        write(&dir.join("a/b.txt"), b"slash");
        write(&dir.join(".tf2tools/thumbnail.jpg"), b"jpeg");
        dir
    }

    #[test]
    fn a_packed_tree_lists_and_reads_as_the_loose_one() {
        let t = tempfile::tempdir().unwrap();
        let dir = hud(t.path());
        let loose = ItemSource::open(&dir, HUD_ZIP).unwrap();
        assert!(!loose.is_packed());
        pack(&dir, &top_meta, &dir.join(HUD_ZIP)).unwrap();
        let packed = ItemSource::open(&dir, HUD_ZIP).unwrap();
        assert!(packed.is_packed());
        let want = loose.files(&top_meta).unwrap();
        assert_eq!(packed.files(&nothing).unwrap(), want);
        assert_eq!(
            packed.entries(&nothing).unwrap(),
            loose.entries(&top_meta).unwrap()
        );
        for (rel, _) in &want {
            assert_eq!(
                packed.read(rel).unwrap(),
                loose.read(rel).unwrap(),
                "{}",
                rel.display()
            );
            assert_eq!(packed.sha256(rel).unwrap(), loose.sha256(rel).unwrap());
        }
    }

    #[test]
    fn files_come_in_walk_order() {
        let t = tempfile::tempdir().unwrap();
        let dir = hud(t.path());
        pack(&dir, &top_meta, &dir.join(HUD_ZIP)).unwrap();
        let packed = ItemSource::open(&dir, HUD_ZIP).unwrap();
        let names: Vec<String> = packed
            .files(&nothing)
            .unwrap()
            .into_iter()
            .map(|(rel, _)| rel.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(
            names,
            [
                "a/b.txt",
                "a-c.txt",
                "info.vdf",
                "materials/vgui/bg.vtf",
                "resource/ui/hudlayout.res"
            ]
        );
    }

    #[test]
    fn the_packed_tree_hash_is_the_loose_one() {
        let t = tempfile::tempdir().unwrap();
        let dir = hud(t.path());
        let loose = ItemSource::open(&dir, HUD_ZIP)
            .unwrap()
            .tree_hash()
            .unwrap();
        pack(&dir, &top_meta, &dir.join(HUD_ZIP)).unwrap();
        let packed = ItemSource::open(&dir, HUD_ZIP)
            .unwrap()
            .tree_hash()
            .unwrap();
        assert_eq!(packed, loose);
        assert_eq!(loose.len(), 64);
    }

    #[test]
    fn extraction_is_byte_identical_and_honours_skip() {
        let t = tempfile::tempdir().unwrap();
        let dir = hud(t.path());
        pack(&dir, &top_meta, &dir.join(HUD_ZIP)).unwrap();
        let packed = ItemSource::open(&dir, HUD_ZIP).unwrap();
        let dest = t.path().join("out");
        let skip = |rel: &Path| rel == Path::new("a");
        let written = packed.extract_into(&dest, &skip).unwrap();
        let loose = ItemSource::Loose(dir.clone());
        let want = loose
            .files(&|rel: &Path| top_meta(rel) || skip(rel))
            .unwrap();
        assert_eq!(written, want.iter().map(|(_, n)| n).sum::<u64>());
        assert_eq!(
            ItemSource::Loose(dest.clone()).files(&nothing).unwrap(),
            want
        );
        for (rel, _) in &want {
            assert_eq!(
                std::fs::read(dest.join(rel)).unwrap(),
                std::fs::read(dir.join(rel)).unwrap()
            );
        }
        assert!(packed.same_as(&dest, &skip).unwrap());
        write(&dest.join("info.vdf"), b"\"edited\" {}");
        assert!(!packed.same_as(&dest, &skip).unwrap());
    }

    #[test]
    fn packing_replaces_an_old_zip_and_leaves_no_tmp() {
        let t = tempfile::tempdir().unwrap();
        let dir = hud(t.path());
        let zip = dir.join(HUD_ZIP);
        pack(&dir, &top_meta, &zip).unwrap();
        write(&dir.join("info.vdf"), b"\"v2\" {}");
        let len = pack(&dir, &top_meta, &zip).unwrap();
        assert_eq!(len, std::fs::metadata(&zip).unwrap().len());
        assert!(!dir.join(".tf2tools/files.zip.tmp").exists());
        let packed = ItemSource::open(&dir, HUD_ZIP).unwrap();
        assert_eq!(packed.read(Path::new("info.vdf")).unwrap(), b"\"v2\" {}");
    }

    #[test]
    fn a_refused_rename_is_retried_then_reported() {
        let t = tempfile::tempdir().unwrap();
        let dir = hud(t.path());
        let zip = dir.join(HUD_ZIP);
        let mut calls = 0;
        let mut flaky = |from: &Path, to: &Path| {
            calls += 1;
            if calls < 3 {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                std::fs::rename(from, to)
            }
        };
        pack_with(&dir, &top_meta, &zip, &mut flaky).unwrap();
        assert_eq!(calls, 3);
        assert!(zip.is_file());

        std::fs::remove_file(&zip).unwrap();
        let mut locked = |_: &Path, _: &Path| Err(io::Error::from(io::ErrorKind::PermissionDenied));
        let err = pack_with(&dir, &top_meta, &zip, &mut locked).unwrap_err();
        assert_eq!(err.source.kind(), io::ErrorKind::PermissionDenied);
        assert!(!zip.exists());
        assert!(!dir.join(".tf2tools/files.zip.tmp").exists());
    }

    #[test]
    fn a_damaged_zip_fails_naming_it() {
        let t = tempfile::tempdir().unwrap();
        let dir = hud(t.path());
        write(&dir.join(HUD_ZIP), b"not a zip at all");
        let Err(err) = ItemSource::open(&dir, HUD_ZIP) else {
            unreachable!("a damaged zip must not open");
        };
        assert_eq!(err.path, dir.join(HUD_ZIP));
        assert_eq!(err.source.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn an_entry_that_climbs_out_is_refused() {
        use std::io::Write;
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("evil");
        let zip_path = dir.join(HUD_ZIP);
        std::fs::create_dir_all(zip_path.parent().unwrap()).unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        zip.start_file("../escape.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();
        assert!(ItemSource::open(&dir, HUD_ZIP).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_name_open_would_refuse_is_never_packed() {
        let t = tempfile::tempdir().unwrap();
        let dir = hud(t.path());
        write(&dir.join("a:b/c.res"), b"\"c\" {}");
        let zip = dir.join(HUD_ZIP);
        let err = pack(&dir, &top_meta, &zip).unwrap_err();
        assert_eq!(err.source.kind(), io::ErrorKind::InvalidData);
        assert!(!zip.exists());
        assert!(!dir.join(".tf2tools/files.zip.tmp").exists());
    }

    #[test]
    fn sizes_count_the_zip_and_the_entries() {
        let t = tempfile::tempdir().unwrap();
        let dir = hud(t.path());
        let loose = ItemSource::open(&dir, HUD_ZIP).unwrap();
        let unpacked = loose.unpacked_bytes().unwrap();
        assert_eq!(loose.packed_bytes().unwrap(), unpacked);
        let len = pack(&dir, &top_meta, &dir.join(HUD_ZIP)).unwrap();
        let packed = ItemSource::open(&dir, HUD_ZIP).unwrap();
        assert_eq!(packed.packed_bytes().unwrap(), len);
        assert_eq!(packed.unpacked_bytes().unwrap(), unpacked);
        assert!(len < unpacked, "the repeated VTF bytes must compress");
    }

    #[cfg(unix)]
    #[test]
    fn links_are_left_out_and_a_non_utf8_name_fails_the_pack() {
        use std::os::unix::ffi::OsStrExt;
        let t = tempfile::tempdir().unwrap();
        let dir = hud(t.path());
        write(&t.path().join("outside.txt"), b"secret");
        std::os::unix::fs::symlink(t.path().join("outside.txt"), dir.join("link.txt")).unwrap();
        pack(&dir, &top_meta, &dir.join(HUD_ZIP)).unwrap();
        let packed = ItemSource::open(&dir, HUD_ZIP).unwrap();
        assert!(
            packed
                .files(&nothing)
                .unwrap()
                .iter()
                .all(|(rel, _)| rel != Path::new("link.txt"))
        );

        let bad = dir.join(std::ffi::OsStr::from_bytes(b"bad\xff.res"));
        write(&bad, b"x");
        let zip = dir.join(".tf2tools/other.zip");
        assert!(pack(&dir, &top_meta, &zip).is_err());
        assert!(!zip.exists() && !dir.join(".tf2tools/other.zip.tmp").exists());
    }
}
