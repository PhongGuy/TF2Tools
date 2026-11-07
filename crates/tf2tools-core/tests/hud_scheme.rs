//! The merged client scheme: colours, aliases, font variants and font files.

use crate::common;

use tf2tools_core::{HudFiles, Platform, Rgba, Scheme};

const SCHEME: &str = r#"#base "clientscheme_colors.res"
#base "../../outside.res"
"Scheme"
{
    "Colors"
    {
        "TanLight"      "235 226 202 255"
        "NoAlpha"       "10 20 30"
        "Broken"        "TanLight"
    }
    "BaseSettings"
    {
        "Panel.FgColor"     "TanLight"
        "Chain.A"           "Chain.B"
        "Chain.B"           "Overridden"
        "Loop.A"            "Loop.B"
        "Loop.B"            "Loop.A"
    }
    "Fonts"
    {
        "HudFontGiant"
        {
            "1" { "name" "TF2" "tall" "24" "weight" "500" "yres" "1 599" }
            "2" { "name" "TF2" "tall" "30" "weight" "500" "yres" "600 1199" "outline" "1" }
            "3" { "name" "TF2 Win" "tall" "40" "yres" "1200 10000" } [$WINDOWS]
            "4" { "name" "TF2 Big" "tall" "44" "antialias" "1" "dropshadow" "1" }
        }
    }
    "CustomFontFiles"
    {
        "1" "resource/tf.ttf"
        "2" { "font" "resource/tf2.ttf" "name" "TF2" }
    }
}"#;

const COLORS: &str = r#""Scheme" { "Colors" { "TanLight" "1 1 1 1" "Overridden" "200 100 50 255" "BaseOnly" "0 255 0 255" } }"#;

fn scheme(platform: Platform) -> Scheme {
    let files = HudFiles::from_memory(vec![
        vec![(
            "resource/clientscheme.res".to_owned(),
            SCHEME.as_bytes().to_vec(),
        )],
        vec![(
            "Resource/ClientScheme_Colors.res".to_owned(),
            COLORS.as_bytes().to_vec(),
        )],
    ]);
    Scheme::load(&files, platform).unwrap()
}

#[test]
fn named_colours_merge_bases_and_follow_base_settings() {
    let s = scheme(Platform::Linux);
    assert_eq!(
        s.named_colour("tanlight"),
        Some(Rgba::new(235, 226, 202, 255)),
        "own beats base"
    );
    assert_eq!(s.named_colour("BaseOnly"), Some(Rgba::new(0, 255, 0, 255)));
    assert_eq!(s.named_colour("NoAlpha"), Some(Rgba::new(10, 20, 30, 0)));
    assert_eq!(
        s.named_colour("Broken"),
        None,
        "Colors values are not followed"
    );
    assert_eq!(
        s.named_colour("Panel.FgColor"),
        Some(Rgba::new(235, 226, 202, 255))
    );
    assert_eq!(
        s.named_colour("Chain.A"),
        Some(Rgba::new(200, 100, 50, 255))
    );
    assert_eq!(s.named_colour("Loop.A"), None);
    assert_eq!(
        s.colour_names(),
        ["BaseOnly", "Broken", "NoAlpha", "Overridden", "TanLight"]
    );
}

#[test]
fn element_colours_take_literals_or_names_and_fall_back_to_white() {
    let s = scheme(Platform::Linux);
    assert_eq!(s.colour("255 0 0 128"), Rgba::new(255, 0, 0, 128));
    assert_eq!(s.colour("255 0 0"), Rgba::new(255, 0, 0, 0));
    assert_eq!(s.colour("12.9 0 0 255"), Rgba::new(12, 0, 0, 255));
    assert_eq!(s.colour(".5 1 2 3"), Rgba::new(0, 1, 2, 3));
    assert_eq!(s.colour("Chain.A"), Rgba::new(200, 100, 50, 255));
    assert_eq!(s.colour("Unknown"), Rgba::WHITE);
    assert_eq!(s.colour("Loop.A"), Rgba::WHITE);
    assert_eq!(Rgba::WHITE.to_array(), [255, 255, 255, 255]);
}

