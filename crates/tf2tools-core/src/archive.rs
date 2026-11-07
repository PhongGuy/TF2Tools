//! Archive extraction for downloaded content: zip, 7z and VPK, told apart by magic
//! bytes. Every entry name is checked before anything is written, so an archive
//! with one unsafe entry extracts nothing at all.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::error::{ArchiveError, VpkError};
use crate::pak;
use crate::panic_mark::catching;

const ZIP_MAGIC: [&[u8]; 2] = [b"PK\x03\x04", b"PK\x05\x06"];
const SEVEN_Z_MAGIC: &[u8] = &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
/// The VPK header magic, `0x55AA1234` little-endian.
pub(crate) const VPK_MAGIC: &[u8] = &[0x34, 0x12, 0xAA, 0x55];
const RAR_MAGIC: &[u8] = b"Rar!";

/// Extracts a zip, 7z or VPK (by magic bytes) into `dest` and returns its content root.
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
        return Err(ArchiveError::Rar {
            path: archive.to_path_buf(),
        });
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
fn sniff(archive: &Path) -> Result<Vec<u8>, ArchiveError> {
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
        let Some(relative) = entry.enclosed_name() else {
            let name = entry.name().map(|n| n.into_owned()).unwrap_or_default();
            return Err(unsafe_path(archive, name));
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

/// Accepts a 7z or VPK entry name only if, with `\` read as a separator, every
/// component is a plain name or `.`, so it cannot climb out of or replace `dest`.
fn check_entry_name(archive: &Path, name: &str) -> Result<(), ArchiveError> {
    let normalized = name.replace('\\', "/");
    let safe = Path::new(&normalized)
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
