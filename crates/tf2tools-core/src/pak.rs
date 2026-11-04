//! VPK reading behind a checked header. The `vpk` crate trusts the header's lengths
//! and can panic or abort on corrupt input, so every pak is vetted here first.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::error::VpkError;
use crate::panic_mark::catching;

const SIGNATURE: u32 = 0x55aa_1234;
const HEADER_V1_LEN: u64 = 12;
const HEADER_V2_EXTRA: u64 = 16;

/// Returns the bytes of the `.wav` entry (extension matched case-insensitively)
/// whose path sorts first by byte order, so the choice does not depend on hash order.
pub fn first_wav_in_vpk(path: &Path) -> Result<Vec<u8>, VpkError> {
    let pak = open_vpk(path)?;
    let (name, entry) = pak
        .tree
        .iter()
        .filter(|(name, _)| {
            name.rsplit_once('.')
                .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("wav"))
        })
        .min_by(|a, b| a.0.cmp(b.0))
        .ok_or_else(|| VpkError::NoWav {
            path: path.to_path_buf(),
        })?;
    read_entry(path, name, entry)
}

/// Opens a VPK after checking its signature, version and that the directory tree
/// fits in the file, which keeps the parser from allocating a corrupt `tree_length`.
pub(crate) fn open_vpk(path: &Path) -> Result<vpk::VPK, VpkError> {
    let io = |source| VpkError::Io {
        path: path.to_path_buf(),
        source,
    };
    let malformed = |message: String| VpkError::Malformed {
        path: path.to_path_buf(),
        message,
    };

    // Opening a FIFO or device blocks or misreads, so only regular files are paks.
    if !std::fs::metadata(path).map_err(io)?.is_file() {
        return Err(malformed("not a regular file".to_owned()));
    }
    let mut file = File::open(path).map_err(io)?;
    let file_len = file.metadata().map_err(io)?.len();
    let mut header = [0u8; HEADER_V1_LEN as usize];
    match file.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(malformed("file is shorter than a VPK header".into()));
        }
        Err(e) => return Err(io(e)),
    }
    let word =
        |i: usize| u32::from_le_bytes([header[i], header[i + 1], header[i + 2], header[i + 3]]);
    if word(0) != SIGNATURE {
        return Err(malformed("bad signature".into()));
    }
    let header_len = match word(4) {
        1 => HEADER_V1_LEN,
        2 => HEADER_V1_LEN + HEADER_V2_EXTRA,
        v => return Err(malformed(format!("unsupported version {v}"))),
    };
    let tree_len = u64::from(word(8));
    if header_len + tree_len > file_len {
        return Err(malformed(format!(
            "directory tree of {tree_len} bytes does not fit in a {file_len}-byte file"
        )));
    }
    drop(file);

    match catching(|| vpk::from_path(path)) {
        Ok(Ok(pak)) => Ok(pak),
        Ok(Err(e)) => Err(malformed(e.to_string())),
        Err(_) => Err(malformed("the VPK parser panicked".into())),
    }
}

/// Reads one entry and requires its full declared length, since the parser
/// silently returns short data when the file is truncated.
pub(crate) fn read_entry(
    path: &Path,
    name: &str,
    entry: &vpk::entry::VPKEntry,
) -> Result<Vec<u8>, VpkError> {
    let data = entry.get().map_err(|source| VpkError::Io {
        path: entry
            .archive_path
            .as_deref()
            .map_or_else(|| path.to_path_buf(), |p| p.to_path_buf()),
        source,
    })?;
    let expected =
        u64::from(entry.dir_entry.preload_length) + u64::from(entry.dir_entry.file_length);
    if data.len() as u64 != expected {
        return Err(VpkError::Malformed {
            path: path.to_path_buf(),
            message: format!("entry {name} holds {} of its {expected} bytes", data.len()),
        });
    }
    Ok(data.into_owned())
}
