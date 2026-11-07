//! Reading the first WAV out of a VPK, and refusing malformed VPKs without panicking.

use crate::common;

use std::path::{Path, PathBuf};
use tf2tools_core::{VpkError, first_wav_in_vpk};

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn returns_the_first_wav_by_path_order() {
    let tmp = tempfile::tempdir().unwrap();
    let vpk = write(
        tmp.path(),
        "tf2tools-hitsound.vpk",
        &common::vpk_bytes(&[
            ("sound/ui/killsound.wav", b"KILL"),
            ("readme.txt", b"hello"),
            ("sound/ui/hitsound.wav", b"HIT"),
        ]),
    );
    assert_eq!(first_wav_in_vpk(&vpk).unwrap(), b"HIT");
}

#[test]
fn wav_extension_match_ignores_case() {
    let tmp = tempfile::tempdir().unwrap();
    let vpk = write(
        tmp.path(),
        "upper.vpk",
        &common::vpk_bytes(&[("readme.txt", b"x"), ("sound/ui/HITSOUND.WAV", b"UPPER")]),
    );
    assert_eq!(first_wav_in_vpk(&vpk).unwrap(), b"UPPER");
}

#[test]
fn a_vpk_without_a_wav_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let vpk = write(
        tmp.path(),
        "nowav.vpk",
        &common::vpk_bytes(&[("readme.txt", b"x")]),
    );
    let err = first_wav_in_vpk(&vpk).unwrap_err();
    assert!(
        matches!(err, VpkError::NoWav { ref path } if *path == vpk),
        "{err:?}"
    );
}

#[test]
fn a_missing_file_is_an_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("absent.vpk");
    let err = first_wav_in_vpk(&missing).unwrap_err();
    assert!(
        matches!(err, VpkError::Io { ref path, .. } if *path == missing),
        "{err:?}"
    );
}

#[test]
fn a_non_vpk_file_is_malformed() {
    let tmp = tempfile::tempdir().unwrap();
    let vpk = write(tmp.path(), "zip.vpk", b"PK\x03\x04this is not a vpk at all");
    let err = first_wav_in_vpk(&vpk).unwrap_err();
    assert!(matches!(err, VpkError::Malformed { .. }), "{err:?}");
}

#[test]
fn an_unsupported_version_is_malformed() {
    let tmp = tempfile::tempdir().unwrap();
    let mut bytes = common::vpk_bytes(&[("a.wav", b"A")]);
    bytes[4..8].copy_from_slice(&3u32.to_le_bytes());
    let vpk = write(tmp.path(), "v3.vpk", &bytes);
    let err = first_wav_in_vpk(&vpk).unwrap_err();
    assert!(matches!(err, VpkError::Malformed { .. }), "{err:?}");
}

#[test]
fn an_oversized_tree_length_is_refused_before_allocating() {
    let tmp = tempfile::tempdir().unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0x55aa_1234u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0xffff_fff0u32.to_le_bytes());
    bytes.extend_from_slice(b"\0\0\0");
    let vpk = write(tmp.path(), "huge.vpk", &bytes);
    let err = first_wav_in_vpk(&vpk).unwrap_err();
    assert!(matches!(err, VpkError::Malformed { .. }), "{err:?}");
}

#[test]
fn a_truncated_vpk_is_malformed_without_panicking() {
    let tmp = tempfile::tempdir().unwrap();
    let full = common::vpk_bytes(&[("sound/ui/hitsound.wav", b"RIFF0000WAVE")]);
    for len in [0, 4, 11, 12, full.len() / 2, full.len() - 1] {
        let vpk = write(tmp.path(), &format!("cut{len}.vpk"), &full[..len]);
        let err = first_wav_in_vpk(&vpk).unwrap_err();
        assert!(
            matches!(err, VpkError::Malformed { .. }),
            "len {len}: {err:?}"
        );
    }
}
