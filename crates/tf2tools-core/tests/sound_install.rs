//! Hitsound and killsound install into tf/custom and the installed-sound scan.

use crate::common;

use std::fs;

use common::{
    LibraryEnv, library_env, set_setting, setting, tree, vpk_bytes, wav_bytes, write_file,
};
use tf2tools_core::{
    InstallError, LibraryError, LibraryKind, Provenance, SoundEntry, SoundFormat, SoundId,
    SoundKind,
};

const UI: &str = "tf2tools-sounds/sound/ui";

fn good_wav() -> Vec<u8> {
    wav_bytes(1, 2, 44_100, 16, 8)
}

fn import_wav(env: &LibraryEnv, name: &str) -> SoundEntry {
    let src = write_file(&env.tmp.path().join(format!("in/{name}.wav")), &good_wav());
    env.lib.import_sound(&src, name, Provenance::Local).unwrap()
}

fn import_vpk(env: &LibraryEnv, name: &str) -> SoundEntry {
    let wav = good_wav();
    let bytes = vpk_bytes(&[("sound/ui/hitsound.wav", wav.as_slice())]);
    let src = write_file(&env.tmp.path().join(format!("in/{name}.vpk")), &bytes);
    env.lib.import_sound(&src, name, Provenance::Local).unwrap()
}

#[test]
fn installing_a_wav_fills_the_slot_and_clears_the_vpk_and_cache() {
    let env = library_env();
    let sound = import_wav(&env, "Ding");
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("tf2tools-hitsound.vpk"), b"old");
    write_file(&custom.join("tf2tools-sounds/sound.cache"), b"cache");
    set_setting(&env.dir, "active_profile_id", "p1");
    env.lib
        .install_sound(sound.id(), SoundKind::Hitsound, &env.tf2)
        .unwrap();
    assert_eq!(tree(&custom), [format!("{UI}/hitsound.wav")]);
    assert_eq!(
        fs::read(custom.join(UI).join("hitsound.wav")).unwrap(),
        good_wav()
    );
    assert_eq!(
        setting(&env.dir, "installed_hitsound_folder").as_deref(),
        Some(sound.folder_name())
    );
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
}

#[test]
fn installing_a_vpk_writes_tf2tools_kind_vpk_and_removes_the_wav() {
    let env = library_env();
    let sound = import_vpk(&env, "Pack");
    let custom = env.tf2.custom_dir();
    write_file(&custom.join(UI).join("killsound.wav"), b"old");
    env.lib
        .install_sound(sound.id(), SoundKind::Killsound, &env.tf2)
        .unwrap();
    assert_eq!(tree(&custom), ["tf2tools-killsound.vpk"]);
    let library_vpk = env
        .dir
        .library_dir(LibraryKind::Sounds)
        .join(sound.folder_name())
        .join("Pack.vpk");
    assert_eq!(
        fs::read(custom.join("tf2tools-killsound.vpk")).unwrap(),
        fs::read(library_vpk).unwrap()
    );
    assert_eq!(
        setting(&env.dir, "installed_killsound_folder").as_deref(),
        Some(sound.folder_name())
    );
}

#[test]
fn uninstall_removes_both_forms_of_one_kind_and_clears_its_settings() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join(UI).join("hitsound.wav"), b"w");
    write_file(&custom.join("tf2tools-hitsound.vpk"), b"v");
    write_file(&custom.join(UI).join("killsound.wav"), b"k");
    set_setting(&env.dir, "installed_hitsound_folder", "x");
    set_setting(&env.dir, "active_profile_id", "p1");
    env.lib
        .uninstall_sound(SoundKind::Hitsound, &env.tf2)
        .unwrap();
    assert_eq!(tree(&custom), [format!("{UI}/killsound.wav")]);
    assert_eq!(
        setting(&env.dir, "installed_hitsound_folder").as_deref(),
        Some("")
    );
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
}

#[test]
fn installing_an_unknown_id_is_not_in_library() {
    let env = library_env();
    let err = env
        .lib
        .install_sound(&SoundId::new("nope"), SoundKind::Hitsound, &env.tf2)
        .unwrap_err();
    assert!(matches!(err, InstallError::NotInLibrary { .. }), "{err:?}");
    assert!(tree(&env.tf2.custom_dir()).is_empty());
}

