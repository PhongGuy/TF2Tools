//! Layout at each screen shape: proportional units, alignment, pins, hiding and hit-testing.

mod common;

use common::hud_stock::{
    HEALTH, HUDLAYOUT, block, label, stock_files, stock_files_with, with_blocks,
};
use tf2tools_core::{HiddenBy, HudFiles, HudLayout, HudModel, PartKey, Pick, Platform, Screen};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn layout(files: &HudFiles, screen: Screen) -> HudLayout {
    HudModel::load(files, Platform::Linux)
        .unwrap()
        .layout(screen)
}

fn hd(files: &HudFiles) -> HudLayout {
    layout(files, Screen::new(1920, 1080))
}

fn rect(l: &HudLayout, k: &str) -> (i32, i32, i32, i32) {
    let b = l.get(&PartKey::new(k)).unwrap_or_else(|| panic!("{k}"));
    (b.x(), b.y(), b.wide(), b.tall())
}

fn health_with(blocks: &str) -> HudFiles {
    stock_files_with(&[(
        "resource/ui/hudplayerhealth.res",
        Some(with_blocks(HEALTH, blocks).as_str()),
    )])
}

#[test]
fn the_six_screen_shapes() {
    let shapes: Vec<(u32, u32)> = Screen::PRESETS
        .iter()
        .map(|s| (s.wide(), s.tall()))
        .collect();
    assert_eq!(
        shapes,
        [
            (1920, 1080),
            (1920, 1200),
            (1440, 1080),
            (1280, 1024),
            (2560, 1080),
            (3840, 1080)
        ]
    );
}

#[test]
fn parts_lay_out_in_whole_pixels_at_1080p() {
    let l = hd(&stock_files());
    assert_eq!(rect(&l, "HudPlayerStatus"), (0, 0, 1920, 1080));
    assert_eq!(
        rect(&l, "HudPlayerStatus/HudPlayerHealth"),
        (0, 810, 562, 270),
        "ypos r120 gives 810"
    );
    assert_eq!(rect(&l, HV), (171, 927, 99, 40));
    assert_eq!(rect(&l, "HudWeaponAmmo"), (1707, 957, 211, 101));
    assert_eq!(rect(&l, "HudWeaponAmmo/AmmoInClip"), (1729, 968, 123, 90));
    assert_eq!(rect(&l, "HudItemEffectMeter"), (1529, 941, 225, 112));
    assert_eq!(
        rect(&l, "CMainTargetID"),
        (676, 798, 567, 78),
        "centred by code"
    );
}

#[test]
fn active_if_blocks_and_own_size_terms() {
    let l = hd(&stock_files());
    assert_eq!(
        rect(&l, "HudMatchStatus/ObjectiveStatusTimePanel"),
        (814, 0, 292, 67)
    );
    assert_eq!(
        rect(&l, "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue"),
        (892, 4, 135, 45)
    );
}

#[test]
fn proportionaltoparent_aligns_to_the_parent() {
    let l = hd(&stock_files());
    assert_eq!(
        rect(&l, "HudPlayerStatus/HudPlayerClass/classmodelholder"),
        (0, 630, 225, 450)
    );
    assert_eq!(
        rect(
            &l,
            "HudPlayerStatus/HudPlayerClass/classmodelholder/classmodelpanel"
        ),
        (22, 698, 180, 337)
    );
}

#[test]
fn other_screen_shapes() {
    let files = stock_files();
    assert_eq!(
        rect(
            &layout(&files, Screen::new(1280, 1024)),
            "HudPlayerStatus/HudPlayerHealth"
        )
        .1,
        768
    );
    assert_eq!(
        rect(&layout(&files, Screen::new(2560, 1080)), "HudWeaponAmmo").0,
        2347
    );
    assert_eq!(
        rect(&layout(&files, Screen::new(1440, 1080)), "CMainTargetID").0,
        436
    );
}

