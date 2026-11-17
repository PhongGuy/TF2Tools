//! The app's translations: the offered languages, the process-wide locale and plural forms.

use std::sync::{Mutex, MutexGuard, PoisonError};

use gpui_kit::SharedString;

pub mod format;

/// A language the app offers, by its catalogue code and its name in that language.
#[derive(Clone, Copy, Debug)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
}

/// Every language with a catalogue in `locales/`, English first.
pub const LANGUAGES: &[Language] = &[
    Language {
        code: "en",
        name: "English",
    },
    Language {
        code: "ru",
        name: "Русский (preview)",
    },
];

// The offered language with this code, so callers get a `'static` code back.
fn offered(code: &str) -> Option<&'static str> {
    LANGUAGES.iter().map(|l| l.code).find(|c| *c == code)
}

/// Switches the process-wide locale, shared with the kit, to `code`; an unknown code
/// switches to English. Returns the code applied.
pub fn apply(code: &str) -> &'static str {
    let applied = offered(code).unwrap_or("en");
    rust_i18n::set_locale(applied);
    applied
}

/// The current locale as an offered code, or "en" when it is anything else.
pub fn current() -> &'static str {
    offered(&rust_i18n::locale()).unwrap_or("en")
}

/// The grammatical form a count takes in a language.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PluralForm {
    One,
    Few,
    Many,
    Other,
}

impl PluralForm {
    // The catalogue key segment that holds this form's text.
    fn suffix(self) -> &'static str {
        match self {
            PluralForm::One => "one",
            PluralForm::Few => "few",
            PluralForm::Many => "many",
            PluralForm::Other => "other",
        }
    }
}

/// The form `n` takes in language `code`: Russian's one/few/many, English's one/other for
/// every other code.
pub fn plural_form(code: &str, n: u64) -> PluralForm {
    if code != "ru" {
        return if n == 1 {
            PluralForm::One
        } else {
            PluralForm::Other
        };
    }
    // Russian keys on the last two digits: 11 to 14 are always "many".
    match (n % 10, n % 100) {
        (_, 11..=14) => PluralForm::Many,
        (1, _) => PluralForm::One,
        (2..=4, _) => PluralForm::Few,
        _ => PluralForm::Many,
    }
}

/// The text of plural `key` for count `n` in the current language, with `%{count}` filled.
/// A missing form falls back to the key's `other` text, then to English.
pub fn plural(key: &str, n: u64) -> SharedString {
    counted_plural(key, n, &n.to_string(), &[])
}

/// Plural `key` chosen by `n`, with `%{count}` filled by `count` (the caller's formatted
/// number) and every other `%{name}` from `vars`.
pub fn counted_plural(key: &str, n: u64, count: &str, vars: &[(&str, &str)]) -> SharedString {
    let code = current();
    let form = format!("{key}.{}", plural_form(code, n).suffix());
    let other = format!("{key}.other");
    let text = crate::_rust_i18n_try_translate(code, &form)
        .or_else(|| crate::_rust_i18n_try_translate(code, &other))
        .map_or(other.clone(), |t| t.into_owned());
    // One pass over the text, so a value that itself holds `%{` is never expanded again.
    let (names, values): (Vec<&str>, Vec<String>) = std::iter::once(("count", count))
        .chain(vars.iter().copied())
        .map(|(name, value)| (name, value.to_owned()))
        .unzip();
    rust_i18n::replace_patterns(&text, &names, &values).into()
}

/// `rust_i18n::t!` as a `SharedString`; takes the same arguments, e.g. `tr!("k", percent = 70)`.
macro_rules! tr {
    ($($arg:tt)*) => {
        gpui_kit::SharedString::from(rust_i18n::t!($($arg)*).into_owned())
    };
}
pub(crate) use tr;

/// The items joined as a list in the current language ("a", "a and b", "a, b and c").
pub fn list(items: &[impl AsRef<str>]) -> SharedString {
    // All but the last item share the plain separator; the language's own joiner adds the last.
    match items {
        [] => SharedString::from(""),
        [only] => only.as_ref().to_owned().into(),
        [rest @ .., last] => {
            let rest: Vec<&str> = rest.iter().map(AsRef::as_ref).collect();
            let rest = rest.join(&*tr!("common.list.separator"));
            tr!("common.list.last", rest = rest, last = last.as_ref())
        }
    }
}

