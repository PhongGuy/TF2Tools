use crate::common;

use common::hud_stock::stock_files;
use common::{library_env, tree, write_file};
use std::collections::BTreeMap;
use tf2tools_core::{
    AssetFile, Change, CreditNote, HudAssets, HudModel, LibraryKind, MakeTarget, MakerFile,
    PartKey, Platform, Recipe,
};

const IMG: &str = "materials/vgui/replay/thumbnails/badge.vtf";
const KEPT: &str = ".tf2tools/borrowed/materials/vgui/replay/thumbnails/badge.vtf";

fn credit(group: u32) -> Change {
    Change::credit(
        group,
        CreditNote::new("Mine HUD", "Mia", "GameBanana item 7", "HudPlayerHealth"),
    )
}

#[test]
fn a_credit_round_trips_through_maker_json() {
    let file = MakerFile::new(BTreeMap::new(), vec![credit(3)]);
    let (back, dropped) = MakerFile::parse(&file.to_json()).unwrap();
    assert_eq!((back, dropped), (file, 0));
}

#[test]
fn a_credit_applies_without_touching_a_file() {
    let model = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let edited = model.with_changes(&[credit(1)]);
    assert_eq!((edited.applied().len(), edited.dropped()), (1, 0));
    assert!(edited.touched().is_empty());
}

#[test]
fn a_save_writes_borrowed_images_and_the_credits_file_and_reopen_reads_them_back() {
    let env = library_env();
    let changes = vec![Change::asset(1, AssetFile::new(IMG, KEPT)), credit(1)];
    let mut assets = HudAssets::new();
    assets.insert(KEPT, b"VTFBYTES".to_vec());
    let saved = env
        .lib
        .save_made_hud_with_assets(
            &MakeTarget::New,
            "Borrower",
            &stock_files(),
            Platform::Linux,
            &changes,
            &assets,
        )
        .unwrap();
    assert_eq!(saved.dropped(), 0);
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name());
    let files = tree(&dir);
    for want in [IMG, KEPT, "TF2Tools-credits.txt", ".tf2tools/maker.json"] {
        assert!(
            files.iter().any(|f| f == want),
            "{want} missing from {files:?}"
        );
    }
    let credits = std::fs::read_to_string(dir.join("TF2Tools-credits.txt")).unwrap();
    assert!(
        credits.contains("Mia")
            && credits.contains("Mine HUD")
            && credits.contains("HudPlayerHealth")
    );
    let made = env
        .lib
        .open_made_hud(saved.entry().id(), &stock_files(), Platform::Linux)
        .unwrap();
    assert_eq!(made.dropped(), 0);
    assert_eq!(made.assets().get(KEPT), Some(b"VTFBYTES".as_slice()));
}

fn images(n: u32) -> (Vec<Change>, HudAssets) {
    let (mut changes, mut assets) = (Vec::new(), HudAssets::new());
    for i in 0..n {
        // Alternate material and texture: every borrowed file counts toward the 64.
        let ext = if i % 2 == 0 { "vtf" } else { "vmt" };
        let path = format!("materials/vgui/replay/thumbnails/i{i}.{ext}");
        let kept = format!(".tf2tools/borrowed/{path}");
        changes.push(Change::asset(i + 1, AssetFile::new(&path, &kept)));
        assets.insert(&kept, vec![0u8; 8]);
    }
    (changes, assets)
}

#[test]
fn the_sixty_fifth_image_file_and_an_oversized_one_are_dropped_on_save() {
    let env = library_env();
    let (changes, assets) = images(65);
    let saved = env
        .lib
        .save_made_hud_with_assets(
            &MakeTarget::New,
            "Caps",
            &stock_files(),
            Platform::Linux,
            &changes,
            &assets,
        )
        .unwrap();
    assert_eq!(saved.dropped(), 1, "only the 65th file");
    let (mut one, mut bytes) = images(0);
    let big = ".tf2tools/borrowed/materials/vgui/replay/thumbnails/big.vtf";
    one.push(Change::asset(
        1,
        AssetFile::new("materials/vgui/replay/thumbnails/big.vtf", big),
    ));
    bytes.insert(big, vec![0u8; 16 * 1024 * 1024 + 1]);
    let saved = env
        .lib
        .save_made_hud_with_assets(
            &MakeTarget::New,
            "Big",
            &stock_files(),
            Platform::Linux,
            &one,
            &bytes,
        )
        .unwrap();
    assert_eq!(saved.dropped(), 1);
}

