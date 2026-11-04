//! The main menu's pictures: the background the chapter list names and the class picture the
//! character list names, both as image values `HudFiles::image` resolves.

use tf2tools_core::{HudFiles, Platform, Screen};

const CHAPTERS: &str = r#""chapters"
{
1	"background_upward"
2	"background_2fort"
}
"BackgroundMaps"
{
1	"background_gravelpit"
}
"#;

const CHARACTERS: &str = r#""characters"
{
"halloween_scout" { "image" "../console/characters/scout_halloween" "holiday_restriction" "halloween_or_fullmoon" }
"unused_spy" { "image" "../console/characters/unused_spy" "weight" "0" }
"basic_demoman" { "image" "../console/characters/demoman" "item_url" "demoman" }
"war_soldier" { "image" "../console/characters/war_soldier" "war_restriction" "heavy_vs_pyro" }
}
"#;

fn files(entries: &[(&str, &str)]) -> HudFiles {
    HudFiles::from_memory(vec![
        entries
            .iter()
            .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
            .collect(),
    ])
}

const WIDE_VMT: (&str, &str) = (
    "materials/console/background_upward_widescreen.vmt",
    r#""UnlitGeneric" { "$baseTexture" "console/background_upward_widescreen" }"#,
);

#[test]
fn the_background_is_the_first_chapter_wide_when_the_screen_is() {
    let f = files(&[("scripts/chapterbackgrounds.txt", CHAPTERS), WIDE_VMT]);
    let bg = |wide| f.menu_background(wide, Platform::Linux).unwrap();
    assert_eq!(bg(false).as_deref(), Some("../console/background_upward"));
    assert_eq!(
        bg(true).as_deref(),
        Some("../console/background_upward_widescreen")
    );
    // Without a wide material the plain one is used on every screen.
    let f = files(&[("scripts/chapterbackgrounds.txt", CHAPTERS)]);
    assert_eq!(
        f.menu_background(true, Platform::Linux).unwrap().as_deref(),
        Some("../console/background_upward")
    );
}

#[test]
fn no_chapter_list_means_no_background() {
    assert_eq!(
        files(&[]).menu_background(true, Platform::Linux).unwrap(),
        None
    );
    let empty = files(&[("scripts/chapterbackgrounds.txt", "\"chapters\" { }")]);
    assert_eq!(empty.menu_background(false, Platform::Linux).unwrap(), None);
    // A list under another root name still names the chapters.
    let other = files(&[(
        "scripts/chapterbackgrounds.txt",
        "\"Chapters\" { 1 \"background_mvm\" }",
    )]);
    assert_eq!(
        other
            .menu_background(false, Platform::Linux)
            .unwrap()
            .as_deref(),
        Some("../console/background_mvm")
    );
}

#[test]
fn the_class_picture_is_the_first_one_shown_outside_holidays() {
    let f = files(&[("scripts/characterbackgrounds.txt", CHARACTERS)]);
    assert_eq!(
        f.menu_character(Platform::Linux).unwrap().as_deref(),
        Some("../console/characters/demoman")
    );
    assert_eq!(files(&[]).menu_character(Platform::Linux).unwrap(), None);
    let all_restricted =
        r#""characters" { "a" { "image" "x" "holiday_restriction" "christmas" } }"#;
    let f = files(&[("scripts/characterbackgrounds.txt", all_restricted)]);
    assert_eq!(f.menu_character(Platform::Linux).unwrap(), None);
}

// Each entry the game never shows outside holidays is passed over for the next one; the weight
// is read as the game's whole-number read takes it, so text or a fraction under 1 is 0.
#[test]
fn every_skip_rule_passes_over_its_entry() {
    let first = |entry: &str| {
        let list = format!(r#""characters" {{ {entry} "last" {{ "image" "kept" }} }}"#);
        let f = files(&[("scripts/characterbackgrounds.txt", list.as_str())]);
        f.menu_character(Platform::Linux).unwrap()
    };
    let skipped = [
        ("empty entry", r#""a" { }"#),
        ("empty image", r#""a" { "image" "  " }"#),
        ("bad weight", r#""a" { "image" "x" "weight" "heavy" }"#),
        ("fraction weight", r#""a" { "image" "x" "weight" "0.5" }"#),
        ("negative weight", r#""a" { "image" "x" "weight" "-2" }"#),
        ("zero weight", r#""a" { "image" "x" "weight" "0" }"#),
        (
            "holiday",
            r#""a" { "image" "x" "holiday_restriction" "christmas" }"#,
        ),
    ];
    for (rule, entry) in skipped {
        assert_eq!(first(entry).as_deref(), Some("kept"), "{rule}");
    }
    // A weight with a number in front counts by that number, as the game reads it.
    for weight in ["3", " 2", "5 heavy", "1.5"] {
        let entry = format!(r#""a" {{ "image" "x" "weight" "{weight}" }}"#);
        assert_eq!(first(&entry).as_deref(), Some("x"), "weight {weight:?}");
    }
}

#[test]
fn a_screen_is_wider_from_sixteen_by_ten() {
    let wider: Vec<bool> = Screen::PRESETS.iter().map(|s| s.is_wider()).collect();
    // 1920x1080, 1920x1200, 1440x1080, 1280x1024, 2560x1080, 3840x1080.
    assert_eq!(wider, [true, true, false, false, true, true]);
}