#[test]
fn pins_follow_the_sibling_corner_pending_probe() {
    fn pin<'a>(own: &'a str, sib: &'a str, to: &'a str) -> [(&'static str, &'a str); 3] {
        [
            ("pin_to_sibling", to),
            ("pin_corner_to_sibling", own),
            ("pin_to_sibling_corner", sib),
        ]
    }
    let blocks = [
        label(
            "PinA",
            "4",
            "-2",
            &pin("PIN_TOPLEFT", "PIN_TOPRIGHT", "PlayerStatusHealthValue"),
        ),
        label("PinB", "4", "-2", &pin("3", "6", "PlayerStatusHealthValue")),
        label("PinC", "0", "0", &[("pin_to_sibling", "PinA")]),
        label("Lost", "4", "-2", &[("pin_to_sibling", "Nope")]),
        label("LoopA", "4", "4", &[("pin_to_sibling", "LoopB")]),
        label("LoopB", "4", "4", &[("pin_to_sibling", "LoopA")]),
    ]
    .concat();
    let l = hd(&health_with(&blocks));
    let at = |k: &str| {
        let b = l
            .get(&PartKey::new(format!(
                "HudPlayerStatus/HudPlayerHealth/{k}"
            )))
            .unwrap();
        (b.x(), b.y(), b.pinned())
    };
    assert_eq!(at("PinA"), (279, 923, true));
    assert_eq!(at("PinB"), (184, 941, true));
    assert_eq!(at("PinC"), (279, 923, true));
    assert_eq!(at("Lost"), (9, 806, false));
    assert_eq!(at("LoopA"), (9, 819, false));
    assert_eq!(at("LoopB"), (9, 819, false));
}

#[test]
fn hidden_parts_and_what_hides_them() {
    let gone = "  Gone\n  {\n    ControlName EditablePanel\n    visible 0\n    Inner { ControlName CExLabel }\n  }\n";
    let blocks = [
        gone.to_owned(),
        label("Faded", "0", "0", &[("alpha", "0")]),
        block("Flat", "CExLabel", &[("wide", "0"), ("tall", "10")]),
        label("Away", "2000", "0", &[]),
        label("Disabled", "0", "0", &[("enabled", "0")]),
    ]
    .concat();
    let l = hd(&health_with(&blocks));
    let hidden = |k: &str| {
        l.get(&PartKey::new(format!(
            "HudPlayerStatus/HudPlayerHealth/{k}"
        )))
        .unwrap()
        .hidden()
    };
    assert_eq!(hidden("Gone"), Some(HiddenBy::Visible));
    assert_eq!(hidden("Gone/Inner"), Some(HiddenBy::Parent));
    assert_eq!(hidden("Faded"), Some(HiddenBy::Alpha));
    assert_eq!(hidden("Flat"), Some(HiddenBy::SizeZero));
    assert_eq!(hidden("Away"), Some(HiddenBy::OffScreen));
    assert_eq!(
        hidden("Disabled"),
        None,
        "enabled 0 does not hide (pending probe)"
    );
    let stock = hd(&stock_files());
    assert_eq!(
        stock.get(&PartKey::new("CMainTargetID")).unwrap().hidden(),
        Some(HiddenBy::Game),
        "the game hides the target ID while you aim at no one"
    );
    let unseen = HUDLAYOUT.replace("HudWeaponAmmo { visible 1", "HudWeaponAmmo { visible 0");
    let ammo = hd(&stock_files_with(&[(
        "scripts/hudlayout.res",
        Some(&unseen),
    )]));
    assert_eq!(
        ammo.get(&PartKey::new("HudWeaponAmmo")).unwrap().hidden(),
        None,
        "element visible is reset"
    );
    assert_eq!(
        stock
            .get(&PartKey::new("HudMatchStatus/ObjectiveStatusTimePanel"))
            .unwrap()
            .hidden(),
        None,
        "code shows the timer"
    );
}

#[test]
fn hit_testing_picks_the_deepest_topmost_clickable_part() {
    let l = hd(&stock_files());
    let hit = |x, y| l.hit(x, y).map(|k| k.as_str().to_owned());
    assert_eq!(
        hit(180, 940).as_deref(),
        Some(HV),
        "zpos 5 above the image's 4"
    );
    assert_eq!(
        hit(170, 890).as_deref(),
        Some("HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage")
    );
    assert_eq!(
        hit(900, 10).as_deref(),
        Some("HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue")
    );
    assert_eq!(
        hit(5, 5),
        None,
        "full-screen containers are not click-selectable"
    );
    for k in [
        "HudPlayerStatus",
        "HudPlayerStatus/HudPlayerClass",
        "CDamageAccountPanel",
        "HudMatchStatus",
    ] {
        assert!(!l.get(&PartKey::new(k)).unwrap().clickable(), "{k}");
    }
    assert!(l.get(&PartKey::new(HV)).unwrap().clickable());
}

const HH: &str = "HudPlayerStatus/HudPlayerHealth";