#[test]
fn a_maker_json_over_the_image_caps_opens_without_the_extra_pictures() {
    let env = library_env();
    let (changes, assets) = images(64);
    let saved = env
        .lib
        .save_made_hud_with_assets(
            &MakeTarget::New,
            "Edge",
            &stock_files(),
            Platform::Linux,
            &changes,
            &assets,
        )
        .unwrap();
    assert_eq!(saved.dropped(), 0, "64 files is exactly the cap");
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name());
    // A hand edit adds a 65th file and its kept copy.
    let extra = "materials/vgui/replay/thumbnails/extra.vtf";
    write_file(&dir.join(format!(".tf2tools/borrowed/{extra}")), b"x");
    let maker = dir.join(".tf2tools/maker.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&maker).unwrap()).unwrap();
    json["changes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "group": 99,
            "asset": { "path": extra, "source": format!(".tf2tools/borrowed/{extra}") }
        }));
    std::fs::write(&maker, json.to_string()).unwrap();
    let made = env
        .lib
        .open_made_hud(saved.entry().id(), &stock_files(), Platform::Linux)
        .unwrap();
    assert_eq!(made.pictures_left_out(), 1);
    assert!(
        made.notice()
            .is_some_and(|n| n.contains("borrowed pictures"))
    );
    assert_eq!(made.dropped(), 1);
    assert_eq!(made.changes().len(), 64);
}

#[test]
fn a_credit_with_no_author_names_the_hud_only() {
    let env = library_env();
    let changes = vec![Change::credit(
        1,
        CreditNote::new("Mine HUD", "", "a file you imported", "HudPlayerHealth"),
    )];
    let saved = env
        .lib
        .save_made_hud_with_assets(
            &MakeTarget::New,
            "Anon",
            &stock_files(),
            Platform::Linux,
            &changes,
            &HudAssets::new(),
        )
        .unwrap();
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name());
    let credits = std::fs::read_to_string(dir.join("TF2Tools-credits.txt")).unwrap();
    assert!(
        credits.contains("Mine HUD") && !credits.contains(" by "),
        "{credits}"
    );
}

#[test]
fn a_recipe_leaves_a_borrowed_part_with_pictures_out_with_its_credit() {
    let mut assets = HudAssets::new();
    assets.insert(KEPT, b"VTFBYTES".to_vec());
    let changes = vec![Change::asset(1, AssetFile::new(IMG, KEPT)), credit(1)];
    let recipe = Recipe::new("Shared", &changes, &assets);
    assert_eq!(recipe.fonts_left_out(), 1);
    assert!(
        recipe.changes().is_empty(),
        "no credit for a part the recipe does not hold"
    );
    assert_eq!(recipe.parts_left_out_for_pictures(), 1);
}

#[test]
fn a_recipe_keeps_the_credit_of_a_borrowed_part_without_pictures() {
    let key = PartKey::new("HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue");
    let model = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let mut changes = model.recolour_part(&key, "255 0 0 255", 2).unwrap();
    changes.push(credit(2));
    let recipe = Recipe::new("Shared", &changes, &HudAssets::new());
    assert_eq!(recipe.changes().len(), changes.len());
    assert_eq!(recipe.parts_left_out_for_pictures(), 0);
}

#[test]
fn a_borrowed_part_with_a_picture_and_a_font_carries_no_orphan_bytes() {
    let font = ".tf2tools/fonts/a.ttf";
    let mut assets = HudAssets::new();
    assets.insert(KEPT, b"VTFBYTES".to_vec());
    assets.insert(font, b"FONTBYTES".to_vec());
    let changes = vec![
        Change::asset(1, AssetFile::new(IMG, KEPT)),
        Change::asset(1, AssetFile::new("resource/fonts/tf2tools/a.ttf", font)),
        credit(1),
    ];
    let recipe = Recipe::new("Shared", &changes, &assets);
    assert!(recipe.changes().is_empty());
    assert_eq!(
        recipe.asset(font),
        None,
        "the font's bytes left with its part"
    );
    assert_eq!(recipe.fonts_left_out(), 2, "the picture and the font");
    assert_eq!(recipe.pictures_left_out(), 1);
    assert_eq!(recipe.parts_left_out_for_pictures(), 1);
}
