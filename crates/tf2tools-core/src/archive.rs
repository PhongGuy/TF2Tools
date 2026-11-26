//! Archive extraction for downloaded content: zip, 7z, RAR and VPK, told apart by magic
//! bytes. Every entry name is checked before anything is written, so an archive
//! with one unsafe entry extracts nothing at all.

use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use crate::error::{ArchiveError, VpkError};
use crate::pak;
use crate::panic_mark::catching;

const ZIP_MAGIC: [&[u8]; 2] = [b"PK\x03\x04", b"PK\x05\x06"];
const SEVEN_Z_MAGIC: &[u8] = &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
/// The VPK header magic, `0x55AA1234` little-endian.
pub(crate) const VPK_MAGIC: &[u8] = &[0x34, 0x12, 0xAA, 0x55];
const RAR_MAGIC: &[u8] = b"Rar!";

/// Extracts a zip, 7z, RAR or VPK (by magic bytes) into `dest` and returns its content root.
/// An unrecognised file is not extracted: its own path is returned and `dest` is untouched.
/// A failure part-way through can leave `dest` partly written, so extract into a scratch
/// directory and remove it on error.
pub fn extract_archive(archive: &Path, dest: &Path) -> Result<PathBuf, ArchiveError> {
    extract_archive_within(archive, dest, &mut Budget::default())
}

/// Unpacked-size and entry-count caps shared by every archive kind.
const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ENTRIES: u64 = 65_536;

/// What one unpack (outer archive plus any inner VPKs) may still write.
#[derive(Debug)]
pub(crate) struct Budget {
    bytes_cap: u64,
    entries_cap: u64,
    bytes_used: u64,
    entries_used: u64,
}

impl Budget {
    pub(crate) fn new(bytes_cap: u64, entries_cap: u64) -> Self {
        Self {
            bytes_cap,
            entries_cap,
            bytes_used: 0,
            entries_used: 0,
        }
    }

    /// Weighs an archive's listed totals before anything is written: the bytes must fit in
    /// what is left without being consumed (writing charges them), the entries are consumed.
    pub(crate) fn admit(
        &mut self,
        archive: &Path,
        listed_bytes: u64,
        entries: u64,
    ) -> Result<(), ArchiveError> {
        let entries_used = self.entries_used.saturating_add(entries);
        if entries_used > self.entries_cap {
            return Err(ArchiveError::TooManyEntries {
                path: archive.to_path_buf(),
                limit: self.entries_cap,
            });
        }
        if self.bytes_used.saturating_add(listed_bytes) > self.bytes_cap {
            return Err(self.too_large(archive));
        }
        self.entries_used = entries_used;
        Ok(())
    }

    /// Consumes `bytes` about to be written, refusing them if they would pass the cap.
    pub(crate) fn charge(&mut self, archive: &Path, bytes: u64) -> Result<(), ArchiveError> {
        let used = self.bytes_used.saturating_add(bytes);
        if used > self.bytes_cap {
            return Err(self.too_large(archive));
        }
        self.bytes_used = used;
        Ok(())
    }

    fn too_large(&self, archive: &Path) -> ArchiveError {
        ArchiveError::TooLarge {
            path: archive.to_path_buf(),
            limit: self.bytes_cap,
        }
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new(MAX_UNPACKED_BYTES, MAX_ENTRIES)
    }
}

/// [`extract_archive`] drawing on `budget`, so several unpacks can share one cap.
pub(crate) fn extract_archive_within(
    archive: &Path,
    dest: &Path,
    budget: &mut Budget,
) -> Result<PathBuf, ArchiveError> {
    let header = sniff(archive)?;
    if ZIP_MAGIC.iter().any(|m| header.starts_with(m)) {
        extract_zip(archive, dest, budget)?;
    } else if header.starts_with(SEVEN_Z_MAGIC) {
        extract_7z(archive, dest, budget)?;
    } else if header.starts_with(VPK_MAGIC) {
        extract_vpk(archive, dest, budget)?;
    } else if header.starts_with(RAR_MAGIC) {
        extract_rar(archive, dest, budget)?;
    } else {
        // A bare WAV or VTF download is its own content.
        return Ok(archive.to_path_buf());
    }
    find_content_root(dest)
}

