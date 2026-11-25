//! What `Library::storage` counts: each kind in its own bucket, packed items unpacked.

use std::path::Path;

use crate::common;
use crate::gb_addon_install::{download, no, setup, vpk};

use common::logstf::{days_ago, now, seed_stored};
use common::packed::{gb_zip, packed_files, zip_path};
use common::{library_env, tree, write_file, zip_bytes};
use tf2tools_core::{
    AddonName, Database, LibraryKind, LogsCache, Provenance, StorageKind, StorageUse,
};

/// Every file length under `root`; a missing folder is 0.
fn lengths(root: &Path) -> u64 {
    tree(root)
        .iter()
        .map(|rel| std::fs::metadata(root.join(rel)).unwrap().len())
        .sum()
}

#[test]
fn the_kinds_are_listed_in_card_order() {
    assert_eq!(
        StorageKind::ALL,
        [
            StorageKind::Huds,
            StorageKind::Sounds,
            StorageKind::Crosshairs,
            StorageKind::Addons,
            StorageKind::LogsTf,
            StorageKind::StoreData,
            StorageKind::ImageCache,
            StorageKind::DemoCache,
        ]
    );
    assert_eq!(StorageUse::default().total(), 0);
}

#[test]
fn an_empty_library_has_only_the_database() {
    let env = library_env();
    let s = env.lib.storage().unwrap();
    for kind in [
        StorageKind::Huds,
        StorageKind::Sounds,
        StorageKind::Crosshairs,
        StorageKind::Addons,
        StorageKind::ImageCache,
        StorageKind::DemoCache,
    ] {
        assert_eq!((s.bytes(kind), s.unpacked(kind)), (0, 0), "{kind:?}");
    }
    assert!(
        s.bytes(StorageKind::StoreData) > 0,
        "the database has pages"
    );
}

#[test]
fn downloaded_demos_are_the_demo_cache_and_partial_downloads_count() {
    let env = library_env();
    let demos = env.dir.demo_cache_dir();
    write_file(&demos.join("7.dem"), &[1u8; 4000]);
    write_file(&demos.join(".8.dem.abc123.part"), &[2u8; 600]);
    let s = env.lib.storage().unwrap();
    assert_eq!(s.bytes(StorageKind::DemoCache), 4600);
    assert_eq!(s.unpacked(StorageKind::DemoCache), 4600);
    assert_eq!(s.bytes(StorageKind::ImageCache), 0);
    for kind in [
        StorageKind::Huds,
        StorageKind::Sounds,
        StorageKind::Crosshairs,
        StorageKind::Addons,
    ] {
        assert_eq!(s.bytes(kind), 0, "{kind:?}");
    }
}

#[test]
fn each_library_kind_counts_only_its_own_folder() {
    let env = library_env();
    let archive = write_file(
        &env.tmp.path().join("in/rayshud.zip"),
        &zip_bytes(&[
            ("rayshud/info.vdf", b"\"rayshud\" {}"),
            ("rayshud/a.res", &[7u8; 4096]),
        ]),
    );
    env.lib.import_hud(&archive, Provenance::Local).unwrap();
    common::import_wav_as(&env, "pop", Provenance::Local);
    common::import_png_as(&env, "Dot", Provenance::Local);
    let vpk_path = write_file(
        &env.tmp.path().join("in/flat.vpk"),
        &common::vpk_bytes(&[("materials/a.vmt", b"x")]),
    );
    let name = AddonName::new("mastercomfig-addon-flat-mouse.vpk").unwrap();
    env.lib.import_addon(&vpk_path, &name, "1").unwrap();
    write_file(&env.dir.root().join("library/stray/big.bin"), &[1u8; 4096]);
    write_file(&env.dir.root().join("hud_presets/p.json"), b"{}");
    let s = env.lib.storage().unwrap();
    let dir = |kind| env.dir.library_dir(kind);
    assert_eq!(s.bytes(StorageKind::Huds), lengths(&dir(LibraryKind::Huds)));
    assert_eq!(
        s.bytes(StorageKind::Sounds),
        lengths(&dir(LibraryKind::Sounds))
    );
    assert_eq!(
        s.bytes(StorageKind::Crosshairs),
        lengths(&dir(LibraryKind::Crosshairs))
    );
    assert_eq!(
        s.bytes(StorageKind::Addons),
        lengths(&dir(LibraryKind::Addons))
    );
    assert!(s.bytes(StorageKind::Sounds) > 0 && s.bytes(StorageKind::Addons) > 0);
    let library: u64 = [
        StorageKind::Huds,
        StorageKind::Sounds,
        StorageKind::Crosshairs,
        StorageKind::Addons,
    ]
    .iter()
    .map(|k| s.bytes(*k))
    .sum();
    assert_eq!(
        library,
        lengths(&env.dir.root().join("library")) - 4096,
        "the stray folder is not counted"
    );
    assert_eq!(
        s.unpacked(StorageKind::Sounds),
        s.bytes(StorageKind::Sounds)
    );
}

