//! A scheme font's height on screen: a `yres`-limited block keeps its `tall`, any other
//! scales with the screen; heights are capped at 127.

use tf2tools_core::{HudFiles, Platform, Scheme};

const SCHEME: &str = r#""Scheme" { "Fonts" {
  "Default" {
    "1" { "name" "Verdana" "tall" "12" "yres" "480 599" }
    "2" { "name" "Verdana" "tall" "20" "yres" "1024 1199" }
  }
  "Health" { "1" { "name" "TF2" "tall" "16" } }
  "Huge" { "1" { "name" "TF2" "tall" "60" } }
  "Negative" { "1" { "name" "TF2" "tall" "-5" } }
  "NegativeLimited" { "1" { "name" "TF2" "tall" "-5" "yres" "1024 1199" } }
} }"#;

fn scheme() -> Scheme {
    let files = HudFiles::from_memory(vec![vec![(
        "resource/clientscheme.res".to_owned(),
        SCHEME.as_bytes().to_vec(),
    )]]);
    Scheme::load(&files, Platform::Linux).unwrap()
}

fn px(name: &str, screen_tall: u32) -> Option<i32> {
    scheme()
        .font(name, screen_tall)
        .map(|f| f.pixel_tall(screen_tall))
}

#[test]
fn a_yres_block_keeps_its_tall() {
    assert_eq!(px("Default", 1080), Some(20));
    assert_eq!(px("Default", 480), Some(12));
    assert_eq!(px("Default", 700), None, "no block covers 700");
}

#[test]
fn a_block_without_yres_scales_with_the_screen() {
    assert_eq!(px("Health", 1080), Some(36));
    assert_eq!(px("Health", 480), Some(16));
}

#[test]
fn no_font_is_taller_than_127_pixels() {
    assert_eq!(px("Huge", 1080), Some(127));
    assert_eq!(px("Huge", 480), Some(60));
}

#[test]
fn no_font_is_shorter_than_zero_pixels() {
    assert_eq!(px("Negative", 1080), Some(0));
    assert_eq!(px("NegativeLimited", 1080), Some(0));
}