/// Serialises tests that switch the process-wide locale; a panicked holder does not wedge it.
#[doc(hidden)]
pub fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    use super::{LANGUAGES, PluralForm, counted_plural, current, list, plural_form};

    const FORMS: [&str; 4] = ["one", "few", "many", "other"];

    // One locale's catalogue as flat key -> value, read without touching the global locale.
    fn messages(code: &str) -> BTreeMap<String, String> {
        crate::_rust_i18n_backend()
            .messages_for_locale(code)
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    // "a.b.few" -> ("a.b", "few") when the last segment is a plural form.
    fn plural_part(key: &str) -> Option<(&str, &str)> {
        key.rsplit_once('.')
            .filter(|(_, form)| FORMS.contains(form))
    }

    fn plain_keys(m: &BTreeMap<String, String>) -> BTreeSet<&str> {
        m.keys()
            .map(String::as_str)
            .filter(|k| plural_part(k).is_none())
            .collect()
    }

    fn plural_bases(m: &BTreeMap<String, String>) -> BTreeMap<&str, BTreeSet<&str>> {
        let mut bases: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for (base, form) in m.keys().filter_map(|k| plural_part(k)) {
            bases.entry(base).or_default().insert(form);
        }
        bases
    }

    // The forms a language's rule can produce, found by asking the rule.
    fn rule_forms(code: &str) -> BTreeSet<&'static str> {
        (0..=200).map(|n| plural_form(code, n).suffix()).collect()
    }

    // The %{name} variables a text uses.
    fn variables(text: &str) -> BTreeSet<&str> {
        text.split("%{")
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
            .collect()
    }

    // Every literal "<surface>.<key>" in `source` whose body is key-shaped up to its closing
    // quote, with the byte offset of its opening quote.
    fn key_sites(source: &str, surfaces: &BTreeSet<String>) -> Vec<(usize, String)> {
        let mut found = Vec::new();
        for surface in surfaces {
            let opener = format!("\"{surface}.");
            let mut from = 0;
            while let Some(at) = source[from..].find(&opener) {
                let quote = from + at;
                let body = &source[quote + 1..];
                let len = body
                    .find(|c: char| {
                        !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
                    })
                    .unwrap_or(body.len());
                if body[len..].starts_with('"') {
                    found.push((quote, body[..len].to_owned()));
                }
                from = quote + 1 + len;
            }
        }
        found
    }

    fn key_literals(source: &str, surfaces: &BTreeSet<String>) -> Vec<String> {
        key_sites(source, surfaces)
            .into_iter()
            .map(|(_, key)| key)
            .collect()
    }

    // A file's shipped code: everything before its test module.
    fn shipped_part(source: &str) -> &str {
        source
            .find("#[cfg(test)]")
            .map_or(source, |at| &source[..at])
    }

    // True when the literal whose opening quote is at `quote` is the first argument of
    // `tr!(` or `plural(`, whitespace and line breaks allowed between.
    fn inside_tr_or_plural(code: &str, quote: usize) -> bool {
        let before = code[..quote].trim_end();
        before.ends_with("tr!(") || before.ends_with("plural(")
    }

    fn src_files() -> Vec<PathBuf> {
        let mut files = Vec::new();
        rust_files(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        files
    }

    // The catalogue sections whose keys live in text the code builds.
    fn message_surfaces() -> BTreeSet<String> {
        [
            "messages",
            "common",
            "updater",
            "maps",
            "logs_tf",
            "profiles",
            "crosshairs_page",
            "hud_maker",
            "hud_store",
            "addon_store",
            "store_grid",
            "confirm",
            "font_check",
        ]
        .map(str::to_owned)
        .into()
    }

    #[test]
    fn counted_plural_fills_the_given_count_and_the_other_variables() {
        assert_eq!(
            counted_plural("settings.sync.items", 1_400, "1,400", &[]),
            "1,400 items cached"
        );
        assert_eq!(
            counted_plural("settings.sync.items", 1, "1", &[]),
            "1 item cached"
        );
        assert_eq!(
            counted_plural(
                "messages.local.copy_updated.no_longer_fit",
                2,
                "2",
                &[("list", "a and b")]
            ),
            "2 of your changes no longer fit: a and b."
        );
        assert_eq!(super::plural("settings.sync.items", 3), "3 items cached");
    }

    #[test]
    fn the_travel_check_passes_keys_inside_counted_plural() {
        let sample = r#"i18n::counted_plural("demo.a", n, &c, &[]); counted_plural(
                "demo.b", n, &c, &[("x", "y")]);"#;
        let surfaces = BTreeSet::from(["demo".to_owned()]);
        let code = shipped_part(sample);
        let sites = key_sites(code, &surfaces);
        assert_eq!(sites.len(), 2);
        assert!(
            sites
                .iter()
                .all(|(quote, _)| inside_tr_or_plural(code, *quote))
        );
    }

    // The 1-based lines of `code` where an OpError action title is a string literal: the first
    // argument of `into_op(` or `OpError::core(`, or an `action: ` field. Comment lines are skipped.
    fn literal_title_lines(code: &str) -> Vec<usize> {
        let mut lines = Vec::new();
        for opener in ["into_op(", "OpError::core(", "action: "] {
            for (at, _) in code.match_indices(opener) {
                let line_start = code[..at].rfind('\n').map_or(0, |n| n + 1);
                let comment = code[line_start..at].trim_start().starts_with("//");
                if !comment && code[at + opener.len()..].trim_start().starts_with('"') {
                    lines.push(code[..at].matches('\n').count() + 1);
                }
            }
        }
        lines.sort_unstable();
        lines
    }

    // "<path under src>:<line>" for each literal title in the shipped code of files `include` keeps.
    fn literal_titles(include: impl Fn(&str) -> bool) -> Vec<String> {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        for file in src_files() {
            let rel = file
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if include(&rel) {
                let source = std::fs::read_to_string(&file).unwrap();
                for line in literal_title_lines(shipped_part(&source)) {
                    found.push(format!("{rel}:{line}"));
                }
            }
        }
        found
    }

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn english_counts_one_and_other() {
        assert_eq!(plural_form("en", 1), PluralForm::One);
        for n in [0, 2, 4, 5, 11, 12, 14, 21, 22, 25, 101, 111] {
            assert_eq!(plural_form("en", n), PluralForm::Other, "en {n}");
        }
    }

    #[test]
    fn russian_counts_one_few_and_many() {
        use PluralForm::{Few, Many, One};
        let cases = [
            (0, Many),
            (1, One),
            (2, Few),
            (4, Few),
            (5, Many),
            (11, Many),
            (12, Many),
            (14, Many),
            (21, One),
            (22, Few),
            (25, Many),
            (101, One),
            (111, Many),
        ];
        for (n, form) in cases {
            assert_eq!(plural_form("ru", n), form, "ru {n}");
        }
    }

    #[test]
    fn an_unknown_language_counts_like_english() {
        assert_eq!(plural_form("xx", 1), PluralForm::One);
        assert_eq!(plural_form("xx", 21), PluralForm::Other);
    }

    #[test]
    fn english_comes_first_and_every_catalogue_is_offered() {
        let names: Vec<(&str, &str)> = LANGUAGES.iter().map(|l| (l.code, l.name)).collect();
        assert_eq!(names, [("en", "English"), ("ru", "Русский (preview)")]);
        let offered: BTreeSet<&str> = LANGUAGES.iter().map(|l| l.code).collect();
        let files: Vec<String> = crate::_rust_i18n_available_locales()
            .into_iter()
            .map(|c| c.into_owned())
            .collect();
        let files: BTreeSet<&str> = files.iter().map(String::as_str).collect();
        assert_eq!(offered, files, "LANGUAGES and locales/*.yml must match");
        // This binary never switches the locale.
        assert_eq!(current(), "en");
    }

    #[test]
    fn every_language_has_exactly_the_english_keys() {
        let en = messages("en");
        assert!(!en.is_empty(), "en.yml is missing or empty");
        for lang in LANGUAGES {
            let theirs = messages(lang.code);
            assert_eq!(
                plain_keys(&theirs),
                plain_keys(&en),
                "{}: keys differ from en",
                lang.code
            );
            let (their_bases, en_bases) = (plural_bases(&theirs), plural_bases(&en));
            assert_eq!(
                their_bases.keys().collect::<Vec<_>>(),
                en_bases.keys().collect::<Vec<_>>(),
                "{}: plural keys differ from en",
                lang.code
            );
            for (base, forms) in their_bases {
                assert_eq!(
                    forms,
                    rule_forms(lang.code),
                    "{}: forms of {base}",
                    lang.code
                );
            }
        }
    }

    #[test]
    fn translations_keep_the_english_variables() {
        let en = messages("en");
        for lang in LANGUAGES {
            for (key, text) in messages(lang.code) {
                let source = match plural_part(&key) {
                    Some((base, _)) => en.get(&format!("{base}.other")),
                    None => en.get(&key),
                };
                let Some(source) = source else { continue };
                assert_eq!(variables(&text), variables(source), "{}: {key}", lang.code);
            }
        }
    }

    #[test]
    fn no_value_is_empty_or_a_yaml_boolean() {
        for lang in LANGUAGES {
            for (key, text) in messages(lang.code) {
                assert!(
                    !matches!(text.as_str(), "" | "true" | "false"),
                    "{}: {key} = {text:?}; double-quote every value",
                    lang.code
                );
            }
        }
    }

    #[test]
    fn the_key_scan_reads_only_literals_under_a_surface() {
        let sample = r#"tr!("demo.card.title"); const T: (&str, &str) = ("demo.a.title", "demo.a.body");
            let prose = "demo. Not a key"; let other = "Demo.title"; plural("demo.items", n);"#;
        let surfaces = BTreeSet::from(["demo".to_owned()]);
        assert_eq!(
            key_literals(sample, &surfaces),
            [
                "demo.card.title",
                "demo.a.title",
                "demo.a.body",
                "demo.items"
            ]
        );
    }

    #[test]
    fn every_key_the_code_names_exists_in_english() {
        let en = messages("en");
        let surfaces: BTreeSet<String> = en
            .keys()
            .filter_map(|k| k.split('.').next())
            .map(str::to_owned)
            .collect();
        let mut files = Vec::new();
        rust_files(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let mut missing = Vec::new();
        for file in files {
            let source = std::fs::read_to_string(&file).unwrap();
            for key in key_literals(&source, &surfaces) {
                if !en.contains_key(&key) && !en.contains_key(&format!("{key}.other")) {
                    missing.push(format!("{}: {key}", file.display()));
                }
            }
        }
        assert!(missing.is_empty(), "keys missing from en.yml: {missing:?}");
    }

    #[test]
    fn english_lists_read_a_b_and_c() {
        let none: [&str; 0] = [];
        assert_eq!(list(&none), "");
        assert_eq!(list(&["a"]), "a");
        assert_eq!(list(&["a", "b"]), "a and b");
        assert_eq!(list(&["a", "b", "c"]), "a, b and c");
        assert_eq!(list(&[String::from("x"), String::from("y")]), "x and y");
    }

    #[test]
    fn the_travel_check_passes_only_keys_directly_inside_tr_or_plural() {
        let sample = r#"tr!("demo.a"); i18n::plural(
                "demo.b", n); const K: &str = "demo.c"; f("demo.d"); tr!("x", v = "demo.e");
            #[cfg(test)]
            mod tests { const T: &str = "demo.f"; }"#;
        let surfaces = BTreeSet::from(["demo".to_owned()]);
        let code = shipped_part(sample);
        let travelling: Vec<String> = key_sites(code, &surfaces)
            .into_iter()
            .filter(|(quote, _)| !inside_tr_or_plural(code, *quote))
            .map(|(_, key)| key)
            .collect();
        assert_eq!(travelling, ["demo.c", "demo.d", "demo.e"]);
    }

    #[test]
    fn message_keys_never_travel_outside_tr_or_plural() {
        let surfaces = message_surfaces();
        let mut travelling = Vec::new();
        for file in src_files() {
            let source = std::fs::read_to_string(&file).unwrap();
            let code = shipped_part(&source);
            for (quote, key) in key_sites(code, &surfaces) {
                if !inside_tr_or_plural(code, quote) {
                    travelling.push(format!("{}: {key}", file.display()));
                }
            }
        }
        assert!(
            travelling.is_empty(),
            "keys outside tr!( or plural(: {travelling:?}"
        );
    }

    #[test]
    fn every_message_key_is_named_in_the_code() {
        let surfaces = message_surfaces();
        let mut named = BTreeSet::new();
        for file in src_files() {
            named.extend(key_literals(
                &std::fs::read_to_string(&file).unwrap(),
                &surfaces,
            ));
        }
        let en = messages("en");
        let orphans: BTreeSet<&str> = en
            .keys()
            .filter(|k| k.split('.').next().is_some_and(|s| surfaces.contains(s)))
            .map(|k| plural_part(k).map_or(k.as_str(), |(base, _)| base))
            .filter(|k| !named.contains(*k))
            .collect();
        assert!(orphans.is_empty(), "keys no code names: {orphans:?}");
    }

    #[test]
    fn the_title_scan_finds_only_literal_titles() {
        let sample = "e.into_op(\"Load\");\nOpError::core(\n    \"Save\", e);\n\
            OpError::Core { action: \"Go\", message };\n// OpError::core(\"Doc\", e)\n\
            e.into_op(tr!(\"demo.load\"));\nOpError::core(save_failed(), e);\nfn f(action: &str) {}\n";
        assert_eq!(literal_title_lines(sample), [1, 2, 4]);
    }

    #[test]
    fn no_action_title_outside_the_pages_is_a_literal() {
        let found = literal_titles(|rel| !rel.starts_with("pages/"));
        assert!(found.is_empty(), "literal action titles: {found:?}");
    }

    #[test]
    fn no_page_action_title_is_a_literal() {
        let found = literal_titles(|rel| rel.starts_with("pages/"));
        assert!(found.is_empty(), "literal action titles: {found:?}");
    }

    // The 1-based lines of `code` where a developer-log label could be translated text: a
    // `.log(` argument that is no literal, or a tracing line that opens with `{action}` or `{title}`.
    fn translated_log_label_lines(code: &str) -> Vec<usize> {
        let mut lines = Vec::new();
        for (at, _) in code.match_indices(".log(") {
            let arg = code[at + ".log(".len()..].trim_start();
            if !(arg.starts_with('"') || arg.starts_with("&format!(\"")) {
                lines.push(code[..at].matches('\n').count() + 1);
            }
        }
        for opener in ["warn!(", "info!(", "error!(", "debug!(", "trace!("] {
            for (at, _) in code.match_indices(opener) {
                let arg = code[at + opener.len()..].trim_start();
                if arg.starts_with("\"{action}") || arg.starts_with("\"{title}") {
                    lines.push(code[..at].matches('\n').count() + 1);
                }
            }
        }
        lines.sort_unstable();
        lines
    }

    #[test]
    fn the_log_label_scan_finds_only_labels_that_may_be_translated() {
        let sample = "f.log(\"HUD update\");\nf.log(&action);\nf.log(&format!(\"start {x}\"));\n\
            tracing::warn!(\"{action}: {}\", d);\ntracing::warn!(\"save: {action}\");\n\
            let m = format!(\"{title} found\");\nerror!(\"{title}: x\");\n";
        assert_eq!(translated_log_label_lines(sample), [2, 4, 7]);
    }

    #[test]
    fn no_developer_log_label_is_translated_text() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        for file in src_files() {
            let source = std::fs::read_to_string(&file).unwrap();
            for line in translated_log_label_lines(shipped_part(&source)) {
                let rel = file
                    .strip_prefix(&src)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push(format!("{rel}:{line}"));
            }
        }
        assert!(
            found.is_empty(),
            "log labels that may be translated: {found:?}"
        );
    }
}