#[test]
fn installed_sounds_reports_each_slot_and_foreign_sounds() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    let empty = env.lib.installed_sounds(&env.tf2).unwrap();
    for kind in [SoundKind::Hitsound, SoundKind::Killsound] {
        assert_eq!(empty.slot(kind).active(), None);
        assert_eq!(empty.slot(kind).managed(), None);
    }
    assert!(empty.conflicts().is_empty());

    let sound = import_wav(&env, "Ding");
    env.lib
        .install_sound(sound.id(), SoundKind::Hitsound, &env.tf2)
        .unwrap();
    let scan = env.lib.installed_sounds(&env.tf2).unwrap();
    assert_eq!(
        scan.slot(SoundKind::Hitsound).active(),
        Some(SoundFormat::Wav)
    );
    assert_eq!(scan.slot(SoundKind::Hitsound).managed(), Some(sound.id()));

    write_file(&custom.join("tf2tools-hitsound.vpk"), b"v");
    write_file(&custom.join(UI).join("killsound.wav"), b"k");
    write_file(&custom.join("zpack/sound/ui/killsound.wav"), b"z");
    write_file(&custom.join("otherpack/sound/ui/hitsound.wav"), b"o");
    write_file(&custom.join("otherpack/sound/ui/killsound.wav"), b"o");
    let scan = env.lib.installed_sounds(&env.tf2).unwrap();
    let hit = scan.slot(SoundKind::Hitsound);
    assert_eq!(hit.active(), Some(SoundFormat::Vpk));
    let kill = scan.slot(SoundKind::Killsound);
    assert_eq!(kill.active(), Some(SoundFormat::Wav));
    assert_eq!(kill.managed(), None);
    let conflicts: Vec<(&str, SoundKind)> = scan
        .conflicts()
        .iter()
        .map(|c| (c.folder_name(), c.kind()))
        .collect();
    assert_eq!(
        conflicts,
        [
            ("otherpack", SoundKind::Hitsound),
            ("otherpack", SoundKind::Killsound),
            ("zpack", SoundKind::Killsound),
        ]
    );
}

#[test]
fn installed_sound_bytes_prefers_the_vpk() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    let err = env
        .lib
        .installed_sound_bytes(SoundKind::Hitsound, &env.tf2)
        .unwrap_err();
    assert!(matches!(err, LibraryError::NotFound { .. }), "{err:?}");

    let wav = wav_bytes(1, 2, 44_100, 16, 2);
    write_file(&custom.join(UI).join("hitsound.wav"), &wav);
    assert_eq!(
        env.lib
            .installed_sound_bytes(SoundKind::Hitsound, &env.tf2)
            .unwrap(),
        wav
    );

    let packed = wav_bytes(1, 2, 44_100, 16, 6);
    write_file(
        &custom.join("tf2tools-hitsound.vpk"),
        &vpk_bytes(&[("sound/ui/hitsound.wav", packed.as_slice())]),
    );
    assert_eq!(
        env.lib
            .installed_sound_bytes(SoundKind::Hitsound, &env.tf2)
            .unwrap(),
        packed
    );
}

#[test]
fn deleting_an_installed_vpk_sound_uninstalls_it() {
    let env = library_env();
    let sound = import_vpk(&env, "Pack");
    env.lib
        .install_sound(sound.id(), SoundKind::Hitsound, &env.tf2)
        .unwrap();
    env.lib.delete_sound(sound.id(), Some(&env.tf2)).unwrap();
    assert!(!env.tf2.custom_dir().join("tf2tools-hitsound.vpk").exists());
    assert_eq!(
        setting(&env.dir, "installed_hitsound_folder").as_deref(),
        Some("")
    );
    assert!(env.lib.list_sounds().unwrap().is_empty());
}

#[test]
fn deleting_an_installed_wav_sound_uninstalls_it() {
    let env = library_env();
    let sound = import_wav(&env, "Ding");
    env.lib
        .install_sound(sound.id(), SoundKind::Hitsound, &env.tf2)
        .unwrap();
    env.lib.delete_sound(sound.id(), Some(&env.tf2)).unwrap();
    assert!(!env.tf2.custom_dir().join(UI).join("hitsound.wav").exists());
    assert_eq!(
        setting(&env.dir, "installed_hitsound_folder").as_deref(),
        Some("")
    );
    let scan = env.lib.installed_sounds(&env.tf2).unwrap();
    assert_eq!(scan.slot(SoundKind::Hitsound).active(), None);
    assert!(env.lib.list_sounds().unwrap().is_empty());
}

#[test]
fn deleting_the_installed_sound_clears_the_active_profile() {
    let env = library_env();
    let sound = import_vpk(&env, "Pack");
    env.lib
        .install_sound(sound.id(), SoundKind::Hitsound, &env.tf2)
        .unwrap();
    set_setting(&env.dir, "active_profile_id", "p1");
    env.lib.delete_sound(sound.id(), Some(&env.tf2)).unwrap();
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
}

#[test]
fn deleting_a_sound_not_installed_keeps_the_active_profile() {
    let env = library_env();
    let sound = import_vpk(&env, "Pack");
    set_setting(&env.dir, "active_profile_id", "p1");
    env.lib.delete_sound(sound.id(), Some(&env.tf2)).unwrap();
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some("p1")
    );
}
