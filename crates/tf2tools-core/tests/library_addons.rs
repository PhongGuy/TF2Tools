//! Mastercomfig addons: import into the library, install with addons.cfg, uninstall, delete.

mod common;

use std::fs;
use std::path::PathBuf;

use common::{LibraryEnv, library_env, sql, tree, vpk_bytes, write_file};
use tf2tools_core::{AddonName, ImportError, InstallError, LibraryError, LibraryKind};

const CFG: &str = "comfig-custom/cfg/app/addons.cfg";

fn name(s: &str) -> AddonName {
    AddonName::new(s).unwrap()
}

fn vpk(env: &LibraryEnv, file: &str) -> PathBuf {
    let bytes = vpk_bytes(&[("cfg/addons/x.cfg", b"m_rawinput 1".as_slice())]);
    write_file(&env.tmp.path().join("dl").join(file), &bytes)
}

fn import(env: &LibraryEnv, asset: &str, tag: &str) -> tf2tools_core::AddonEntry {
    env.lib
        .import_addon(&vpk(env, asset), &name(asset), tag)
        .unwrap()
}

#[test]
fn addon_names_must_be_plain_mastercomfig_vpks() {
    assert!(AddonName::new("mastercomfig-addon-flat-mouse.vpk").is_some());
    assert!(AddonName::new("mastercomfig-addon-lowmem.VPK").is_some());
    for bad in [
        "flat-mouse.vpk",
        "mastercomfig-addon-x.zip",
        "mastercomfig-addon-../x.vpk",
        "mastercomfig-addon-a/b.vpk",
        "mastercomfig-addon-a\\b.vpk",
        "mastercomfig-addon-.vpk",
    ] {
        assert!(AddonName::new(bad).is_none(), "{bad}");
    }
}

#[test]
fn import_copies_into_the_library_and_upserts_the_tag() {
    let env = library_env();
    sql(&env.dir)
        .execute(
            "INSERT INTO store_addons (asset_name, display_name, download_url, release_tag, fetched_at)
             VALUES ('mastercomfig-addon-flat-mouse.vpk', 'Flat mouse', 'https://example.invalid/a.vpk', '9.9.9', '2026-07-04T17:44:58Z')",
            [],
        )
        .unwrap();
    let asset = "mastercomfig-addon-flat-mouse.vpk";
    let src = vpk(&env, asset);
    let first = env.lib.import_addon(&src, &name(asset), "9.9.8").unwrap();
    assert_eq!(first.name().as_str(), asset);
    assert_eq!(first.display_name(), "Flat Mouse");
    assert_eq!(
        first.description(),
        "Ensures consistent raw mouse input with no OS acceleration and 1:1 zoom sensitivity"
    );
    assert_eq!(first.installed_tag(), "9.9.8");
    assert_eq!(first.store_tag(), Some("9.9.9"));
    assert!(first.update_available());
    assert!(!first.is_installed());
    let stored = env.dir.library_dir(LibraryKind::Addons).join(asset);
    assert_eq!(fs::read(&stored).unwrap(), fs::read(&src).unwrap());

    let second = env.lib.import_addon(&src, &name(asset), "9.9.9").unwrap();
    assert_eq!(second.installed_tag(), "9.9.9");
    assert!(!second.update_available());
    assert_eq!(second.created_at(), first.created_at());
    assert_eq!(env.lib.list_addons(None).unwrap(), vec![second]);
}

#[test]
fn unknown_addons_get_a_derived_display_name() {
    let env = library_env();
    let entry = import(&env, "mastercomfig-addon-big-hud-fix.vpk", "1.0");
    assert_eq!(entry.display_name(), "Big Hud Fix");
    assert_eq!(entry.description(), "Mastercomfig addon");
    assert_eq!(entry.store_tag(), None);
    assert!(!entry.update_available());
}

#[test]
fn import_refuses_a_file_that_is_not_a_vpk() {
    let env = library_env();
    let src = write_file(&env.tmp.path().join("dl/fake.vpk"), b"not a vpk");
    let err = env
        .lib
        .import_addon(&src, &name("mastercomfig-addon-fake.vpk"), "1")
        .unwrap_err();
    assert!(matches!(err, ImportError::NotAVpk { .. }), "{err:?}");
    assert!(env.lib.list_addons(None).unwrap().is_empty());
    assert!(tree(&env.dir.library_dir(LibraryKind::Addons)).is_empty());
}

#[test]
fn install_copies_the_vpk_removes_its_cache_and_writes_addons_cfg() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    import(&env, "mastercomfig-addon-no-tutorial.vpk", "1");
    import(&env, "mastercomfig-addon-flat-mouse.vpk", "1");
    write_file(
        &custom.join("mastercomfig-addon-flat-mouse.vpk.sound.cache"),
        b"c",
    );
    env.lib
        .install_addon(&name("mastercomfig-addon-flat-mouse.vpk"), &env.tf2)
        .unwrap();
    env.lib
        .install_addon(&name("mastercomfig-addon-no-tutorial.vpk"), &env.tf2)
        .unwrap();
    assert_eq!(
        tree(&custom),
        [
            CFG,
            "mastercomfig-addon-flat-mouse.vpk",
            "mastercomfig-addon-no-tutorial.vpk",
        ]
    );
    assert_eq!(
        fs::read_to_string(custom.join(CFG)).unwrap(),
        "exec addons/flat-mouse.cfg\nexec addons/no-tutorial.cfg\n"
    );
    let listed = env.lib.list_addons(Some(&env.tf2)).unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|a| a.is_installed()));
}

#[test]
fn uninstalling_the_last_addon_removes_addons_cfg_but_keeps_the_entry() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    let asset = name("mastercomfig-addon-flat-mouse.vpk");
    import(&env, asset.as_str(), "1");
    env.lib.install_addon(&asset, &env.tf2).unwrap();
    write_file(
        &custom.join("mastercomfig-addon-flat-mouse.vpk.sound.cache"),
        b"c",
    );
    env.lib.uninstall_addon(&asset, &env.tf2).unwrap();
    assert!(tree(&custom).is_empty());
    assert!(custom.join("comfig-custom/cfg/app").is_dir());
    let listed = env.lib.list_addons(Some(&env.tf2)).unwrap();
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].is_installed());
}

#[test]
fn installing_an_addon_not_in_the_library_fails() {
    let env = library_env();
    let err = env
        .lib
        .install_addon(&name("mastercomfig-addon-flat-mouse.vpk"), &env.tf2)
        .unwrap_err();
    assert!(matches!(err, InstallError::NotInLibrary { .. }), "{err:?}");
    assert!(tree(&env.tf2.custom_dir()).is_empty());
}

#[test]
fn delete_with_tf2_uninstalls_and_forgets() {
    let env = library_env();
    let asset = name("mastercomfig-addon-flat-mouse.vpk");
    import(&env, asset.as_str(), "1");
    env.lib.install_addon(&asset, &env.tf2).unwrap();
    env.lib.delete_addon(&asset, Some(&env.tf2)).unwrap();
    assert!(tree(&env.tf2.custom_dir()).is_empty());
    assert!(tree(&env.dir.library_dir(LibraryKind::Addons)).is_empty());
    assert!(env.lib.list_addons(None).unwrap().is_empty());
    let err = env.lib.delete_addon(&asset, None).unwrap_err();
    assert!(matches!(err, LibraryError::NotFound { .. }), "{err:?}");
}