/// The parts under (x, y) best first, every part shown, a part drawing when its text is not empty.
fn pick_text(files: &HudFiles, x: i32, y: i32) -> Vec<String> {
    let model = HudModel::load(files, Platform::Linux).unwrap();
    let l = model.layout(Screen::new(1920, 1080));
    let draws = |k: &PartKey| {
        model
            .part(k)
            .and_then(|p| p.value("labelText"))
            .is_some_and(|t| !t.text().is_empty())
    };
    l.pick(x, y, |_| true, draws)
        .into_iter()
        .map(|k| k.as_str().to_owned())
        .collect()
}

#[test]
fn a_click_picks_a_drawn_label_before_an_invisible_container_over_it() {
    let cover = block(
        "Cover",
        "EditablePanel",
        &[
            ("xpos", "70"),
            ("ypos", "50"),
            ("zpos", "9"),
            ("wide", "60"),
            ("tall", "30"),
        ],
    );
    // Right of the class model, which also reaches the health value.
    let picked = pick_text(&health_with(&cover), 240, 940);
    assert_eq!(
        picked,
        [
            HV.to_owned(),
            format!("{HH}/Cover"),
            format!("{HH}/PlayerStatusHealthImage"),
            HH.to_owned(),
        ],
        "drawn first; then what draws nothing, smallest first; never a full-screen panel"
    );
}

#[test]
fn of_two_drawn_parts_the_one_painted_on_top_comes_first() {
    let blocks = [
        label("Low", "76", "52", &[("zpos", "6"), ("labelText", "a")]),
        label("High", "78", "54", &[("zpos", "7"), ("labelText", "b")]),
        label("Late", "78", "54", &[("zpos", "6"), ("labelText", "c")]),
    ]
    .concat();
    let picked = pick_text(&health_with(&blocks), 180, 940);
    assert_eq!(
        picked[..4],
        [
            format!("{HH}/High"),
            format!("{HH}/Low"),
            format!("{HH}/Late"),
            HV.to_owned()
        ],
        "higher zpos first, then the earlier of equal zpos"
    );
}

#[test]
fn a_label_with_empty_text_is_picked_after_drawn_parts() {
    let blank = label("Blank", "76", "52", &[("zpos", "9"), ("labelText", "\"\"")]);
    let picked = pick_text(&health_with(&blank), 180, 940);
    assert_eq!(picked[0], HV);
    assert_eq!(picked[1], format!("{HH}/Blank"));
}

#[test]
fn a_pick_skips_parts_not_shown() {
    let model = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let l = model.layout(Screen::new(1920, 1080));
    let shown = |b: &tf2tools_core::PartBox| b.key().as_str() != HV;
    let picked = l.pick(180, 940, shown, |_| true);
    assert!(!picked.iter().any(|k| k.as_str() == HV), "{picked:?}");
    assert!(l.pick(5, 5, |_| true, |_| true).is_empty());
}

#[test]
fn extreme_values_saturate_instead_of_overflowing() {
    // Values past i32 evaluate to i32::MAX or i32::MIN; layout must still never fail.
    let (far, near) = ("99999999999", "-99999999999");
    let blocks = [
        label(
            "Far",
            far,
            far,
            &[("pin_to_sibling", "PlayerStatusHealthValue")],
        ),
        block(
            "Huge",
            "CExLabel",
            &[
                ("xpos", near),
                ("ypos", near),
                ("wide", near),
                ("tall", near),
            ],
        ),
    ]
    .concat();
    let mut files = vec![(
        "resource/ui/hudplayerhealth.res",
        Some(with_blocks(HEALTH, &blocks)),
    )];
    files.push((
        "scripts/hudlayout.res",
        Some(HUDLAYOUT.replace("wide 252", &format!("wide {near}"))),
    ));
    let files: Vec<(&str, Option<&str>)> = files.iter().map(|(p, t)| (*p, t.as_deref())).collect();
    let l = hd(&stock_files_with(&files));
    let part = |k: &str| {
        l.get(&PartKey::new(format!(
            "HudPlayerStatus/HudPlayerHealth/{k}"
        )))
        .unwrap()
    };
    let pinned = part("Far");
    assert_eq!((pinned.x(), pinned.y()), (i32::MAX, i32::MAX));
    assert_eq!(pinned.hidden(), Some(HiddenBy::OffScreen));
    assert_eq!(part("Huge").hidden(), Some(HiddenBy::SizeZero));
    let target = l.get(&PartKey::new("CMainTargetID")).unwrap();
    assert_eq!(target.x(), i32::MAX / 2, "(1920 - i32::MIN) / 2 saturates");
    // The game's hide is reported before the zero size; Huge covers a size-zero box.
    assert_eq!(target.hidden(), Some(HiddenBy::Game));
}

