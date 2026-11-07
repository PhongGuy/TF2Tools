//! Every writer into the game refuses when the game folder has moved away, and creates
//! nothing where it used to be.

use crate::common;

use common::{
    LibraryEnv, builtin_catalog, import_hud_as, import_png_as, import_wav_as, library_env,
    vpk_bytes, write_file,
};
use tf2tools_core::{
    AddonName, ApplyProfileError, CrosshairRef, DraftSlot, InstallError, ProfileDraft, Provenance,
    ReinstallError, SoundKind,
};

/// Moves the whole game folder away, as a player moving their Steam library would.
fn move_game_away(env: &LibraryEnv) {
    let game = env.tf2.game_dir();
    std::fs::rename(game, env.tmp.path().join("moved")).unwrap();
}

fn assert_nothing_created(env: &LibraryEnv) {
    assert!(
        !env.tf2.game_dir().exists(),
        "the game folder was recreated"
    );
}

fn assert_game_missing(result: Result<impl std::fmt::Debug, InstallError>) {
    match result {
        Err(InstallError::GameMissing { .. }) => {}
        other => panic!("expected GameMissing, got {other:?}"),
    }
}

#[test]
fn an_install_knows_whether_its_game_folder_is_still_there() {
    let env = library_env();
    assert!(env.tf2.is_present());
    move_game_away(&env);
    assert!(!env.tf2.is_present());
}

#[test]
fn installing_a_hud_into_a_moved_game_fails_and_creates_nothing() {
    let env = library_env();
    let hud = import_hud_as(&env, "rayshud", Provenance::Local);
    move_game_away(&env);
    assert_game_missing(env.lib.install_hud(hud.id(), &env.tf2));
    assert_nothing_created(&env);
}

#[test]
fn uninstalling_a_hud_from_a_moved_game_fails() {
    let env = library_env();
    let hud = import_hud_as(&env, "rayshud", Provenance::Local);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    move_game_away(&env);
    assert_game_missing(env.lib.uninstall_hud(&env.tf2));
    assert_game_missing(env.lib.uninstall_one_hud(&env.tf2, "rayshud", &[]));
    assert_nothing_created(&env);
}

#[test]
fn reinstalling_a_hud_into_a_moved_game_fails_and_creates_nothing() {
    let env = library_env();
    let hud = import_hud_as(&env, "rayshud", Provenance::Local);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    move_game_away(&env);
    assert!(matches!(
        env.lib.reinstall_hud(hud.id(), &env.tf2),
        Err(ReinstallError::GameMissing { .. })
    ));
    assert!(matches!(
        env.lib.recover_reinstall(&env.tf2),
        Err(ReinstallError::GameMissing { .. })
    ));
    assert_nothing_created(&env);
}

#[test]
fn installing_a_sound_into_a_moved_game_fails_and_creates_nothing() {
    let env = library_env();
    let sound = import_wav_as(&env, "Ding", Provenance::Local);
    move_game_away(&env);
    assert_game_missing(
        env.lib
            .install_sound(sound.id(), SoundKind::Hitsound, &env.tf2),
    );
    assert_game_missing(env.lib.uninstall_sound(SoundKind::Killsound, &env.tf2));
    assert_nothing_created(&env);
}

#[test]
fn applying_crosshairs_to_a_moved_game_fails_and_creates_nothing() {
    let env = library_env();
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Builtin("cpma_9".into()),
        )
        .unwrap();
    move_game_away(&env);
    assert_game_missing(env.lib.apply_crosshairs(&env.tf2, &builtin_catalog()));
    assert_game_missing(env.lib.remove_crosshairs(&env.tf2));
    assert_game_missing(env.lib.uninstall_crosshair(dot.id(), &env.tf2));
    assert_nothing_created(&env);
}

#[test]
fn installing_an_addon_into_a_moved_game_fails_and_creates_nothing() {
    let env = library_env();
    let asset = AddonName::new("mastercomfig-addon-flat-mouse.vpk").unwrap();
    let bytes = vpk_bytes(&[("cfg/addons/x.cfg", b"m_rawinput 1".as_slice())]);
    let src = write_file(&env.tmp.path().join("dl").join(asset.as_str()), &bytes);
    env.lib.import_addon(&src, &asset, "flat-mouse").unwrap();
    move_game_away(&env);
    assert_game_missing(env.lib.install_addon(&asset, &env.tf2));
    assert_game_missing(env.lib.uninstall_addon(&asset, &env.tf2));
    assert_nothing_created(&env);
}

#[test]
fn applying_a_profile_to_a_moved_game_fails_and_creates_nothing() {
    let env = library_env();
    let hud = import_hud_as(&env, "rayshud", Provenance::Local);
    let draft = ProfileDraft::new("Mine").with_hud(DraftSlot::Item(hud.id().clone()));
    let profile = env.lib.create_profile(&draft).unwrap();
    move_game_away(&env);
    let result = env
        .lib
        .apply_profile(profile.id(), &env.tf2, &builtin_catalog(), false);
    assert!(matches!(result, Err(ApplyProfileError::GameMissing { .. })));
    assert_nothing_created(&env);
}

#[test]
fn a_game_with_tf_but_no_custom_folder_still_installs() {
    let env = library_env();
    let hud = import_hud_as(&env, "rayshud", Provenance::Local);
    std::fs::remove_dir_all(env.tf2.custom_dir()).unwrap();
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert!(env.tf2.custom_dir().join("rayshud").is_dir());
}
