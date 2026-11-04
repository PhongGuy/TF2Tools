//! How a label with `wrap` or `centerwrap` on breaks its text into lines, as the game's text
//! image does it. Widths come from the caller: real glyphs for drawing, an estimate for checks.

/// The lines `text` draws in a wrapped label `room` wide, with `width` giving each character's
/// advance. Lines keep their trailing spaces, as the game's do, and joined with the newlines
/// they lost they give `text` back.
pub fn wrap_lines(text: &str, room: f32, mut width: impl FnMut(char) -> f32) -> Vec<&str> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        break_paragraph(paragraph, room, &mut width, &mut lines);
    }
    lines
}

// A character past `room` ends the line, back at its word's start; a word that opened the line
// can never fit, so it breaks at that character, which the game leaves uncounted on the next
// line. A space keeps the last word's start, so one past the edge takes that word down too.
// Going back always lands after the line's start, so this ends.
fn break_paragraph<'a>(
    text: &'a str,
    room: f32,
    width: &mut impl FnMut(char) -> f32,
    lines: &mut Vec<&'a str>,
) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let (mut at, mut line_start, mut used) = (0, 0, 0.);
    let (mut word, mut in_word, mut line_is_fresh, mut word_opens_line) = (0, false, true, true);
    while let Some(&(_, ch)) = chars.get(at) {
        if ch.is_whitespace() {
            in_word = false;
        } else if !in_word {
            (word, in_word, word_opens_line) = (at, true, line_is_fresh);
        }
        if !ch.is_control() {
            line_is_fresh = false;
        }
        let advance = width(ch);
        if used + advance > room {
            let start = if word_opens_line { at } else { word };
            let byte = chars[start].0;
            lines.push(&text[line_start..byte]);
            line_start = byte;
            at = if word_opens_line { at + 1 } else { word };
            (used, in_word, line_is_fresh) = (0., false, true);
            continue;
        }
        used += advance;
        at += 1;
    }
    lines.push(&text[line_start..]);
}

/// Where a wrapped label's lines start, from its box's left edge: `column` is its
/// `textAlignment` column (0 west, 1 middle, 2 east) and `inset` its x text inset. The lines
/// sit in a block as wide as the box less the inset, which the alignment moves only by that
/// inset; a negative inset makes the block too wide for the box, and it starts at the west.
pub fn wrapped_text_left(column: usize, inset: i32) -> i32 {
    let block = match column {
        _ if inset < 0 => 0,
        1 => inset / 2,
        2 => inset,
        _ => 0,
    };
    match column {
        0 => block.saturating_add(inset),
        2 => block.saturating_sub(inset),
        _ => block,
    }
}