#[test]
fn every_new_layout_has_its_own_generation() {
    let model = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let a = model.layout(Screen::new(1920, 1080));
    let b = model.layout(Screen::new(1920, 1080));
    assert_ne!(a.generation(), b.generation());
    assert_eq!(a.clone().generation(), a.generation());
    let order: Vec<&str> = a.paint_order().map(|b| b.key().as_str()).collect();
    let at = |k: &str| order.iter().position(|o| *o == k).unwrap();
    assert!(
        at("HudPlayerStatus/HudPlayerHealth") < at(HV),
        "parents paint first"
    );
    assert!(
        at("HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage") < at(HV),
        "zpos 4 under zpos 5"
    );
}

/// The stock ammo panel's background and reserve count, trimmed to their 640x480 geometry.
const STOCK_RESERVE: &str = r#""Resource/UI/HudAmmoWeapons.res"
{
  HudWeaponAmmoBG { ControlName CTFImagePanel xpos 4 ypos 0 zpos 1 wide 90 tall 45 image ../hud/ammo_blue_bg }
  AmmoInReserve { ControlName CExLabel font HudFontMediumSmall xpos 59 ypos 8 zpos 7 wide 40 tall 27 textAlignment south-west labelText %AmmoInReserve% }
}"#;

#[test]
fn a_box_past_its_parent_shows_only_inside_it() {
    let files = stock_files_with(&[("resource/ui/hudammoweapons.res", Some(STOCK_RESERVE))]);
    let l = hd(&files);
    let k = |s: &str| PartKey::new(s);
    let right = |r: (i32, i32, i32, i32)| r.0 + r.2;
    let parent = rect(&l, "HudWeaponAmmo");
    let bg = rect(&l, "HudWeaponAmmo/HudWeaponAmmoBG");
    let reserve = rect(&l, "HudWeaponAmmo/AmmoInReserve");
    // The file's box runs 5 units past the 94-unit panel, and so past the background's edge.
    assert!(right(reserve) > right(parent), "{reserve:?} in {parent:?}");
    let shown = l.visible(&k("HudWeaponAmmo/AmmoInReserve")).unwrap();
    assert_eq!(right(shown), right(parent), "the panel cuts it off");
    assert_eq!(right(shown), right(bg), "so it ends with the background");
    assert_eq!(
        (shown.0, shown.1, shown.3),
        (reserve.0, reserve.1, reserve.3)
    );
    assert_eq!(l.visible(&k("HudWeaponAmmo/HudWeaponAmmoBG")), Some(bg));
    assert_eq!(l.visible(&k("Missing")), None);
}

#[test]
fn a_box_wholly_outside_its_parent_shows_whole() {
    let off = r#""Resource/UI/HudAmmoWeapons.res" { Far { ControlName CExLabel xpos 120 ypos 0 wide 20 tall 10 labelText 1 } }"#;
    let l = hd(&stock_files_with(&[(
        "resource/ui/hudammoweapons.res",
        Some(off),
    )]));
    let far = rect(&l, "HudWeaponAmmo/Far");
    assert!(far.0 > rect(&l, "HudWeaponAmmo").0 + rect(&l, "HudWeaponAmmo").2);
    // Nothing of it would show, so the selection keeps its whole box rather than vanish.
    assert_eq!(l.visible(&PartKey::new("HudWeaponAmmo/Far")), Some(far));
}

#[test]
fn a_click_past_the_parent_misses_the_part_it_cuts_off() {
    let files = stock_files_with(&[("resource/ui/hudammoweapons.res", Some(STOCK_RESERVE))]);
    let reserve = "HudWeaponAmmo/AmmoInReserve".to_owned();
    // The reserve's box spans x 1839..1929, its panel ends at 1918.
    assert!(pick_text(&files, 1900, 1000).contains(&reserve));
    assert!(!pick_text(&files, 1919, 1000).contains(&reserve));
}

#[test]
fn shown_is_the_cut_box_and_none_when_nothing_would_show() {
    let reserve_files =
        stock_files_with(&[("resource/ui/hudammoweapons.res", Some(STOCK_RESERVE))]);
    let l = hd(&reserve_files);
    let reserve = PartKey::new("HudWeaponAmmo/AmmoInReserve");
    assert_eq!(l.shown(&reserve), l.visible(&reserve));
    assert_eq!(l.shown(&PartKey::new("Missing")), None);
    let off = r#""Resource/UI/HudAmmoWeapons.res" { Far { ControlName CExLabel xpos 120 ypos 0 wide 20 tall 10 labelText 1 } }"#;
    let l = hd(&stock_files_with(&[(
        "resource/ui/hudammoweapons.res",
        Some(off),
    )]));
    assert_eq!(l.shown(&PartKey::new("HudWeaponAmmo/Far")), None);
}

