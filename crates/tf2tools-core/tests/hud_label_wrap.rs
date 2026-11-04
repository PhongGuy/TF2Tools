//! Wrapped labels: where the game's line breaks fall, and the text area the checks read.

mod common;

use common::hud_stock::{HEALTH, block, stock_files_with, with_blocks};
use tf2tools_core::{GameText, HudModel, PartKey, Platform, Screen, wrap_lines, wrapped_text_left};

const UNDER: &str = "HudPlayerStatus/HudPlayerHealth/";
const HD: Screen = Screen::new(1920, 1080);
/// The checks' guess at a glyph's advance, as a share of the font's height.
const GLYPH: f64 = 0.55;

/// `text` broken in a box `room` characters wide, every character one unit wide.
fn wrapped(text: &str, room: u16) -> Vec<&str> {
    wrap_lines(text, f32::from(room), |_| 1.)
}

/// `text` broken with every glyph `glyph` wide. A looping break never returns, so the count
/// stops it inside the closure: no bound on the lines could, and they would eat memory first.
fn counted(text: &str, room: f32, glyph: f32) -> Vec<&str> {
    let limit = 4 * text.chars().count() + 8;
    let mut calls = 0;
    wrap_lines(text, room, |_| {
        calls += 1;
        assert!(calls <= limit, "{text:?} in room {room} loops");
        glyph
    })
}

#[test]
fn a_line_breaks_at_the_start_of_the_word_that_does_not_fit() {
    assert_eq!(
        wrapped("Stick with the Medic", 11),
        ["Stick with ", "the Medic"]
    );
}

#[test]
fn text_that_fits_stays_on_one_line() {
    assert_eq!(wrapped("Stick", 11), ["Stick"]);
    assert_eq!(wrapped("", 11), [""]);
}

#[test]
fn a_word_longer_than_a_line_breaks_where_it_overflows() {
    // The character that opens the next line is not counted, as in the game.
    assert_eq!(wrapped("abcdefghij", 4), ["abcd", "efghi", "j"]);
}

#[test]
fn only_whitespace_ends_a_word() {
    assert_eq!(wrapped("well-known fact", 7), ["well-kn", "own fact"]);
}

#[test]
fn an_explicit_newline_starts_a_line_and_resets_the_width() {
    assert_eq!(wrapped("ab\ncd ef", 5), ["ab", "cd ef"]);
    assert_eq!(wrapped("ab\n\ncd\n", 5), ["ab", "", "cd", ""]);
}

#[test]
fn a_space_past_the_edge_takes_the_word_before_it_down_as_in_the_game() {
    assert_eq!(wrapped("aaa bbb c", 7), ["aaa ", "bbb c"]);
}

#[test]
fn no_room_at_all_still_ends_with_every_character_kept() {
    assert_eq!(counted("ab cd", 0., 1.).concat(), "ab cd");
}

#[test]
fn wrapping_ends_whatever_the_room_and_the_text() {
    let long = "x".repeat(40);
    let texts = [
        "",
        "   ",
        "   leading spaces",
        "\t\u{1}\tcontrol\u{7} chars\r\n\t",
        "aa   bb\n   \n  cc",
        "a word too long: xxxxxxxxxxxxxxxxxxxxxxxx yy",
        long.as_str(),
    ];
    for text in texts {
        for room in [0., -1., -100., f32::NAN, 0.5, 1., 3.] {
            for glyph in [1., 0.25] {
                let lines = counted(text, room, glyph);
                assert_eq!(lines.concat(), text.replace('\n', ""), "room {room}");
            }
        }
    }
}

#[test]
fn glyph_widths_decide_the_breaks() {
    let narrow_i = |ch: char| if ch == 'i' { 0.25 } else { 1. };
    assert_eq!(wrap_lines("iiii iiii", 3., narrow_i), ["iiii iiii"]);
    assert_eq!(wrap_lines("mmmm mmmm", 5., narrow_i), ["mmmm ", "mmmm"]);
}

