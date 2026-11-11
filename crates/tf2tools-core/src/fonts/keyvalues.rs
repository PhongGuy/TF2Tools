//! A lenient reader for Valve KeyValues (`.res`) files as HUDs ship them: no escapes,
//! `[$COND]` conditionals, case-insensitive paths, and `#base`/`#include` files combined by the
//! engine's rules. Malformed input is
//! skipped rather than rejected, since the game itself accepts it.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::Platform;

/// How many `#base`/`#include` levels below the start file are followed.
pub(crate) const MAX_INCLUDE_DEPTH: usize = 16;

/// Blocks nested deeper than this are skipped whole, so hostile input cannot exhaust the stack.
pub(crate) const MAX_BLOCK_DEPTH: usize = 64;

/// Files larger than this are skipped, never read whole.
pub(crate) const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// A pair's value: a string or a nested block.
pub(crate) enum KvValue {
    Text(String),
    Block(Vec<KvPair>),
}

/// One key and its value, conditionals already applied.
pub(crate) struct KvPair {
    pub(crate) key: String,
    pub(crate) value: KvValue,
}

/// A top-level `#base` or `#include` line, in file order.
pub(crate) enum KvDirective {
    Base(String),
    Include(String),
}

/// One file: its pairs with false conditionals dropped, and its directives in file order.
pub(crate) struct KvFile {
    pub(crate) pairs: Vec<KvPair>,
    pub(crate) directives: Vec<KvDirective>,
}

enum Token {
    Str(String),
    Open,
    Close,
    Cond(String),
}

/// What one lexeme of a KeyValues file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexKind {
    /// The UTF-8 byte order mark at offset 0.
    Bom,
    /// A run of C `isspace` bytes.
    Space,
    /// `//` up to, not including, the next newline.
    Comment,
    /// A `"` string; unclosed when it runs to the end of the file.
    Quoted { closed: bool },
    /// An unquoted word.
    Bare,
    /// `{`.
    Open,
    /// `}`.
    Close,
    /// A `[condition]`; unclosed when a newline or the end cuts it short.
    Cond { closed: bool },
}

/// One lexeme and the bytes it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Lexeme {
    pub(crate) kind: LexKind,
    pub(crate) span: std::ops::Range<usize>,
}

/// The engine's token separators: C `isspace`, which unlike `char::is_whitespace` excludes
/// Unicode spaces such as U+00A0.
fn is_c_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Splits `bytes` into lexemes that cover every byte once, in order.
pub(crate) fn lex(bytes: &[u8]) -> Vec<Lexeme> {
    let mut lexemes = Vec::new();
    let mut at = 0;
    if bytes.starts_with(b"\xef\xbb\xbf") {
        lexemes.push(Lexeme {
            kind: LexKind::Bom,
            span: 0..3,
        });
        at = 3;
    }
    // Each arm finds where its lexeme ends; delimiters are ASCII, so spans of UTF-8 input
    // always fall on char boundaries.
    let find = |from: usize, stop: &dyn Fn(u8) -> bool| {
        bytes[from..]
            .iter()
            .position(|&b| stop(b))
            .map_or(bytes.len(), |i| from + i)
    };
    while at < bytes.len() {
        let (kind, end) = match bytes[at] {
            b if is_c_space(b) => (LexKind::Space, find(at, &|b| !is_c_space(b))),
            b'{' => (LexKind::Open, at + 1),
            b'}' => (LexKind::Close, at + 1),
            b'/' if bytes[at..].starts_with(b"//") => (LexKind::Comment, find(at, &|b| b == b'\n')),
            b'"' => {
                let end = find(at + 1, &|b| b == b'"');
                let closed = end < bytes.len();
                (LexKind::Quoted { closed }, end + usize::from(closed))
            }
            b'[' => {
                let end = find(at + 1, &|b| b == b']' || b == b'\n');
                let closed = bytes.get(end) == Some(&b']');
                (LexKind::Cond { closed }, end + usize::from(closed))
            }
            _ => {
                let stop = |b: u8| is_c_space(b) || matches!(b, b'{' | b'}' | b'"');
                (LexKind::Bare, find(at + 1, &stop))
            }
        };
        lexemes.push(Lexeme {
            kind,
            span: at..end,
        });
        at = end;
    }
    lexemes
}

/// Splits text into strings, braces and `[conditions]`, dropping `//` comments outside quotes.
/// A quoted string runs to the next `"` with no escapes, or to the end of the text.
fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let text_of = |span: std::ops::Range<usize>| String::from_utf8_lossy(&bytes[span]).into_owned();
    // Quotes and brackets are stripped by trimming the opening byte and, when present, the closing one.
    let inner = |span: &std::ops::Range<usize>, closed: bool| {
        text_of(span.start + 1..span.end - usize::from(closed))
    };
    lex(bytes)
        .into_iter()
        .filter_map(|lexeme| match lexeme.kind {
            LexKind::Bom | LexKind::Space | LexKind::Comment => None,
            LexKind::Open => Some(Token::Open),
            LexKind::Close => Some(Token::Close),
            LexKind::Bare => Some(Token::Str(text_of(lexeme.span))),
            LexKind::Quoted { closed } => Some(Token::Str(inner(&lexeme.span, closed))),
            LexKind::Cond { closed } => Some(Token::Cond(inner(&lexeme.span, closed))),
        })
        .collect()
}

