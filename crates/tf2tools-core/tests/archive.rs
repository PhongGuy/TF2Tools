//! Archive extraction by magic bytes, content-root descent and unsafe-entry refusal.

use crate::common;

use std::fs;
use std::path::{Path, PathBuf};
use tf2tools_core::{ArchiveError, extract_archive, find_content_root};

fn write(path: &Path, bytes: &[u8]) -> PathBuf {
    fs::write(path, bytes).unwrap();
    path.to_path_buf()
}

#[test]
fn zip_extracts_and_descends_single_wrapper_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = write(
        &tmp.path().join("hud.zip"),
        &common::zip_bytes(&[
            ("myhud-abc123/myhud/info.vdf", b"\"hud\" {}"),
            ("myhud-abc123/myhud/resource/ui/a.res", b"res"),
        ]),
    );
    let dest = tmp.path().join("out");
    let root = extract_archive(&archive, &dest).unwrap();
    assert_eq!(root, dest.join("myhud-abc123").join("myhud"));
    assert_eq!(fs::read(root.join("resource/ui/a.res")).unwrap(), b"res");
}

#[test]
fn zip_with_several_top_level_entries_returns_dest() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = write(
        &tmp.path().join("sounds.zip"),
        &common::zip_bytes(&[("a.wav", b"A"), ("b/c.wav", b"C")]),
    );
    let dest = tmp.path().join("out");
    assert_eq!(extract_archive(&archive, &dest).unwrap(), dest);
    assert_eq!(fs::read(dest.join("b/c.wav")).unwrap(), b"C");
}

#[test]
fn zip_directory_entries_are_created() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = write(
        &tmp.path().join("dirs.zip"),
        &common::zip_bytes(&[("pack/", b""), ("pack/sub/", b""), ("pack/sub/x.txt", b"x")]),
    );
    let dest = tmp.path().join("out");
    assert_eq!(
        extract_archive(&archive, &dest).unwrap(),
        dest.join("pack").join("sub")
    );
}

#[test]
fn seven_z_extracts() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("sounds.7z");
    common::write_7z(
        &archive,
        &[("sounds/hit.wav", b"HIT"), ("sounds/kill.wav", b"KILL")],
    );
    let dest = tmp.path().join("out");
    let root = extract_archive(&archive, &dest).unwrap();
    assert_eq!(root, dest.join("sounds"));
    assert_eq!(fs::read(root.join("kill.wav")).unwrap(), b"KILL");
}

#[test]
fn vpk_is_detected_by_magic_and_fully_extracted() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = write(
        &tmp.path().join("pack.bin"),
        &common::vpk_bytes(&[("sound/ui/hitsound.wav", b"HIT"), ("readme.txt", b"R")]),
    );
    let dest = tmp.path().join("out");
    assert_eq!(extract_archive(&archive, &dest).unwrap(), dest);
    assert_eq!(
        fs::read(dest.join("sound/ui/hitsound.wav")).unwrap(),
        b"HIT"
    );
    assert_eq!(fs::read(dest.join("readme.txt")).unwrap(), b"R");
}

#[test]
fn rar_is_rejected_and_dest_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = write(&tmp.path().join("hud.rar"), b"Rar!\x1a\x07\x01\x00junk");
    let dest = tmp.path().join("out");
    let err = extract_archive(&archive, &dest).unwrap_err();
    assert!(
        matches!(err, ArchiveError::Rar { ref path } if *path == archive),
        "{err:?}"
    );
    assert!(!dest.exists());
}

#[test]
fn a_bare_file_is_returned_as_is() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write(&tmp.path().join("hit.wav"), b"RIFF\x24\0\0\0WAVEfmt ");
    let dest = tmp.path().join("out");
    assert_eq!(extract_archive(&file, &dest).unwrap(), file);
    assert!(!dest.exists());
}

fn assert_refused_with_nothing_written(archive: &Path, tmp: &Path) {
    let dest = tmp.join("box").join("out");
    let err = extract_archive(archive, &dest).unwrap_err();
    assert!(matches!(err, ArchiveError::UnsafePath { .. }), "{err:?}");
    assert!(
        !tmp.join("box").join("evil.txt").exists(),
        "wrote outside dest"
    );
    assert!(!dest.join("ok.txt").exists(), "wrote before refusing");
}

#[test]
fn zip_slip_is_refused_before_anything_is_written() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = write(
        &tmp.path().join("slip.zip"),
        &common::zip_bytes(&[("ok.txt", b"ok"), ("../evil.txt", b"evil")]),
    );
    assert_refused_with_nothing_written(&archive, tmp.path());
}

#[test]
fn seven_z_traversal_is_refused_before_anything_is_written() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("slip.7z");
    common::write_7z(&archive, &[("ok.txt", b"ok"), ("../evil.txt", b"evil")]);
    assert_refused_with_nothing_written(&archive, tmp.path());
}

#[test]
fn vpk_traversal_is_refused_before_anything_is_written() {
    let tmp = tempfile::tempdir().unwrap();
    for (i, evil) in ["../evil.txt", "..\\evil.txt"].into_iter().enumerate() {
        let archive = write(
            &tmp.path().join(format!("slip{i}.vpk")),
            &common::vpk_bytes(&[("ok.txt", b"ok"), (evil, b"evil")]),
        );
        assert_refused_with_nothing_written(&archive, tmp.path());
    }
}

#[test]
fn corrupt_archives_are_malformed_without_panicking() {
    let tmp = tempfile::tempdir().unwrap();
    let vpk = common::vpk_bytes(&[("sound/ui/hitsound.wav", b"RIFF0000WAVE")]);
    let cases: [(&str, Vec<u8>); 3] = [
        ("cut.vpk", vpk[..vpk.len() / 2].to_vec()),
        (
            "bad.7z",
            [&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C][..], &[0xEE; 40][..]].concat(),
        ),
        ("bad.zip", [&b"PK\x03\x04"[..], &[0xEE; 40][..]].concat()),
    ];
    for (name, bytes) in cases {
        let archive = write(&tmp.path().join(name), &bytes);
        let err = extract_archive(&archive, &tmp.path().join(format!("out-{name}"))).unwrap_err();
        assert!(
            matches!(err, ArchiveError::Malformed { .. }),
            "{name}: {err:?}"
        );
    }
}

#[test]
fn a_missing_archive_is_an_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("absent.zip");
    let err = extract_archive(&missing, &tmp.path().join("out")).unwrap_err();
    assert!(
        matches!(err, ArchiveError::Io { ref path, .. } if *path == missing),
        "{err:?}"
    );
}

#[test]
fn find_content_root_descends_only_through_lone_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let nested = tmp.path().join("x");
    fs::create_dir_all(nested.join("only/a")).unwrap();
    fs::create_dir_all(nested.join("only/b")).unwrap();
    assert_eq!(find_content_root(&nested).unwrap(), nested.join("only"));

    let single_file = tmp.path().join("y");
    fs::create_dir_all(&single_file).unwrap();
    fs::write(single_file.join("lone.txt"), b"x").unwrap();
    assert_eq!(find_content_root(&single_file).unwrap(), single_file);

    let empty = tmp.path().join("z");
    fs::create_dir_all(&empty).unwrap();
    assert_eq!(find_content_root(&empty).unwrap(), empty);

    let err = find_content_root(&tmp.path().join("absent")).unwrap_err();
    assert!(matches!(err, ArchiveError::Io { .. }), "{err:?}");
}
