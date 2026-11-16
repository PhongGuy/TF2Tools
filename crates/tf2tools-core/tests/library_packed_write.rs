//! Imports, kept HUDs and store updates land packed; deleting removes either form.

use crate::common;

use common::hud_copy::store_item;
use common::packed::{hud_dir, loose_files, packed_files, zip_path};
use common::{library_env, tree, write_file, zip_bytes};
use tf2tools_core::{HudEntry, Provenance, RgbaImage, rgba_to_vtf};

fn import(
    env: &common::LibraryEnv,
    name: &str,
    files: &[(&str, &[u8])],
    provenance: Provenance,
) -> HudEntry {
    let named: Vec<(String, &[u8])> = files
        .iter()
        .map(|(rel, b)| (format!("darhud/{rel}"), *b))
        .collect();
    let refs: Vec<(&str, &[u8])> = named.iter().map(|(p, b)| (p.as_str(), *b)).collect();
    let archive = write_file(
        &env.tmp.path().join(format!("in/{name}.zip")),
        &zip_bytes(&refs),
    );
    env.lib.import_hud(&archive, provenance).unwrap()
}

#[test]
fn an_imported_hud_is_stored_packed_with_its_thumbnail() {
    let env = library_env();
    let red =
        rgba_to_vtf(&RgbaImage::from_pixels(64, 32, [220u8, 20, 20, 255].repeat(64 * 32)).unwrap())
            .unwrap();
    let hud = import(
        &env,
        "v1",
        &[
            ("info.vdf", b"\"darhud\" {}"),
            ("materials/console/background_upward_widescreen.vtf", &red),
        ],
        Provenance::Local,
    );
    assert!(zip_path(&env, &hud).is_file());
    assert_eq!(loose_files(&env, &hud), Vec::<String>::new());
    let names: Vec<String> = packed_files(&env, &hud)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(
        names,
        [
            "info.vdf",
            "materials/console/background_upward_widescreen.vtf"
        ]
    );
    assert!(hud.has_thumbnail());
    assert!(
        hud_dir(&env, &hud)
            .join(".tf2tools/thumbnail.jpg")
            .is_file()
    );
}

#[test]
fn a_store_update_replaces_a_packed_hud_whole() {
    let env = library_env();
    let v1 = import(
        &env,
        "v1",
        &[("info.vdf", b"\"v1\" {}"), ("resource/old.res", b"old")],
        store_item(),
    );
    let v2 = import(&env, "v2", &[("info.vdf", b"\"v2\" {}")], store_item());
    assert_eq!(v2.folder_name(), v1.folder_name());
    assert_eq!(
        packed_files(&env, &v2),
        [("info.vdf".to_owned(), b"\"v2\" {}".to_vec())]
    );
    assert_eq!(loose_files(&env, &v2), Vec::<String>::new());
    let meta: Vec<String> = tree(&hud_dir(&env, &v2).join(".tf2tools"));
    assert!(meta.iter().all(|f| !f.ends_with(".tmp")), "{meta:?}");
}

#[test]
fn a_store_update_leaves_the_installed_copy_in_place() {
    let env = library_env();
    let v1 = import(&env, "v1", &[("info.vdf", b"\"v1\" {}")], store_item());
    env.lib.install_hud(v1.id(), &env.tf2).unwrap();
    import(&env, "v2", &[("info.vdf", b"\"v2\" {}")], store_item());
    let installed = env.tf2.custom_dir().join("darhud/info.vdf");
    assert_eq!(std::fs::read(installed).unwrap(), b"\"v1\" {}");
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[test]
fn a_kept_external_hud_is_stored_packed() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("manual/info.vdf"), b"\"manual\" {}");
    write_file(&custom.join("manual/resource/mine.res"), b"mine");
    let external = env.lib.external_huds(&env.tf2).unwrap();
    let kept = env.lib.keep_external_huds(&external, &env.tf2).unwrap();
    assert_eq!(kept.len(), 1);
    assert!(zip_path(&env, &kept[0]).is_file());
    assert_eq!(loose_files(&env, &kept[0]), Vec::<String>::new());
    assert_eq!(
        packed_files(&env, &kept[0])[1],
        ("resource/mine.res".to_owned(), b"mine".to_vec())
    );
    assert!(custom.join("manual/resource/mine.res").is_file());
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[test]
fn imported_huds_install_and_switch_without_a_conversion() {
    let env = library_env();
    let first = import(
        &env,
        "a",
        &[("info.vdf", b"\"a\" {}"), ("resource/a.res", b"a")],
        Provenance::Local,
    );
    env.lib.install_hud(first.id(), &env.tf2).unwrap();
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        ["darhud/info.vdf", "darhud/resource/a.res"]
    );
}

#[test]
fn deleting_a_packed_hud_removes_its_folder() {
    let env = library_env();
    let hud = import(&env, "v1", &[("info.vdf", b"\"v1\" {}")], Provenance::Local);
    env.lib.delete_hud(hud.id()).unwrap();
    assert!(!hud_dir(&env, &hud).exists());
}

#[test]
fn an_import_whose_zip_does_not_read_back_stays_loose() {
    let mut env = library_env();
    env.lib.set_pack_rename(common::packed::rename_then_damage);
    let files: &[(&str, &[u8])] = &[("info.vdf", b"\"a\" {}"), ("resource/a.res", b"a")];
    let hud = import(&env, "a", files, Provenance::Local);
    assert!(!zip_path(&env, &hud).exists());
    let loose: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(rel, bytes)| ((*rel).to_owned(), bytes.to_vec()))
        .collect();
    assert_eq!(common::packed::library_files(&env, &hud), loose);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        ["darhud/info.vdf", "darhud/resource/a.res"]
    );
}
