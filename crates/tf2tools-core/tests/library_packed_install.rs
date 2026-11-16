//! Installing, switching, reinstalling and checking HUDs whose library copy is packed.

use crate::common;

use common::hud_copy::snapshot;
use common::packed::{convert, hud_dir, loose_hud, zip_path};
use common::{builtin_catalog, library_env, set_setting, setting, tree, write_file};
use tf2tools_core::{DraftSlot, InstallError, ProfileDraft, Provenance, Reinstall};

const FILES: &[(&str, &[u8])] = &[
    ("resource/clientscheme.res", b"\"Scheme\" {}"),
    ("resource/ui/hudlayout.res", b"\"layout\" {}"),
];

#[test]
fn a_packed_hud_installs_the_same_files_as_its_loose_form() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let custom = env.tf2.custom_dir();
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let loose = snapshot(&custom.join("rayshud"));
    env.lib.uninstall_hud(&env.tf2).unwrap();
    convert(&env);
    assert!(zip_path(&env, &hud).is_file());
    let done = env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(done.folder(), "rayshud");
    assert_eq!(snapshot(&custom.join("rayshud")), loose);
    assert!(!custom.join("rayshud/.tf2tools").exists());
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some("rayshud")
    );
}

#[test]
fn switching_between_packed_huds_leaves_only_the_new_one() {
    let env = library_env();
    let first = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let second = loose_hud(&env, "flawhud", FILES, Provenance::Local);
    convert(&env);
    env.lib.install_hud(first.id(), &env.tf2).unwrap();
    env.lib.install_hud(second.id(), &env.tf2).unwrap();
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        [
            "flawhud/info.vdf",
            "flawhud/resource/clientscheme.res",
            "flawhud/resource/ui/hudlayout.res"
        ]
    );
}

#[test]
fn a_packed_hud_with_leftovers_installs_only_its_zip() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    convert(&env);
    write_file(
        &hud_dir(&env, &hud).join("resource/stale.res"),
        b"left by a crash",
    );
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert!(
        !env.tf2
            .custom_dir()
            .join("rayshud/resource/stale.res")
            .exists()
    );
    assert!(
        env.tf2
            .custom_dir()
            .join("rayshud/resource/clientscheme.res")
            .is_file()
    );
}

#[test]
fn a_packed_hud_installs_under_the_folder_its_files_name() {
    let env = library_env();
    let hud = loose_hud(
        &env,
        "flawhud-main",
        &[(
            "resource/ui/mainmenuoverride.res",
            b"\"image\" \"../../custom/flawhud/materials/x\"",
        )],
        Provenance::Local,
    );
    convert(&env);
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "flawhud"
    );
}

#[test]
fn an_unrecorded_install_of_a_packed_hud_can_be_put_back_until_edited() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    convert(&env);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    set_setting(&env.dir, "installed_hud_fingerprint", "");
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
    write_file(
        &env.tf2
            .custom_dir()
            .join("rayshud/resource/clientscheme.res"),
        b"edited",
    );
    let external = env.lib.external_huds(&env.tf2).unwrap();
    assert_eq!(external.len(), 1);
    assert!(external[0].has_changes());
}

#[cfg(target_os = "linux")]
#[test]
fn a_lowercased_install_of_a_packed_hud_can_be_put_back() {
    let env = library_env();
    let hud = loose_hud(
        &env,
        "rayshud",
        &[
            ("Resource/ClientScheme.res", b"#base \"Resource/Other.res\""),
            ("Resource/Other.res", b"\"Other\" {}"),
        ],
        Provenance::Local,
    );
    convert(&env);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert!(
        env.tf2
            .custom_dir()
            .join("rayshud/resource/clientscheme.res")
            .is_file()
    );
    set_setting(&env.dir, "installed_hud_fingerprint", "");
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[test]
fn the_live_link_reinstalls_a_packed_hud() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    convert(&env);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let before = snapshot(&env.tf2.custom_dir().join("rayshud"));
    let done = env.lib.reinstall_hud(hud.id(), &env.tf2).unwrap();
    assert!(
        matches!(&done, Reinstall::Updated { folder } if folder == "rayshud"),
        "{done:?}"
    );
    assert_eq!(snapshot(&env.tf2.custom_dir().join("rayshud")), before);
}

#[test]
fn a_profile_installs_its_packed_hud() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    convert(&env);
    let draft = ProfileDraft::new("P").with_hud(DraftSlot::Item(hud.id().clone()));
    let profile = env.lib.create_profile(&draft).unwrap();
    env.lib
        .apply_profile(profile.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap();
    assert!(
        env.tf2
            .custom_dir()
            .join("rayshud/resource/ui/hudlayout.res")
            .is_file()
    );
}

#[test]
fn a_damaged_zip_fails_the_install_naming_it_and_keeps_the_old_hud() {
    let env = library_env();
    let first = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let second = loose_hud(&env, "flawhud", FILES, Provenance::Local);
    convert(&env);
    env.lib.install_hud(first.id(), &env.tf2).unwrap();
    std::fs::write(zip_path(&env, &second), b"not a zip").unwrap();
    let err = env.lib.install_hud(second.id(), &env.tf2).unwrap_err();
    let zip = zip_path(&env, &second);
    assert!(
        matches!(&err, InstallError::Io { path, .. } if *path == zip),
        "{err:?}"
    );
    assert!(env.tf2.custom_dir().join("rayshud/info.vdf").is_file());
}