/// Descends while a directory holds exactly one entry and that entry is a directory.
pub fn find_content_root(dir: &Path) -> Result<PathBuf, ArchiveError> {
    let io = |source| ArchiveError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let entries = fs::read_dir(dir)
        .map_err(io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io)?;
    if let [only] = entries.as_slice()
        && only.file_type().map_err(io)?.is_dir()
    {
        return find_content_root(&only.path());
    }
    Ok(dir.to_path_buf())
}

/// Reads up to the first 8 bytes; a shorter file is sniffed on what it has.
pub(crate) fn sniff(archive: &Path) -> Result<Vec<u8>, ArchiveError> {
    let io = |source| io_error(archive, source);
    let mut header = Vec::with_capacity(8);
    File::open(archive)
        .map_err(io)?
        .take(8)
        .read_to_end(&mut header)
        .map_err(io)?;
    Ok(header)
}

fn extract_zip(archive: &Path, dest: &Path, budget: &mut Budget) -> Result<(), ArchiveError> {
    let file = File::open(archive).map_err(|e| io_error(archive, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| malformed(archive, e))?;

    // Pass 1 resolves every entry to a path inside `dest` before pass 2 writes any.
    // It also sums the listed sizes so an archive over budget is refused before `dest` exists.
    let mut targets = Vec::with_capacity(zip.len());
    let mut listed: u64 = 0;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| malformed(archive, e))?;
        listed = listed.saturating_add(entry.size());
        // `enclosed_name` quietly strips `C:` and a leading `/`, so the raw name is
        // judged first and `enclosed_name` stays only as a second guard.
        let raw = entry.name().map_err(|e| malformed(archive, e))?;
        check_entry_name(archive, &raw)?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(unsafe_path(archive, raw.into_owned()));
        };
        targets.push((dest.join(relative), entry.is_dir()));
    }

    budget.admit(archive, listed, zip.len() as u64)?;
    create_dir(dest)?;
    for (i, (target, is_dir)) in targets.into_iter().enumerate() {
        if is_dir {
            create_dir(&target)?;
            continue;
        }
        let mut entry = zip.by_index(i).map_err(|e| malformed(archive, e))?;
        stream_to_file(archive, &mut entry, &target, budget)?;
    }
    Ok(())
}

/// sevenz-rust2 can panic on corrupt input, so both the listing and the
/// extraction run under `catching`, which marks the panic as handled.
fn extract_7z(archive: &Path, dest: &Path, budget: &mut Budget) -> Result<(), ArchiveError> {
    let listing = match catching(|| sevenz_rust2::Archive::open(archive)) {
        Ok(Ok(listing)) => listing,
        Ok(Err(e)) => return Err(malformed(archive, e)),
        Err(_) => return Err(malformed(archive, "the 7z reader panicked")),
    };
    let mut listed: u64 = 0;
    for entry in &listing.files {
        check_entry_name(archive, entry.name())?;
        listed = listed.saturating_add(entry.size());
    }
    budget.admit(archive, listed, listing.files.len() as u64)?;

    create_dir(dest)?;
    // Our own failure (over budget, a failed write) is kept here and wins over the error
    // sevenz reports for it, so `TooLarge` is not mistaken for a corrupt archive.
    let mut failure: Option<ArchiveError> = None;
    let result = catching(|| {
        sevenz_rust2::decompress_file_with_extract_fn(archive, dest, |entry, reader, path| {
            write_7z_entry(archive, entry.is_directory(), reader, path, budget)
                .map(|()| true)
                .map_err(|e| {
                    let message = e.to_string();
                    failure = Some(e);
                    sevenz_rust2::Error::Other(message.into())
                })
        })
    });
    match (failure, result) {
        (Some(e), _) => Err(e),
        (None, Ok(Ok(()))) => Ok(()),
        (None, Ok(Err(e))) => Err(malformed(archive, e)),
        (None, Err(_)) => Err(malformed(archive, "the 7z reader panicked")),
    }
}

