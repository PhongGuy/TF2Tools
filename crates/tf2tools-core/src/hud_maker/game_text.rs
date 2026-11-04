//! The game's localised text: the `Tokens` block of a "lang" file such as `tf_english.txt`,
//! read with an escape-aware tokenizer because values hold `\"` that the lossy KeyValues
//! reader would desync on.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::fonts::Platform;
use crate::fonts::keyvalues::condition_holds;
use crate::hud_maker::HudFiles;

/// Bytes past this are never read; the stock `tf_english.txt` is about 3 MB.
const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// The game's localised text, token name (no `#`) to English, matched ignoring ASCII case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct GameText {
    texts: HashMap<String, String>,
}

impl GameText {
    /// The lang files `load` reads, in order; a later file's pair overrides an earlier one's.
    pub const FILES: [&'static str; 2] = ["resource/gameui_english.txt", "resource/tf_english.txt"];

    /// Every file of `FILES` that `files` holds, merged; a missing or unreadable file adds nothing.
    pub fn load(files: &HudFiles, platform: Platform) -> GameText {
        let mut texts = HashMap::new();
        for path in Self::FILES {
            if let Ok(Some(file)) = files.read(path) {
                texts.extend(GameText::parse(file.bytes(), platform).texts);
            }
        }
        GameText { texts }
    }

    /// Reads one "lang" file: UTF-16LE or UTF-16BE by byte order mark, else UTF-8 (BOM
    /// skipped). Pairs come from the `Tokens` block inside the first top-level block; a pair
    /// whose `[$COND]` fails for `platform` is skipped and a later pair overrides an earlier one.
    /// Never fails: malformed input yields whatever pairs could be read.
    pub fn parse(bytes: &[u8], platform: Platform) -> GameText {
        let source = decode(&bytes[..bytes.len().min(MAX_INPUT_BYTES)]);
        // One forward pass: enter the first top-level block, then its `Tokens` block, and
        // read that block's pairs; tokens are never all held at once.
        let mut tokens = tokenize(source.chars()).peekable();
        let mut texts = HashMap::new();
        if enter_block(&mut tokens, |_| true)
            && enter_block(&mut tokens, |key| key.eq_ignore_ascii_case("Tokens"))
        {
            while let Some(entry) = next_entry(&mut tokens) {
                match entry {
                    Entry::Block { .. } => skip_block(&mut tokens),
                    Entry::Pair { key, value, cond } => {
                        if cond.is_none_or(|cond| condition_holds(&cond, platform)) {
                            texts.insert(key.to_ascii_lowercase(), value);
                        }
                    }
                }
            }
        }
        GameText { texts }
    }

    /// The text for token `name` (without `#`), ignoring ASCII case.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.texts
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// `text` as a label draws it: `#name` with a known name becomes its text, an unknown
    /// `#name` stays, and hotkey markers are then hidden. For display only: what a file
    /// stores keeps its markers, so never write this back.
    pub fn show<'a>(&'a self, text: &'a str) -> Cow<'a, str> {
        let resolved = text
            .strip_prefix('#')
            .and_then(|name| self.get(name))
            .unwrap_or(text);
        without_hotkey_markers(resolved)
    }

    /// How many distinct tokens are known.
    pub fn len(&self) -> usize {
        self.texts.len()
    }

    /// Whether no token is known.
    pub fn is_empty(&self) -> bool {
        self.texts.is_empty()
    }
}

/// `text` without its hotkey markers: a lone `&` is dropped, even at the very end, and `&&`
/// draws one `&`. Text with no `&` is borrowed as it came.
fn without_hotkey_markers(text: &str) -> Cow<'_, str> {
    if !text.contains('&') {
        return Cow::Borrowed(text);
    }
    let mut shown = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        // The `&` of a pair is kept by consuming its twin; a lone one is simply not copied.
        if c != '&' || chars.next_if_eq(&'&').is_some() {
            shown.push(c);
        }
    }
    Cow::Owned(shown)
}

/// Picks the encoding from the byte order mark; bad units become U+FFFD and an odd trailing
/// byte is dropped, so no input panics.
fn decode(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], from: fn([u8; 2]) -> u16| {
        let units = rest.as_chunks::<2>().0.iter().map(|&pair| from(pair));
        char::decode_utf16(units)
            .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Open,
    Close,
    Text(String),
    Cond(String),
}

