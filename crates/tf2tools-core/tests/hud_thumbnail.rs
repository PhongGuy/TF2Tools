//! HUD card pictures made from the HUD's own widescreen main-menu background.

use crate::common;

use std::path::PathBuf;

use common::hud_copy::{game_for, import_stock_hud};
use common::hud_stock::stock_files;
use common::{LibraryEnv, library_env, sql, write_file, zip_bytes};
use tf2tools_core::{
    HudEntry, LibraryKind, MakeTarget, Platform, Provenance, RgbaImage, read_picture, rgba_to_vtf,
};

const RED: [u8; 4] = [220, 20, 20, 255];
const BLUE: [u8; 4] = [20, 20, 220, 255];

/// A solid 2:1 VTF, the shape of a widescreen menu texture.
fn vtf(colour: [u8; 4]) -> Vec<u8> {
    let pixels = colour.repeat(64 * 32);
    rgba_to_vtf(&RgbaImage::from_pixels(64, 32, pixels).unwrap()).unwrap()
}

fn import(env: &LibraryEnv, folder: &str, files: &[(&str, Vec<u8>)]) -> HudEntry {
    let mut entries: Vec<(String, Vec<u8>)> =
        vec![(format!("{folder}/info.vdf"), b"\"hud\" {}".to_vec())];
    entries.extend(
        files
            .iter()
            .map(|(rel, bytes)| (format!("{folder}/{rel}"), bytes.clone())),
    );
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let archive = write_file(
        &env.tmp.path().join(format!("in/{folder}.zip")),
        &zip_bytes(&refs),
    );
    env.lib.import_hud(&archive, Provenance::Local).unwrap()
}

fn thumbnail_file(env: &LibraryEnv, hud: &HudEntry) -> PathBuf {
    env.dir
        .library_dir(LibraryKind::Huds)
        .join(hud.folder_name())
        .join(".tf2tools/thumbnail.jpg")
}

/// The thumbnail's size and its centre pixel's colour.
fn picture(path: &std::path::Path) -> ((u32, u32), [u8; 3]) {
    let image = read_picture(path).unwrap();
    let (w, h) = (image.width(), image.height());
    let at = ((h / 2 * w + w / 2) * 4) as usize;
    let p = &image.pixels()[at..at + 3];
    ((w, h), [p[0], p[1], p[2]])
}

fn reddish(rgb: [u8; 3]) -> bool {
    rgb[0] > 150 && rgb[1] < 80 && rgb[2] < 80
}

fn listed(env: &LibraryEnv, hud: &HudEntry) -> HudEntry {
    let huds = env.lib.list_huds().unwrap();
    huds.into_iter().find(|h| h.id() == hud.id()).unwrap()
}

#[test]
fn an_import_with_a_widescreen_background_gets_a_card_picture() {
    let env = library_env();
    let hud = import(
        &env,
        "budhud",
        &[(
            "materials/console/background_upward_widescreen.vtf",
            vtf(RED),
        )],
    );
    assert!(hud.has_thumbnail());
    assert!(listed(&env, &hud).has_thumbnail());
    let path = env.lib.hud_thumbnail(&hud).expect("a thumbnail path");
    assert_eq!(path, thumbnail_file(&env, &hud));
    let (size, centre) = picture(&path);
    assert_eq!(size, (480, 270));
    assert!(reddish(centre), "{centre:?}");
}

#[test]
fn an_import_without_a_background_keeps_the_plain_card() {
    let env = library_env();
    let hud = import(
        &env,
        "plain",
        &[("resource/ui/hudlayout.res", b"x".to_vec())],
    );
    assert!(!hud.has_thumbnail());
    assert!(!listed(&env, &hud).has_thumbnail());
    assert_eq!(env.lib.hud_thumbnail(&hud), None);
    assert!(!thumbnail_file(&env, &hud).exists());
}

#[test]
fn a_background_that_does_not_decode_keeps_the_plain_card() {
    let env = library_env();
    let hud = import(
        &env,
        "broken",
        &[(
            "materials/console/background_upward_widescreen.vtf",
            b"not a texture".to_vec(),
        )],
    );
    assert!(!hud.has_thumbnail());
    assert!(!thumbnail_file(&env, &hud).exists());
}