/// Evaluates one `[...]` conditional as source-sdk-2013 `EvaluateConditional` does.
pub(crate) fn condition_holds(expr: &str, platform: Platform) -> bool {
    // The engine takes one leading `!`, then lets the first symbol found anywhere in the text,
    // in its fixed order, decide; `&&` and `||` mean nothing to it. No symbol is false.
    let expr = expr.trim();
    let expr = expr.strip_prefix('[').unwrap_or(expr);
    let negated = expr.starts_with('!');
    let upper = expr.to_ascii_uppercase();
    let windows = platform == Platform::Windows;
    let symbols = [
        ("$DECK", false),
        ("$X360", false),
        ("$WIN32", true),
        ("$WINDOWS", windows),
        ("$OSX", false),
        ("$LINUX", !windows),
        ("$POSIX", !windows),
    ];
    symbols
        .iter()
        .find(|(symbol, _)| upper.contains(symbol))
        .is_some_and(|&(_, holds)| holds != negated)
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    platform: Platform,
}

impl Parser<'_> {
    fn next_cond(&mut self) -> bool {
        if let Some(Token::Cond(expr)) = self.tokens.get(self.at) {
            self.at += 1;
            condition_holds(expr, self.platform)
        } else {
            true
        }
    }

    /// Skips the rest of a block whose `{` was just consumed, counting braces in a plain loop.
    fn skip_block(&mut self) {
        let mut open = 1usize;
        while let Some(token) = self.tokens.get(self.at) {
            self.at += 1;
            match token {
                Token::Open => open += 1,
                Token::Close => open -= 1,
                Token::Str(_) | Token::Cond(_) => {}
            }
            if open == 0 {
                return;
            }
        }
    }

    /// Reads the block whose `{` was just consumed, or skips it past `MAX_BLOCK_DEPTH`.
    fn block(&mut self, depth: usize) -> Option<Vec<KvPair>> {
        if depth >= MAX_BLOCK_DEPTH {
            self.skip_block();
            None
        } else {
            Some(self.pairs(depth + 1))
        }
    }

    /// Reads pairs until the block's `}` or the end. At top level (depth 0) a stray `}` is
    /// skipped; inside a block it closes the block. A key with no value is dropped, and so is
    /// a key whose block is too deep.
    fn pairs(&mut self, depth: usize) -> Vec<KvPair> {
        let top = depth == 0;
        let mut pairs = Vec::new();
        while let Some(token) = self.tokens.get(self.at) {
            self.at += 1;
            let key = match token {
                Token::Str(key) => key.clone(),
                Token::Close if top => continue,
                Token::Close => break,
                Token::Open => {
                    self.block(depth);
                    continue;
                }
                Token::Cond(_) => continue,
            };
            let before = self.next_cond();
            let value = match self.tokens.get(self.at) {
                Some(Token::Str(text)) => {
                    self.at += 1;
                    KvValue::Text(text.clone())
                }
                Some(Token::Open) => {
                    self.at += 1;
                    match self.block(depth) {
                        Some(children) => KvValue::Block(children),
                        None => continue,
                    }
                }
                _ => continue,
            };
            // The engine reads only the target after a top-level #base or #include and never
            // evaluates a trailing conditional, so the directive always applies.
            let directive = top
                && matches!(value, KvValue::Text(_))
                && (key.eq_ignore_ascii_case("#base") || key.eq_ignore_ascii_case("#include"));
            let after = self.next_cond() || directive;
            if before && after {
                pairs.push(KvPair { key, value });
            }
        }
        pairs
    }
}

/// Parses one file's text for `platform`, splitting top-level `#base`/`#include` from its pairs.
pub(crate) fn parse(text: &str, platform: Platform) -> KvFile {
    let tokens = tokenize(text);
    let all = Parser {
        tokens: &tokens,
        at: 0,
        platform,
    }
    .pairs(0);
    let mut file = KvFile {
        pairs: Vec::new(),
        directives: Vec::new(),
    };
    for pair in all {
        let base = pair.key.eq_ignore_ascii_case("#base");
        let include = pair.key.eq_ignore_ascii_case("#include");
        match pair.value {
            KvValue::Text(target) if base => file.directives.push(KvDirective::Base(target)),
            KvValue::Text(target) if include => file.directives.push(KvDirective::Include(target)),
            value @ KvValue::Text(_) | value @ KvValue::Block(_) => file.pairs.push(KvPair {
                key: pair.key,
                value,
            }),
        }
    }
    file
}

