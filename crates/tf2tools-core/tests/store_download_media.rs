//! Sound and crosshair downloads: which files inside a download become library entries,
//! and how they are named.

use crate::common;

use std::io::Cursor;
use std::path::Path;

use common::{
    StoreEnv, count, gb_file, sql, store_env, tmp_is_empty, tree, vpk_bytes, wav_bytes, zip_bytes,
};
use tf2tools_core::{
    CrosshairFormat, DownloadError, GbId, ImportError, LibraryEntry, LibraryError, LibraryKind,
    Provenance, SoundFormat, StoreKind,
};

fn wav() -> Vec<u8> {
    wav_bytes(1, 2, 44100, 16, 8)
}

fn mono() -> Vec<u8> {
    wav_bytes(1, 1, 44100, 16, 8)
}

fn vtf() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/crosshairs/vtf/Brackets_TobYZ.vtf"),
    )
    .unwrap()
}

fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(32, 32, image::Rgba([255, 0, 0, 255]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn seed_store_rows(env: &StoreEnv) {
    sql(&env.dir)
        .execute_batch(
            "INSERT INTO store_sounds (gb_id, name, author, fetched_at) VALUES (93300, 'Jeremy Kim', 'legend', 'x');
             INSERT INTO store_crosshairs (gb_id, name, author, fetched_at) VALUES (456577, 'Kunai', 'Vexcenot', 'x');",
        )
        .unwrap();
}

async fn fetch(
    env: &StoreEnv,
    kind: StoreKind,
    gb_id: u64,
    file_name: &str,
    body: Vec<u8>,
) -> Result<Vec<LibraryEntry>, DownloadError> {
    let file = gb_file(env, kind, gb_id, file_name, body, "").await;
    env.client.download(kind, &file, &|_| {}).await
}

fn sounds(entries: &[LibraryEntry]) -> Vec<(String, SoundFormat, String)> {
    entries
        .iter()
        .map(|e| match e {
            LibraryEntry::Sound(s) => (
                s.name().to_owned(),
                s.format(),
                s.original_filename().to_owned(),
            ),
            other => panic!("not a sound: {other:?}"),
        })
        .collect()
}

fn crosshairs(entries: &[LibraryEntry]) -> Vec<(String, CrosshairFormat)> {
    entries
        .iter()
        .map(|e| match e {
            LibraryEntry::Crosshair(c) => (c.name().to_owned(), c.format()),
            other => panic!("not a crosshair: {other:?}"),
        })
        .collect()
}

fn s(text: &str) -> String {
    text.to_owned()
}

#[tokio::test]
async fn a_bare_wav_becomes_one_sound_named_after_the_store_item() {
    let env = store_env().await;
    seed_store_rows(&env);
    let entries = fetch(&env, StoreKind::Sounds, 93300, "hit.wav", wav())
        .await
        .unwrap();
    assert_eq!(
        sounds(&entries),
        [(s("Jeremy Kim"), SoundFormat::Wav, s("hit.wav"))]
    );
    let LibraryEntry::Sound(sound) = &entries[0] else {
        unreachable!()
    };
    assert!(
        matches!(sound.provenance(), Provenance::GameBanana { gb_id, .. } if *gb_id == GbId::new(93300))
    );
    assert!(tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn every_playable_wav_in_an_archive_becomes_a_sound() {
    let env = store_env().await;
    seed_store_rows(&env);
    let (good, bad) = (wav(), mono());
    let archive = zip_bytes(&[
        ("pack/b.wav", good.as_slice()),
        ("pack/a.wav", good.as_slice()),
        ("pack/mono.wav", bad.as_slice()),
        ("pack/readme.txt", b"hi".as_slice()),
    ]);
    let entries = fetch(&env, StoreKind::Sounds, 93300, "pack.zip", archive)
        .await
        .unwrap();
    assert_eq!(
        sounds(&entries),
        [
            (s("Jeremy Kim - a"), SoundFormat::Wav, s("a.wav")),
            (s("Jeremy Kim - b"), SoundFormat::Wav, s("b.wav")),
        ]
    );
}

#[tokio::test]
async fn vpks_in_a_sound_archive_win_over_wavs() {
    let env = store_env().await;
    seed_store_rows(&env);
    let good = wav();
    let vpk = vpk_bytes(&[("sound/ui/hitsound.wav", good.as_slice())]);
    let archive = zip_bytes(&[
        ("pack/hit.vpk", vpk.as_slice()),
        ("pack/a.wav", good.as_slice()),
    ]);
    let entries = fetch(&env, StoreKind::Sounds, 93300, "pack.zip", archive)
        .await
        .unwrap();
    assert_eq!(
        sounds(&entries),
        [(s("Jeremy Kim"), SoundFormat::Vpk, s("hit.vpk"))]
    );
}

#[tokio::test]
async fn a_bare_vpk_sound_is_imported_whole() {
    let env = store_env().await;
    seed_store_rows(&env);
    let good = wav();
    let vpk = vpk_bytes(&[("sound/ui/hitsound.wav", good.as_slice())]);
    let entries = fetch(&env, StoreKind::Sounds, 93300, "hit.vpk", vpk)
        .await
        .unwrap();
    assert_eq!(
        sounds(&entries),
        [(s("Jeremy Kim"), SoundFormat::Vpk, s("hit.vpk"))]
    );
}

#[tokio::test]
async fn a_sound_without_a_store_row_is_named_after_its_file() {
    let env = store_env().await;
    let entries = fetch(&env, StoreKind::Sounds, 5, "boop.wav", wav())
        .await
        .unwrap();
    assert_eq!(
        sounds(&entries),
        [(s("boop"), SoundFormat::Wav, s("boop.wav"))]
    );
}

#[tokio::test]
async fn mp3_only_downloads_are_refused() {
    let env = store_env().await;
    let archive = zip_bytes(&[
        ("pack/a.mp3", b"ID3".as_slice()),
        ("pack/b.txt", b"x".as_slice()),
    ]);
    let result = fetch(&env, StoreKind::Sounds, 6, "pack.zip", archive).await;
    assert!(
        matches!(&result, Err(DownloadError::NoUsableFiles { reason }) if reason.contains("MP3/OGG")),
        "{result:?}"
    );
    assert_eq!(count(&env.dir, "library_sounds"), 0);
    assert!(tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn unplayable_wavs_are_named_in_the_refusal() {
    let env = store_env().await;
    let bad = mono();
    let archive = zip_bytes(&[
        ("pack/mono.wav", bad.as_slice()),
        ("pack/b.txt", b"x".as_slice()),
    ]);
    let result = fetch(&env, StoreKind::Sounds, 7, "pack.zip", archive).await;
    assert!(
        matches!(&result, Err(DownloadError::NoUsableFiles { reason }) if reason.contains("mono.wav")),
        "{result:?}"
    );
}

#[tokio::test]
async fn vtfs_in_an_archive_become_crosshairs_and_scripts_are_skipped() {
    let env = store_env().await;
    seed_store_rows(&env);
    let (vtf, png) = (vtf(), png());
    let archive = zip_bytes(&[
        (
            "pack/materials/vgui/replay/thumbnails/b.vtf",
            vtf.as_slice(),
        ),
        (
            "pack/materials/vgui/replay/thumbnails/a.vtf",
            vtf.as_slice(),
        ),
        ("pack/scripts/c.vtf", vtf.as_slice()),
        ("pack/scripts/tf_weapon_smg.txt", b"x".as_slice()),
        ("pack/preview.png", png.as_slice()),
    ]);
    let entries = fetch(&env, StoreKind::Crosshairs, 456577, "pack.zip", archive)
        .await
        .unwrap();
    assert_eq!(
        crosshairs(&entries),
        [
            (s("Kunai - a"), CrosshairFormat::Vtf),
            (s("Kunai - b"), CrosshairFormat::Vtf)
        ]
    );
    assert!(tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn pngs_are_used_when_there_is_no_vtf() {
    let env = store_env().await;
    seed_store_rows(&env);
    let png = png();
    let archive = zip_bytes(&[
        ("x/dot.png", png.as_slice()),
        ("x/readme.txt", b"x".as_slice()),
    ]);
    let entries = fetch(&env, StoreKind::Crosshairs, 456577, "x.zip", archive)
        .await
        .unwrap();
    assert_eq!(crosshairs(&entries), [(s("Kunai"), CrosshairFormat::Png)]);
}

#[tokio::test]
async fn a_vpk_inside_a_crosshair_archive_is_opened() {
    let env = store_env().await;
    seed_store_rows(&env);
    let vtf = vtf();
    let vpk = vpk_bytes(&[("materials/vgui/replay/thumbnails/dot.vtf", vtf.as_slice())]);
    let archive = zip_bytes(&[
        ("x/pack.vpk", vpk.as_slice()),
        ("x/readme.txt", b"x".as_slice()),
    ]);
    let entries = fetch(&env, StoreKind::Crosshairs, 456577, "x.zip", archive)
        .await
        .unwrap();
    assert_eq!(crosshairs(&entries), [(s("Kunai"), CrosshairFormat::Vtf)]);
}

#[tokio::test]
async fn a_bare_vtf_is_one_crosshair() {
    let env = store_env().await;
    seed_store_rows(&env);
    let entries = fetch(&env, StoreKind::Crosshairs, 456577, "dot.vtf", vtf())
        .await
        .unwrap();
    assert_eq!(crosshairs(&entries), [(s("Kunai"), CrosshairFormat::Vtf)]);
}

#[tokio::test]
async fn a_crosshair_download_with_nothing_usable_is_refused() {
    let env = store_env().await;
    let archive = zip_bytes(&[
        ("x/readme.txt", b"x".as_slice()),
        ("x/notes.md", b"y".as_slice()),
    ]);
    let result = fetch(&env, StoreKind::Crosshairs, 8, "x.zip", archive).await;
    assert!(
        matches!(&result, Err(DownloadError::NoUsableFiles { reason }) if reason.contains("VTF or PNG")),
        "{result:?}"
    );
    assert_eq!(count(&env.dir, "library_crosshairs"), 0);
    assert!(tmp_is_empty(&env.dir));
}

/// Fails every library_sounds insert after the first, as a full disk would.
const FAIL_SECOND_SOUND: &str = "CREATE TRIGGER fail_second_sound BEFORE INSERT ON library_sounds
    WHEN (SELECT COUNT(*) FROM library_sounds) >= 1
    BEGIN SELECT RAISE(ABORT, 'disk full'); END;";

async fn two_wav_file(env: &StoreEnv) -> tf2tools_core::StoreFile {
    let good = wav();
    let archive = zip_bytes(&[
        ("pack/a.wav", good.as_slice()),
        ("pack/b.wav", good.as_slice()),
    ]);
    gb_file(env, StoreKind::Sounds, 93300, "pack.zip", archive, "").await
}

#[tokio::test]
async fn a_failed_entry_rolls_back_the_download_and_a_retry_succeeds() {
    let env = store_env().await;
    seed_store_rows(&env);
    sql(&env.dir).execute_batch(FAIL_SECOND_SOUND).unwrap();
    let file = two_wav_file(&env).await;
    let result = env.client.download(StoreKind::Sounds, &file, &|_| {}).await;
    assert!(
        matches!(
            &result,
            Err(DownloadError::Import(ImportError::Database { .. }))
        ),
        "{result:?}"
    );
    assert_eq!(count(&env.dir, "library_sounds"), 0);
    assert!(tree(&env.dir.library_dir(LibraryKind::Sounds)).is_empty());
    assert!(tmp_is_empty(&env.dir));

    sql(&env.dir)
        .execute_batch("DROP TRIGGER fail_second_sound;")
        .unwrap();
    let entries = env
        .client
        .download(StoreKind::Sounds, &file, &|_| {})
        .await
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(count(&env.dir, "library_sounds"), 2);
}

#[tokio::test]
async fn a_rollback_that_fails_reports_what_stayed() {
    let env = store_env().await;
    seed_store_rows(&env);
    sql(&env.dir).execute_batch(FAIL_SECOND_SOUND).unwrap();
    sql(&env.dir)
        .execute_batch(
            "CREATE TRIGGER keep_sounds BEFORE DELETE ON library_sounds
             BEGIN SELECT RAISE(ABORT, 'locked'); END;",
        )
        .unwrap();
    let file = two_wav_file(&env).await;
    match env.client.download(StoreKind::Sounds, &file, &|_| {}).await {
        Err(DownloadError::RollbackFailed {
            imported,
            cause,
            source,
        }) => {
            assert_eq!(
                sounds(&imported),
                [(s("Jeremy Kim - a"), SoundFormat::Wav, s("a.wav"))]
            );
            assert!(
                matches!(*cause, DownloadError::Import(ImportError::Database { .. })),
                "{cause:?}"
            );
            assert!(
                matches!(source, LibraryError::Database { .. }),
                "{source:?}"
            );
        }
        other => panic!("expected RollbackFailed, got {other:?}"),
    }
    assert_eq!(count(&env.dir, "library_sounds"), 1);
}

#[tokio::test]
async fn a_failed_crosshair_entry_rolls_back_the_others() {
    let env = store_env().await;
    seed_store_rows(&env);
    sql(&env.dir)
        .execute_batch(
            "CREATE TRIGGER fail_second_crosshair BEFORE INSERT ON library_crosshairs
             WHEN (SELECT COUNT(*) FROM library_crosshairs) >= 1
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    let vtf = vtf();
    let archive = zip_bytes(&[("x/a.vtf", vtf.as_slice()), ("x/b.vtf", vtf.as_slice())]);
    let result = fetch(&env, StoreKind::Crosshairs, 456577, "x.zip", archive).await;
    assert!(
        matches!(&result, Err(DownloadError::Import(_))),
        "{result:?}"
    );
    assert_eq!(count(&env.dir, "library_crosshairs"), 0);
    assert!(tree(&env.dir.library_dir(LibraryKind::Crosshairs)).is_empty());
}