#[test]
fn the_chapter_list_picks_the_background_and_its_material_is_followed() {
    let env = library_env();
    let chapters = b"\"chapters\"\n{\n\t\"1\"\t\"background_zed\"\n}\n".to_vec();
    let vmt = b"\"UnlitGeneric\"\n{\n\t\"$basetexture\" \"vgui/replay/thumbnails/menu_wide\"\n}\n"
        .to_vec();
    let hud = import(
        &env,
        "chosen",
        &[
            ("scripts/chapterbackgrounds.txt", chapters),
            ("materials/console/background_aaa_widescreen.vtf", vtf(BLUE)),
            ("materials/console/background_zed_widescreen.vmt", vmt),
            ("materials/vgui/replay/thumbnails/menu_wide.vtf", vtf(RED)),
        ],
    );
    assert!(hud.has_thumbnail());
    let (_, centre) = picture(&thumbnail_file(&env, &hud));
    assert!(reddish(centre), "{centre:?}");
}

#[test]
fn adopting_a_hud_from_tf_custom_makes_its_picture() {
    let env = library_env();
    let root = env.tf2.custom_dir().join("manual");
    write_file(&root.join("info.vdf"), b"manual");
    write_file(
        &root.join("materials/console/background_2fort_widescreen.vtf"),
        &vtf(RED),
    );
    let found = env.lib.external_huds(&env.tf2).unwrap();
    let kept = env.lib.keep_external_huds(&found, &env.tf2).unwrap();
    assert!(kept[0].has_thumbnail());
    assert!(thumbnail_file(&env, &kept[0]).is_file());
}

#[test]
fn a_copy_and_its_saves_carry_the_picture() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    // The stock helper takes text, so the texture is written into the base afterwards.
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let base_dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(base.folder_name());
    write_file(
        &base_dir.join("materials/console/background_upward_widescreen.vtf"),
        &vtf(RED),
    );
    let copy = env.lib.copy_hud(base.id()).unwrap();
    assert!(copy.has_thumbnail());
    assert!(thumbnail_file(&env, &copy).is_file());

    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let saved = env
        .lib
        .save_made_hud(
            &MakeTarget::Existing(copy.id().clone()),
            copy.name(),
            &files,
            Platform::Linux,
            &[],
        )
        .unwrap();
    assert!(saved.entry().has_thumbnail());
    assert!(thumbnail_file(&env, saved.entry()).is_file());

    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let again = env
        .lib
        .save_made_hud(
            &MakeTarget::CopyOf(copy.id().clone()),
            "Mine 2",
            &files,
            Platform::Linux,
            &[],
        )
        .unwrap();
    assert!(again.entry().has_thumbnail());
    assert!(listed(&env, again.entry()).has_thumbnail());
}

#[test]
fn a_made_hud_without_a_background_has_no_picture() {
    let env = library_env();
    let saved = env
        .lib
        .save_made_hud(
            &MakeTarget::New,
            "Mine",
            &stock_files(),
            Platform::Linux,
            &[],
        )
        .unwrap();
    assert!(!saved.entry().has_thumbnail());
    assert!(!thumbnail_file(&env, saved.entry()).exists());
}

#[test]
fn an_existing_library_gets_missing_pictures_made_once() {
    let env = library_env();
    let pictured = import(
        &env,
        "older",
        &[(
            "materials/console/background_upward_widescreen.vtf",
            vtf(RED),
        )],
    );
    let plain = import(&env, "plainer", &[]);
    // As a library from before thumbnails: the flag clear and no file.
    std::fs::remove_file(thumbnail_file(&env, &pictured)).unwrap();
    sql(&env.dir)
        .execute("UPDATE library_huds SET has_thumbnail = 0", [])
        .unwrap();

    assert_eq!(env.lib.make_missing_hud_thumbnails().unwrap(), 1);
    assert!(listed(&env, &pictured).has_thumbnail());
    assert!(thumbnail_file(&env, &pictured).is_file());
    assert!(!listed(&env, &plain).has_thumbnail());

    // The pass runs once per library, so a HUD it found without a picture is not tried again.
    std::fs::remove_file(thumbnail_file(&env, &pictured)).unwrap();
    sql(&env.dir)
        .execute("UPDATE library_huds SET has_thumbnail = 0", [])
        .unwrap();
    assert_eq!(env.lib.make_missing_hud_thumbnails().unwrap(), 0);
    assert!(!listed(&env, &pictured).has_thumbnail());
}