/// Writes one 7z entry through the budget guard: a directory is created, a file streamed.
fn write_7z_entry(
    archive: &Path,
    is_dir: bool,
    mut reader: &mut dyn Read,
    path: &Path,
    budget: &mut Budget,
) -> Result<(), ArchiveError> {
    if is_dir {
        create_dir(path)
    } else {
        stream_to_file(archive, &mut reader, path, budget)
    }
}

fn extract_vpk(archive: &Path, dest: &Path, _budget: &mut Budget) -> Result<(), ArchiveError> {
    let from_vpk = |e: VpkError| match e {
        VpkError::Io { path, source } => ArchiveError::Io { path, source },
        other @ VpkError::Malformed { .. } | other @ VpkError::NoWav { .. } => {
            malformed(archive, other)
        }
    };
    let vpk = pak::open_vpk(archive).map_err(from_vpk)?;
    for name in vpk.tree.keys() {
        check_entry_name(archive, name)?;
    }

    create_dir(dest)?;
    for (name, entry) in &vpk.tree {
        let bytes = pak::read_entry(archive, name, entry).map_err(from_vpk)?;
        write_file(&dest.join(name.replace('\\', "/")), &bytes)?;
    }
    Ok(())
}

/// RAR names come as raw bytes, so every member is checked from the index before
/// `dest` exists, and again as it is written in case the stream disagrees with the index.
fn extract_rar(archive: &Path, dest: &Path, budget: &mut Budget) -> Result<(), ArchiveError> {
    let reader = rars::ArchiveReader::read_path(archive).map_err(|e| rar_error(archive, &e))?;
    let (mut listed, mut entries) = (0u64, 0u64);
    for member in reader.members() {
        check_entry_name(archive, &String::from_utf8_lossy(&member.meta.name))?;
        listed = listed.saturating_add(member.meta.unpacked_size);
        entries += 1;
    }
    budget.admit(archive, listed, entries)?;

    create_dir(dest)?;
    // Each output file must be `'static`, so the budget is lent to them through an Rc and
    // handed back once `rars` is done.
    let shared = Rc::new(RefCell::new(std::mem::take(budget)));
    // `rars` can only carry its own error type out, so our failures (an unsafe name, a
    // failed write) are kept in this slot and win over whatever `rars` reports.
    let failure: RarFailure = Rc::default();
    let result = reader.extract_to(None, |meta| {
        open_rar_target(archive, dest, meta, &failure, &shared).map_err(|e| {
            let message = e.to_string();
            *failure.borrow_mut() = Some(e);
            rars::Error::from(std::io::Error::other(message))
        })
    });
    *budget = shared.take();
    match (failure.take(), result) {
        (Some(e), _) => Err(e),
        (None, Err(e)) => Err(rar_error(archive, &e)),
        (None, Ok(())) => Ok(()),
    }
}

type RarFailure = Rc<RefCell<Option<ArchiveError>>>;

/// A directory member is created and its (empty) stream discarded; a file member
/// streams straight into a file under `dest`.
fn open_rar_target(
    archive: &Path,
    dest: &Path,
    meta: &rars::ExtractedEntryMeta,
    failure: &RarFailure,
    budget: &Rc<RefCell<Budget>>,
) -> Result<Box<dyn Write>, ArchiveError> {
    let name = String::from_utf8_lossy(&meta.name);
    check_entry_name(archive, &name)?;
    let target = dest.join(name.replace('\\', "/"));
    if meta.is_directory {
        create_dir(&target)?;
        return Ok(Box::new(std::io::sink()));
    }
    if let Some(parent) = target.parent() {
        create_dir(parent)?;
    }
    let file = File::create(&target).map_err(|e| io_error(&target, e))?;
    Ok(Box::new(RarFile {
        file,
        target,
        archive: archive.to_path_buf(),
        failure: Rc::clone(failure),
        budget: Rc::clone(budget),
    }))
}

