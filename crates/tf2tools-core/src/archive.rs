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
    let header = sniff(archive)?;
    if ZIP_MAGIC.iter().any(|m| header.starts_with(m)) {
        extract_zip(archive, dest)?;
    } else if header.starts_with(SEVEN_Z_MAGIC) {
        extract_7z(archive, dest)?;
    } else if header.starts_with(VPK_MAGIC) {
        extract_vpk(archive, dest)?;
    } else if header.starts_with(RAR_MAGIC) {
        extract_rar(archive, dest)?;
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

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let file = File::open(archive).map_err(|e| io_error(archive, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| malformed(archive, e))?;

    // Pass 1 resolves every entry to a path inside `dest` before pass 2 writes any.
    let mut targets = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| malformed(archive, e))?;
        // `enclosed_name` quietly strips `C:` and a leading `/`, so the raw name is
        // judged first and `enclosed_name` stays only as a second guard.
        let raw = entry.name().map_err(|e| malformed(archive, e))?;
        check_entry_name(archive, &raw)?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(unsafe_path(archive, raw.into_owned()));
        };
        targets.push((dest.join(relative), entry.is_dir()));
    }

    create_dir(dest)?;
    for (i, (target, is_dir)) in targets.into_iter().enumerate() {
        if is_dir {
            create_dir(&target)?;
            continue;
        }
        let mut entry = zip.by_index(i).map_err(|e| malformed(archive, e))?;
        stream_to_file(archive, &mut entry, &target)?;
    }
    Ok(())
}

/// sevenz-rust2 can panic on corrupt input, so both the listing and the
/// extraction run under `catching`, which marks the panic as handled.
fn extract_7z(archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let listing = match catching(|| sevenz_rust2::Archive::open(archive)) {
        Ok(Ok(listing)) => listing,
        Ok(Err(e)) => return Err(malformed(archive, e)),
        Err(_) => return Err(malformed(archive, "the 7z reader panicked")),
    };
    for entry in &listing.files {
        check_entry_name(archive, entry.name())?;
    }

    create_dir(dest)?;
    match catching(|| sevenz_rust2::decompress_file(archive, dest)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(malformed(archive, e)),
        Err(_) => Err(malformed(archive, "the 7z reader panicked")),
    }
}

fn extract_vpk(archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
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
fn extract_rar(archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let reader = rars::ArchiveReader::read_path(archive).map_err(|e| rar_error(archive, &e))?;
    for member in reader.index().members() {
        check_entry_name(archive, &String::from_utf8_lossy(member.name_bytes()))?;
    }

    create_dir(dest)?;
    // `rars` can only carry its own error type out, so our failures (an unsafe name, a
    // failed write) are kept in this slot and win over whatever `rars` reports.
    let failure: RarFailure = Rc::default();
    let result = reader.extract_to(None, |meta| {
        open_rar_target(archive, dest, meta, &failure).map_err(|e| {
            let message = e.to_string();
            *failure.borrow_mut() = Some(e);
            rars::Error::from(std::io::Error::other(message))
        })
    });
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
        failure: Rc::clone(failure),
    }))
}

/// An unbuffered output file that records its own write failure as `Io`, so a full disk
/// is not reported as a corrupt archive.
struct RarFile {
    file: File,
    target: PathBuf,
    failure: RarFailure,
}

impl Write for RarFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
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
fn check_entry_name(archive: &Path, name: &str) -> Result<(), ArchiveError> {
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

/// Copies through a fixed buffer so a zip bomb cannot exhaust memory; a read
/// failure is the archive's fault (`Malformed`), a write failure is `Io`.
fn stream_to_file(
    archive: &Path,
    entry: &mut impl Read,
    target: &Path,
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
