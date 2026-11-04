//! Picked-picture fixtures: content-style ids, kept sources and a save helper.

use super::LibraryEnv;
use super::hud_stock::stock_files;
use tf2tools_core::{AssetFile, Change, HudAssets, LibraryKind, MakeTarget, Platform, SavedHud};

pub const CODE_VMT: &str = "materials/hud/health_color.vmt";

pub fn id(n: u32) -> String {
    format!("{n:016x}")
}

pub fn kept(n: u32, path: &str) -> String {
    format!(".tf2tools/images/{}/{path}", id(n))
}

pub fn tex(n: u32) -> String {
    format!("materials/vgui/replay/thumbnails/tf2tools/{}.vtf", id(n))
}

/// Pick `n` in `group`: its texture and a material at `vmt`, bytes held in `assets`.
pub fn pick(n: u32, group: u32, vmt: &str, assets: &mut HudAssets) -> Vec<Change> {
    assets.insert(&kept(n, &tex(n)), vec![n as u8; 100]);
    assets.insert(&kept(n, vmt), format!("vmt {n}").into_bytes());
    vec![
        Change::asset(group, AssetFile::new(&tex(n), &kept(n, &tex(n)))),
        Change::asset(group, AssetFile::new(vmt, &kept(n, vmt))),
    ]
}

pub fn save(
    env: &LibraryEnv,
    target: &MakeTarget,
    changes: &[Change],
    assets: &HudAssets,
) -> SavedHud {
    env.lib
        .save_made_hud_with_assets(
            target,
            "Pics",
            &stock_files(),
            Platform::Linux,
            changes,
            assets,
        )
        .unwrap()
}

pub fn folder(env: &LibraryEnv, saved: &SavedHud) -> std::path::PathBuf {
    env.dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name())
}
