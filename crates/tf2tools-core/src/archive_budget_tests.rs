use std::io::Read;

use super::*;
use crate::test_archives::{
    bytes_under, rar_bytes, vpk_shared, write_7z, zip_bytes, zip_with_listed_size,
};

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

const RARS: [rars::ArchiveVersion; 2] = [rars::ArchiveVersion::Rar29, rars::ArchiveVersion::Rar50];
const FOUR: &[(&str, &[u8])] = &[
    ("d/a.txt", b"a"),
    ("d/b.txt", b"b"),
    ("d/c.txt", b"c"),
    ("d/e.txt", b"e"),
];

#[test]
fn a_7z_over_budget_is_refused_before_dest_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("bomb.7z");
    write_7z(&archive, &[("a.bin", &[0u8; 100 * 1024])]);
    let dest = tmp.path().join("out");
    let err = extract_archive_within(&archive, &dest, &mut Budget::new(64 * KIB, 100)).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooLarge { limit, .. } if limit == 64 * KIB),
        "{err:?}"
    );
    assert!(!dest.exists());
}

#[test]
fn a_7z_whose_entries_sum_past_the_budget_is_refused_up_front() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("two.7z");
    write_7z(
        &archive,
        &[("a.bin", &[1u8; 40 * 1024]), ("b.bin", &[2u8; 40 * 1024])],
    );
    let dest = tmp.path().join("out");
    let err = extract_archive_within(&archive, &dest, &mut Budget::new(64 * KIB, 100)).unwrap_err();
    assert!(matches!(err, ArchiveError::TooLarge { .. }), "{err:?}");
    extract_archive_within(&archive, &dest, &mut Budget::new(80 * KIB, 100)).unwrap();
    assert_eq!(fs::read(dest.join("b.bin")).unwrap(), vec![2u8; 40 * 1024]);
}

#[test]
fn a_rar_over_budget_is_refused_before_dest_exists() {
    let tmp = tempfile::tempdir().unwrap();
    for version in RARS {
        let archive = tmp.path().join(format!("{version:?}.rar"));
        fs::write(
            &archive,
            rar_bytes(version, &[("a.bin", &[0u8; 100 * 1024])]),
        )
        .unwrap();
        let dest = tmp.path().join(format!("out-{version:?}"));
        let err =
            extract_archive_within(&archive, &dest, &mut Budget::new(64 * KIB, 100)).unwrap_err();
        assert!(
            matches!(err, ArchiveError::TooLarge { limit, .. } if limit == 64 * KIB),
            "{version:?}: {err:?}"
        );
        assert!(!dest.exists(), "{version:?}");
    }
}

#[test]
fn a_rar_whose_entries_sum_past_the_budget_is_refused_up_front() {
    let tmp = tempfile::tempdir().unwrap();
    for version in RARS {
        let archive = tmp.path().join(format!("{version:?}.rar"));
        fs::write(
            &archive,
            rar_bytes(
                version,
                &[("a.bin", &[1u8; 40 * 1024]), ("b.bin", &[2u8; 40 * 1024])],
            ),
        )
        .unwrap();
        let dest = tmp.path().join(format!("out-{version:?}"));
        let err =
            extract_archive_within(&archive, &dest, &mut Budget::new(64 * KIB, 100)).unwrap_err();
        assert!(
            matches!(err, ArchiveError::TooLarge { .. }),
            "{version:?}: {err:?}"
        );
        extract_archive_within(&archive, &dest, &mut Budget::new(80 * KIB, 100)).unwrap();
    }
}

#[test]
fn a_7z_or_rar_with_more_entries_than_the_budget_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let seven = tmp.path().join("many.7z");
    write_7z(&seven, FOUR);
    let mut archives = vec![seven];
    for version in RARS {
        let rar = tmp.path().join(format!("many-{version:?}.rar"));
        fs::write(&rar, rar_bytes(version, FOUR)).unwrap();
        archives.push(rar);
    }
    for archive in archives {
        let dest = tmp.path().join("out");
        let err =
            extract_archive_within(&archive, &dest, &mut Budget::new(64 * KIB, 3)).unwrap_err();
        assert!(
            matches!(err, ArchiveError::TooManyEntries { limit: 3, .. }),
            "{archive:?}: {err:?}"
        );
        assert!(!dest.exists(), "{archive:?}");
    }
}

#[test]
fn the_7z_entry_writer_stops_at_the_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("out").join("a.bin");
    let mut endless = std::io::repeat(7).take(1024 * KIB);
    let mut budget = Budget::new(64 * KIB, 10);
    let err =
        write_7z_entry(Path::new("x.7z"), false, &mut endless, &target, &mut budget).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooLarge { limit, .. } if limit == 64 * KIB),
        "{err:?}"
    );
    assert!(fs::metadata(&target).unwrap().len() <= 64 * KIB);
}

#[test]
fn a_rar_output_file_stops_at_the_shared_cap_and_records_too_large() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("a.bin");
    let failure: RarFailure = Rc::default();
    let budget = Rc::new(RefCell::new(Budget::new(64 * KIB, 10)));
    budget
        .borrow_mut()
        .charge(Path::new("x.rar"), 60 * KIB)
        .unwrap();
    let mut out = RarFile {
        file: File::create(&target).unwrap(),
        target: target.clone(),
        archive: PathBuf::from("x.rar"),
        failure: Rc::clone(&failure),
        budget: Rc::clone(&budget),
    };
    out.write_all(&[0u8; 4 * 1024]).unwrap();
    assert!(out.write(&[0u8; 1]).is_err());
    let recorded = failure.take();
    assert!(
        matches!(recorded, Some(ArchiveError::TooLarge { limit, .. }) if limit == 64 * KIB),
        "{recorded:?}"
    );
    assert_eq!(fs::metadata(&target).unwrap().len(), 4 * KIB);
}

#[test]
fn a_vpk_whose_entries_share_bytes_is_refused_on_its_listed_total() {
    let tmp = tempfile::tempdir().unwrap();
    let vpk = tmp.path().join("shared.vpk");
    fs::write(&vpk, vpk_shared(1000, &[1u8; 1024])).unwrap();
    assert!(fs::metadata(&vpk).unwrap().len() < 64 * KIB);
    let dest = tmp.path().join("out");
    let err = extract_archive_within(&vpk, &dest, &mut Budget::new(256 * KIB, 2000)).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooLarge { limit, .. } if limit == 256 * KIB),
        "{err:?}"
    );
    assert!(!dest.exists());
    extract_archive_within(&vpk, &dest, &mut Budget::new(1000 * KIB, 2000)).unwrap();
}

#[test]
fn a_vpk_with_more_entries_than_the_budget_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let vpk = tmp.path().join("many.vpk");
    fs::write(&vpk, vpk_shared(4, b"x")).unwrap();
    let dest = tmp.path().join("out");
    let err = extract_archive_within(&vpk, &dest, &mut Budget::new(64 * KIB, 3)).unwrap_err();
    assert!(
        matches!(err, ArchiveError::TooManyEntries { limit: 3, .. }),
        "{err:?}"
    );
    assert!(!dest.exists());
}
