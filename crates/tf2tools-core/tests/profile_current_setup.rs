//! Reading what is installed now, and saving it as a profile.

use crate::common;

use common::{
    FLAT_ADDON as FLAT, LibraryEnv, PYRO_ADDON as PYRO, gb, import_addon_as as addon,
    import_hud_as, import_png_as, import_wav_as, library_env, place_mastercomfig,
    set_mastercomfig_state, write_file,
};
use tf2tools_core::{
    CrosshairRef, Preset, ProfileCrosshairRef, ProfileItem, Provenance, SlotMode, SoundKind,
};

fn outside(env: &LibraryEnv) {
    place_mastercomfig(env, None);
}

fn managed(env: &LibraryEnv, preset: &str) {
    place_mastercomfig(env, Some(preset));
}

#[test]
fn the_current_setup_reads_the_installed_items() {
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

    let setup = env.lib.current_setup(&env.tf2).unwrap();
    let SlotMode::Set(item) = &setup.hud else {
        panic!("{:?}", setup.hud)
    };
    assert_eq!(item.folder_name(), hud.folder_name());
    assert_eq!(item.provenance(), &gb(42));
    assert_eq!(
        setup.sounds.hit.as_ref().map(ProfileItem::folder_name),
        Some(hit.folder_name())
    );
    assert_eq!(setup.sounds.kill, None);
    let refs: Vec<_> = setup
        .crosshairs
        .iter()
        .map(|c| (c.weapon_script(), c.crosshair().clone()))
        .collect();
    assert_eq!(
        refs,
        [
            (
                "tf_weapon_rocketlauncher",
                ProfileCrosshairRef::Library {
                    folder_name: dot.folder_name().to_owned()
                }
            ),
            (
                "tf_weapon_scattergun",
                ProfileCrosshairRef::Builtin("cpma_9".into())
            ),
        ]
    );
    assert_eq!(setup.graphics, SlotMode::None);
    assert!(setup.addons.is_empty());
}

#[test]
fn no_hud_is_none_and_a_hud_the_library_cannot_name_is_leave() {
    let env = library_env();
    assert_eq!(env.lib.current_setup(&env.tf2).unwrap().hud, SlotMode::None);
    write_file(&env.tf2.custom_dir().join("foreign/info.vdf"), b"x");
    assert_eq!(
        env.lib.current_setup(&env.tf2).unwrap().hud,
        SlotMode::Leave
    );
}

#[test]
fn graphics_is_the_managed_choices_or_leave_for_an_outside_install() {
    let env = library_env();
    outside(&env);
    assert_eq!(
        env.lib.current_setup(&env.tf2).unwrap().graphics,
        SlotMode::Leave
    );
    set_mastercomfig_state(&env.dir, "9.100.1", "high");
    let setup = env.lib.current_setup(&env.tf2).unwrap();
    let SlotMode::Set(graphics) = &setup.graphics else {
        panic!("{:?}", setup.graphics)
    };
    assert_eq!(graphics.choices.preset(), Preset::High);
}

#[test]
fn addons_are_the_installed_library_addons() {
    let env = library_env();
    managed(&env, "medium");
    let flat = addon(&env, FLAT);
    addon(&env, PYRO);
    env.lib.install_addon(&flat, &env.tf2).unwrap();
    assert_eq!(env.lib.current_setup(&env.tf2).unwrap().addons, [flat]);
}

#[test]
fn save_current_without_mastercomfig_takes_graphics_out_and_leaves_addons() {
    let env = library_env();
    let flat = addon(&env, FLAT);
    env.lib.install_addon(&flat, &env.tf2).unwrap();
    let p = env.lib.save_current_profile("Now", &env.tf2).unwrap();
    assert_eq!(p.hud(), &SlotMode::None);
    assert!(matches!(p.sounds(), SlotMode::Set(_)), "{:?}", p.sounds());
    assert!(
        matches!(p.crosshairs(), SlotMode::Set(_)),
        "{:?}",
        p.crosshairs()
    );
    assert_eq!(p.graphics(), &SlotMode::None);
    assert_eq!(p.addons(), &SlotMode::Leave);
}

#[test]
fn save_current_with_a_managed_mastercomfig_saves_its_choices_and_only_the_installed_addons() {
    let env = library_env();
    managed(&env, "low");
    let flat = addon(&env, FLAT);
    addon(&env, PYRO);
    env.lib.install_addon(&flat, &env.tf2).unwrap();
    let p = env.lib.save_current_profile("Now", &env.tf2).unwrap();
    let SlotMode::Set(graphics) = p.graphics() else {
        panic!("{:?}", p.graphics())
    };
    assert_eq!(graphics.choices.preset(), Preset::Low);
    let SlotMode::Set(addons) = p.addons() else {
        panic!("{:?}", p.addons())
    };
    assert_eq!(addons.on, [flat]);
    assert!(addons.only_these);
}

#[test]
fn save_current_over_an_outside_mastercomfig_leaves_graphics_and_sets_addons() {
    let env = library_env();
    outside(&env);
    let p = env.lib.save_current_profile("Now", &env.tf2).unwrap();
    assert_eq!(p.graphics(), &SlotMode::Leave);
    let SlotMode::Set(addons) = p.addons() else {
        panic!("{:?}", p.addons())
    };
    assert!(addons.on.is_empty() && addons.only_these);
}

#[test]
fn an_unmanaged_hud_is_leave_in_the_setup_and_none_in_a_saved_profile() {
    let env = library_env();
    write_file(&env.tf2.custom_dir().join("foreign/info.vdf"), b"x");
    assert_eq!(
        env.lib.current_setup(&env.tf2).unwrap().hud,
        SlotMode::Leave
    );
    let p = env.lib.save_current_profile("Mine", &env.tf2).unwrap();
    assert_eq!(p.hud(), &SlotMode::None);
}
