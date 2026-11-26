use std::io::Read;

use super::*;
use crate::test_archives::{bytes_under, zip_bytes, zip_with_listed_size};

const KIB: u64 = 1024;

#[test]
fn the_byte_that_passes_the_cap_fails() {
    let a = Path::new("x.zip");
    let mut budget = Budget::new(10, 5);
    budget.charge(a, 4).unwrap();
    budget.charge(a, 6).unwrap();
    let err = budget.charge(a, 1).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooLarge { limit: 10, .. }),
        "{err:?}"
    );
}

#[test]
fn admit_weighs_listed_totals_against_what_is_left() {
    let a = Path::new("x.zip");
    let mut budget = Budget::new(10, 5);
    budget.admit(a, 10, 5).unwrap();
    let err = budget.admit(a, 0, 1).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooManyEntries { limit: 5, .. }),
        "{err:?}"
    );

    let mut budget = Budget::new(10, 5);
    budget.charge(a, 3).unwrap();
    let err = budget.admit(a, 8, 1).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooLarge { limit: 10, .. }),
        "{err:?}"
    );
    let err = Budget::new(10, 5).admit(a, u64::MAX, 1).unwrap_err();
    assert!(matches!(err, ArchiveError::TooLarge { .. }), "{err:?}");
}

#[test]
fn the_default_budget_is_two_gib_and_65536_entries() {
    let a = Path::new("x.zip");
    let gib2 = 2 * 1024 * 1024 * 1024;
    Budget::default().admit(a, gib2, 65_536).unwrap();
    let err = Budget::default().admit(a, gib2 + 1, 1).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooLarge { limit, .. } if limit == gib2),
        "{err:?}"
    );
    let err = Budget::default().admit(a, 0, 65_537).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooManyEntries { limit: 65_536, .. }),
        "{err:?}"
    );
}

#[test]
fn the_write_guard_stops_a_stream_at_the_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("out").join("a.bin");
    let mut endless = std::io::repeat(7).take(1024 * KIB);
    let mut budget = Budget::new(64 * KIB, 10);
    let err = stream_to_file(Path::new("x.zip"), &mut endless, &target, &mut budget).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooLarge { limit, .. } if limit == 64 * KIB),
        "{err:?}"
    );
    assert!(fs::metadata(&target).unwrap().len() <= 64 * KIB);
}

#[test]
fn a_zip_listed_over_budget_is_refused_before_dest_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("bomb.zip");
    fs::write(
        &zip,
        zip_with_listed_size("a.txt", b"hello", 100 * 1024, false),
    )
    .unwrap();
    let dest = tmp.path().join("out");
    let err = extract_archive_within(&zip, &dest, &mut Budget::new(64 * KIB, 100)).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooLarge { ref path, limit } if *path == zip && limit == 64 * KIB),
        "{err:?}"
    );
    assert!(!dest.exists());
}

#[test]
fn a_zip_that_lies_about_its_size_never_writes_past_the_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("liar.zip");
    fs::write(
        &zip,
        zip_with_listed_size("a.bin", &[0u8; 200 * 1024], 1, true),
    )
    .unwrap();
    let dest = tmp.path().join("out");
    let result = extract_archive_within(&zip, &dest, &mut Budget::new(64 * KIB, 100));
    // Settles whether the zip crate stops at the listed size itself (Malformed) or our
    // guard does (TooLarge); the implementer logs which.
    eprintln!("lying zip result: {result:?}");
    assert!(
        matches!(
            result,
            Err(ArchiveError::TooLarge { .. } | ArchiveError::Malformed { .. })
        ),
        "{result:?}"
    );
    assert!(bytes_under(&dest) <= 64 * KIB);
}

#[test]
fn a_zip_with_more_entries_than_the_budget_is_refused_and_directories_count() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("many.zip");
    fs::write(
        &zip,
        zip_bytes(&[
            ("d/", b""),
            ("d/e/", b""),
            ("d/a.txt", b"a"),
            ("d/b.txt", b"b"),
        ]),
    )
    .unwrap();
    let dest = tmp.path().join("out");
    let err = extract_archive_within(&zip, &dest, &mut Budget::new(64 * KIB, 3)).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooManyEntries { limit: 3, .. }),
        "{err:?}"
    );
    assert!(!dest.exists());
    extract_archive_within(&zip, &dest, &mut Budget::new(64 * KIB, 4)).unwrap();
}