#[test]
fn a_part_its_parent_cuts_off_whole_takes_no_click() {
    let off = r#""Resource/UI/HudAmmoWeapons.res" { Far { ControlName CExLabel xpos 120 ypos 0 wide 20 tall 10 labelText 1 } }"#;
    let files = stock_files_with(&[("resource/ui/hudammoweapons.res", Some(off))]);
    let far = rect(&hd(&files), "HudWeaponAmmo/Far");
    let picked = pick_text(&files, far.0 + 1, far.1 + 1);
    assert!(
        !picked.contains(&"HudWeaponAmmo/Far".to_owned()),
        "{picked:?}"
    );
}

/// A coloured footer with a button over it whose icon sits one panel down, all in HudPlayerHealth.
const FOOTER_BUTTON: &str = r#"  Footer { ControlName EditablePanel xpos 0 ypos 0 zpos 1 wide 120 tall 40 bgcolor_override "40 40 40 255" }
  Btn { ControlName CExImageButton xpos 10 ypos 5 zpos 3 wide 40 tall 30 labelText ""
    Inner { ControlName EditablePanel xpos 0 ypos 0 wide 40 tall 30
      Icon { ControlName ImagePanel xpos 2 ypos 2 wide 10 tall 10 image glyph_achievements } } }
"#;

/// The parts of FOOTER_BUTTON under the point `at` finds in `inside`'s box, best first, a part
/// drawing when it has a colour or an image.
fn pick_footer(inside: &str, at: impl Fn((i32, i32, i32, i32)) -> (i32, i32)) -> Vec<Pick> {
    let model = HudModel::load(&health_with(FOOTER_BUTTON), Platform::Linux).unwrap();
    let l = model.layout(Screen::new(1920, 1080));
    let (x, y) = at(rect(&l, &format!("{HH}/{inside}")));
    let draws = |k: &PartKey| {
        model
            .part(k)
            .is_some_and(|p| p.value("bgcolor_override").is_some() || p.value("image").is_some())
    };
    let ours = |p: &Pick| {
        ["Footer", "Btn"]
            .iter()
            .any(|n| p.key().as_str().contains(n))
    };
    let mut picks = l.pick_ranked(x, y, |_| true, draws);
    picks.retain(ours);
    assert_eq!(
        picks.iter().map(|p| p.key()).collect::<Vec<_>>(),
        l.pick(x, y, |_| true, draws)
            .into_iter()
            .filter(|k| ["Footer", "Btn"].iter().any(|n| k.as_str().contains(n)))
            .collect::<Vec<_>>(),
        "pick and pick_ranked agree"
    );
    picks
}

fn names(picks: &[Pick]) -> Vec<&str> {
    let skip = HH.len() + 1;
    picks.iter().map(|p| &p.key().as_str()[skip..]).collect()
}

#[test]
fn a_click_on_a_buttons_drawn_piece_picks_the_button_first() {
    let picks = pick_footer("Btn/Inner/Icon", |(x, y, w, h)| (x + w / 2, y + h / 2));
    assert_eq!(
        names(&picks),
        ["Btn", "Btn/Inner/Icon", "Footer", "Btn/Inner"],
        "the button stands in for its nearest drawn piece, then what draws nothing"
    );
    assert!(picks[0].draws(), "the button counts as drawing there");
    assert_eq!(picks[0].rank(), picks[1].rank(), "at its icon's place");
    assert!(picks[0].rank() > picks[2].rank(), "over the footer");
}

#[test]
fn away_from_its_pieces_a_button_that_draws_nothing_comes_after_drawn_parts() {
    let picks = pick_footer("Btn", |(x, y, w, h)| (x + w - 2, y + h - 2));
    assert_eq!(names(&picks), ["Footer", "Btn/Inner", "Btn"]);
    assert!(!picks[2].draws());
    let model = HudModel::load(&health_with(FOOTER_BUTTON), Platform::Linux).unwrap();
    let l = model.layout(Screen::new(1920, 1080));
    let btn = PartKey::new(format!("{HH}/Btn"));
    assert_eq!(l.paint_rank(&btn), Some(picks[2].rank()));
}
