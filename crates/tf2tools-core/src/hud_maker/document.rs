//! A KeyValues file kept byte for byte, so an untouched file writes back identical, with its
//! entries read as the engine reads them and single values editable in place.

use crate::error::KvParseError;
use crate::fonts::Platform;
use crate::fonts::keyvalues::{
    LexKind, Lexeme, MAX_BLOCK_DEPTH, MAX_FILE_BYTES, condition_holds, lex,
};

/// A KeyValues file held losslessly: its bytes, the lexemes that cover them and its entries.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct KvDocument {
    bytes: Vec<u8>,
    lexemes: Vec<Lexeme>,
    entries: Vec<KvEntry>,
}

/// One key and its value as the engine reads it, kept even when its conditionals fail.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct KvEntry {
    key: String,
    value: KvEntryValue,
    before: Option<String>,
    after: Option<String>,
    /// A top-level `#base`/`#include`, whose trailing conditional the engine never evaluates.
    directive: bool,
    line: u32,
    key_lexeme: usize,
    value_lexeme: usize,
}

/// An entry's value: a string or a nested block of entries.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum KvEntryValue {
    /// A quoted or bare string, decoded lossily.
    Text(String),
    /// A `{ ... }` block.
    Block(Vec<KvEntry>),
}

/// Names one value lexeme of one document; stays valid across [`KvDocument::set_text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KvValueId(usize);

impl KvEntry {
    /// The key as written, without quotes.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The value: text or a block.
    pub fn value(&self) -> &KvEntryValue {
        &self.value
    }

    /// The text value, or `None` for a block.
    pub fn text(&self) -> Option<&str> {
        match &self.value {
            KvEntryValue::Text(text) => Some(text),
            KvEntryValue::Block(_) => None,
        }
    }

    /// The block's entries, empty for a text value.
    pub fn children(&self) -> &[KvEntry] {
        match &self.value {
            KvEntryValue::Block(children) => children,
            KvEntryValue::Text(_) => &[],
        }
    }

    /// The last conditional's inner text, such as `$LINUX`, if the entry has one.
    pub fn cond(&self) -> Option<&str> {
        self.after.as_deref().or(self.before.as_deref())
    }

    /// Whether every conditional the engine evaluates holds on `platform`.
    pub fn holds(&self, platform: Platform) -> bool {
        let holds = |cond: Option<&str>| cond.is_none_or(|c| condition_holds(c, platform));
        holds(self.before.as_deref()) && (self.directive || holds(self.after.as_deref()))
    }

    /// The 1-based line of the key.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The id of the value's lexeme (the `{` for a block), for [`KvDocument::set_text`].
    pub fn value_id(&self) -> KvValueId {
        KvValueId(self.value_lexeme)
    }
}

