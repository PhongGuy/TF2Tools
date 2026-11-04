mod common;

use std::path::Path;
use tf2tools_core::{DataDir, LibraryKind};

#[test]
fn at_lays_out_v5_paths_under_root() {
    let dir = DataDir::at("/data/com.tf2tools.app");
    assert_eq!(dir.root(), Path::new("/data/com.tf2tools.app"));
    assert_eq!(
        dir.db_path(),
        Path::new("/data/com.tf2tools.app/tf2tools.db")
    );
    assert_eq!(dir.tmp_dir(), Path::new("/data/com.tf2tools.app/tmp"));
    assert_eq!(
        dir.library_dir(LibraryKind::Huds),
        Path::new("/data/com.tf2tools.app/library/huds")
    );
    assert_eq!(
        dir.library_dir(LibraryKind::Sounds),
        Path::new("/data/com.tf2tools.app/library/sounds")
    );
    assert_eq!(
        dir.library_dir(LibraryKind::Crosshairs),
        Path::new("/data/com.tf2tools.app/library/crosshairs")
    );
    assert_eq!(
        dir.library_dir(LibraryKind::Addons),
        Path::new("/data/com.tf2tools.app/library/addons")
    );
}

#[test]
fn library_kind_all_lists_each_kind_once() {
    assert_eq!(
        LibraryKind::ALL,
        [
            LibraryKind::Huds,
            LibraryKind::Sounds,
            LibraryKind::Crosshairs,
            LibraryKind::Addons
        ]
    );
}

#[test]
fn platform_default_is_the_tauri_app_data_dir() {
    let expected = dirs::data_dir()
        .expect("platform data dir")
        .join("com.tf2tools.app");
    let dir = DataDir::platform_default().expect("platform default");
    assert_eq!(dir.root(), expected);
}

#[test]
fn at_does_not_touch_the_filesystem() {
    let (_tmp, dir) = common::temp_data_dir();
    assert!(!dir.root().exists());
}

#[test]
fn at_keeps_the_image_cache_under_root() {
    let dir = DataDir::at("/data/com.tf2tools.app");
    assert_eq!(
        dir.image_cache_dir(),
        Path::new("/data/com.tf2tools.app/cache/images")
    );
}

#[test]
fn platform_default_keeps_images_in_the_user_cache_dir() {
    let expected = dirs::cache_dir()
        .expect("platform cache dir")
        .join("tf2tools/images");
    assert_eq!(
        DataDir::platform_default().unwrap().image_cache_dir(),
        expected
    );
}

#[test]
fn with_cache_home_puts_images_where_platform_default_does() {
    let dir = DataDir::at("/run/data/com.tf2tools.app").with_cache_home("/run/cache");
    assert_eq!(dir.root(), Path::new("/run/data/com.tf2tools.app"));
    assert_eq!(
        dir.image_cache_dir(),
        Path::new("/run/cache/tf2tools/images")
    );
}
