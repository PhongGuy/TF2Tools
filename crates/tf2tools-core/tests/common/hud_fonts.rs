//! A HUD modelled on the font survey, shared by the scan, check and install tests.

use std::path::{Path, PathBuf};

use super::sfnt::sfnt_bytes;
use super::{vpk_bytes, write_file};

pub const SCHEME: &str = r#"
#base "scheme/Fonts_Extra.res"
#base "../../../cfg/myhud/hud_font.txt"
"Scheme"
{
    "Fonts"
    {
        "Main"  { "1" { "name" "Surface" "tall" "12" } }
        "Alt"   { "1" { "name" "surface" } }
        "Code"  { "1" { "name" "code-pro-bold" } }
        "Stock" { "1" { "name" "TF2 Build" } }
        "Sys"   { "1" { "name" "Verdana" } }
        "Sym"   { "1" { "name" "Marlett" } }
        "Pak"   { "1" { "name" "Packed" } }
        "Evil"  { "1" { "name" "Evil" } }
        "Win"   { "1" { "name" "WinFont" [$WINDOWS] } }
    }
    "CustomFontFiles"
    {
        "1" "resource/fonts/surface_semiBold.ttf"
        "2" { "font" "resource/fonts/Code-Pro.otf" "name" "Code Pro" }
        "3" "resource/tf2build.ttf"
        "4" "resource/linux_fonts/tf2.ttf"
        "5" "resource/fonts/packed.ttf"
        "6" "resource/fonts/marlett.ttf"
        "7" "resource/fonts/winfont.ttf"
        "8" "resource\fonts\evil.ttf"
    }
}
"#;

pub const EXTRA: &str = r#"
"Scheme"
{
    "Fonts" { "E" { "1" { "name" "Extra Sans" } } }
    "CustomFontFiles"
    {
        "9" "resource/fonts/extra.ttf"
        "10" "resource/fonts/unused.ttf"
        "11" "resource/fonts/broken.ttf"
    }
}
"#;

pub const OUTSIDE_BASE: &str = r#""Scheme" { "Fonts" { "O" { "1" { "name" "Outside" } } } "CustomFontFiles" { "12" "resource/fonts/outside.ttf" } }"#;

/// Builds `<root>/custom/myhud` plus the `<root>/cfg` file its scheme `#base`s, and on Unix
/// a `resource/fonts/evil.ttf` symlink to `<root>/evil.ttf`. Returns the HUD directory.
pub fn fixture_hud(root: &Path) -> PathBuf {
    let hud = root.join("custom/myhud");
    let put = |rel: &str, bytes: &[u8]| {
        write_file(&hud.join(rel), bytes);
    };
    put("resource/clientscheme.res", SCHEME.as_bytes());
    put("resource/scheme/fonts_extra.res", EXTRA.as_bytes());
    write_file(
        &root.join("cfg/myhud/hud_font.txt"),
        OUTSIDE_BASE.as_bytes(),
    );
    put(
        "resource/fonts/surface_semibold.ttf",
        &sfnt_bytes(&[(1, "Surface"), (4, "Surface SemiBold")], false),
    );
    put(
        "resource/fonts/Code-Pro.otf",
        &sfnt_bytes(&[(1, "Code Pro"), (4, "code-pro-bold")], true),
    );
    put(
        "resource/fonts/marlett.ttf",
        &sfnt_bytes(&[(1, "Marlett"), (4, "Marlett")], false),
    );
    put(
        "resource/fonts/winfont.ttf",
        &sfnt_bytes(&[(1, "WinFont"), (4, "WinFont Regular")], false),
    );
    put(
        "resource/fonts/extra.ttf",
        &sfnt_bytes(
            &[
                (1, "Extra Sans Text"),
                (16, "Extra Sans"),
                (4, "Extra Sans Text Regular"),
            ],
            false,
        ),
    );
    put(
        "resource/fonts/unused.ttf",
        &sfnt_bytes(&[(1, "Unused")], false),
    );
    put("resource/fonts/broken.ttf", b"not a font");
    put(
        "resource/fonts/outside.ttf",
        &sfnt_bytes(&[(1, "Outside")], false),
    );
    let packed = sfnt_bytes(&[(1, "Packed")], false);
    put(
        "resource/pak_dir.vpk",
        &vpk_bytes(&[("resource/fonts/packed.ttf", &packed)]),
    );
    #[cfg(unix)]
    {
        let target = write_file(&root.join("evil.ttf"), &sfnt_bytes(&[(1, "Evil")], false));
        std::os::unix::fs::symlink(target, hud.join("resource/fonts/evil.ttf")).unwrap();
    }
    hud
}