#[test]
fn wrapped_lines_start_where_the_game_puts_them_whatever_the_column() {
    // The lines sit in a block as wide as the box less the inset, so the block fills the box.
    assert_eq!(
        [0, 1, 2].map(|column| wrapped_text_left(column, 10)),
        [10, 5, 0]
    );
    // A negative inset makes the block wider than the box, which the game then starts west.
    assert_eq!(
        [0, 1, 2].map(|column| wrapped_text_left(column, -10)),
        [-10, 0, 10]
    );
}

fn k(name: &str) -> PartKey {
    PartKey::new(format!("{UNDER}{name}"))
}

/// The stock HUD with one 60 by 60 unit label inside the health panel, keys `extra` added.
fn with_label(words: &str, extra: &[(&str, &str)]) -> HudModel {
    with_sized_label("60", words, extra)
}

/// As `with_label`, the label `wide` units wide.
fn with_sized_label(wide: &str, words: &str, extra: &[(&str, &str)]) -> HudModel {
    let text = format!("\"{words}\"");
    let mut keys = vec![
        ("xpos", "100"),
        ("ypos", "10"),
        ("wide", wide),
        ("tall", "60"),
        ("font", "HudFontSmall"),
        ("labelText", text.as_str()),
    ];
    keys.extend_from_slice(extra);
    let blocks = block("Tip", "CExLabel", &keys);
    let health = with_blocks(HEALTH, &blocks);
    let files = stock_files_with(&[("resource/ui/hudplayerhealth.res", Some(&health))]);
    HudModel::load(&files, Platform::Linux).unwrap()
}

/// The label's box wide, its font's height and its estimated glyph advance, at 1920x1080.
fn metrics(m: &HudModel) -> (i32, i32, f64) {
    let layout = m.layout(HD);
    let b = layout.get(&k("Tip")).unwrap();
    let scheme = m.scheme().font("HudFontSmall", HD.tall()).unwrap();
    let tall = scheme.pixel_tall(HD.tall());
    (b.wide(), tall, f64::from(tall) * GLYPH)
}

const LONG: &str = "aa bb cc dd ee ff gg";

#[test]
fn a_wrapped_labels_extent_is_its_broken_lines() {
    let m = with_label(LONG, &[("wrap", "1"), ("textAlignment", "north-west")]);
    let (wide, tall, glyph) = metrics(&m);
    let lines = wrap_lines(LONG, wide as f32, |_| glyph as f32);
    assert!(lines.len() > 1, "{lines:?}");
    let widest = (lines.iter().map(|l| l.trim_end().chars().count()).max()).unwrap();
    let layout = m.layout(HD);
    let b = layout.get(&k("Tip")).unwrap();
    let (x, y, w, h) = m
        .text_extent(&layout, &k("Tip"), &GameText::default())
        .unwrap();
    assert_eq!((x, y), (b.x(), b.y()));
    assert_eq!(w, (widest as f64 * glyph).ceil() as i32);
    assert_eq!(h, tall * lines.len() as i32);
}

#[test]
fn an_unwrapped_labels_extent_stays_one_line_clipped_to_its_box() {
    let m = with_label(LONG, &[("textAlignment", "north-west")]);
    let (wide, tall, _) = metrics(&m);
    let layout = m.layout(HD);
    let (_, _, w, h) = m
        .text_extent(&layout, &k("Tip"), &GameText::default())
        .unwrap();
    assert_eq!((w, h), (wide, tall));
}

#[test]
fn wrap_off_explicitly_counts_as_unwrapped() {
    let m = with_label(LONG, &[("wrap", "0"), ("textAlignment", "north-west")]);
    let (_, tall, _) = metrics(&m);
    let layout = m.layout(HD);
    let h = (m.text_extent(&layout, &k("Tip"), &GameText::default()))
        .unwrap()
        .3;
    assert_eq!(h, tall);
}

