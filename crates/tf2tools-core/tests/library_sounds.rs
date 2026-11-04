//! Sound library: WAV and VPK import with provenance, rename, delete and playback bytes.

mod common;

use std::fs;

use common::{LibraryEnv, library_env, sql, tree, vpk_bytes, wav_bytes, write_file};
use tf2tools_core::{
    GbId, ImportError, LibraryError, LibraryKind, Provenance, SoundEntry, SoundFormat, SoundId,
    WavError,
};

fn good_wav() -> Vec<u8> {
    wav_bytes(1, 2, 44_100, 16, 8)
}

fn import_wav(env: &LibraryEnv, file: &str, name: &str) -> SoundEntry {
    let src = write_file(&env.tmp.path().join("in").join(file), &good_wav());
    env.lib.import_sound(&src, name, Provenance::Local).unwrap()
}

fn columns(env: &LibraryEnv, id: &SoundId) -> (String, Option<i64>, String) {
    sql(&env.dir)
        .query_row(
            "SELECT source, store_gb_id, sound_type FROM library_sounds WHERE id = ?1",
            [id.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
}

fn sounds_dir_is_empty(env: &LibraryEnv) -> bool {
    fs::read_dir(env.dir.library_dir(LibraryKind::Sounds))
        .unwrap()
        .count()
        == 0
}

#[test]
fn a_wav_imports_into_a_named_folder() {
    let env = library_env();
    let sound = import_wav(&env, "Boop Sound.wav", "Boop Sound");
    assert_eq!(sound.name(), "Boop Sound");
    assert_eq!(sound.original_filename(), "Boop Sound.wav");
    assert_eq!(sound.format(), SoundFormat::Wav);
    assert_eq!(sound.provenance(), &Provenance::Local);
    let id8 = &sound.id().as_str()[..8];
    assert_eq!(sound.folder_name(), format!("boop-sound-{id8}"));
    let folder = env
        .dir
        .library_dir(LibraryKind::Sounds)
        .join(sound.folder_name());
    assert_eq!(tree(&folder), ["Boop Sound.wav"]);
    assert_eq!(
        columns(&env, sound.id()),
        ("local".into(), None, "wav".into())
    );
    assert_eq!(env.lib.list_sounds().unwrap(), vec![sound]);
}

#[test]
fn a_gamebanana_vpk_is_recorded_as_store_with_its_gb_id() {
    let env = library_env();
    let wav = good_wav();
    let src = write_file(
        &env.tmp.path().join("in/pack.VPK"),
        &vpk_bytes(&[("sound/ui/hitsound.wav", wav.as_slice())]),
    );
    let gb = Provenance::GameBanana {
        gb_id: GbId::new(12345),
        author: "someone".into(),
    };
    let sound = env.lib.import_sound(&src, "Pack", gb).unwrap();
    assert_eq!(sound.format(), SoundFormat::Vpk);
    // Sounds have no author column, so the author does not survive the round trip.
    assert_eq!(
        sound.provenance(),
        &Provenance::GameBanana {
            gb_id: GbId::new(12345),
            author: String::new()
        }
    );
    assert_eq!(
        columns(&env, sound.id()),
        ("store".into(), Some(12345), "vpk".into())
    );
}

#[test]
fn an_unsupported_wav_is_refused_and_nothing_is_written() {
    let env = library_env();
    let src = write_file(
        &env.tmp.path().join("mono.wav"),
        &wav_bytes(1, 1, 22_050, 16, 8),
    );
    let err = env
        .lib
        .import_sound(&src, "Mono", Provenance::Local)
        .unwrap_err();
    assert!(
        matches!(err, ImportError::Wav(WavError::Unsupported { .. })),
        "{err:?}"
    );
    assert!(sounds_dir_is_empty(&env));
    assert!(env.lib.list_sounds().unwrap().is_empty());
}

#[test]
fn a_vpk_without_the_magic_is_refused() {
    let env = library_env();
    let src = write_file(&env.tmp.path().join("fake.vpk"), b"not a vpk at all");
    let err = env
        .lib
        .import_sound(&src, "Fake", Provenance::Local)
        .unwrap_err();
    assert!(matches!(err, ImportError::NotAVpk { .. }), "{err:?}");
    assert!(sounds_dir_is_empty(&env));
}

#[test]
fn other_extensions_and_curated_provenance_are_refused() {
    let env = library_env();
    let mp3 = write_file(&env.tmp.path().join("boop.mp3"), b"ID3");
    let err = env
        .lib
        .import_sound(&mp3, "Boop", Provenance::Local)
        .unwrap_err();
    assert!(
        matches!(err, ImportError::UnsupportedFile { .. }),
        "{err:?}"
    );

    let wav = write_file(&env.tmp.path().join("ok.wav"), &good_wav());
    let curated = Provenance::Curated {
        hud_id: "rayshud".into(),
    };
    let err = env.lib.import_sound(&wav, "Ok", curated).unwrap_err();
    assert!(matches!(err, ImportError::UnsupportedProvenance), "{err:?}");
    assert!(sounds_dir_is_empty(&env));
}

#[test]
fn a_hostile_name_stays_inside_the_library() {
    let env = library_env();
    let sound = import_wav(&env, "x.wav", "../../escape/..\\x");
    let folder = sound.folder_name();
    assert!(!folder.contains('/') && !folder.contains('\\') && !folder.starts_with('.'));
    assert!(
        env.dir
            .library_dir(LibraryKind::Sounds)
            .join(folder)
            .is_dir()
    );
    assert!(!env.dir.root().join("escape").exists());
    assert!(!env.dir.root().join("library/escape").exists());
    assert_eq!(sound.name(), "../../escape/..\\x");
}

#[test]
fn rename_changes_only_the_display_name() {
    let env = library_env();
    let sound = import_wav(&env, "a.wav", "Old");
    env.lib.rename_sound(sound.id(), "New Name").unwrap();
    let listed = env.lib.list_sounds().unwrap();
    assert_eq!(listed[0].name(), "New Name");
    assert_eq!(listed[0].folder_name(), sound.folder_name());
}

#[test]
fn unknown_ids_are_not_found() {
    let env = library_env();
    let id = SoundId::new("nope");
    for err in [
        env.lib.rename_sound(&id, "x").unwrap_err(),
        env.lib.delete_sound(&id, None).unwrap_err(),
        env.lib.sound_bytes(&id).unwrap_err(),
    ] {
        assert!(matches!(err, LibraryError::NotFound { .. }), "{err:?}");
    }
}

#[test]
fn sound_bytes_reads_the_wav_or_the_first_wav_inside_a_vpk() {
    let env = library_env();
    let wav = import_wav(&env, "a.wav", "A");
    assert_eq!(env.lib.sound_bytes(wav.id()).unwrap(), good_wav());

    let first = good_wav();
    let other = wav_bytes(1, 2, 44_100, 16, 4);
    let src = write_file(
        &env.tmp.path().join("in/two.vpk"),
        &vpk_bytes(&[
            ("sound/ui/zz.wav", other.as_slice()),
            ("sound/ui/aa.wav", first.as_slice()),
        ]),
    );
    let vpk = env
        .lib
        .import_sound(&src, "Two", Provenance::Local)
        .unwrap();
    assert_eq!(env.lib.sound_bytes(vpk.id()).unwrap(), first);
}

#[test]
fn delete_removes_the_folder_and_row() {
    let env = library_env();
    let sound = import_wav(&env, "a.wav", "A");
    env.lib.delete_sound(sound.id(), None).unwrap();
    assert!(env.lib.list_sounds().unwrap().is_empty());
    assert!(sounds_dir_is_empty(&env));
}