/// Writes `pairs` as KeyValues text: every key and value quoted, blocks braced, tabs for depth.
/// Nothing is escaped, since the lexer has no escapes and a parsed value never holds a quote.
pub(crate) fn write_pairs(pairs: &[KvPair]) -> String {
    fn write(pairs: &[KvPair], depth: usize, out: &mut String) {
        let tabs = "\t".repeat(depth);
        for pair in pairs {
            match &pair.value {
                KvValue::Text(text) => {
                    out.push_str(&format!("{tabs}\"{}\" \"{text}\"\n", pair.key))
                }
                KvValue::Block(inner) => {
                    out.push_str(&format!("{tabs}\"{}\"\n{tabs}{{\n", pair.key));
                    write(inner, depth + 1, out);
                    out.push_str(&format!("{tabs}}}\n"));
                }
            }
        }
    }
    let mut out = String::new();
    write(pairs, 0, &mut out);
    out
}

/// The children of a block value.
pub(crate) fn block(pair: &KvPair) -> Option<&[KvPair]> {
    match &pair.value {
        KvValue::Block(children) => Some(children),
        KvValue::Text(_) => None,
    }
}

/// The first block named `name`, ignoring case.
pub(crate) fn block_named<'a>(pairs: &'a [KvPair], name: &str) -> Option<&'a [KvPair]> {
    pairs
        .iter()
        .find(|pair| pair.key.eq_ignore_ascii_case(name))
        .and_then(block)
}

/// The first text value named `name`, ignoring case.
pub(crate) fn text<'a>(pairs: &'a [KvPair], name: &str) -> Option<&'a str> {
    pairs.iter().find_map(|pair| match &pair.value {
        KvValue::Text(t) if pair.key.eq_ignore_ascii_case(name) => Some(t.as_str()),
        KvValue::Text(_) | KvValue::Block(_) => None,
    })
}

/// `start` combined with its `#base`/`#include` files, at most 16 levels below `start`: each
/// `#base` file's first block merges into the file's first block (own keys win, earlier bases
/// beat later ones), and each `#include` file's pairs follow the file's own. A `start` that is
/// missing, outside `root` or over 8 MB is `Ok(vec![])`; any other read error on `start` is
/// `Err`. Other files that are missing, unreadable, over 8 MB, already visited or outside `root`
/// are skipped silently. `root` must be canonical.
pub(crate) fn load(root: &Path, start: &Path, platform: Platform) -> io::Result<Vec<KvPair>> {
    let start = match start.canonicalize() {
        Ok(path) => path,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    if !start.starts_with(root) {
        return Ok(Vec::new());
    }
    let Some(text) = read_capped(&start)? else {
        return Ok(Vec::new());
    };
    Ok(load_from(&FsSource { root }, &start, &text, platform))
}

/// Where `#base`/`#include` targets are found and read: the filesystem or the game's files.
pub(crate) trait KvSource {
    /// A file's identity, used to stop include loops.
    type Id: Clone + Eq + std::hash::Hash;
    /// The file `rel` names relative to the directory holding `from`, if it exists.
    fn resolve(&self, from: &Self::Id, rel: &str) -> Option<Self::Id>;
    /// The file's bytes; `None` when unreadable or over the size cap.
    fn read(&self, id: &Self::Id) -> Option<Vec<u8>>;
}

/// Files under a canonical `root`, found case-insensitively and named by canonical path.
struct FsSource<'a> {
    root: &'a Path,
}

impl KvSource for FsSource<'_> {
    type Id = PathBuf;

    fn resolve(&self, from: &PathBuf, rel: &str) -> Option<PathBuf> {
        resolve_ci(self.root, from.parent().unwrap_or(self.root), rel)
    }

    fn read(&self, id: &PathBuf) -> Option<Vec<u8>> {
        read_capped(id).ok().flatten()
    }
}

/// `start` (already read as `bytes`) merged with its `#base`/`#include` chain as `load` documents.
pub(crate) fn load_from<S: KvSource>(
    source: &S,
    start: &S::Id,
    bytes: &[u8],
    platform: Platform,
) -> Vec<KvPair> {
    let mut visited = HashSet::from([start.clone()]);
    load_text(source, start, bytes, platform, 0, &mut visited)
}

