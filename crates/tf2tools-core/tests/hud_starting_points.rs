//! The bundled starting points, built from the edit builders.

mod common;

use common::hud_stock::{AMMO, HEALTH, HUDLAYOUT, KILLFEED, stock_files, stock_files_with};
use tf2tools_core::{ChangeTarget, HudModel, PartKey, Platform, Screen, StartingPoint};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const FONTS: &str = r#""Scheme" { "Colors" { "TanDark" "117 107 94 255" "TanLight" "235 226 202 255" }
  "Fonts" { "HudFontMedium" { "1" { "name" "TF2 Build" "tall" "16" } } } }"#;
const BASECHAT: &str = r#""Resource/UI/BaseChat.res" { HudChat { xpos 10 ypos 275 } }"#;
const MAINMENU: &str = r#""Resource/UI/MainMenuOverride.res" { FriendsContainer { visible 1 } }"#;

/// The stock files plus option targets and a font on both numbers; the stock HUDLAYOUT
/// already holds the killfeed block.
fn full_model() -> HudModel {
    let health = HEALTH.replace("font HudClassHealth", "font HudFontMedium");
    let ammo = r#""Resource/UI/HudAmmoWeapons.res" { AmmoInClip { ControlName CExLabel xpos 10 ypos 5 wide 55 tall 40 font HudFontMedium labelText %Ammo% } }"#;
    let files = stock_files_with(&[
        ("resource/clientscheme.res", Some(FONTS)),
        ("resource/ui/hudplayerhealth.res", Some(health.as_str())),
        ("resource/ui/hudammoweapons.res", Some(ammo)),
        ("scripts/hudlayout.res", Some(HUDLAYOUT)),
        ("resource/ui/basechat.res", Some(BASECHAT)),
        ("resource/ui/mainmenuoverride.res", Some(MAINMENU)),
    ]);
    HudModel::load(&files, Platform::Linux).unwrap()
}

fn value(m: &HudModel, part: &str, key: &str) -> Option<String> {
    m.part(&PartKey::new(part))?
        .value(key)
        .map(|l| l.text().to_owned())
}

#[test]
fn every_starting_point_applies_cleanly_to_the_default_hud() {
    let m = full_model();
    for which in StartingPoint::ALL {
        let recipe = m.starting_point(which);
        assert_eq!(recipe.name(), which.label());
        assert!(!recipe.changes().is_empty(), "{which:?}");
        assert!(recipe.changes().iter().all(|c| c.group() == 0));
        let applied = m.apply_recipe(&recipe, &[], 0);
        assert_eq!(applied.skipped(), 0, "{which:?}");
        assert_eq!(m.with_changes(applied.changes()).dropped(), 0, "{which:?}");
    }
    let labels: Vec<&str> = StartingPoint::ALL.iter().map(|s| s.label()).collect();
    assert_eq!(
        labels,
        [
            "Competitive minimal",
            "Big numbers",
            "Streamer clean",
            "Centre HUD"
        ]
    );
}

#[test]
fn the_stock_fixture_takes_every_starting_point_whole() {
    // The plain fixture holds the chat and main menu files and fonts on both numbers.
    let m = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    for which in StartingPoint::ALL {
        let recipe = m.starting_point(which);
        let applied = m.apply_recipe(&recipe, &[], 0);
        assert_eq!(applied.skipped(), 0, "{which:?}");
        // The builders' change counts when no step is refused.
        let expected = match which {
            StartingPoint::BigNumbers => 10,
            StartingPoint::CentreHud => 6,
            _ => 2,
        };
        assert_eq!(recipe.changes().len(), expected, "{which:?}");
    }
}

#[test]
fn big_numbers_makes_both_numbers_half_again_as_big() {
    let m = full_model();
    let recipe = m.starting_point(StartingPoint::BigNumbers);
    let edited = m.with_changes(recipe.changes()).into_model();
    for part in [HV, "HudWeaponAmmo/AmmoInClip"] {
        let font = value(&edited, part, "font").unwrap();
        assert_eq!(
            edited.scheme().font(&font, 1080).unwrap().tall(),
            24,
            "{part}"
        );
    }
}

#[test]
fn big_numbers_grows_each_box_with_its_font() {
    let m = full_model();
    let screen = Screen::new(1920, 1080);
    let before = m.layout(screen);
    let edited = m
        .with_changes(m.starting_point(StartingPoint::BigNumbers).changes())
        .into_model();
    let after = edited.layout(screen);
    for part in [HV, "HudWeaponAmmo/AmmoInClip"] {
        let (old, new) = (
            before.get(&PartKey::new(part)).unwrap(),
            after.get(&PartKey::new(part)).unwrap(),
        );
        // 16 to 24 is half again; whole HUD units round each side by a pixel or two.
        for (was, now) in [(old.wide(), new.wide()), (old.tall(), new.tall())] {
            assert!((now - was * 3 / 2).abs() <= 2, "{part}: {was} to {now}");
        }
    }
}

