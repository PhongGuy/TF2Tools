//! The low ammo image: the game grows it past the ammo background as ammo runs out.

use crate::common;

use common::hud_stock::stock_files_with;
use tf2tools_core::{GameSituation, HudModel, PartKey, Platform, Screen};

const HD: Screen = Screen::new(1920, 1080);
const LOW: &str = "HudWeaponAmmo/HudWeaponLowAmmoImage";
const AMMO_RES: &str = r#""Resource/UI/HudAmmoWeapons.res"
{
  HudWeaponAmmoBG { ControlName CTFImagePanel xpos 4 ypos 0 zpos 1 wide 90 tall 45 image ../hud/ammo_blue_bg teambg_2 ../hud/ammo_red_bg }
  HudWeaponLowAmmoImage { ControlName ImagePanel xpos 4 ypos 0 zpos 0 wide 90 tall 45 visible 0 image ../hud/ammo_red_bg }
}
"#;

fn low_box(s: &GameSituation) -> (i32, i32, i32, i32) {
    let m = HudModel::load(
        &stock_files_with(&[("resource/ui/hudammoweapons.res", Some(AMMO_RES))]),
        Platform::Linux,
    )
    .unwrap();
    let b = m.layout_in(HD, s).get(&PartKey::new(LOW)).cloned().unwrap();
    (b.x(), b.y(), b.wide(), b.tall())
}

#[test]
fn the_low_ammo_image_grows_past_the_background_as_ammo_runs_out() {
    // SizeLowAmmoIndicator: grow = round((threshold - ammo) / threshold * 5) px on each side.
    let s = GameSituation::default();
    let base = low_box(&s);
    let grown = |p: u8, by: i32| {
        let r = low_box(&s.clone().with_ammo_percent(p));
        let (x, y, w, t) = base;
        assert_eq!(r, (x - by, y - by, w + 2 * by, t + 2 * by), "at {p}%");
    };
    grown(0, 5);
    grown(20, 2);
    grown(30, 1);
    grown(39, 0);
}
