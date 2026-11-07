//! Adopting addon VPKs already in tf/custom into an empty addon library, as v5's backfill.

use crate::common;

use std::fs;

use common::{LibraryEnv, count, library_env, sql, vpk_bytes, write_file};
use tf2tools_core::{AddonName, LibraryKind};

const CFG: &str = "comfig-custom/cfg/app/addons.cfg";
const FLAT: &str = "mastercomfig-addon-flat-mouse.vpk";
const TUTOR: &str = "mastercomfig-addon-no-tutorial.vpk";

fn addon_bytes(tag: &str) -> Vec<u8> {
    vpk_bytes(&[("cfg/addons/x.cfg", tag.as_bytes())])
}

fn install_raw(env: &LibraryEnv, file: &str) -> Vec<u8> {
    let bytes = addon_bytes(file);
    write_file(&env.tf2.custom_dir().join(file), &bytes);
    bytes
}

fn seed_store_tag(env: &LibraryEnv, asset: &str, tag: &str) {
    sql(&env.dir)
        .execute(
            "INSERT INTO store_addons (asset_name, display_name, download_url, release_tag,
                 fetched_at)
             VALUES (?1, 'x', 'https://example.invalid/x', ?2, '2025-01-01T00:00:00+00:00')",
            [asset, tag],
        )
        .unwrap();
}

#[test]
fn installed_addons_are_copied_recorded_and_listed_in_addons_cfg() {
    let env = library_env();
    let flat = install_raw(&env, FLAT);
    install_raw(&env, TUTOR);
    write_file(&env.tf2.custom_dir().join("other.vpk"), b"x");
    write_file(&env.tf2.custom_dir().join("mastercomfig-addon-.vpk"), b"x");
    seed_store_tag(&env, FLAT, "9.10.3");

    assert_eq!(env.lib.adopt_installed_addons(&env.tf2).unwrap(), 2);
    let library = env.dir.library_dir(LibraryKind::Addons);
    assert_eq!(fs::read(library.join(FLAT)).unwrap(), flat);
    assert!(library.join(TUTOR).is_file());
    let addons = env.lib.list_addons(Some(&env.tf2)).unwrap();
    let tags: Vec<(&str, &str, bool)> = addons
        .iter()
        .map(|a| (a.name().as_str(), a.installed_tag(), a.is_installed()))
        .collect();
    assert!(tags.contains(&(FLAT, "9.10.3", true)));
    assert!(tags.contains(&(TUTOR, "unknown", true)));
    assert_eq!(tags.len(), 2);
    let cfg = fs::read_to_string(env.tf2.custom_dir().join(CFG)).unwrap();
    assert!(cfg.contains("flat-mouse") && cfg.contains("no-tutorial"));
}

#[test]
fn a_library_with_addons_adopts_nothing() {
    let env = library_env();
    let src = write_file(&env.tmp.path().join("dl").join(FLAT), &addon_bytes("a"));
    env.lib
        .import_addon(&src, &AddonName::new(FLAT).unwrap(), "1.0")
        .unwrap();
    install_raw(&env, TUTOR);
    assert_eq!(env.lib.adopt_installed_addons(&env.tf2).unwrap(), 0);
    assert_eq!(count(&env.dir, "library_addons"), 1);
    assert!(
        !env.dir
            .library_dir(LibraryKind::Addons)
            .join(TUTOR)
            .exists()
    );
}

#[test]
fn no_installed_addons_adopts_nothing_and_writes_no_cfg() {
    let env = library_env();
    assert_eq!(env.lib.adopt_installed_addons(&env.tf2).unwrap(), 0);
    assert!(!env.tf2.custom_dir().join(CFG).exists());
}

#[test]
fn a_failed_copy_still_records_the_row() {
    let env = library_env();
    install_raw(&env, FLAT);
    // Database::open created the empty addons dir; a file in its place makes every copy fail.
    let library = env.dir.library_dir(LibraryKind::Addons);
    fs::remove_dir(&library).unwrap();
    write_file(&library, b"not a directory");
    assert_eq!(env.lib.adopt_installed_addons(&env.tf2).unwrap(), 1);
    assert_eq!(count(&env.dir, "library_addons"), 1);
}

#[test]
fn a_stale_library_file_is_replaced_by_the_installed_copy() {
    let env = library_env();
    let installed = install_raw(&env, FLAT);
    write_file(
        &env.dir.library_dir(LibraryKind::Addons).join(FLAT),
        b"stale",
    );
    assert_eq!(env.lib.adopt_installed_addons(&env.tf2).unwrap(), 1);
    assert_eq!(
        fs::read(env.dir.library_dir(LibraryKind::Addons).join(FLAT)).unwrap(),
        installed
    );
}