#[test]
fn competitive_minimal_hides_the_class_and_keeps_three_rows() {
    let m = full_model();
    let edited = m.with_changes(
        m.starting_point(StartingPoint::CompetitiveMinimal)
            .changes(),
    );
    let model = edited.model();
    let class = model.layout(Screen::new(1920, 1080));
    assert!(
        class
            .get(&PartKey::new("HudPlayerStatus/HudPlayerClass"))
            .unwrap()
            .hidden()
            .is_some()
    );
    assert_eq!(
        value(model, "HudDeathNotice", "MaxDeathNotices").as_deref(),
        Some("3")
    );
}

#[test]
fn centre_hud_writes_centre_anchored_positions() {
    let m = full_model();
    let edited = m
        .with_changes(m.starting_point(StartingPoint::CentreHud).changes())
        .into_model();
    for (part, key) in [
        ("HudWeaponAmmo", "xpos"),
        ("HudWeaponAmmo", "ypos"),
        ("HudPlayerStatus/HudPlayerHealth", "xpos"),
        ("HudPlayerStatus/HudPlayerHealth", "ypos"),
    ] {
        let text = value(&edited, part, key).unwrap();
        assert!(text.starts_with('c'), "{part} {key}: {text}");
    }
}

/// The y at 1920x1080 a value's text sits on: its box's middle plus half its font.
fn baseline(m: &HudModel, part: &str) -> i32 {
    let b = m.layout(Screen::new(1920, 1080));
    let b = b.get(&PartKey::new(part)).unwrap();
    let font = value(m, part, "font").unwrap();
    let tall = m.scheme().font(&font, 1080).unwrap().tall();
    b.y() + b.tall() / 2 + tall * 9 / 8
}

#[test]
fn centre_hud_puts_both_values_on_one_line_beside_the_crosshair() {
    let m = full_model();
    let edited = m
        .with_changes(m.starting_point(StartingPoint::CentreHud).changes())
        .into_model();
    let layout = edited.layout(Screen::new(1920, 1080));
    let health = layout.get(&PartKey::new(HV)).unwrap();
    let ammo = layout
        .get(&PartKey::new("HudWeaponAmmo/AmmoInClip"))
        .unwrap();
    let (h, a) = (
        baseline(&edited, HV),
        baseline(&edited, "HudWeaponAmmo/AmmoInClip"),
    );
    assert!((h - a).abs() <= 3, "health on {h}, ammo on {a}");
    // Beside the crosshair: each within 70 px of the middle, the line just under it.
    let health_right = health.x() + health.wide();
    assert!(
        (890..960).contains(&health_right),
        "health ends at {health_right}"
    );
    assert!(
        (961..1030).contains(&ammo.x()),
        "ammo starts at {}",
        ammo.x()
    );
    assert!((540..640).contains(&h), "the line is at {h}");
}

#[test]
fn centre_hud_lines_up_values_in_different_fonts() {
    // The stock fixture draws health in a 16 tall font and ammo in a 33 tall one.
    let m = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let ammo = "HudWeaponAmmo/AmmoInClip";
    let tall = |part: &str| {
        let font = value(&m, part, "font").unwrap();
        m.scheme().font(&font, 480).unwrap().tall()
    };
    assert_eq!((tall(HV), tall(ammo)), (16, 33));
    let edited = m
        .with_changes(m.starting_point(StartingPoint::CentreHud).changes())
        .into_model();
    let (h, a) = (baseline(&edited, HV), baseline(&edited, ammo));
    assert!((h - a).abs() <= 3, "health on {h}, ammo on {a}");
}

#[test]
fn a_starting_point_leaves_out_what_the_hud_refuses() {
    // The stock files without the killfeed have no option targets and no fonts on the numbers.
    let layout = HUDLAYOUT.replace(KILLFEED, "");
    let health = HEALTH.replace("    font HudClassHealth\n", "");
    let ammo = AMMO.replace(" font HudFontGiant", "");
    let files = stock_files_with(&[
        ("scripts/hudlayout.res", Some(layout.as_str())),
        ("resource/ui/basechat.res", None),
        ("resource/ui/mainmenuoverride.res", None),
        ("resource/ui/hudplayerhealth.res", Some(health.as_str())),
        ("resource/ui/hudammoweapons.res", Some(ammo.as_str())),
    ]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    assert!(
        m.starting_point(StartingPoint::StreamerClean)
            .changes()
            .is_empty()
    );
    assert!(
        m.starting_point(StartingPoint::BigNumbers)
            .changes()
            .is_empty()
    );
    let minimal = m.starting_point(StartingPoint::CompetitiveMinimal);
    assert!(
        minimal
            .changes()
            .iter()
            .all(|c| !matches!(c.target(), ChangeTarget::Option(_)))
    );
    assert!(
        !minimal.changes().is_empty(),
        "the class portrait still hides"
    );
}
