//! The HUD Maker's checking tools: every screen shape at once, contrast and colour-blind views.

use crate::common;

use common::hud_stock::{HEALTH, HUDLAYOUT, label, stock_files, stock_files_with, with_blocks};
use tf2tools_core::{
    GameText, HudFiles, HudModel, PartKey, Platform, Rect, Screen, ScreenCheck, ScreenProblem,
};

const UNDER: &str = "HudPlayerStatus/HudPlayerHealth/";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const HD: Screen = Screen::new(1920, 1080);

fn model_of(files: &HudFiles) -> HudModel {
    HudModel::load(files, Platform::Linux).unwrap()
}

/// The stock HUD with `blocks` added inside HudPlayerHealth, which sits at the screen's left
/// edge, 120 units up from its bottom.
fn health_with(blocks: &str) -> HudModel {
    let text = with_blocks(HEALTH, blocks);
    model_of(&stock_files_with(&[(
        "resource/ui/hudplayerhealth.res",
        Some(&text),
    )]))
}

fn k(name: &str) -> PartKey {
    PartKey::new(format!("{UNDER}{name}"))
}

/// The HUD as opened, checked against itself.
fn opened(m: &HudModel) -> Vec<ScreenCheck> {
    let text = GameText::default();
    m.check_screens(&m.screen_baseline(&text), &text)
}

/// `m` with `part` moved by (`dx`, `dy`) px at 1920x1080, checked against `m` as opened.
fn moved(m: &HudModel, part: &PartKey, dx: i32, dy: i32) -> Vec<ScreenCheck> {
    let changes = m.move_part(part, dx, dy, HD, 1).unwrap();
    let text = GameText::default();
    m.with_changes(&changes)
        .into_model()
        .check_screens(&m.screen_baseline(&text), &text)
}

/// Each screen's flags as (shape, part name, problem), in `Screen::PRESETS` order.
fn flags(checks: &[ScreenCheck]) -> Vec<(String, String, ScreenProblem)> {
    checks
        .iter()
        .flat_map(|c| {
            let shape = format!("{}x{}", c.screen().wide(), c.screen().tall());
            c.flags().iter().map(move |f| {
                let name = f.part().as_str().trim_start_matches(UNDER).to_owned();
                (shape.clone(), name, f.problem().clone())
            })
        })
        .collect()
}

#[test]
fn the_stock_hud_is_laid_out_on_all_six_shapes_with_no_flags() {
    let checks = opened(&model_of(&stock_files()));
    let shapes: Vec<(u32, u32)> = checks
        .iter()
        .map(|c| (c.screen().wide(), c.screen().tall()))
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
    assert!(flags(&checks).is_empty(), "{:?}", flags(&checks));
    // Each check carries its own layout, so the view never lays the HUD out again.
    let hv = PartKey::new(HV);
    let b = checks[3].layout().get(&hv).unwrap();
    assert_eq!((b.x(), b.y(), b.wide(), b.tall()), (162, 878, 93, 38));
    assert!(checks[3].drawn().contains(&hv));
}

#[test]
fn a_part_off_the_narrow_shapes_only_is_flagged_there() {
    // 600 units in is x 1350 (45 px wide) on a 1080-line screen, inside even 1440 px; at 1024
    // lines it is x 1280, wholly past a 1280 px screen.
    // The text is wider than its box, so its drawn extent is what leaves.
    let m = health_with(&label(
        "Edge",
        "600",
        "10",
        &[("labelText", "\"Edge label\"")],
    ));
    assert_eq!(
        flags(&opened(&m)),
        [(
            "1280x1024".to_owned(),
            "Edge".to_owned(),
            ScreenProblem::LeavesScreen
        )]
    );
    // 625 units: x 1406 + 45 crosses 4:3's right edge too. Every position is a fixed share of
    // the screen's width in units plus an offset, and 5:4 is the narrowest (600 units to 4:3's
    // 640), so no part can leave 4:3 alone.
    let m = health_with(&label(
        "Edge",
        "625",
        "10",
        &[("labelText", "\"Edge label\"")],
    ));
    let shapes: Vec<String> = flags(&opened(&m)).into_iter().map(|f| f.0).collect();
    assert_eq!(shapes, ["1440x1080", "1280x1024"]);
}

#[test]
fn a_part_off_every_shape_as_opened_is_never_flagged() {
    let m = health_with(&label("Gone", "9999", "10", &[("labelText", "Gone")]));
    assert!(flags(&opened(&m)).is_empty());
    // Moved by the player but still off every screen: still a hide.
    assert!(flags(&moved(&m, &k("Gone"), -450, 0)).is_empty());
    // Moved to 1500 units it shows on 32:9 alone; a part the HUD hid stays unflagged.
    let back = moved(&m, &k("Gone"), -19122, 0);
    let wide = back[5].layout().get(&k("Gone")).unwrap();
    assert_eq!((wide.x(), wide.hidden()), (3377, None));
    assert!(flags(&back).is_empty());
}

#[test]
fn an_overlap_the_hud_shipped_on_some_shapes_is_not_flagged() {
    // r200 is x 1470 at 1920x1080 and 854 at 1280x1024, where it meets plain 400 (853 to 895):
    // the HUD was made that way, so only overlaps the player makes are flagged.
    let m = health_with(
        &(label("Right", "r200", "100", &[("labelText", "R")])
            + &label("Left", "400", "100", &[("labelText", "L")])),
    );
    assert!(flags(&opened(&m)).is_empty());
}