#[test]
fn centerwrap_wraps_alone_and_centres_the_lines_in_the_box() {
    let m = with_label(
        LONG,
        &[("centerwrap", "1"), ("textAlignment", "north-west")],
    );
    let (_, tall, _) = metrics(&m);
    let layout = m.layout(HD);
    let b = layout.get(&k("Tip")).unwrap();
    let (x, _, w, h) = m
        .text_extent(&layout, &k("Tip"), &GameText::default())
        .unwrap();
    assert!(h > tall, "{h}");
    assert!((2 * x + w - (2 * b.x() + b.wide())).abs() <= 1, "{x} {w}");
}

#[test]
fn plain_wrap_starts_at_the_west_edge_whatever_the_alignment() {
    let m = with_label("aa bb", &[("wrap", "1"), ("textAlignment", "east")]);
    let layout = m.layout(HD);
    let b = layout.get(&k("Tip")).unwrap();
    let x = (m.text_extent(&layout, &k("Tip"), &GameText::default()))
        .unwrap()
        .0;
    assert_eq!(x, b.x());
}

#[test]
fn a_wrapped_block_keeps_its_vertical_alignment() {
    let m = with_label(LONG, &[("wrap", "1"), ("textAlignment", "south-west")]);
    let layout = m.layout(HD);
    let b = layout.get(&k("Tip")).unwrap();
    let (_, y, _, h) = m
        .text_extent(&layout, &k("Tip"), &GameText::default())
        .unwrap();
    assert_eq!(y + h, b.y() + b.tall());
}

#[test]
fn the_x_inset_narrows_the_width_the_lines_break_in() {
    // Ten glyphs fit the 135 px box, four the 55 px left by the inset.
    let words = "aaaa bbbb cccc";
    let plain = with_label(words, &[("wrap", "1"), ("textAlignment", "north-west")]);
    let (_, tall, _) = metrics(&plain);
    let height = |m: &HudModel| {
        let layout = m.layout(HD);
        (m.text_extent(&layout, &k("Tip"), &GameText::default()))
            .unwrap()
            .3
    };
    assert_eq!(height(&plain), 2 * tall);
    let inset = with_label(
        words,
        &[
            ("wrap", "1"),
            ("textAlignment", "north-west"),
            ("textinsetx", "80"),
        ],
    );
    assert_eq!(height(&inset), 3 * tall);
}

#[test]
fn an_explicit_newline_counts_as_a_line_when_wrapped() {
    let m = with_label(r"aa\nbb", &[("wrap", "1")]);
    let (_, tall, _) = metrics(&m);
    let layout = m.layout(HD);
    let h = (m.text_extent(&layout, &k("Tip"), &GameText::default()))
        .unwrap()
        .3;
    assert_eq!(h, 2 * tall);
}

#[test]
fn a_wrapped_label_that_fits_on_screen_is_not_flagged_for_its_one_line_length() {
    // Two words that each fit, on two lines: only their short width lies on the screen, so
    // the box's far side hanging off the edge takes no text with it.
    let words = "aaaa bbbb";
    let m = with_sized_label("30", words, &[("wrap", "1")]);
    let (wide, _, glyph) = metrics(&m);
    assert!(4. * glyph < f64::from(wide) && 9. * glyph > f64::from(wide));
    let text = GameText::default();
    let baseline = m.screen_baseline(&text);
    // Moved so the box's right edge passes the 16:9 screen's while the first word stays on it.
    let layout = m.layout(HD);
    let b = layout.get(&k("Tip")).unwrap();
    let dx = HD.wide() as i32 - b.x() - wide * 7 / 8;
    let changes = m.move_part(&k("Tip"), dx, 0, HD, 1).unwrap();
    let moved = m.with_changes(&changes).into_model();
    let flagged = (moved.check_screens(&baseline, &text).into_iter())
        .find(|c| c.screen() == HD)
        .unwrap();
    assert!(flagged.flags().is_empty(), "{:?}", flagged.flags());
}