#[test]
fn fonts_pick_the_variant_for_the_screen_height() {
    let linux = scheme(Platform::Linux);
    let at = |s: &Scheme, tall| {
        s.font("hudfontgiant", tall)
            .map(|v| (v.family().to_owned(), v.tall()))
    };
    assert_eq!(at(&linux, 480), Some(("TF2".to_owned(), 24)));
    assert_eq!(at(&linux, 1080), Some(("TF2".to_owned(), 30)));
    assert_eq!(at(&linux, 1440), Some(("TF2 Big".to_owned(), 44)));
    assert_eq!(
        at(&scheme(Platform::Windows), 1440),
        Some(("TF2 Win".to_owned(), 40))
    );
    let v = linux.font("HudFontGiant", 1080).unwrap();
    assert_eq!(
        (v.weight(), v.yres(), v.outline(), v.dropshadow()),
        (500, Some((600, 1199)), true, false)
    );
    let big = linux.font("HudFontGiant", 1440).unwrap();
    assert!(big.antialias() && big.dropshadow() && !big.additive() && !big.italic());
    assert_eq!(big.blur(), 0);
    assert!(linux.font("Missing", 1080).is_none());
    assert_eq!(linux.font_files(), ["resource/tf.ttf", "resource/tf2.ttf"]);
}

#[test]
fn a_hud_without_a_scheme_has_white_and_no_fonts() {
    let s = Scheme::load(&HudFiles::from_memory(vec![]), Platform::Linux).unwrap();
    assert_eq!(s.colour("TanLight"), Rgba::WHITE);
    assert!(s.font("HudFontGiant", 1080).is_none());
    assert!(s.font_files().is_empty());
}

#[test]
fn font_yres_reads_like_sscanf_d() {
    let text = r#""Scheme" { "Fonts" { "F" {
        "1" { "name" "Junk" "yres" "+600 1199x" }
        "2" { "name" "Neg" "yres" "-5 9" }
        "3" { "name" "Any" }
    } } }"#;
    let files = HudFiles::from_memory(vec![vec![(
        "resource/clientscheme.res".to_owned(),
        text.as_bytes().to_vec(),
    )]]);
    let s = Scheme::load(&files, Platform::Linux).unwrap();
    let v = s.font("F", 1080).unwrap();
    assert_eq!((v.family(), v.yres()), ("Junk", Some((600, 1199))));
    // A negative bound clamps to 0, which no screen height falls below.
    let neg = s.font("F", 5).unwrap();
    assert_eq!((neg.family(), neg.yres()), ("Neg", Some((0, 9))));
    assert_eq!(s.font("F", 20).unwrap().family(), "Any");
}

#[test]
fn a_scheme_start_over_the_kv_cap_is_empty_without_being_read() {
    // The VPK holds 3 bytes, so reading either claimed length would fail.
    for claimed in [9 * 1024 * 1024, 65 * 1024 * 1024] {
        let vpk = common::vpk_claiming("resource/clientscheme.res", claimed);
        let (_dir, tf) = common::game_with_vpk(&vpk);
        let s = Scheme::load(&HudFiles::open_game(&tf).unwrap(), Platform::Linux).unwrap();
        assert!(s.font_files().is_empty(), "{claimed}");
    }
}

#[test]
fn a_scalable_image_border_reads_its_image_and_corners() {
    let text = r#""Scheme" { "Borders" {
        TFThinLineBorder { "bordertype" "scalable_image" "backgroundtype" "2" "image" "../hud/tournament_panel_brown"
            "src_corner_height" "23" "src_corner_width" "23" "draw_corner_width" "8" "draw_corner_height" "8" }
        Plain { "inset" "0 0 0 0" }
        NoImage { "bordertype" "scalable_image" "image" "" }
        Wrong { "bordertype" "image" "image" "../hud/x" } } }"#;
    let files = HudFiles::from_memory(vec![vec![(
        "resource/clientscheme.res".to_owned(),
        text.as_bytes().to_vec(),
    )]]);
    let scheme = Scheme::load(&files, Platform::Linux).unwrap();
    let border = scheme.image_border("tfthinlineborder").unwrap();
    assert_eq!(
        (border.image(), border.src_corner(), border.draw_corner()),
        ("../hud/tournament_panel_brown", (23, 23), (8, 8))
    );
    for name in ["Plain", "NoImage", "Wrong", "Missing"] {
        assert!(scheme.image_border(name).is_none(), "{name}");
    }
}