#[test]
fn a_label_moved_onto_another_on_one_shape_is_flagged_there() {
    // Left at 312 units is clear of Right everywhere; 200 px right at 1920x1080 brings it back to
    // about 400 units, where it meets Right at 1280x1024 only.
    let m = health_with(
        &(label("Right", "r200", "100", &[("labelText", "R")])
            + &label("Left", "312", "100", &[("labelText", "L")])),
    );
    assert!(flags(&opened(&m)).is_empty());
    assert_eq!(
        flags(&moved(&m, &k("Left"), 200, 0)),
        [
            (
                "1280x1024".to_owned(),
                "Right".to_owned(),
                ScreenProblem::Overlaps(k("Left"))
            ),
            (
                "1280x1024".to_owned(),
                "Left".to_owned(),
                ScreenProblem::Overlaps(k("Right"))
            ),
        ]
    );
}

#[test]
fn an_overlap_the_hud_shipped_on_every_shape_is_not_flagged() {
    // A sits under B on every shape, as a background sits under its text.
    let m = health_with(
        &(label("A", "10", "10", &[("labelText", "A")])
            + &label("B", "15", "12", &[("labelText", "B")])),
    );
    assert!(flags(&opened(&m)).is_empty());
    // An edit elsewhere leaves it alone.
    assert!(flags(&moved(&m, &PartKey::new(HV), 0, -20)).is_empty());
}

#[test]
fn a_label_dragged_onto_another_is_flagged_on_every_shape() {
    // C at 200 units (x 450) dragged 430 px left lands on A (10 units, 20 wide) everywhere.
    let m = health_with(
        &(label("A", "10", "10", &[("labelText", "A")])
            + &label("C", "200", "10", &[("labelText", "C")])),
    );
    assert!(flags(&opened(&m)).is_empty());
    let found = flags(&moved(&m, &k("C"), -430, 0));
    assert_eq!(found.len(), 12, "{found:?}");
    for shape in Screen::PRESETS {
        let shape = format!("{}x{}", shape.wide(), shape.tall());
        assert!(found.contains(&(
            shape.clone(),
            "C".to_owned(),
            ScreenProblem::Overlaps(k("A"))
        )));
        assert!(found.contains(&(shape, "A".to_owned(), ScreenProblem::Overlaps(k("C")))));
    }
}

#[test]
fn a_label_dragged_partly_off_every_shape_is_flagged() {
    // A at x 22 dragged 40 px left starts off the left edge on every shape.
    let m = health_with(&label("A", "10", "10", &[("labelText", "A")]));
    let found = flags(&moved(&m, &k("A"), -40, 0));
    assert_eq!(found.len(), 6, "{found:?}");
    assert!(
        found
            .iter()
            .all(|f| f.1 == "A" && f.2 == ScreenProblem::LeavesScreen)
    );
}

#[test]
fn a_part_never_overlaps_its_own_parent_or_child() {
    // The painted bar is 420 units wide (896 px at 1280x1024), so its child at r200 (854) sits
    // on it there only. Checked against the stock HUD, every overlap here is new: the unrelated
    // label at the same place is flagged, the child is not.
    let bar = r#"  Bar
  {
    ControlName EditablePanel
    xpos 0
    ypos 0
    wide 420
    tall 50
    bgcolor_override "0 0 0 200"
    Tag { ControlName CExLabel xpos r200 ypos 10 wide 20 tall 10 labelText T }
  }
"#;
    let m = health_with(&(bar.to_owned() + &label("Loose", "r200", "10", &[("labelText", "L")])));
    let text = GameText::default();
    let stock = model_of(&stock_files()).screen_baseline(&text);
    let found = flags(&m.check_screens(&stock, &text));
    assert!(
        found.contains(&(
            "1280x1024".to_owned(),
            "Loose".to_owned(),
            ScreenProblem::Overlaps(k("Bar"))
        )),
        "{found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|f| f.1 == "Bar/Tag" && f.2 == ScreenProblem::Overlaps(k("Bar"))),
        "{found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|f| f.1 == "Bar" && f.2 == ScreenProblem::Overlaps(k("Bar/Tag"))),
        "{found:?}"
    );
}

/// The default HUD's target ID: two labels far wider than their text, filled by the game.
const TARGET_ID: &str = r#""Resource/UI/TargetID.res"
{
  TargetNameLabel { ControlName Label xpos 8 ypos 5 wide 640 tall 24 font HudFontSmall
    labelText "%targetname%" textAlignment North-West }
  MoveableSubPanel { ControlName EditablePanel xpos 0 ypos 0 wide 640 tall 50
    MoveableKeyLabel { ControlName Label xpos 0 ypos 22 wide 640 tall 24 font HudFontSmall
      labelText "%movekey%" textAlignment North } }
  Spare { ControlName Label xpos 8 ypos 40 wide 640 tall 24 font HudFontSmall labelText "" }
}
"#;

// The target ID's labels in the health account panel, placed where the target ID sits: the
// game hides the target ID itself in the preview's situation.
fn target_id_with(extra: &str) -> HudModel {
    let labels = TARGET_ID
        .replace("labelText \"\" }", &format!("labelText \"{extra}\" }}"))
        .replace(
            "\"Resource/UI/TargetID.res\"\n{",
            "\"Resource/UI/HudHealthAccount.res\"\n{\n  CHealthAccountPanel { }",
        );
    let layout = HUDLAYOUT.replace(
        "CHealthAccountPanel { xpos c-60 ypos c-8 wide 120 tall 70 }",
        "CHealthAccountPanel { xpos c-126 ypos 355 wide 252 tall 35 }",
    );
    model_of(&stock_files_with(&[
        ("resource/ui/hudhealthaccount.res", Some(&labels)),
        ("scripts/hudlayout.res", Some(&layout)),
    ]))
}