/// One already-read file combined with its directives' files as `load` describes; the visited
/// set of file ids stops loops, and the depth cap stops runaway chains.
fn load_text<S: KvSource>(
    source: &S,
    path: &S::Id,
    bytes: &[u8],
    platform: Platform,
    depth: usize,
    visited: &mut HashSet<S::Id>,
) -> Vec<KvPair> {
    let file = parse(&String::from_utf8_lossy(bytes), platform);
    let mut own = file.pairs.into_iter();
    let mut first = own.next();
    let mut included = Vec::new();
    if depth < MAX_INCLUDE_DEPTH {
        // Directives load in file order so the visited set marks files as the engine reaches them.
        // Unlike the engine, a file expands only where first reached, even if reached again later.
        for directive in &file.directives {
            let (KvDirective::Base(target) | KvDirective::Include(target)) = directive;
            let Some(found) = source.resolve(path, target) else {
                continue;
            };
            if !visited.insert(found.clone()) {
                continue;
            }
            let Some(bytes) = source.read(&found) else {
                continue;
            };
            let loaded = load_text(source, &found, &bytes, platform, depth + 1, visited);
            match directive {
                KvDirective::Base(_) => merge_base(&mut first, loaded),
                KvDirective::Include(_) => included.extend(loaded),
            }
        }
    }
    first.into_iter().chain(own).chain(included).collect()
}

/// Merges a loaded base file into the including file's first pair, as `MergeBaseKeys` does:
/// only the base's first block counts, and it becomes the first pair when the file has none.
fn merge_base(first: &mut Option<KvPair>, base: Vec<KvPair>) {
    let Some(base) = base.into_iter().next() else {
        return;
    };
    let KvValue::Block(theirs) = base.value else {
        return;
    };
    match first {
        None => {
            *first = Some(KvPair {
                key: base.key,
                value: KvValue::Block(theirs),
            })
        }
        Some(KvPair {
            value: KvValue::Block(mine),
            ..
        }) => merge_children(mine, theirs),
        Some(_) => {}
    }
}

/// Merges `base` into `own` as `RecursiveMergeKeyValues` does: a key `own` lacks is appended,
/// two blocks of one name merge recursively, and otherwise the first copy in `own` wins. Names
/// match ignoring ASCII case, as the engine's key symbols do, and the first-seen spelling stays.
fn merge_children(own: &mut Vec<KvPair>, base: Vec<KvPair>) {
    // Index each lowercased name's first copy once, and register appended keys, so merging
    // stays linear per level and a later duplicate in the base merges into its first copy.
    let mut first: HashMap<String, usize> = HashMap::new();
    for (at, pair) in own.iter().enumerate() {
        first.entry(pair.key.to_ascii_lowercase()).or_insert(at);
    }
    for pair in base {
        match first.get(&pair.key.to_ascii_lowercase()) {
            Some(&at) => {
                if let (KvValue::Block(mine), KvValue::Block(theirs)) =
                    (&mut own[at].value, pair.value)
                {
                    merge_children(mine, theirs);
                }
            }
            None => {
                first.insert(pair.key.to_ascii_lowercase(), own.len());
                own.push(pair);
            }
        }
    }
}

fn read_capped(path: &Path) -> io::Result<Option<Vec<u8>>> {
    super::read_regular_capped(path, MAX_FILE_BYTES)
}

