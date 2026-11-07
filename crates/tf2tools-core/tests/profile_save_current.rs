//! Saving the installed HUD, sounds and crosshair assignments as a new profile.

use crate::common;

use common::{gb, import_hud_as, import_png_as, import_wav_as, library_env, write_file};
use tf2tools_core::{CrosshairRef, ProfileCrosshairRef, ProfileError, Provenance, SoundKind};

#[test]
fn save_current_reads_the_installed_state_from_disk() {
    let env = library_env();
    let hud = import_hud_as(&env, "budhud", gb(42));
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    env.lib
        .install_sound(hit.id(), SoundKind::Hitsound, &env.tf2)
        .unwrap();
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Builtin("cpma_9".into()),
        )
        .unwrap();
    env.lib
        .assign(
            "tf_weapon_rocketlauncher",
            &CrosshairRef::Library(dot.id().clone()),
        )
        .unwrap();

    let p = env.lib.save_current_profile(" Now ", &env.tf2).unwrap();
    assert_eq!(p.name(), "Now");
    assert_eq!(p.hud().unwrap().folder_name(), hud.folder_name());
    assert_eq!(p.hud().unwrap().provenance(), &gb(42));
    assert_eq!(p.hitsound().unwrap().folder_name(), hit.folder_name());
    assert!(p.killsound().is_none());
    let refs: Vec<_> = p
        .crosshairs()
        .iter()
        .map(|c| c.crosshair().clone())
        .collect();
    assert_eq!(
        refs,
        [
            ProfileCrosshairRef::Library {
                folder_name: dot.folder_name().to_owned()
            },
            ProfileCrosshairRef::Builtin("cpma_9".into()),
        ]
    );
    assert_eq!(env.lib.profiles().unwrap(), [p]);
}

#[test]
fn an_unmanaged_or_ambiguous_hud_leaves_the_slot_empty() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("foreign/info.vdf"), b"x");
    let p = env.lib.save_current_profile("One", &env.tf2).unwrap();
    assert!(p.hud().is_none());

    let hud = import_hud_as(&env, "budhud", Provenance::Local);
    let named = env.lib.external_huds(&env.tf2).unwrap();
    env.lib
        .install_hud_discarding(hud.id(), &env.tf2, &named)
        .unwrap();
    write_file(&custom.join("second/info.vdf"), b"x");
    let p = env.lib.save_current_profile("Two", &env.tf2).unwrap();
    assert!(p.hud().is_none());
}

#[test]
fn a_foreign_sound_file_is_not_saved() {
    let env = library_env();
    write_file(
        &env.tf2
            .custom_dir()
            .join("tf2tools-sounds/sound/ui/hitsound.wav"),
        b"not ours",
    );
    let p = env.lib.save_current_profile("Mine", &env.tf2).unwrap();
    assert!(p.hitsound().is_none());
}

#[test]
fn a_deleted_installed_sound_is_gone_from_the_game_and_the_saved_profile() {
    let env = library_env();
    let hit = import_wav_as(&env, "Ding", Provenance::Local);
    env.lib
        .install_sound(hit.id(), SoundKind::Hitsound, &env.tf2)
        .unwrap();
    env.lib.delete_sound(hit.id(), Some(&env.tf2)).unwrap();
    let p = env.lib.save_current_profile("After", &env.tf2).unwrap();
    assert!(p.hitsound().is_none());
    let scan = env.lib.installed_sounds(&env.tf2).unwrap();
    assert_eq!(scan.slot(SoundKind::Hitsound).active(), None);
}

#[test]
fn save_current_refuses_a_blank_name() {
    let env = library_env();
    assert!(matches!(
        env.lib.save_current_profile("  ", &env.tf2),
        Err(ProfileError::EmptyName)
    ));
}

#[test]
fn a_blank_name_is_refused_before_the_installed_state_is_scanned() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    std::fs::remove_dir_all(&custom).unwrap();
    std::fs::write(&custom, b"not a folder").unwrap();
    // The scan fails on this tree...
    assert!(matches!(
        env.lib.save_current_profile("Named", &env.tf2),
        Err(ProfileError::Library(_))
    ));
    // ...yet a blank name is what the caller hears.
    assert!(matches!(
        env.lib.save_current_profile("  ", &env.tf2),
        Err(ProfileError::EmptyName)
    ));
}