/// An unbuffered output file that charges the shared budget before each write and records
/// its own failure (`TooLarge`, or `Io` for a full disk) so neither reads as a corrupt archive.
struct RarFile {
    file: File,
    target: PathBuf,
    archive: PathBuf,
    failure: RarFailure,
    budget: Rc<RefCell<Budget>>,
}

impl Write for RarFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let charged = self
            .budget
            .borrow_mut()
            .charge(&self.archive, buf.len() as u64);
        if let Err(e) = charged {
            let message = e.to_string();
            self.failure.borrow_mut().get_or_insert(e);
            return Err(std::io::Error::other(message));
        }
        self.file.write(buf).inspect_err(|e| self.record(e))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush().inspect_err(|e| self.record(e))
    }
}

impl RarFile {
    fn record(&self, error: &std::io::Error) {
        let copy = std::io::Error::new(error.kind(), error.to_string());
        self.failure
            .borrow_mut()
            .get_or_insert_with(|| io_error(&self.target, copy));
    }
}

/// A password problem is `Encrypted` whichever way `rars` was built: with the
/// `encryption` feature it asks for a password, without it the feature is disabled.
fn rar_error(archive: &Path, error: &rars::Error) -> ArchiveError {
    let encrypted = matches!(
        error.kind(),
        rars::ErrorKind::PasswordRequired | rars::ErrorKind::BadPassword
    ) || matches!(
        error.root_cause(),
        rars::Error::FeatureDisabled {
            feature: "encryption"
        } | rars::Error::UnsupportedEncryption { .. }
    );
    if encrypted {
        ArchiveError::Encrypted {
            path: archive.to_path_buf(),
        }
    } else {
        malformed(archive, error)
    }
}

/// Accepts an entry name of any format only if, with `\` read as a separator, every
/// component is a plain name or `.` and the first is not a drive prefix (`C:`), so it
/// cannot climb out of or replace `dest` on any platform.
pub(crate) fn check_entry_name(archive: &Path, name: &str) -> Result<(), ArchiveError> {
    let normalized = name.replace('\\', "/");
    let first = normalized.split('/').next().unwrap_or_default().as_bytes();
    let drive = first.len() >= 2 && first[0].is_ascii_alphabetic() && first[1] == b':';
    let safe = !drive
        && Path::new(&normalized)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if safe {
        Ok(())
    } else {
        Err(unsafe_path(archive, name.to_owned()))
    }
}

/// Copies through a fixed buffer so a zip bomb cannot exhaust memory, charging each chunk
/// to `budget` before it is written; a read failure is `Malformed`, a write failure `Io`.
fn stream_to_file(
    archive: &Path,
    entry: &mut impl Read,
    target: &Path,
    budget: &mut Budget,
) -> Result<(), ArchiveError> {
    if let Some(parent) = target.parent() {
        create_dir(parent)?;
    }
    let mut out = File::create(target).map_err(|e| io_error(target, e))?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = match entry.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(malformed(archive, e)),
        };
        budget.charge(archive, n as u64)?;
        out.write_all(&buf[..n]).map_err(|e| io_error(target, e))?;
    }
}

fn create_dir(dir: &Path) -> Result<(), ArchiveError> {
    fs::create_dir_all(dir).map_err(|e| io_error(dir, e))
}

fn write_file(target: &Path, bytes: &[u8]) -> Result<(), ArchiveError> {
    if let Some(parent) = target.parent() {
        create_dir(parent)?;
    }
    fs::write(target, bytes).map_err(|e| io_error(target, e))
}

fn io_error(path: &Path, source: std::io::Error) -> ArchiveError {
    ArchiveError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn malformed(archive: &Path, message: impl ToString) -> ArchiveError {
    ArchiveError::Malformed {
        path: archive.to_path_buf(),
        message: message.to_string(),
    }
}

fn unsafe_path(archive: &Path, entry: String) -> ArchiveError {
    ArchiveError::UnsafePath {
        path: archive.to_path_buf(),
        entry,
    }
}

#[cfg(test)]
#[path = "archive_budget_tests.rs"]
mod budget_tests;
