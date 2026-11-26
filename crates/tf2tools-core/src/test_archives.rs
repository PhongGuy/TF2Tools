//! Small archives built in code for the unpack-budget tests.

use std::io::Write;
use std::path::Path;

/// A zip holding `entries`; a name ending in `/` is a directory.
pub(crate) fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for &(name, bytes) in entries {
        if let Some(dir) = name.strip_suffix('/') {
            zip.add_directory(dir, options).unwrap();
        } else {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
    }
    zip.finish().unwrap().into_inner()
}

/// A one-file zip whose local and central headers claim `listed` unpacked bytes, whatever
/// `body` really holds. `deflate` picks the method; stored otherwise.
pub(crate) fn zip_with_listed_size(name: &str, body: &[u8], listed: u32, deflate: bool) -> Vec<u8> {
    let method = if deflate {
        zip::CompressionMethod::Deflated
    } else {
        zip::CompressionMethod::Stored
    };
    let options = zip::write::SimpleFileOptions::default().compression_method(method);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file(name, options).unwrap();
    zip.write_all(body).unwrap();
    let mut bytes = zip.finish().unwrap().into_inner();
    // Uncompressed size sits 22 bytes into a local header and 24 into a central one.
    for (signature, offset) in [(b"PK\x03\x04", 22), (b"PK\x01\x02", 24)] {
        let at = bytes.windows(4).position(|w| w == signature).unwrap();
        bytes[at + offset..at + offset + 4].copy_from_slice(&listed.to_le_bytes());
    }
    bytes
}

/// Total bytes of the files under `dir`; 0 when it does not exist.
pub(crate) fn bytes_under(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().unwrap().len())
        .sum()
}

/// A 7z at `path` holding `entries` as files.
pub(crate) fn write_7z(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    for &(name, bytes) in entries {
        writer
            .push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(bytes))
            .unwrap();
    }
    writer.finish().unwrap();
}

/// A RAR of `version` holding `entries` as files.
pub(crate) fn rar_bytes(version: rars::ArchiveVersion, entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = rars::Builder::new(version);
    for &(name, bytes) in entries {
        builder
            .add_bytes(name.as_bytes().to_vec(), bytes.to_vec(), None, None)
            .unwrap();
    }
    let mut out = Vec::new();
    builder
        .write_to(&mut out, &rars::WriterResources::default(), None)
        .unwrap();
    out
}

/// A single-file VPK whose `count` entries (`d/f<i>.txt`) all point at the same `body`.
pub(crate) fn vpk_shared(count: usize, body: &[u8]) -> Vec<u8> {
    let mut tree = Vec::new();
    for i in 0..count {
        for part in ["txt", "d", &format!("f{i}")] {
            tree.extend_from_slice(part.as_bytes());
            tree.push(0);
        }
        tree.extend_from_slice(&0u32.to_le_bytes()); // crc, not checked by readers
        tree.extend_from_slice(&0u16.to_le_bytes()); // preload length
        tree.extend_from_slice(&0x7fffu16.to_le_bytes()); // data follows the tree
        tree.extend_from_slice(&0u32.to_le_bytes()); // every entry at offset 0
        tree.extend_from_slice(&(body.len() as u32).to_le_bytes());
        tree.extend_from_slice(&0xffffu16.to_le_bytes());
        tree.extend_from_slice(&[0, 0]); // end of names, end of paths
    }
    tree.push(0);
    let mut out = Vec::new();
    out.extend_from_slice(&0x55aa_1234u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
    out.extend(tree);
    out.extend_from_slice(body);
    out
}
