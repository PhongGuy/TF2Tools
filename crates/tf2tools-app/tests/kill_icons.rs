//! The game's killfeed icons for logs.tf weapon names, cut from their sheets once per TF2 folder.

use crate::support;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::kill_icons::LoadedKillIcons;
use tf2tools_app::services::{Tf2State, install_tf2_state};
use tf2tools_core::{RgbaImage, Tf2Install, rgba_to_vtf};

const TABLE: &str = r#""sprites/640_hud" { TextureFileRefs { dfile { prefix d_ } dnegfile { prefix dneg_ } }
  TextureData {
    Scattergun { dfile HUD/d_images dnegfile HUD/dneg_images x 0 y 0 width 6 height 2 }
    tf_projectile_rocket { dfile HUD/d_images x 2 y 2 width 4 height 2 }
    lost_sheet { dfile HUD/d_missing x 0 y 0 width 4 height 2 }
    off_sheet { dfile HUD/d_images x 6 y 0 width 4 height 2 }
    plus_sign { font Icons character + }
  } }"#;

fn load(cx: &mut TestAppContext, env: &support::Env) -> Entity<LoadedKillIcons> {
    support::init(cx);
    support::install_services(cx, support::services(env, support::no_steam()));
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    cx.update(|cx| install_tf2_state(cx, found));
    let icons = cx.update(LoadedKillIcons::global);
    support::wait_real(cx, "the icons", |cx| {
        cx.update(|cx| icons.read(cx).loads() == 1 && !icons.read(cx).loading())
    });
    icons
}

fn size_of(
    cx: &mut TestAppContext,
    icons: &Entity<LoadedKillIcons>,
    name: &str,
) -> Option<(i32, i32)> {
    cx.update(|cx| {
        icons.read(cx).icons().get(name).map(|i| {
            let s = i.size(0);
            (s.width.0, s.height.0)
        })
    })
}

#[gpui_kit::test]
fn each_log_name_gets_its_d_icon_cut_from_the_games_sheet(cx: &mut TestAppContext) {
    let env = support::env();
    let sheet = RgbaImage::from_pixels(8, 4, [235, 226, 202, 255].repeat(32)).unwrap();
    let vtf = rgba_to_vtf(&sheet).unwrap();
    let vmt: &[u8] = br#""UnlitGeneric" { "$baseTexture" "HUD/d_images" }"#;
    support::seed_stock_game_with_bytes(
        &env.game,
        &[("scripts/mod_textures.txt", Some(TABLE))],
        &[
            ("materials/hud/d_images.vmt", vmt),
            ("materials/hud/d_images.vtf", &vtf),
        ],
    );
    let icons = load(cx, &env);
    assert_eq!(
        size_of(cx, &icons, "scattergun"),
        Some((6, 2)),
        "keys ignore case"
    );
    assert_eq!(size_of(cx, &icons, "SCATTERGUN"), Some((6, 2)));
    assert_eq!(size_of(cx, &icons, "tf_projectile_rocket"), Some((4, 2)));
    assert_eq!(size_of(cx, &icons, "lost_sheet"), None, "a missing sheet");
    assert_eq!(
        size_of(cx, &icons, "off_sheet"),
        None,
        "a region off its sheet"
    );
    assert_eq!(size_of(cx, &icons, "plus_sign"), None, "a font glyph");
    assert_eq!(size_of(cx, &icons, "world"), None);
    assert_eq!(
        cx.update(|cx| icons.read(cx).icons().len()),
        2,
        "no dneg_ entries"
    );
}

#[gpui_kit::test]
fn a_game_without_icon_tables_has_no_icons(cx: &mut TestAppContext) {
    let env = support::env();
    let icons = load(cx, &env);
    assert!(cx.update(|cx| icons.read(cx).icons().is_empty()));
}