impl KvDocument {
    /// Reads `bytes` as a KeyValues file. Any bytes are accepted except a file over 8 MiB
    /// ([`KvParseError::TooLarge`]), one starting with a UTF-16 byte order mark
    /// ([`KvParseError::Utf16`]) or blocks nested over 64 deep ([`KvParseError::TooDeep`]).
    pub fn parse(bytes: &[u8]) -> Result<KvDocument, KvParseError> {
        let size = bytes.len() as u64;
        if size > MAX_FILE_BYTES {
            return Err(KvParseError::TooLarge { bytes: size });
        }
        if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            return Err(KvParseError::Utf16);
        }
        let lexemes = lex(bytes);
        // A stray `}` at the top level closes nothing, so depth never drops below zero.
        let mut depth = 0usize;
        for lexeme in &lexemes {
            match lexeme.kind {
                LexKind::Open => {
                    depth += 1;
                    if depth > MAX_BLOCK_DEPTH {
                        return Err(KvParseError::TooDeep);
                    }
                }
                LexKind::Close => depth = depth.saturating_sub(1),
                LexKind::Bom
                | LexKind::Space
                | LexKind::Comment
                | LexKind::Quoted { .. }
                | LexKind::Bare
                | LexKind::Cond { .. } => {}
            }
        }
        Ok(Self::build(bytes.to_vec(), lexemes))
    }

    /// Builds the entry tree over already-lexed bytes whose depth is known to be in bounds.
    fn build(bytes: Vec<u8>, lexemes: Vec<Lexeme>) -> KvDocument {
        let entries = Builder::new(&bytes, &lexemes).entries(true);
        KvDocument {
            bytes,
            lexemes,
            entries,
        }
    }

    /// The document's bytes, exactly as they would be written.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The top-level entries in file order, directives included.
    pub fn entries(&self) -> &[KvEntry] {
        &self.entries
    }

    /// Entries reached by matching each name ignoring ASCII case at each level, in file order.
    pub fn find(&self, path: &[&str]) -> Vec<&KvEntry> {
        let Some((last, parents)) = path.split_last() else {
            return Vec::new();
        };
        let mut level: Vec<&KvEntry> = self.entries.iter().collect();
        for name in parents {
            level = level
                .into_iter()
                .filter(|e| e.key.eq_ignore_ascii_case(name))
                .flat_map(|e| e.children())
                .collect();
        }
        level.retain(|e| e.key.eq_ignore_ascii_case(last));
        level
    }

    /// Replaces one text value, quoting only when needed; false (no change) for a block value,
    /// an unknown id, or text containing `"`.
    pub fn set_text(&mut self, id: KvValueId, text: &str) -> bool {
        if text.contains('"') || !holds_text_value(&self.entries, id) {
            return false;
        }
        // A bare replacement must lex back as exactly one bare lexeme, so ids stay valid.
        let span = self.lexemes[id.0].span.clone();
        let was_quoted = matches!(self.lexemes[id.0].kind, LexKind::Quoted { .. });
        let quote = was_quoted
            || text.is_empty()
            || text
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '{' | '}' | '[' | '/'));
        let written = if quote {
            format!("\"{text}\"")
        } else {
            text.to_owned()
        };
        let mut bytes = std::mem::take(&mut self.bytes);
        bytes.splice(span, written.into_bytes());
        let lexemes = lex(&bytes);
        *self = Self::build(bytes, lexemes);
        true
    }

    /// Appends `"key" "value" [cond]` as the last entry of the last block on `block`; false (no
    /// change) if there is none or the key, value or cond holds `"`, CR or LF (or the cond `]`).
    pub fn insert_text(
        &mut self,
        block: &[&str],
        key: &str,
        value: &str,
        cond: Option<&str>,
    ) -> bool {
        self.last_block(block)
            .is_some_and(|id| self.insert_text_into(id, key, value, cond))
    }

    /// As [`KvDocument::insert_text`], into the block whose `{` is `block` (a block entry's
    /// [`KvEntry::value_id`]); false when `block` names no block.
    pub(crate) fn insert_text_into(
        &mut self,
        block: KvValueId,
        key: &str,
        value: &str,
        cond: Option<&str>,
    ) -> bool {
        if !writable(key, value, cond) {
            return false;
        }
        let Some(slot) = self.slot(block) else {
            return false;
        };
        let (at, written) = match slot {
            Slot::Lines { at, indent, eol } => {
                let cond = cond.map(|c| format!("\t[{c}]")).unwrap_or_default();
                (at, format!("{indent}\"{key}\"\t\"{value}\"{cond}{eol}"))
            }
            Slot::Inline { at, gap } => {
                let cond = cond.map(|c| format!("[{c}] ")).unwrap_or_default();
                (at, format!("{gap}\"{key}\" \"{value}\" {cond}"))
            }
        };
        self.splice_in(at, written);
        true
    }

    /// Deletes the text entry whose value is `id`: its key, conditionals and value. The whole
    /// line goes, newline included, when nothing else is on it. Otherwise the entry and the
    /// spaces and tabs after it on its line go, leaving one space when text would otherwise
    /// join across the gap. False (no change) for a block value or an unknown id. Every id
    /// taken before the call is invalid after it.
    pub fn remove_entry(&mut self, id: KvValueId) -> bool {
        let Some(entry) = text_entry(&self.entries, id) else {
            return false;
        };
        // The span runs from the key through the value and a trailing conditional, which the
        // builder takes as the next non-trivia lexeme after the value.
        let start = self.lexemes[entry.key_lexeme].span.start;
        let mut last = entry.value_lexeme;
        if let Some(next) = (last + 1..self.lexemes.len()).find(|&i| {
            !matches!(
                self.lexemes[i].kind,
                LexKind::Space | LexKind::Comment | LexKind::Bom
            )
        }) && matches!(self.lexemes[next].kind, LexKind::Cond { .. })
        {
            last = next;
        }
        let end = self.lexemes[last].span.end;
        let blank = |b: &u8| matches!(b, b' ' | b'\t');
        let line_start = self.bytes[..start]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        let after = end + self.bytes[end..].iter().take_while(|&b| blank(b)).count();
        let rest = &self.bytes[after..];
        let eol = if rest.starts_with(b"\r\n") {
            Some(2)
        } else if rest.starts_with(b"\n") || rest.is_empty() {
            Some(usize::from(!rest.is_empty()))
        } else {
            None
        };
        let span = match eol {
            Some(eol) if self.bytes[line_start..start].iter().all(blank) => line_start..after + eol,
            _ => start..after,
        };
        // Text left touching on both sides would lex as one token, so keep a space between.
        let glued = |b: Option<&u8>| b.is_some_and(|b| !b.is_ascii_whitespace());
        let joins = glued(span.start.checked_sub(1).and_then(|i| self.bytes.get(i)))
            && glued(self.bytes.get(span.end));
        let mut bytes = std::mem::take(&mut self.bytes);
        bytes.splice(span, joins.then_some(b' '));
        let lexemes = lex(&bytes);
        *self = Self::build(bytes, lexemes);
        true
    }

    /// Appends `"name" [cond] { "key" "value" [cond] ... }` as the last entry of the last block
    /// on `parent`, laid out like `insert_text`: on its own lines indented like the block's last
    /// child (a closing brace alone on its line), else spliced inline before the brace. False
    /// (no change) when there is no such block, the name or any cond is blank, or any name,
    /// key, value or cond fails `writable`.
    pub fn insert_block(
        &mut self,
        parent: &[&str],
        name: &str,
        cond: Option<&str>,
        lines: &[(&str, &str, Option<&str>)],
    ) -> bool {
        self.last_block(parent)
            .is_some_and(|id| self.insert_block_into(id, name, cond, lines))
    }

    /// As [`KvDocument::insert_block`], into the block whose `{` is `parent` (a block entry's
    /// [`KvEntry::value_id`]); false when `parent` names no block.
    pub(crate) fn insert_block_into(
        &mut self,
        parent: KvValueId,
        name: &str,
        cond: Option<&str>,
        lines: &[(&str, &str, Option<&str>)],
    ) -> bool {
        // A blank name or `[]` cond would write a block the engine cannot address or match.
        let blank_cond = |c: Option<&str>| c.is_some_and(|c| c.trim().is_empty());
        let ok = |k: &str, v: &str, c: Option<&str>| writable(k, v, c) && !blank_cond(c);
        if name.trim().is_empty()
            || !ok(name, "", cond)
            || !lines.iter().all(|&(k, v, c)| ok(k, v, c))
        {
            return false;
        }
        let Some(slot) = self.slot(parent) else {
            return false;
        };
        let (at, written) = match slot {
            Slot::Lines { at, indent, eol } => {
                let cond = cond.map(|c| format!("\t[{c}]")).unwrap_or_default();
                let mut out = format!("{indent}\"{name}\"{cond}{eol}{indent}{{{eol}");
                for (key, value, cond) in lines {
                    let cond = cond.map(|c| format!("\t[{c}]")).unwrap_or_default();
                    out.push_str(&format!("{indent}\t\"{key}\"\t\"{value}\"{cond}{eol}"));
                }
                out.push_str(&format!("{indent}}}{eol}"));
                (at, out)
            }
            Slot::Inline { at, gap } => {
                let cond = cond.map(|c| format!("[{c}] ")).unwrap_or_default();
                let mut out = format!("{gap}\"{name}\" {cond}{{ ");
                for (key, value, cond) in lines {
                    let cond = cond.map(|c| format!("[{c}] ")).unwrap_or_default();
                    out.push_str(&format!("\"{key}\" \"{value}\" {cond}"));
                }
                out.push_str("} ");
                (at, out)
            }
        };
        self.splice_in(at, written);
        true
    }

    /// Appends an empty top-level block `"name" { }` at the end of the file; false (no change)
    /// when the name is blank or not writable.
    pub(crate) fn append_root_block(&mut self, name: &str) -> bool {
        if name.trim().is_empty() || !writable(name, "", None) {
            return false;
        }
        let eol = if self.bytes.windows(2).any(|w| w == b"\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        // A last line without its newline would glue the block to it.
        let lead = match self.bytes.last() {
            Some(b'\n') | None => "",
            Some(_) => eol,
        };
        let at = self.bytes.len();
        self.splice_in(at, format!("{lead}\"{name}\"{eol}{{{eol}}}{eol}"));
        true
    }

    /// The `{` of the last block on `path`, if any.
    fn last_block(&self, path: &[&str]) -> Option<KvValueId> {
        (self.find(path).into_iter())
            .rfind(|e| e.text().is_none())
            .map(KvEntry::value_id)
    }

    /// Where new text goes in the block whose `{` is `block`. A `}` alone on its line takes new
    /// lines above it, indented like the block's last child; a block closed inline takes text
    /// just before the brace. `None` when there is no such block or it is unclosed.
    fn slot(&self, block: KvValueId) -> Option<Slot> {
        let target = block_entry(&self.entries, block)?;
        let close = self.closing_brace(target.value_lexeme)?;
        let at = self.lexemes[close].span.start;
        let line_start = self.bytes[..at]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        if self.bytes[line_start..at]
            .iter()
            .all(|b| matches!(b, b' ' | b'\t'))
        {
            let indent = match target.children().last() {
                Some(last) => self.indent_of_line(last.line),
                None => [&self.bytes[line_start..at], b"\t"].concat(),
            };
            let eol = if self.bytes.windows(2).any(|w| w == b"\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let indent = String::from_utf8_lossy(&indent).into_owned();
            Some(Slot::Lines {
                at: line_start,
                indent,
                eol,
            })
        } else {
            // A value glued to the brace (`"1"}`) needs a separator before the new text.
            let glued = at > 0 && !self.bytes[at - 1].is_ascii_whitespace();
            let gap = if glued { " " } else { "" };
            Some(Slot::Inline { at, gap })
        }
    }

    /// Splices `written` in at byte `at` and re-reads the document.
    /// Puts `"key"\t"value"` on its own line as the first entry of the block whose `{` is
    /// `block`, leaving every other byte as it was; false when `block` names no block or the
    /// text is not writable.
    pub(crate) fn insert_text_first(&mut self, block: KvValueId, key: &str, value: &str) -> bool {
        if !writable(key, value, None) {
            return false;
        }
        let Some(open) = self
            .lexemes
            .get(block.0)
            .filter(|l| l.kind == LexKind::Open)
        else {
            return false;
        };
        let after = open.span.end;
        let rest = &self.bytes[after..];
        let line_end = rest.iter().position(|&b| b == b'\n');
        // When the brace ends its line the entry goes on the next line; otherwise it opens one.
        let (at, written) = match line_end {
            Some(n) if rest[..n].iter().all(|b| b.is_ascii_whitespace()) => {
                let eol = if n > 0 && rest[n - 1] == b'\r' {
                    "\r\n"
                } else {
                    "\n"
                };
                (after + n + 1, format!("\t\"{key}\"\t\"{value}\"{eol}"))
            }
            _ => (after, format!("\n\t\"{key}\"\t\"{value}\"\n")),
        };
        self.splice_in(at, written);
        true
    }

    fn splice_in(&mut self, at: usize, written: String) {
        let mut bytes = std::mem::take(&mut self.bytes);
        bytes.splice(at..at, written.into_bytes());
        let lexemes = lex(&bytes);
        *self = Self::build(bytes, lexemes);
    }

    /// The lexeme index of the `}` matching the `{` at `open`, if the block is closed.
    fn closing_brace(&self, open: usize) -> Option<usize> {
        let mut depth = 0usize;
        for (i, lexeme) in self.lexemes.iter().enumerate().skip(open) {
            match lexeme.kind {
                LexKind::Open => depth += 1,
                LexKind::Close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                LexKind::Bom
                | LexKind::Space
                | LexKind::Comment
                | LexKind::Quoted { .. }
                | LexKind::Bare
                | LexKind::Cond { .. } => {}
            }
        }
        None
    }

    /// The leading spaces and tabs of the 1-based `line`.
    fn indent_of_line(&self, line: u32) -> Vec<u8> {
        let start = match line {
            0 | 1 => 0,
            n => self
                .bytes
                .iter()
                .enumerate()
                .filter(|(_, b)| **b == b'\n')
                .nth(n as usize - 2)
                .map_or(0, |(i, _)| i + 1),
        };
        self.bytes[start..]
            .iter()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .copied()
            .collect()
    }
}

/// Where `KvDocument::slot` places new text.
enum Slot {
    /// New lines at `at`, each starting with `indent` and ending with `eol`.
    Lines {
        at: usize,
        indent: String,
        eol: &'static str,
    },
    /// Inline text at `at`, after `gap`.
    Inline { at: usize, gap: &'static str },
}

/// Whether text can be written as quoted KeyValues without breaking out of its quotes or line:
/// no `"`, CR or LF anywhere, and no `]` in a conditional.
pub(super) fn writable(key: &str, value: &str, cond: Option<&str>) -> bool {
    let plain = |s: &str| !s.contains(['"', '\r', '\n']);
    plain(key) && plain(value) && cond.is_none_or(|c| plain(c) && !c.contains(']'))
}

fn holds_text_value(entries: &[KvEntry], id: KvValueId) -> bool {
    text_entry(entries, id).is_some()
}

/// The text entry whose value lexeme is `id`, searched through every block.
fn text_entry(entries: &[KvEntry], id: KvValueId) -> Option<&KvEntry> {
    entries.iter().find_map(|e| match &e.value {
        KvEntryValue::Text(_) => (e.value_lexeme == id.0).then_some(e),
        KvEntryValue::Block(children) => text_entry(children, id),
    })
}

/// The block entry whose `{` lexeme is `id`, searched through every block.
fn block_entry(entries: &[KvEntry], id: KvValueId) -> Option<&KvEntry> {
    entries.iter().find_map(|e| match &e.value {
        KvEntryValue::Text(_) => None,
        KvEntryValue::Block(children) => match e.value_lexeme == id.0 {
            true => Some(e),
            false => block_entry(children, id),
        },
    })
}

/// Reads entries over the non-trivia lexemes with the control flow of the lossy parser's
/// `Parser::pairs`, keeping entries whose conditionals fail.
struct Builder<'a> {
    bytes: &'a [u8],
    lexemes: &'a [Lexeme],
    /// Indexes of the lexemes that are not whitespace, comments or the BOM.
    tokens: Vec<usize>,
    at: usize,
    /// Newlines seen before `line_at`, so line numbers are counted in one forward pass.
    line: u32,
    line_at: usize,
}

impl<'a> Builder<'a> {
    fn new(bytes: &'a [u8], lexemes: &'a [Lexeme]) -> Self {
        let tokens = (0..lexemes.len())
            .filter(|&i| {
                !matches!(
                    lexemes[i].kind,
                    LexKind::Bom | LexKind::Space | LexKind::Comment
                )
            })
            .collect();
        Builder {
            bytes,
            lexemes,
            tokens,
            at: 0,
            line: 1,
            line_at: 0,
        }
    }

    fn kind(&self, at: usize) -> Option<LexKind> {
        self.tokens.get(at).map(|&i| self.lexemes[i].kind)
    }

    /// The lexeme's text with its quotes or brackets stripped, decoded lossily.
    fn text(&self, index: usize) -> String {
        let Lexeme { kind, span } = &self.lexemes[index];
        let (start, end) = match *kind {
            LexKind::Quoted { closed } | LexKind::Cond { closed } => {
                (span.start + 1, span.end - usize::from(closed))
            }
            LexKind::Bom
            | LexKind::Space
            | LexKind::Comment
            | LexKind::Bare
            | LexKind::Open
            | LexKind::Close => (span.start, span.end),
        };
        String::from_utf8_lossy(&self.bytes[start..end]).into_owned()
    }

    fn line_of(&mut self, index: usize) -> u32 {
        let start = self.lexemes[index].span.start;
        let newlines = self.bytes[self.line_at..start]
            .iter()
            .filter(|&&b| b == b'\n')
            .count();
        self.line += newlines as u32;
        self.line_at = start;
        self.line
    }

    fn next_cond(&mut self) -> Option<String> {
        if let Some(LexKind::Cond { .. }) = self.kind(self.at) {
            self.at += 1;
            Some(self.text(self.tokens[self.at - 1]))
        } else {
            None
        }
    }

    /// Reads entries until the block's `}` or the end. At top level a stray `}` is skipped;
    /// inside a block it closes the block. A key with no value gives no entry.
    fn entries(&mut self, top: bool) -> Vec<KvEntry> {
        let mut entries = Vec::new();
        while let Some(kind) = self.kind(self.at) {
            let key_index = self.tokens[self.at];
            self.at += 1;
            match kind {
                LexKind::Quoted { .. } | LexKind::Bare => {}
                LexKind::Close if top => continue,
                LexKind::Close => break,
                LexKind::Open => {
                    self.entries(false);
                    continue;
                }
                LexKind::Bom | LexKind::Space | LexKind::Comment | LexKind::Cond { .. } => continue,
            }
            let line = self.line_of(key_index);
            let key = self.text(key_index);
            let before = self.next_cond();
            let value_lexeme = match self.kind(self.at) {
                Some(LexKind::Quoted { .. } | LexKind::Bare | LexKind::Open) => {
                    self.tokens[self.at]
                }
                _ => continue,
            };
            self.at += 1;
            let value = if self.lexemes[value_lexeme].kind == LexKind::Open {
                KvEntryValue::Block(self.entries(false))
            } else {
                KvEntryValue::Text(self.text(value_lexeme))
            };
            let directive = top
                && matches!(value, KvEntryValue::Text(_))
                && (key.eq_ignore_ascii_case("#base") || key.eq_ignore_ascii_case("#include"));
            let after = self.next_cond();
            entries.push(KvEntry {
                key,
                value,
                before,
                after,
                directive,
                line,
                key_lexeme: key_index,
                value_lexeme,
            });
        }
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::Platform;
    use crate::fonts::keyvalues::{self, KvDirective, KvPair, KvValue};

    type Row = (usize, String, Option<String>);

    fn parse_rows(pairs: &[KvPair], depth: usize, out: &mut Vec<Row>) {
        for pair in pairs {
            match &pair.value {
                KvValue::Text(text) => out.push((depth, pair.key.clone(), Some(text.clone()))),
                KvValue::Block(children) => {
                    out.push((depth, pair.key.clone(), None));
                    parse_rows(children, depth + 1, out);
                }
            }
        }
    }

    fn is_directive(entry: &KvEntry) -> bool {
        entry.text().is_some()
            && (entry.key().eq_ignore_ascii_case("#base")
                || entry.key().eq_ignore_ascii_case("#include"))
    }

    fn doc_rows(entries: &[KvEntry], depth: usize, platform: Platform, out: &mut Vec<Row>) {
        for entry in entries {
            if (depth == 0 && is_directive(entry)) || !entry.holds(platform) {
                continue;
            }
            out.push((
                depth,
                entry.key().to_owned(),
                entry.text().map(str::to_owned),
            ));
            doc_rows(entry.children(), depth + 1, platform, out);
        }
    }

    #[test]
    fn entries_agree_with_the_lossy_parser() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hud/kv");
        let mut inputs: Vec<Vec<u8>> = [
            "jofrehud-basechat.res",
            "jofrehud-huddamageaccount.res",
            "budhud-hudteamgoaltournament.res",
        ]
        .iter()
        .map(|name| std::fs::read(dir.join(name)).unwrap())
        .collect();
        inputs.extend([
            b"}}} \"stray\" { \"k\" } \"dangling\"".to_vec(),
            b"\"P\" { \"wide\" \"10\" [$_disabled_] \"tall\" [$WIN32] \"5\" [$X\n { \"x\" \"y\" } [$LINUX] }".to_vec(),
            b"#base \"a.res\" [$X360]\n\"S\" { \"#base\" \"nested\" [$X360] \"k\" { } }".to_vec(),
            b"\"latin1\" \"caf\xe9\"".to_vec(),
        ]);
        for bytes in &inputs {
            let doc = KvDocument::parse(bytes).unwrap();
            for platform in [Platform::Linux, Platform::Windows] {
                let file = keyvalues::parse(&String::from_utf8_lossy(bytes), platform);
                let (mut want, mut got) = (Vec::new(), Vec::new());
                parse_rows(&file.pairs, 0, &mut want);
                doc_rows(doc.entries(), 0, platform, &mut got);
                assert_eq!(got, want, "{}", String::from_utf8_lossy(bytes));
                let want_dirs: Vec<&str> = file
                    .directives
                    .iter()
                    .map(|d| match d {
                        KvDirective::Base(t) | KvDirective::Include(t) => t.as_str(),
                    })
                    .collect();
                let got_dirs: Vec<&str> = doc
                    .entries()
                    .iter()
                    .filter(|e| is_directive(e))
                    .filter_map(|e| e.text())
                    .collect();
                assert_eq!(got_dirs, want_dirs);
            }
        }
    }
}
