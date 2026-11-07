//! A made HUD's added font files: written on save, read back on reopen.

use crate::common;

use common::hud_stock::stock_files_with;
use common::{LibraryEnv, library_env, tree};
use tf2tools_core::{
    Change, ChangeTarget, HudAssets, HudFiles, HudModel, LibraryKind, MakeTarget, Platform,
    SavedHud,
};

const SCHEME: &str = r#""Scheme" { "Colors" { "TanDark" "117 107 94 255" } }"#;
const FONT: &str = "resource/fonts/tf2tools/big_numbers.ttf";
const KEPT: &str = ".tf2tools/fonts/big_numbers.ttf";

fn files() -> HudFiles {
    stock_files_with(&[("resource/clientscheme.res", Some(SCHEME))])
}

fn font_bytes() -> Vec<u8> {
    common::sfnt::sfnt_bytes(&[(1, "Big Numbers")], false)
}

fn add_font() -> (Vec<Change>, HudAssets) {
    let m = HudModel::load(&files(), Platform::Linux).unwrap();
    let (file, changes) = m
        .add_font_file("Big Numbers.ttf", &font_bytes(), 1)
        .unwrap();
    let mut assets = HudAssets::new();
    assets.insert(file.source(), font_bytes());
    (changes, assets)
}

fn save(env: &LibraryEnv, target: &MakeTarget, changes: &[Change], assets: &HudAssets) -> SavedHud {
    env.lib
        .save_made_hud_with_assets(target, "Fonty", &files(), Platform::Linux, changes, assets)
        .unwrap()
}

fn folder(env: &LibraryEnv, saved: &SavedHud) -> std::path::PathBuf {
    env.dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name())
}

#[test]
fn a_save_writes_the_font_and_its_kept_copy() {
    let env = library_env();
    let (changes, assets) = add_font();
    let saved = save(&env, &MakeTarget::New, &changes, &assets);
    assert_eq!(saved.dropped(), 0);
    let dir = folder(&env, &saved);
    assert_eq!(
        tree(&dir),
        [
            KEPT,
            ".tf2tools/maker.json",
            "info.vdf",
            "resource/clientscheme.res",
            FONT
        ]
    );
    assert_eq!(std::fs::read(dir.join(FONT)).unwrap(), font_bytes());
    assert_eq!(std::fs::read(dir.join(KEPT)).unwrap(), font_bytes());
}

#[test]
fn an_asset_without_bytes_is_left_out_and_counted() {
    let env = library_env();
    let (changes, _) = add_font();
    let saved = save(&env, &MakeTarget::New, &changes, &HudAssets::new());
    assert_eq!(
        saved.dropped(),
        2,
        "the asset and its CustomFontFiles entry"
    );
    let dir = folder(&env, &saved);
    assert!(!dir.join(FONT).exists() && !dir.join(KEPT).exists());
    assert!(
        !dir.join("resource/clientscheme.res").exists(),
        "nothing else in the group was written"
    );
    let maker = std::fs::read_to_string(dir.join(".tf2tools/maker.json")).unwrap();
    assert!(!maker.contains("\"asset\""));
    assert!(!maker.contains("CustomFontFiles"));
}

#[test]
fn reopen_reads_the_kept_copy_and_a_resave_keeps_it() {
    let env = library_env();
    let (changes, assets) = add_font();
    let saved = save(&env, &MakeTarget::New, &changes, &assets);
    let made = env
        .lib
        .open_made_hud(saved.entry().id(), &files(), Platform::Linux)
        .unwrap();
    assert_eq!(made.dropped(), 0);
    assert_eq!(made.assets().get(KEPT), Some(font_bytes().as_slice()));
    assert!(
        made.changes()
            .iter()
            .any(|c| matches!(c.target(), ChangeTarget::Asset(_)))
    );
    let target = MakeTarget::Existing(saved.entry().id().clone());
    let again = save(&env, &target, made.changes(), made.assets());
    assert_eq!(again.dropped(), 0);
    assert_eq!(
        std::fs::read(folder(&env, &again).join(FONT)).unwrap(),
        font_bytes()
    );
}

#[test]
fn a_missing_kept_copy_counts_on_reopen() {
    let env = library_env();
    let (changes, assets) = add_font();
    let saved = save(&env, &MakeTarget::New, &changes, &assets);
    std::fs::remove_file(folder(&env, &saved).join(KEPT)).unwrap();
    let made = env
        .lib
        .open_made_hud(saved.entry().id(), &files(), Platform::Linux)
        .unwrap();
    assert_eq!(made.dropped(), 2);
    assert!(made.assets().is_empty());
    assert!(made.changes().is_empty(), "the group went with its file");
}

#[test]
fn a_resave_with_a_new_face_under_the_same_name_keeps_the_new_bytes() {
    let env = library_env();
    let (changes, assets) = add_font();
    let saved = save(&env, &MakeTarget::New, &changes, &assets);

    // The player reset the font and picked a different file with the same name.
    let other = common::sfnt::sfnt_bytes(&[(1, "Other Numbers")], false);
    let m = HudModel::load(&files(), Platform::Linux).unwrap();
    let (file, changes) = m.add_font_file("Big Numbers.ttf", &other, 1).unwrap();
    let mut assets = HudAssets::new();
    assets.insert(file.source(), other.clone());
    let target = MakeTarget::Existing(saved.entry().id().clone());
    let again = save(&env, &target, &changes, &assets);

    let dir = folder(&env, &again);
    assert_eq!(std::fs::read(dir.join(FONT)).unwrap(), other);
    assert_eq!(std::fs::read(dir.join(KEPT)).unwrap(), other);
}