#[test]
fn packed_huds_and_crosshairs_count_their_files_unpacked() {
    let env = library_env();
    let archive = write_file(
        &env.tmp.path().join("in/rayshud.zip"),
        &zip_bytes(&[
            ("rayshud/info.vdf", b"\"rayshud\" {}"),
            ("rayshud/materials/bg.vtf", &[7u8; 65536]),
        ]),
    );
    let hud = env.lib.import_hud(&archive, Provenance::Local).unwrap();
    let dot = common::import_png_as(&env, "Dot", Provenance::Local);
    let s = env.lib.storage().unwrap();
    let hud_zip = std::fs::metadata(zip_path(&env, &hud)).unwrap().len();
    let hud_files: u64 = packed_files(&env, &hud)
        .iter()
        .map(|(_, b)| b.len() as u64)
        .sum();
    assert_eq!(
        s.unpacked(StorageKind::Huds),
        s.bytes(StorageKind::Huds) - hud_zip + hud_files
    );
    let folder = env
        .dir
        .library_dir(LibraryKind::Crosshairs)
        .join(dot.folder_name());
    let cross_zip = std::fs::metadata(folder.join("copy.zip")).unwrap().len();
    let cross_files: u64 = common::packed::crosshair_zip_files(&env.dir, dot.folder_name())
        .iter()
        .map(|(_, b)| b.len() as u64)
        .sum();
    assert_eq!(
        s.unpacked(StorageKind::Crosshairs),
        s.bytes(StorageKind::Crosshairs) - cross_zip + cross_files
    );
}

#[tokio::test]
async fn gamebanana_add_on_copies_count_as_add_ons_and_unpack() {
    let s = setup().await;
    let d = download(&s, 7, "Toon", "toon.vpk", vpk(&[0u8; 65536]), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &["toon.vpk".to_owned()], &s.tf2, &no)
        .unwrap();
    let use_ = s.env.lib.storage().unwrap();
    let gb = s.env.dir.gb_addon_dir();
    assert_eq!(
        use_.bytes(StorageKind::Addons),
        lengths(&gb) + lengths(&s.env.dir.library_dir(LibraryKind::Addons))
    );
    let zip = std::fs::metadata(gb_zip(&s.env.dir, 7)).unwrap().len();
    let vpk_len = vpk(&[0u8; 65536]).len() as u64;
    assert_eq!(
        use_.unpacked(StorageKind::Addons),
        use_.bytes(StorageKind::Addons) - zip + vpk_len
    );
}

#[test]
fn store_images_and_weapon_icons_are_the_image_cache() {
    let env = library_env();
    write_file(&env.dir.image_cache_dir().join("ab/cd.webp"), &[1u8; 3000]);
    write_file(
        &env.dir.weapon_icon_dir().join("scattergun.png"),
        &[2u8; 500],
    );
    let s = env.lib.storage().unwrap();
    assert_eq!(s.bytes(StorageKind::ImageCache), 3500);
    assert_eq!(s.unpacked(StorageKind::ImageCache), 3500);
}

#[test]
fn the_logs_cache_counts_what_the_logs_tf_card_counts() {
    let env = library_env();
    seed_stored(&env.dir, 1, days_ago(2), now());
    let s = env.lib.storage().unwrap();
    let logs = LogsCache::new(Database::open(&env.dir).unwrap())
        .space(now())
        .unwrap();
    assert_eq!(s.bytes(StorageKind::LogsTf), logs.cache_bytes());
    assert!(s.bytes(StorageKind::LogsTf) > 0);
    assert_eq!(
        s.unpacked(StorageKind::LogsTf),
        s.bytes(StorageKind::LogsTf)
    );
}

#[test]
fn store_data_is_the_database_files_less_the_logs_cache() {
    let env = library_env();
    seed_stored(&env.dir, 1, days_ago(2), now());
    let s = env.lib.storage().unwrap();
    let db = env.dir.db_path();
    let len = |suffix: &str| {
        std::fs::metadata(format!("{}{suffix}", db.display())).map_or(0, |m| m.len())
    };
    let files = len("") + len("-wal") + len("-shm");
    assert_eq!(
        s.bytes(StorageKind::StoreData),
        files - s.bytes(StorageKind::LogsTf)
    );
    let sum: u64 = StorageKind::ALL.iter().map(|k| s.bytes(*k)).sum();
    assert_eq!(s.total(), sum);
}

// Folder permissions that refuse a listing are a Unix feature.
#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_read_fails_the_measure() {
    use std::os::unix::fs::PermissionsExt as _;
    use tf2tools_core::StorageError;
    struct Unlock(std::path::PathBuf);
    impl Drop for Unlock {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
    let env = library_env();
    let locked = env.dir.library_dir(LibraryKind::Sounds);
    std::fs::create_dir_all(locked.join("x")).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let _unlock = Unlock(locked.clone());
    if std::fs::read_dir(&locked).is_ok() {
        eprintln!("skipped: this user can list a folder with no permissions");
        return;
    }
    let err = env.lib.storage().unwrap_err();
    assert!(matches!(err, StorageError::Io { .. }), "{err:?}");
}