#[test]
fn labels_the_game_fills_or_leaves_empty_never_leave_the_screen() {
    // Wide boxes hang off the narrow shapes, but nothing is drawn there until the game fills them.
    let m = target_id_with("");
    assert!(flags(&opened(&m)).is_empty(), "{:?}", flags(&opened(&m)));
    let found = flags(&moved(
        &m,
        &PartKey::new("CHealthAccountPanel/TargetNameLabel"),
        2000,
        0,
    ));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_label_counts_for_leaving_by_its_text_not_its_box() {
    // Short text at the box's west edge stays on every shape though the box does not.
    let m = target_id_with("Hi");
    assert!(flags(&opened(&m)).is_empty(), "{:?}", flags(&opened(&m)));
    // Moved 1100 px right at 1920x1080, the text is still on 16:9 but off the 5:4 screen.
    let found = flags(&moved(
        &m,
        &PartKey::new("CHealthAccountPanel/Spare"),
        1100,
        0,
    ));
    assert!(
        found.contains(&(
            "1280x1024".to_owned(),
            "CHealthAccountPanel/Spare".to_owned(),
            ScreenProblem::LeavesScreen
        )),
        "{found:?}"
    );
    assert!(!found.iter().any(|f| f.0 == "1920x1080"), "{found:?}");
}

#[test]
fn a_label_that_paints_its_box_leaves_by_its_box() {
    // No text, but a painted background: dragged past the left edge, it is off every shape.
    let m = health_with(&label(
        "Boxed",
        "10",
        "10",
        &[
            ("labelText", r#""""#),
            ("bgcolor_override", r#""0 0 0 200""#),
        ],
    ));
    assert!(flags(&opened(&m)).is_empty(), "{:?}", flags(&opened(&m)));
    let found = flags(&moved(&m, &k("Boxed"), -40, 0));
    assert_eq!(found.len(), 6, "{found:?}");
    assert!(found.iter().all(|f| f.2 == ScreenProblem::LeavesScreen));
}

#[test]
fn a_text_label_draws_in_its_text_extent_and_a_painted_one_in_its_box() {
    // A 20-unit box holds 11-unit text, so the box's edges run well clear of the glyphs.
    let caps = |name: &str, ypos: &str, extra: &[(&str, &str)]| {
        let mut keys = vec![
            ("xpos", "100"),
            ("ypos", ypos),
            ("wide", "60"),
            ("tall", "20"),
            ("font", "HudFontSmall"),
            ("textAlignment", "east"),
            ("labelText", r#""Caps:""#),
        ];
        keys.extend_from_slice(extra);
        block(name, "CExLabel", &keys)
    };
    let m = health_with(
        &(caps("Caps", "30", &[])
            + &caps("Boxed", "60", &[("bgcolor_override", r#""0 0 0 200""#)])),
    );
    let (layout, text) = (m.layout(HD), GameText::default());
    let b = layout.get(&k("Caps")).unwrap();
    let tall = (m.scheme().font("HudFontSmall", HD.tall()))
        .unwrap()
        .pixel_tall(HD.tall());
    let wide = (5.0 * f64::from(tall) * 0.55).ceil() as i32;
    let Rect {
        x,
        y,
        wide: w,
        tall: h,
        ..
    } = m.text_extent(&layout, &k("Caps"), &text).unwrap();
    assert_eq!((w, h), (wide, tall));
    assert_eq!(
        x + w,
        b.x() + b.wide(),
        "east puts the text at the box's right edge"
    );
    assert_eq!(y, b.y() + (b.tall() - tall) / 2, "centred in the box");
    let boxed = layout.get(&k("Boxed")).unwrap();
    assert_eq!(
        m.text_extent(&layout, &k("Boxed"), &text),
        Some(Rect::new(boxed.x(), boxed.y(), boxed.wide(), boxed.tall()))
    );
    assert_eq!(m.text_extent(&layout, &k("Missing"), &text), None);
}

#[test]
fn multi_line_text_is_as_wide_as_its_widest_line() {
    // Five short lines stay on 16:9 where one line of all of them would not.
    let m = target_id_with(r"Hi\nHi\nHi\nHi\nHi");
    let found = flags(&moved(
        &m,
        &PartKey::new("CHealthAccountPanel/Spare"),
        1100,
        0,
    ));
    assert!(!found.iter().any(|f| f.0 == "1920x1080"), "{found:?}");
    assert!(found.iter().any(|f| f.0 == "1280x1024"), "{found:?}");
}

/// The stock HUD with a `yres`-limited font and a scaled one, both 10 tall as written, and a
/// ten-letter label in `font` whose box runs off 1920x1080 while its text may not.
fn label_in_font(font: &str) -> HudModel {
    let scheme = SCHEME.replace(
        "    HudFontSmall {",
        "    Fixed { \"1\" { name Verdana tall 10 yres \"480 2160\" } }\n    \
         Scaled { \"1\" { name Verdana tall 10 } }\n    HudFontSmall {",
    );
    // 813 units in is x 1829 at 1920x1080; ten letters 10 px tall end near 1884, 22 px tall
    // end past 1920.
    let blocks = block(
        "Words",
        "CExLabel",
        &[
            ("xpos", "813"),
            ("ypos", "10"),
            ("wide", "200"),
            ("tall", "20"),
            ("font", font),
            ("labelText", "ABCDEFGHIJ"),
        ],
    );
    let health = with_blocks(HEALTH, &blocks);
    model_of(&stock_files_with(&[
        ("resource/clientscheme.res", Some(&scheme)),
        ("resource/ui/hudplayerhealth.res", Some(&health)),
    ]))
}

#[test]
fn a_label_in_a_yres_limited_font_is_measured_at_its_written_tall() {
    let on_hd = |m: &HudModel| flags(&opened(m)).iter().any(|f| f.0 == "1920x1080");
    assert!(!on_hd(&label_in_font("Fixed")));
    assert!(on_hd(&label_in_font("Scaled")));
}

#[test]
fn a_hotkey_marker_takes_no_room_in_a_labels_text_extent() {
    let wide_label = |name: &str, ypos: &str, words: &str| {
        let words = format!("\"{words}\"");
        block(
            name,
            "CExLabel",
            &[
                ("xpos", "10"),
                ("ypos", ypos),
                ("wide", "400"),
                ("tall", "20"),
                ("labelText", &words),
            ],
        )
    };
    let m = health_with(
        &(wide_label("Marked", "10", "&Salt &&&& &Pepper")
            + &wide_label("Drawn", "40", "Salt xx Pepper")),
    );
    let (layout, text) = (m.layout(HD), GameText::default());
    let width = |name: &str| m.text_extent(&layout, &k(name), &text).unwrap().wide;
    assert_eq!(width("Marked"), width("Drawn"));
}

#[test]
fn bare_panels_are_not_drawn_parts() {
    // HudPlayerStatus spans the screen on every shape and paints nothing, so nothing overlaps it.
    let checks = opened(&model_of(&stock_files()));
    assert!(!checks[0].drawn().contains(&PartKey::new("HudPlayerStatus")));
}

use common::hud_stock::{SCHEME, block};
use tf2tools_core::{Backdrop, MIN_CONTRAST, Rgba, Vision, contrast_ratio};

fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    Rgba::new(r, g, b, 255)
}

/// A `wide` x `tall` backdrop of one colour.
fn solid(wide: u32, tall: u32, c: Rgba) -> Backdrop {
    let px = c.to_array();
    Backdrop::new(wide, tall, px.repeat((wide * tall) as usize)).unwrap()
}

#[test]
fn contrast_follows_wcag_2() {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-3;
    assert!(close(contrast_ratio(Rgba::WHITE, rgb(0, 0, 0)), 21.0));
    assert!(close(
        contrast_ratio(rgb(119, 119, 119), Rgba::WHITE),
        4.478
    ));
    // TanDark on black and TanLight on white, both from the stock scheme.
    assert!(close(
        contrast_ratio(rgb(117, 107, 94), rgb(0, 0, 0)),
        4.021
    ));
    assert!(close(
        contrast_ratio(rgb(235, 226, 202), Rgba::WHITE),
        1.291
    ));
    // Symmetric, and alpha is not read.
    assert!(close(
        contrast_ratio(rgb(0, 0, 0), Rgba::new(255, 255, 255, 0)),
        21.0
    ));
    assert_eq!(MIN_CONTRAST, 3.0);
}

#[test]
fn colour_blind_views_use_machado_at_full_severity() {
    let seen = |v: Vision, c: Rgba| v.simulate(c).to_array();
    assert_eq!(seen(Vision::Normal, rgb(180, 92, 77)), [180, 92, 77, 255]);
    assert_eq!(seen(Vision::Protanopia, rgb(255, 0, 0)), [109, 95, 0, 255]);
    assert_eq!(seen(Vision::Protanopia, rgb(0, 255, 0)), [255, 229, 0, 255]);
    assert_eq!(
        seen(Vision::Deuteranopia, rgb(255, 0, 0)),
        [163, 144, 0, 255]
    );
    assert_eq!(
        seen(Vision::Deuteranopia, rgb(0, 0, 255)),
        [0, 61, 251, 255]
    );
    assert_eq!(seen(Vision::Tritanopia, rgb(0, 255, 0)), [0, 247, 217, 255]);
    assert_eq!(seen(Vision::Tritanopia, rgb(0, 0, 255)), [0, 107, 150, 255]);
    // The stock red and blue team colours draw closer under protanopia; alpha is kept.
    assert_eq!(
        seen(Vision::Protanopia, Rgba::new(180, 92, 77, 128)),
        [114, 106, 75, 128]
    );
    assert_eq!(
        seen(Vision::Protanopia, rgb(104, 124, 155)),
        [113, 125, 156, 255]
    );
    let mut pixels = vec![255, 0, 0, 255, 0, 0, 255, 7];
    Vision::Deuteranopia.simulate_rgba(&mut pixels);
    assert_eq!(pixels, [163, 144, 0, 255, 0, 61, 251, 7]);
}

#[test]
fn a_backdrop_is_stretched_over_the_screen_and_averaged_under_a_box() {
    // Two pixels: black on the left half of the screen, white on the right.
    let half = Backdrop::new(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255]).unwrap();
    assert_eq!(half.mean_under(HD, 0, 0, 100, 100), Some(rgb(0, 0, 0)));
    assert_eq!(
        half.mean_under(HD, 1800, 500, 100, 100),
        Some(rgb(255, 255, 255))
    );
    assert_eq!(
        half.mean_under(HD, 900, 0, 200, 10),
        Some(rgb(128, 128, 128))
    );
    // Only the part on the screen counts; a box wholly off it has nothing beneath.
    assert_eq!(half.mean_under(HD, -50, 0, 100, 10), Some(rgb(0, 0, 0)));
    assert_eq!(half.mean_under(HD, 1920, 0, 100, 10), None);
    assert_eq!(half.mean_under(HD, 0, 0, 0, 10), None);
    assert!(Backdrop::new(2, 2, vec![0; 15]).is_none());
    assert!(Backdrop::new(0, 0, Vec::new()).is_none());
}

#[test]
fn text_is_hard_to_read_under_three_to_one() {
    let tan = k("Tan");
    let m = health_with(&label(
        "Tan",
        "10",
        "10",
        &[("labelText", "T"), ("fgcolor", "TanDark")],
    ));
    let layout = m.layout(HD);
    // Tan is TanDark: 4.02 on black, 5.22 on white, 1.04 on a backdrop of nearly its colour.
    assert_eq!(m.text_colour(m.part(&tan).unwrap()), rgb(117, 107, 94));
    assert!(
        !m.hard_to_read(&layout, &solid(4, 4, rgb(0, 0, 0)), Vision::Normal)
            .contains(&tan)
    );
    assert!(
        !m.hard_to_read(&layout, &solid(4, 4, Rgba::WHITE), Vision::Normal)
            .contains(&tan)
    );
    let beige = solid(4, 4, rgb(120, 110, 95));
    let hard = m.hard_to_read(&layout, &beige, Vision::Normal);
    assert!(hard.contains(&tan), "{hard:?}");
}

#[test]
fn faint_text_is_mixed_into_the_backdrop_first() {
    // White at alpha 40 over black draws as 40 40 40: 1.42 to 1.
    let m = health_with(&label(
        "Faint",
        "10",
        "10",
        &[("labelText", "F"), ("fgcolor", r#""255 255 255 40""#)],
    ));
    assert_eq!(
        m.text_colour(m.part(&k("Faint")).unwrap()),
        Rgba::new(255, 255, 255, 40)
    );
    let hard = m.hard_to_read(&m.layout(HD), &solid(4, 4, rgb(0, 0, 0)), Vision::Normal);
    assert!(hard.contains(&k("Faint")), "{hard:?}");
}

#[test]
fn a_colour_blind_view_checks_the_colours_it_shows() {
    // Red on dark grey: 3.69 to 1 normally, 2.31 under protanopia, 4.60 under deuteranopia.
    let m = health_with(&label(
        "Red",
        "10",
        "10",
        &[("labelText", "R"), ("fgcolor", r#""255 0 0 255""#)],
    ));
    let (layout, grey) = (m.layout(HD), solid(4, 4, rgb(40, 40, 40)));
    assert_eq!(m.text_colour(m.part(&k("Red")).unwrap()), rgb(255, 0, 0));
    let red_hard = |v: Vision| m.hard_to_read(&layout, &grey, v).contains(&k("Red"));
    assert!(!red_hard(Vision::Normal));
    assert!(red_hard(Vision::Protanopia));
    assert!(!red_hard(Vision::Deuteranopia));
}

#[test]
fn hidden_and_off_screen_labels_are_not_checked() {
    let m = health_with(
        &(label(
            "Gone",
            "9999",
            "10",
            &[("labelText", "G"), ("fgcolor", r#""0 0 0 255""#)],
        ) + &block(
            "Off",
            "CExLabel",
            &[
                ("wide", "0"),
                ("tall", "10"),
                ("labelText", "O"),
                ("fgcolor", r#""0 0 0 255""#),
            ],
        )),
    );
    let hard = m.hard_to_read(&m.layout(HD), &solid(4, 4, rgb(0, 0, 0)), Vision::Normal);
    assert!(
        !hard.contains(&k("Gone")) && !hard.contains(&k("Off")),
        "{hard:?}"
    );
}

#[test]
fn a_pile_of_two_thousand_labels_is_checked_in_normal_test_time() {
    // Every label meets every other on every shape, two million pairs a shape: past the pair
    // cap the shape is too crowded to judge overlaps, so the pass stops early and flags none.
    let pile: String = (0..2000)
        .map(|i| label(&format!("P{i}"), "10", "10", &[("labelText", "P")]))
        .collect();
    let m = health_with(&pile);
    let checks = opened(&m);
    assert!(flags(&checks).is_empty());
    // The tile says so, rather than "No problems".
    assert!(checks.iter().all(|c| c.too_crowded()));
    let few = opened(&health_with(&label("A", "10", "10", &[("labelText", "A")])));
    assert!(few.iter().all(|c| !c.too_crowded()));
    let found = flags(&moved(&m, &k("P0"), 0, -40));
    assert!(
        found.iter().all(|f| f.2 == ScreenProblem::LeavesScreen),
        "{:?}",
        &found[..found.len().min(4)]
    );
}

// What sits behind a label: the HUD's own fills and pictures, over the backdrop.

use common::hud_stock::{AMMO, SCOREBOARD, WIN_PANEL};
use tf2tools_core::Scene;

const BEIGE: Rgba = Rgba::new(235, 226, 202, 255);

/// The names of the labels `m` warns about at 1920x1080 over a backdrop of `c`.
fn warned(m: &HudModel, c: Rgba) -> Vec<String> {
    (m.hard_to_read(&m.layout(HD), &solid(4, 4, c), Vision::Normal)
        .iter())
    .map(|key| m.part(key).unwrap().name().to_owned())
    .collect()
}

/// `warned`, kept to the labels of HudPlayerHealth, where these tests add theirs.
fn warned_in_health(m: &HudModel, c: Rgba) -> Vec<String> {
    let names = warned(m, c);
    let in_health = |name: &String| m.part(&k(name)).is_some();
    names.into_iter().filter(in_health).collect()
}

/// A panel `wide` units wide and 20 tall at (`xpos`, 0), with `extra` keys.
fn panel_at(xpos: &str, wide: &str, extra: &[(&str, &str)]) -> String {
    let mut keys = vec![
        ("xpos", xpos),
        ("ypos", "0"),
        ("wide", wide),
        ("tall", "20"),
    ];
    keys.extend_from_slice(extra);
    block("Panel", "EditablePanel", &keys)
}

/// A 40x20 unit panel at (0, 0), with `extra` keys, for a `Text` label to sit over.
fn panel(extra: &[(&str, &str)]) -> String {
    panel_at("0", "40", extra)
}

/// White text at (5, 5), painted over layer 2 and under layer 4.
fn white_text() -> String {
    label(
        "Text",
        "5",
        "5",
        &[
            ("zpos", "3"),
            ("labelText", "T"),
            ("fgcolor", r#""255 255 255 255""#),
        ],
    )
}

/// The stock scheme with the default HUD's picture borders.
fn scheme_with_picture_borders() -> String {
    const BORDERS: &str = "  Borders {
    TFThinLineBorder { bordertype scalable_image image ../hud/tournament_panel_brown src_corner_height 2 src_corner_width 2 draw_corner_width 8 draw_corner_height 8 }
    TFFatLineBorderBlueBG { bordertype scalable_image image ../hud/color_panel_blu src_corner_height 2 src_corner_width 2 draw_corner_width 8 draw_corner_height 8 }
  }
  Fonts\n";
    assert!(
        SCHEME.contains("  Fonts\n"),
        "the stock scheme should hold the Fonts section to insert before"
    );
    SCHEME.replacen("  Fonts\n", BORDERS, 1)
}

#[test]
fn the_scoreboard_stats_read_on_their_panel() {
    // The default HUD's stat labels sit in the panel the board's picture border draws.
    let stats = SCOREBOARD.replacen(
        "Kills { ControlName CExLabel xpos 0 ypos 0 wide 40 tall 10 visible 0 labelText %kills% }",
        "KillsLabel { ControlName CExLabel xpos 80 ypos 0 zpos 3 wide 95 tall 20 fgcolor TanLight labelText Kills: }
         DeathsLabel { ControlName CExLabel xpos 80 ypos 20 zpos 3 wide 95 tall 20 fgcolor TanLight labelText Deaths: }
         CapturesLabel { ControlName CExLabel xpos 180 ypos 0 zpos 3 wide 95 tall 20 fgcolor TanLight labelText Captures: }",
        1,
    );
    let scheme = scheme_with_picture_borders();
    let load = |board: &str| {
        let files = stock_files_with(&[
            ("resource/ui/scoreboard.res", Some(board)),
            ("resource/clientscheme.res", Some(&scheme)),
        ]);
        HudModel::load_scene(&files, Platform::Linux, Scene::Scoreboard).unwrap()
    };
    let bare = warned(&load(&stats), BEIGE);
    for name in ["KillsLabel", "DeathsLabel", "CapturesLabel"] {
        assert!(bare.iter().any(|n| n == name), "{bare:?}");
    }
    let backed = with_blocks(
        &stats,
        r#"MainBG { ControlName EditablePanel xpos 0 ypos 38 zpos -1 wide 640 tall 410 border TFThinLineBorder }
           ShadedBar { ControlName ImagePanel xpos 4 ypos 372 zpos -2 wide 633 tall 72 fillcolor "0 0 0 153" }"#,
    );
    let hard = warned(&load(&backed), BEIGE);
    for name in ["KillsLabel", "DeathsLabel", "CapturesLabel"] {
        assert!(!hard.iter().any(|n| n == name), "{hard:?}");
    }
}

#[test]
fn the_win_banner_reads_on_its_team_panel() {
    // No colour key on the label, and the game's code sets the panel's border, not the file.
    let bare = HudModel::load_scene(&stock_files(), Platform::Linux, Scene::WinPanel).unwrap();
    assert!(
        warned(&bare, Rgba::WHITE)
            .iter()
            .any(|n| n == "WinningTeamLabel")
    );
    let scheme = scheme_with_picture_borders();
    let files = stock_files_with(&[
        ("resource/ui/winpanel.res", Some(WIN_PANEL)),
        ("resource/clientscheme.res", Some(&scheme)),
    ]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::WinPanel).unwrap();
    let hard = warned(&m, Rgba::WHITE);
    assert!(!hard.iter().any(|n| n == "WinningTeamLabel"), "{hard:?}");
}

#[test]
fn the_ammo_reads_on_its_red_box() {
    let load = |text: &str| {
        let files = stock_files_with(&[("resource/ui/hudammoweapons.res", Some(text))]);
        model_of(&files)
    };
    assert!(warned(&load(AMMO), BEIGE).iter().any(|n| n == "AmmoInClip"));
    let boxed = with_blocks(
        AMMO,
        "HudWeaponAmmoBG { ControlName CTFImagePanel xpos 4 ypos 0 zpos 1 wide 90 tall 45
           image ../hud/ammo_red_bg teambg_2 ../hud/ammo_red_bg teambg_3 ../hud/ammo_blue_bg }",
    )
    .replacen("xpos 10 ypos 5", "zpos 5 xpos 10 ypos 5", 1);
    let hard = warned(&load(&boxed), BEIGE);
    assert!(!hard.iter().any(|n| n == "AmmoInClip"), "{hard:?}");
}

#[test]
fn text_over_the_bare_backdrop_beside_a_panel_still_warns() {
    let far = [("zpos", "2"), ("bgcolor_override", r#""0 0 0 255""#)];
    let m = health_with(&(panel_at("100", "40", &far) + &white_text()));
    assert_eq!(warned_in_health(&m, Rgba::WHITE), ["Text"]);
}

#[test]
fn an_opaque_panel_behind_the_text_replaces_the_backdrop() {
    let black = [("zpos", "2"), ("bgcolor_override", r#""0 0 0 255""#)];
    let white = [("zpos", "2"), ("fillcolor", r#""255 255 255 255""#)];
    let on = |keys: &[(&str, &str)], c| {
        warned_in_health(&health_with(&(panel(keys) + &white_text())), c)
    };
    assert_eq!(on(&black, Rgba::WHITE), [""; 0]);
    assert_eq!(on(&white, rgb(0, 0, 0)), ["Text"]);
}

#[test]
fn a_translucent_panel_only_partly_counts() {
    // TanDark reads on white and on black; half black over white is the grey it does not.
    let m = health_with(
        &(panel(&[("zpos", "2"), ("bgcolor_override", r#""0 0 0 128""#)])
            + &label(
                "Tan",
                "5",
                "5",
                &[("zpos", "3"), ("labelText", "T"), ("fgcolor", "TanDark")],
            )),
    );
    assert_eq!(warned_in_health(&m, Rgba::WHITE), ["Tan"]);
    assert_eq!(warned_in_health(&m, rgb(0, 0, 0)), [""; 0]);
}

#[test]
fn a_panel_painted_after_the_text_does_not_count() {
    let m = health_with(
        &(panel(&[("zpos", "4"), ("bgcolor_override", r#""255 255 255 255""#)]) + &white_text()),
    );
    assert_eq!(warned_in_health(&m, rgb(0, 0, 0)), [""; 0]);
}

#[test]
fn a_panel_that_does_not_draw_does_not_count() {
    let white = ("bgcolor_override", r#""255 255 255 255""#);
    let z = ("zpos", "2");
    for keys in [
        vec![z, white, ("visible", "0")],
        vec![z, white, ("paintbackground", "0")],
        vec![z, ("bgcolor_override", r#""255 255 255 0""#)],
    ] {
        let m = health_with(&(panel(&keys) + &white_text()));
        assert_eq!(warned_in_health(&m, rgb(0, 0, 0)), [""; 0], "{keys:?}");
    }
}

#[test]
fn a_panel_counts_from_exactly_half_of_the_text_box() {
    // At 2.25 px a unit the text box spans x 11 to 101: half is 45 px, which a panel 25 units
    // wide reaches to the pixel (to x 56) and one 24 wide misses (to x 54).
    let text = block(
        "Text",
        "CExLabel",
        &[
            ("xpos", "5"),
            ("ypos", "5"),
            ("zpos", "3"),
            ("wide", "40"),
            ("tall", "10"),
            ("labelText", "T"),
            ("fgcolor", r#""255 255 255 255""#),
        ],
    );
    let upto = |wide: &'static str| {
        let keys = [("zpos", "2"), ("bgcolor_override", r#""255 255 255 255""#)];
        let m = health_with(&(panel_at("0", wide, &keys) + &text));
        warned_in_health(&m, rgb(0, 0, 0))
    };
    assert_eq!(upto("25"), ["Text"]);
    assert_eq!(upto("24"), [""; 0]);
}

/// Text in a label whose box runs far past the 20x10 parent that cuts it, over a white panel
/// when `panelled`.
fn cut_label(panelled: bool) -> HudModel {
    let panel = match panelled {
        true => {
            r#"Fill { ControlName EditablePanel xpos 0 ypos 0 zpos 1 wide 20 tall 10 bgcolor_override "255 255 255 255" }"#
        }
        false => "",
    };
    health_with(&format!(
        r#"  Holder {{ ControlName EditablePanel xpos 0 ypos 0 wide 20 tall 10
             {panel}
             Text {{ ControlName CExLabel xpos 0 ypos 0 zpos 3 wide 640 tall 10 labelText T fgcolor "255 255 255 255" }} }}
"#
    ))
}

#[test]
fn a_panel_is_measured_against_the_part_of_the_label_that_shows() {
    // The panel covers all of what shows, though only a thirtieth of the label's box.
    let hard = warned(&cut_label(true), rgb(0, 0, 0));
    assert!(hard.iter().any(|n| n == "Text"), "{hard:?}");
}

#[test]
fn the_backdrop_is_read_under_the_part_of_the_label_that_shows() {
    // White on the left half of the screen, black on the right. What shows lies on the white;
    // the whole box would average to a grey white text reads on.
    let half = Backdrop::new(2, 1, vec![255, 255, 255, 255, 0, 0, 0, 255]).unwrap();
    let m = cut_label(false);
    let hard = m.hard_to_read(&m.layout(HD), &half, Vision::Normal);
    assert!(
        hard.iter().any(|key| m.part(key).unwrap().name() == "Text"),
        "{hard:?}"
    );
}

/// The stock HUD with the picture borders and `blocks` added inside HudPlayerHealth.
fn health_with_borders(blocks: &str) -> HudModel {
    let (health, scheme) = (with_blocks(HEALTH, blocks), scheme_with_picture_borders());
    model_of(&stock_files_with(&[
        ("resource/ui/hudplayerhealth.res", Some(&health)),
        ("resource/clientscheme.res", Some(&scheme)),
    ]))
}

#[test]
fn a_part_s_fill_covers_its_border_and_its_picture_covers_its_fill() {
    let z = ("zpos", "2");
    let white = ("bgcolor_override", r#""255 255 255 255""#);
    let border = ("border", "TFThinLineBorder");
    let image = ("image", "../hud/some_picture");
    let over = |keys: &[(&str, &str)]| {
        warned_in_health(
            &health_with_borders(&(panel(keys) + &white_text())),
            rgb(0, 0, 0),
        )
    };
    // Over the border the fill is what shows, a known white the white text cannot read on.
    assert_eq!(over(&[z, white, border]), ["Text"]);
    // Over the fill the picture is what shows, and nothing is known about it.
    assert_eq!(over(&[z, white, image]), [""; 0]);
    assert_eq!(over(&[z, white, border, image]), [""; 0]);
}

#[test]
fn a_panel_counts_only_where_its_parent_shows_it() {
    // The fill reaches past its 20x10 parent, which cuts it before it gets to the text.
    let m = health_with(
        &(r#"  Holder { ControlName EditablePanel xpos 0 ypos 0 zpos 1 wide 20 tall 10
             Fill { ControlName EditablePanel xpos 0 ypos 0 wide 100 tall 10 bgcolor_override "255 255 255 255" } }
"#
        .to_owned()
            + &label(
                "Text",
                "30",
                "0",
                &[
                    ("zpos", "3"),
                    ("labelText", "T"),
                    ("fgcolor", r#""255 255 255 255""#),
                ],
            )),
    );
    assert_eq!(warned_in_health(&m, rgb(0, 0, 0)), [""; 0]);
}

#[test]
fn a_label_s_own_background_is_behind_its_text() {
    let own = |bg: &'static str| {
        health_with(&label(
            "Own",
            "5",
            "5",
            &[
                ("labelText", "O"),
                ("fgcolor", r#""255 255 255 255""#),
                ("bgcolor_override", bg),
            ],
        ))
    };
    assert_eq!(
        warned_in_health(&own(r#""255 255 255 255""#), rgb(0, 0, 0)),
        ["Own"]
    );
    assert_eq!(
        warned_in_health(&own(r#""0 0 0 255""#), Rgba::WHITE),
        [""; 0]
    );
}

#[test]
fn no_colour_is_claimed_for_text_over_a_picture_the_core_cannot_read() {
    let picture = panel(&[("zpos", "2"), ("image", "../hud/some_picture")]);
    let m = health_with(&(picture.clone() + &white_text()));
    assert_eq!(warned_in_health(&m, Rgba::WHITE), [""; 0]);
    // A translucent fill over the picture is still unknown; an opaque one is a known colour.
    let over = |bg: &str| {
        let keys = [
            ("xpos", "0"),
            ("ypos", "0"),
            ("zpos", "2"),
            ("wide", "40"),
            ("tall", "20"),
            ("bgcolor_override", bg),
        ];
        let lid = block("Lid", "EditablePanel", &keys);
        let low = picture.replace("zpos 2", "zpos 1");
        warned_in_health(&health_with(&(low + &lid + &white_text())), Rgba::WHITE)
    };
    assert_eq!(over(r#""255 255 255 100""#), [""; 0]);
    assert_eq!(over(r#""255 255 255 255""#), ["Text"]);
}

#[test]
fn the_health_number_reads_on_the_cross_the_game_paints() {
    // The cross has no image key in the file: the game's code paints it behind the number.
    let m = model_of(&stock_files());
    let hard = m.hard_to_read(
        &m.layout(HD),
        &solid(4, 4, rgb(120, 110, 95)),
        Vision::Normal,
    );
    assert!(!hard.contains(&PartKey::new(HV)), "{hard:?}");
}

/// A label of `text` over a panel of `fill`.
fn text_on_panel(text: &str, fill: &str) -> HudModel {
    let colour = format!("{text:?}");
    health_with(
        &(panel(&[("zpos", "2"), ("bgcolor_override", fill)])
            + &label(
                "Text",
                "5",
                "5",
                &[("zpos", "3"), ("labelText", "T"), ("fgcolor", &colour)],
            )),
    )
}

#[test]
fn a_colour_blind_view_judges_an_opaque_panel_as_it_shows_it() {
    // Dark grey on red, over a white backdrop: 3.69 to 1, 2.31 red-blind, 4.60 green-blind.
    let m = text_on_panel("40 40 40 255", r#""255 0 0 255""#);
    let (layout, white) = (m.layout(HD), solid(4, 4, Rgba::WHITE));
    let hard = |v: Vision| m.hard_to_read(&layout, &white, v).contains(&k("Text"));
    assert!(!hard(Vision::Normal));
    assert!(hard(Vision::Protanopia));
    assert!(!hard(Vision::Deuteranopia));
}

#[test]
fn a_colour_blind_view_mixes_a_translucent_panel_as_it_shows_it() {
    // Half red over black draws 128 0 0, or 55 48 0 red-blind: 2.66 to 1 becomes 3.21 for this
    // grey, so the warning goes only if the view's red is what gets mixed.
    let m = text_on_panel("125 125 125 255", r#""255 0 0 128""#);
    let (layout, black) = (m.layout(HD), solid(4, 4, rgb(0, 0, 0)));
    let hard = |v: Vision| m.hard_to_read(&layout, &black, v).contains(&k("Text"));
    assert!(hard(Vision::Normal));
    assert!(!hard(Vision::Protanopia));
}