/// Resolves `rel` (`/` or `\` separated, `.`/`..`/empty segments allowed) against the directory
/// `base`, each segment exact first, then by a case-insensitive directory scan. Returns the
/// canonical path, or `None` when absent or when it lands outside the canonical `root`.
pub(crate) fn resolve_ci(root: &Path, base: &Path, rel: &str) -> Option<PathBuf> {
    // `..` pops lexically and may never leave `root`; a directory is listed only once its
    // canonical form is inside `root`, and the final canonical check catches symlink escapes.
    if !base.starts_with(root) {
        return None;
    }
    let mut current = base.to_path_buf();
    for segment in rel.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." => {
                if !current.pop() || !current.starts_with(root) {
                    return None;
                }
            }
            name => {
                let exact = current.join(name);
                if fs::symlink_metadata(&exact).is_ok() {
                    current = exact;
                    continue;
                }
                if !current.canonicalize().ok()?.starts_with(root) {
                    return None;
                }
                let found = fs::read_dir(&current).ok()?.flatten().find(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(|entry| entry.eq_ignore_ascii_case(name))
                })?;
                current = found.path();
            }
        }
    }
    let canonical = current.canonicalize().ok()?;
    canonical.starts_with(root).then_some(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn texts(pairs: &[KvPair], key: &str) -> Vec<String> {
        let mut out = Vec::new();
        for pair in pairs {
            match &pair.value {
                KvValue::Text(t) if pair.key.eq_ignore_ascii_case(key) => out.push(t.clone()),
                KvValue::Text(_) => {}
                KvValue::Block(children) => out.extend(texts(children, key)),
            }
        }
        out
    }

    fn put(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Writes `files` under a fresh root and loads `resource/clientscheme.res` for Linux.
    fn load_tree(files: &[(&str, &str)]) -> Vec<KvPair> {
        let tmp = tempfile::tempdir().unwrap();
        for (rel, text) in files {
            put(tmp.path(), rel, text);
        }
        let root = tmp.path().canonicalize().unwrap();
        load(
            &root,
            &root.join("resource/clientscheme.res"),
            Platform::Linux,
        )
        .unwrap()
    }

    fn top_keys(pairs: &[KvPair]) -> Vec<&str> {
        pairs.iter().map(|p| p.key.as_str()).collect()
    }

    /// The children of the `nth` block named exactly `key` among `pairs`.
    fn block<'a>(pairs: &'a [KvPair], key: &str, nth: usize) -> &'a [KvPair] {
        pairs
            .iter()
            .filter_map(|p| match &p.value {
                KvValue::Block(children) if p.key == key => Some(children.as_slice()),
                KvValue::Text(_) | KvValue::Block(_) => None,
            })
            .nth(nth)
            .unwrap_or_else(|| panic!("no block {key} #{nth}"))
    }

    #[test]
    fn own_keys_beat_their_base() {
        let pairs = load_tree(&[
            (
                "resource/clientscheme.res",
                r#"#base "base.res"
"Scheme" { "Fonts" { "Default" { "1" { "name" "Own" } } } }"#,
            ),
            (
                "resource/base.res",
                r#""Scheme" { "Fonts" {
    "Default" { "1" { "name" "Base" "tall" "12" } }
    "Extra" { "1" { "name" "FromBase" } } } }"#,
            ),
        ]);
        assert_eq!(texts(&pairs, "name"), ["Own", "FromBase"]);
        assert_eq!(
            texts(&pairs, "tall"),
            ["12"],
            "a key only the base has is copied in"
        );
        assert_eq!(top_keys(&pairs), ["Scheme"]);
    }

    #[test]
    fn an_earlier_base_beats_a_later_one() {
        let pairs = load_tree(&[
            (
                "resource/clientscheme.res",
                "#base \"a.res\"\n#base \"b.res\"\n\"Scheme\" { }",
            ),
            ("resource/a.res", r#""Scheme" { "k" "A" }"#),
            ("resource/b.res", r#""Scheme" { "k" "B" "only_b" "B2" }"#),
        ]);
        assert_eq!(texts(&pairs, "k"), ["A"]);
        assert_eq!(texts(&pairs, "only_b"), ["B2"]);
    }

    #[test]
    fn the_first_duplicate_in_one_file_wins() {
        let pairs = load_tree(&[
            (
                "resource/clientscheme.res",
                r#"#base "base.res"
"Scheme" { "B" { "x" "1" } "B" { "x" "2" } }"#,
            ),
            (
                "resource/base.res",
                r#""Scheme" { "k" "First" "k" "Second" "B" { "y" "base" } }"#,
            ),
        ]);
        let scheme = block(&pairs, "Scheme", 0);
        assert_eq!(
            texts(scheme, "k"),
            ["First"],
            "a base duplicate merges into the first copy"
        );
        assert_eq!(top_keys(block(scheme, "B", 0)), ["x", "y"]);
        assert_eq!(top_keys(block(scheme, "B", 1)), ["x"]);
    }

    #[test]
    fn nested_bases_merge_inner_files_first() {
        let pairs = load_tree(&[
            (
                "resource/clientscheme.res",
                "#base \"a.res\"\n\"S\" { \"k1\" \"own\" }",
            ),
            (
                "resource/a.res",
                "#base \"b.res\"\n\"S\" { \"k1\" \"a\" \"k2\" \"a\" }",
            ),
            ("resource/b.res", r#""S" { "k1" "b" "k2" "b" "k3" "b" }"#),
        ]);
        assert_eq!(texts(&pairs, "k1"), ["own"]);
        assert_eq!(texts(&pairs, "k2"), ["a"]);
        assert_eq!(texts(&pairs, "k3"), ["b"]);
    }

    #[test]
    fn a_missing_base_is_ignored() {
        let pairs = load_tree(&[
            (
                "resource/clientscheme.res",
                "#base \"missing.res\"\n#base \"real.res\"\n\"S\" { \"k\" \"own\" }",
            ),
            ("resource/real.res", r#""S" { "j" "real" }"#),
        ]);
        assert_eq!(top_keys(&pairs), ["S"]);
        assert_eq!(top_keys(block(&pairs, "S", 0)), ["k", "j"]);
    }

    #[test]
    fn merge_matches_names_case_insensitively() {
        // The engine's key names are case-insensitive symbols, so "fonts" and "Fonts" are one
        // key; the first-seen spelling stays.
        let pairs = load_tree(&[
            (
                "resource/clientscheme.res",
                "#base \"base.res\"\n\"S\" { \"fonts\" { \"a\" \"own\" } }",
            ),
            (
                "resource/base.res",
                r#""S" { "Fonts" { "A" "base" "b" "base" } }"#,
            ),
        ]);
        assert_eq!(texts(&pairs, "a"), ["own"]);
        assert_eq!(texts(&pairs, "b"), ["base"]);
        assert_eq!(top_keys(block(&pairs, "S", 0)), ["fonts"]);
    }

    #[test]
    fn includes_append_after_own_keys() {
        let pairs = load_tree(&[
            (
                "resource/clientscheme.res",
                "#include \"inc.res\"\n\"S\" { \"k\" \"own\" }",
            ),
            (
                "resource/inc.res",
                r#""T" { "k" "inc" } "U" { "k" "inc2" }"#,
            ),
        ]);
        assert_eq!(top_keys(&pairs), ["S", "T", "U"]);
        assert_eq!(texts(&pairs, "k"), ["own", "inc", "inc2"]);
    }

    #[test]
    fn a_file_of_only_bases_takes_the_first_base_block() {
        let pairs = load_tree(&[
            (
                "resource/clientscheme.res",
                "#base \"a.res\"\n#base \"b.res\"",
            ),
            ("resource/a.res", r#""Scheme" { "k" "A" }"#),
            ("resource/b.res", r#""Scheme" { "k" "B" "j" "B" }"#),
        ]);
        assert_eq!(top_keys(&pairs), ["Scheme"]);
        assert_eq!(texts(&pairs, "k"), ["A"]);
        assert_eq!(texts(&pairs, "j"), ["B"]);
    }

    #[test]
    fn only_a_bases_first_top_level_block_merges() {
        let pairs = load_tree(&[
            (
                "resource/clientscheme.res",
                "#base \"base.res\"\n\"S\" { \"k\" \"own\" }",
            ),
            (
                "resource/base.res",
                r#""S" { "j" "base" } "Other" { "z" "dropped" }"#,
            ),
        ]);
        assert_eq!(texts(&pairs, "j"), ["base"]);
        assert!(texts(&pairs, "z").is_empty());
    }

    #[test]
    fn tokens_comments_and_unquoted_words() {
        let file = parse(
            "\u{feff}// leading comment\n\"Scheme\"\n{\n  Fonts // trailing\n  {\n    \"Default\" { \"1\" { \"name\" \"Verdana\"// glued\n      tall 12 } }\n  }\n  \"url\" \"http://x/y\"\n}\n",
            Platform::Linux,
        );
        assert_eq!(texts(&file.pairs, "name"), ["Verdana"]);
        assert_eq!(texts(&file.pairs, "tall"), ["12"]);
        assert_eq!(texts(&file.pairs, "url"), ["http://x/y"]);
        assert!(file.directives.is_empty());
    }

    #[test]
    fn backslashes_are_not_escapes() {
        let file = parse(
            r#""CustomFontFiles" { "1" "resource\fonts\a.ttf" }"#,
            Platform::Linux,
        );
        assert_eq!(texts(&file.pairs, "1"), [r"resource\fonts\a.ttf"]);
    }

    #[test]
    fn conditionals_follow_the_engine_rules() {
        // EvaluateConditional: $WIN32 is any PC, one leading `!`, first symbol in engine order
        // decides.
        const TEXT: &str = r#"
"Fonts"
{
    "a" { "name" "Pc" } [$WIN32]
    "b" [$WINDOWS] { "name" "WinOnly" }
    "c" { "name" "PosixOnly" } [$POSIX]
    "d" { "name" "NotMac" } [!$OSX]
    "e" { "name" "Console" } [$X360]
    "f" { "name" "Disabled" } [$_disabled_]
    "g" { "name" "NotPc" } [!$WIN32]
    "h" { "name" "LinuxOnly" } [$linux]
    "i" { "name" "Deck" } [$DECK]
    "j" { "name" "X360CheckedFirst" } [$WIN32||$X360]
    "k" { "name" "Unknown" } [!$_disabled_]
    "name" "Plain"
    "name" "NotLinux" [!$LINUX]
}
"#;
        let names = |p| texts(&parse(TEXT, p).pairs, "name");
        assert_eq!(
            names(Platform::Linux),
            ["Pc", "PosixOnly", "NotMac", "LinuxOnly", "Plain"]
        );
        assert_eq!(
            names(Platform::Windows),
            ["Pc", "WinOnly", "NotMac", "Plain", "NotLinux"]
        );
    }

    #[test]
    fn directives_are_collected_in_order_not_paired() {
        let file = parse(
            "#base \"a.res\"\n#include \"B.res\"\n#base \"c.res\" [$X360]\n#BASE \"d.res\"\n\"Scheme\" { }",
            Platform::Linux,
        );
        let got: Vec<(bool, &str)> = file
            .directives
            .iter()
            .map(|d| match d {
                KvDirective::Base(t) => (true, t.as_str()),
                KvDirective::Include(t) => (false, t.as_str()),
            })
            .collect();
        assert_eq!(
            got,
            [
                (true, "a.res"),
                (false, "B.res"),
                (true, "c.res"),
                (true, "d.res")
            ]
        );
        assert_eq!(file.pairs.len(), 1);
    }

    #[test]
    fn a_conditional_after_a_directive_is_ignored() {
        // LoadFromBuffer reads only the target after #base/#include, so the directive applies.
        let pairs = load_tree(&[
            (
                "resource/clientscheme.res",
                "#base \"a.res\" [$X360]\n#include \"inc.res\" [!$LINUX]\n\"S\" { \"k\" \"own\" }",
            ),
            ("resource/a.res", r#""S" { "j" "base" }"#),
            ("resource/inc.res", r#""T" { "i" "inc" }"#),
        ]);
        assert_eq!(top_keys(&pairs), ["S", "T"]);
        assert_eq!(texts(&pairs, "j"), ["base"]);
    }

    #[test]
    fn malformed_input_is_skipped_not_fatal() {
        let file = parse(
            "}\n\"Scheme\" { \"Fonts\" { \"x\" { \"name\" \"Kept\" } }\n\"unterminated",
            Platform::Linux,
        );
        assert_eq!(texts(&file.pairs, "name"), ["Kept"]);
    }

    #[test]
    fn bases_resolve_case_insensitively_and_skip_missing_escapes_and_loops() {
        let tmp = tempfile::tempdir().unwrap();
        let hud = tmp.path().join("hud");
        put(
            &hud,
            "resource/clientscheme.res",
            "#base \"scheme/Fonts.res\"\n#base \"missing.res\"\n#base \"../../outside.res\"\n#base \"clientscheme.res\"\n\"Scheme\" { \"Fonts\" { \"a\" { \"name\" \"Root\" } } }",
        );
        put(
            &hud,
            "resource/scheme/fonts.res",
            "#base \"../clientscheme.res\"\n\"Scheme\" { \"Fonts\" { \"b\" { \"name\" \"Based\" } } }",
        );
        put(
            tmp.path(),
            "outside.res",
            "\"Scheme\" { \"name\" \"Escaped\" }",
        );
        let root = hud.canonicalize().unwrap();
        let pairs = load(
            &root,
            &root.join("resource/clientscheme.res"),
            Platform::Linux,
        )
        .unwrap();
        assert_eq!(texts(&pairs, "name"), ["Root", "Based"]);
    }

    #[test]
    fn includes_loop_and_depth_limit() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..20 {
            put(
                tmp.path(),
                &format!("resource/f{i}.res"),
                &format!("#base \"f{}.res\"\n\"S\" {{ \"name{i}\" \"F{i}\" }}", i + 1),
            );
        }
        let root = tmp.path().canonicalize().unwrap();
        let pairs = load(&root, &root.join("resource/f0.res"), Platform::Linux).unwrap();
        for i in 0..=16 {
            assert_eq!(
                texts(&pairs, &format!("name{i}")),
                [format!("F{i}")],
                "level {i}"
            );
        }
        assert!(
            texts(&pairs, "name17").is_empty(),
            "f0 plus 16 levels of #base"
        );
    }

    #[test]
    fn a_missing_start_file_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let pairs = load(
            &root,
            &root.join("resource/clientscheme.res"),
            Platform::Linux,
        )
        .unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn resolve_ci_handles_case_separators_and_escapes() {
        let tmp = tempfile::tempdir().unwrap();
        let hud = tmp.path().join("hud");
        put(&hud, "resource/fonts/a.ttf", "x");
        put(tmp.path(), "outside.ttf", "x");
        let root = hud.canonicalize().unwrap();
        assert_eq!(
            resolve_ci(&root, &root, r"RESOURCE\Fonts\A.TTF"),
            Some(root.join("resource/fonts/a.ttf"))
        );
        assert_eq!(
            resolve_ci(&root, &root.join("resource"), "./fonts/../fonts/a.ttf"),
            Some(root.join("resource/fonts/a.ttf"))
        );
        assert_eq!(resolve_ci(&root, &root, "../outside.ttf"), None);
        assert_eq!(resolve_ci(&root, &root, "resource/fonts/b.ttf"), None);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tmp.path().join("outside.ttf"), hud.join("link.ttf"))
                .unwrap();
            assert_eq!(resolve_ci(&root, &root, "link.ttf"), None);
        }
    }

    #[test]
    fn deeply_nested_blocks_do_not_overflow_the_stack() {
        assert!(
            parse(&"{".repeat(100_000), Platform::Linux)
                .pairs
                .is_empty()
        );
        let file = parse(&"a {".repeat(100_000), Platform::Linux);
        assert_eq!(file.pairs.len(), 1);
        let file = parse(
            &format!(
                "{}\"name\" \"Deep\"{}\"name\" \"Kept\"",
                "a {".repeat(100),
                "}".repeat(100)
            ),
            Platform::Linux,
        );
        assert_eq!(texts(&file.pairs, "name"), ["Kept"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_start_file_linked_outside_the_root_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let hud = tmp.path().join("hud");
        put(tmp.path(), "outside.res", "\"S\" { \"name\" \"Escaped\" }");
        fs::create_dir_all(hud.join("resource")).unwrap();
        std::os::unix::fs::symlink(
            tmp.path().join("outside.res"),
            hud.join("resource/clientscheme.res"),
        )
        .unwrap();
        let root = hud.canonicalize().unwrap();
        let pairs = load(
            &root,
            &root.join("resource/clientscheme.res"),
            Platform::Linux,
        )
        .unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn oversized_files_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let big = |rel: &str, name: &str| {
            put(
                tmp.path(),
                rel,
                &format!("\"S\" {{ \"name\" \"{name}\" }}\n"),
            );
            let file = fs::OpenOptions::new()
                .write(true)
                .open(tmp.path().join(rel))
                .unwrap();
            file.set_len(MAX_FILE_BYTES + 1).unwrap();
        };
        big("resource/big.res", "Big");
        big("resource/bigstart.res", "BigStart");
        put(
            tmp.path(),
            "resource/clientscheme.res",
            "#base \"big.res\"\n\"S\" { \"name\" \"Small\" }",
        );
        let root = tmp.path().canonicalize().unwrap();
        let load_names = |rel: &str| {
            texts(
                &load(&root, &root.join(rel), Platform::Linux).unwrap(),
                "name",
            )
        };
        assert_eq!(load_names("resource/clientscheme.res"), ["Small"]);
        assert!(load_names("resource/bigstart.res").is_empty());
    }

    #[test]
    fn resolve_ci_rejects_a_path_that_leaves_the_root_and_returns() {
        let tmp = tempfile::tempdir().unwrap();
        let hud = tmp.path().join("hud");
        put(&hud, "resource/fonts/a.ttf", "x");
        let root = hud.canonicalize().unwrap();
        assert_eq!(
            resolve_ci(&root, &root, "../hud/resource/fonts/a.ttf"),
            None
        );
        assert_eq!(
            resolve_ci(&root, &root, "../HUD/resource/fonts/a.ttf"),
            None
        );
        assert_eq!(
            resolve_ci(&root, root.parent().unwrap(), "hud/resource/fonts/a.ttf"),
            None
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn current_platform_is_linux_here() {
        assert_eq!(Platform::current(), Some(Platform::Linux));
    }

    #[test]
    fn lexemes_cover_every_byte_in_order() {
        use LexKind::*;
        let text = "\u{feff}// c\r\n\"K\" [$WIN32] { bare 15//p0.043 [$X\n\"open";
        let lexemes = lex(text.as_bytes());
        let mut at = 0;
        for lexeme in &lexemes {
            assert_eq!(lexeme.span.start, at, "gap or overlap before {lexeme:?}");
            at = lexeme.span.end;
        }
        assert_eq!(at, text.len());
        let kinds: Vec<LexKind> = lexemes.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            [
                Bom,
                Comment,
                Space,
                Quoted { closed: true },
                Space,
                Cond { closed: true },
                Space,
                Open,
                Space,
                Bare,
                Space,
                Bare,
                Space,
                Cond { closed: false },
                Space,
                Quoted { closed: false },
            ]
        );
        assert_eq!(&text[lexemes[1].span.clone()], "// c\r");
        assert_eq!(&text[lexemes[11].span.clone()], "15//p0.043");
    }

    #[test]
    fn only_c_whitespace_separates_tokens() {
        // A no-break space used to split tokens (char::is_whitespace); C isspace keeps it inside.
        let file = parse("\"P\" { \"k\" a\u{a0}b \"v\" x\u{b}y }", Platform::Linux);
        assert_eq!(texts(&file.pairs, "k"), ["a\u{a0}b"]);
        assert_eq!(texts(&file.pairs, "v"), ["x"]);
    }

    #[test]
    fn write_pairs_round_trips_blocks_values_and_backslashes() {
        let text = "\"Root\"\n{\n\tA { xpos 5 image \"..\\hud\\x y\" }\n\tB \"two words\"\n}\n";
        let first = parse(text, Platform::Linux);
        let again = parse(&write_pairs(&first.pairs), Platform::Linux);
        assert_eq!(dump(&first.pairs), dump(&again.pairs));
        assert!(dump(&first.pairs)[0].contains("..\\hud\\x y"));
    }

    // Keys, values and nesting as plain strings, since KvPair has no PartialEq.
    fn dump(pairs: &[KvPair]) -> Vec<String> {
        pairs
            .iter()
            .map(|p| match &p.value {
                KvValue::Text(t) => format!("{}={t}", p.key),
                KvValue::Block(inner) => format!("{}{{{}}}", p.key, dump(inner).join(",")),
            })
            .collect()
    }
}