/// Splits KeyValues text into braces, strings (quoted with escapes, or bare words) and
/// `[...]` conditions, yielded one at a time; `//` comments are dropped only between tokens,
/// never inside a string.
fn tokenize(chars: impl Iterator<Item = char>) -> impl Iterator<Item = Token> {
    let mut chars = chars.peekable();
    std::iter::from_fn(move || {
        while let Some(c) = chars.next() {
            match c {
                c if c.is_whitespace() => {}
                '/' if chars.peek() == Some(&'/') => {
                    chars.find(|&c| c == '\n');
                }
                '{' => return Some(Token::Open),
                '}' => return Some(Token::Close),
                '"' => {
                    let mut text = String::new();
                    while let Some(c) = chars.next() {
                        match c {
                            '"' => break,
                            // Only the four known escapes decode; any other backslash, including
                            // one at the very end, stays literal.
                            '\\' => match chars.next() {
                                Some('n') => text.push('\n'),
                                Some('t') => text.push('\t'),
                                Some('\\') => text.push('\\'),
                                Some('"') => text.push('"'),
                                Some(other) => {
                                    text.push('\\');
                                    text.push(other);
                                }
                                None => text.push('\\'),
                            },
                            c => text.push(c),
                        }
                    }
                    return Some(Token::Text(text));
                }
                '[' => {
                    let mut cond = String::new();
                    for c in chars.by_ref() {
                        if c == ']' || c == '\n' {
                            break;
                        }
                        cond.push(c);
                    }
                    return Some(Token::Cond(cond));
                }
                c => {
                    let mut word = String::from(c);
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() || matches!(c, '{' | '}' | '"') {
                            break;
                        }
                        word.push(c);
                        chars.next();
                    }
                    return Some(Token::Text(word));
                }
            }
        }
        None
    })
}

enum Entry {
    /// `key {`; the stream is left just inside the braces.
    Block { key: String },
    Pair {
        key: String,
        value: String,
        cond: Option<String>,
    },
}

type Stream<I> = std::iter::Peekable<I>;

/// The next entry of the block body the stream is in, or `None` at its closing brace or the
/// end of input. Stray braces and conditions are stepped over so one malformed line cannot
/// end the walk early.
fn next_entry<I: Iterator<Item = Token>>(tokens: &mut Stream<I>) -> Option<Entry> {
    loop {
        match tokens.next()? {
            Token::Close => return None,
            Token::Open => skip_block(tokens),
            Token::Cond(_) => {}
            Token::Text(key) => match tokens.peek() {
                Some(Token::Open) => {
                    tokens.next();
                    return Some(Entry::Block { key });
                }
                Some(Token::Text(_)) => {
                    let Some(Token::Text(value)) = tokens.next() else {
                        continue;
                    };
                    let cond = tokens
                        .next_if(|token| matches!(token, Token::Cond(_)))
                        .and_then(|token| match token {
                            Token::Cond(cond) => Some(cond),
                            _ => None,
                        });
                    return Some(Entry::Pair { key, value, cond });
                }
                _ => {}
            },
        }
    }
}

/// Steps into the first block of the current body whose key passes `wanted`, skipping the
/// others; false when the body ends without one.
fn enter_block<I: Iterator<Item = Token>>(
    tokens: &mut Stream<I>,
    wanted: impl Fn(&str) -> bool,
) -> bool {
    while let Some(entry) = next_entry(tokens) {
        if let Entry::Block { key } = entry {
            if wanted(&key) {
                return true;
            }
            skip_block(tokens);
        }
    }
    false
}

/// Moves past the closing brace of the block the stream has just stepped inside. Brace
/// counting rather than recursion, so deep nesting cannot overflow the stack.
fn skip_block<I: Iterator<Item = Token>>(tokens: &mut Stream<I>) {
    let mut depth = 1usize;
    for token in tokens {
        match token {
            Token::Open => depth += 1,
            Token::Close => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_come_one_at_a_time() {
        // Reading past the first block panics, so only a tokenizer that yields on demand
        // passes; one that collects first would hold every token of a hostile file at once.
        let chars = "{ }"
            .chars()
            .chain(std::iter::from_fn(|| panic!("read past the first block")));
        let mut tokens = tokenize(chars);
        assert_eq!(tokens.next(), Some(Token::Open));
        assert_eq!(tokens.next(), Some(Token::Close));
    }
}
