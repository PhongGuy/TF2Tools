//! Pictures the player picks: kept by content, saved, capped and reopened.

use crate::common;

use common::hud_pictures::{CODE_VMT, folder, kept, pick, save, tex};
use common::hud_stock::stock_files;
use common::{library_env, tree};
use tf2tools_core::{AssetFile, Change, HudAssets, MakeTarget, Platform, pictures_fit};

#[test]
fn a_pick_saves_at_its_game_paths_and_its_kept_copies() {
    let env = library_env();
    let mut assets = HudAssets::new();
    let changes = pick(1, 1, CODE_VMT, &mut assets);
    let saved = save(&env, &MakeTarget::New, &changes, &assets);
    assert_eq!(saved.dropped(), 0);
    let files = tree(&folder(&env, &saved));
    for path in [
        tex(1),
        CODE_VMT.to_owned(),
        kept(1, &tex(1)),
        kept(1, CODE_VMT),
    ] {
        assert!(files.contains(&path), "{path} in {files:?}");
    }
}

#[test]
fn an_overridden_pick_keeps_its_copy_through_reopen_and_resave() {
    let env = library_env();
    let mut assets = HudAssets::new();
    let mut changes = pick(1, 1, CODE_VMT, &mut assets);
    changes.extend(pick(2, 2, CODE_VMT, &mut assets));
    let saved = save(&env, &MakeTarget::New, &changes, &assets);
    let dir = folder(&env, &saved);
    assert_eq!(std::fs::read(dir.join(CODE_VMT)).unwrap(), b"vmt 2");
    assert_eq!(
        std::fs::read(dir.join(kept(1, CODE_VMT))).unwrap(),
        b"vmt 1"
    );
    let made = env
        .lib
        .open_made_hud(saved.entry().id(), &stock_files(), Platform::Linux)
        .unwrap();
    assert_eq!(made.dropped(), 0);
    assert_eq!(made.changes().len(), 4);
    assert_eq!(
        made.assets().get(&kept(1, CODE_VMT)),
        Some(b"vmt 1".as_slice())
    );
    let target = MakeTarget::Existing(saved.entry().id().clone());
    assert_eq!(
        save(&env, &target, made.changes(), made.assets()).dropped(),
        0
    );
}

#[test]
fn only_picture_paths_under_a_content_id_are_kept() {
    let env = library_env();
    let mut assets = HudAssets::new();
    let menu = "scripts/chapterbackgrounds.txt";
    assets.insert(&kept(3, menu), b"chapters".to_vec());
    assets.insert(".tf2tools/images/12/materials/a.vtf", vec![1]);
    assets.insert(&kept(4, "scripts/other.txt"), vec![1]);
    let changes = vec![
        Change::asset(1, AssetFile::new(menu, &kept(3, menu))),
        Change::asset(
            2,
            AssetFile::new("materials/a.vtf", ".tf2tools/images/12/materials/a.vtf"),
        ),
        Change::asset(
            3,
            AssetFile::new("scripts/other.txt", &kept(4, "scripts/other.txt")),
        ),
    ];
    let saved = save(&env, &MakeTarget::New, &changes, &assets);
    assert_eq!(saved.dropped(), 2);
    let files = tree(&folder(&env, &saved));
    assert!(files.contains(&menu.to_owned()) && files.contains(&kept(3, menu)));
    assert!(!files.contains(&"materials/a.vtf".to_owned()));
    assert!(!files.contains(&"scripts/other.txt".to_owned()));
}

#[test]
fn picked_and_borrowed_pictures_share_the_caps() {
    let mut assets = HudAssets::new();
    let mut changes = Vec::new();
    for n in 1..=32 {
        changes.extend(pick(n, n, &format!("materials/vgui/m{n}.vmt"), &mut assets));
    }
    assert!(pictures_fit(&changes, &assets), "64 files fit");
    let borrowed = ".tf2tools/borrowed/materials/vgui/b.vmt";
    assets.insert(borrowed, vec![1]);
    let mut more = changes.clone();
    more.push(Change::asset(
        99,
        AssetFile::new("materials/vgui/b.vmt", borrowed),
    ));
    assert!(!pictures_fit(&more, &assets), "a 65th file does not");
    let mut big = HudAssets::new();
    big.insert(&kept(1, &tex(1)), vec![0; 16 * 1024 * 1024 + 1]);
    let one = vec![Change::asset(1, AssetFile::new(&tex(1), &kept(1, &tex(1))))];
    assert!(!pictures_fit(&one, &big), "a file over 16 MiB does not");
}
