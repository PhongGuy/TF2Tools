//! The app's translations: the offered languages, the process-wide locale and plural forms.

use std::sync::{Mutex, MutexGuard, PoisonError};

use gpui_kit::SharedString;

pub mod format;
pub mod names;

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
            "nav",
            "huds",
            "sounds",
            "addons",
            "external_huds",
            "widgets",
            "updater",
            "maps",
            "logs_tf",
            "profiles",
            "crosshairs_page",
            "hud_maker",
            "hud_store",
            "addon_store",
            "store_grid",
            "store_download",
            "sound_store",
            "store_detail",
            "addon_detail",
            "confirm",
            "font_check",
            "header",
            "shell",
            "install_parts",
            "activity_log",
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

    #[test]
    fn russian_says_each_idea_one_way() {
        let ru = messages("ru");
        let banned = [
            "попробуйте ещё раз",
            "попробуйте еще раз",
            "попробуйте сохранить ещё раз",
            "предпрослуш",
            "никнейм",
        ];
        let found: Vec<String> = ru
            .iter()
            .filter(|(_, v)| {
                let v = v.to_lowercase();
                banned.iter().any(|b| v.contains(b))
            })
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        assert!(found.is_empty(), "a second wording for one idea: {found:?}");
        let damaged = [
            "updater.failure.damaged",
            "messages.store.download.damaged",
            "messages.store.gb_fetch.damaged",
        ]
        .map(|k| ru.get(k).map(String::as_str));
        assert_eq!(
            damaged,
            [Some("Скачанный файл повреждён. Попробуйте снова."); 3]
        );
    }

    #[test]
    fn english_action_titles_read_as_a_failure_or_an_outcome() {
        let en = messages("en");
        let bare = ["Load ", "Save ", "List ", "Make ", "Measure ", "Trim "];
        let found: Vec<String> = en
            .iter()
            // Matched by segments so the key scans never read this prefix as a key.
            .filter(|(k, _)| k.split('.').take(2).eq(["messages", "action"]))
            .filter(|(_, v)| {
                v.starts_with(char::is_lowercase)
                    || v.ends_with('.')
                    || bare.iter().any(|b| v.starts_with(b))
            })
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        assert!(
            found.is_empty(),
            "titles that don't read as a failure: {found:?}"
        );
    }

    #[test]
    fn the_english_scan_skips_comments_developer_text_and_non_prose() {
        let sample = r##"// "Not code" in a comment
    /* a "block" /* nested "one" */ still "comment" */
    let a = "Delete HUD"; let b = 'x'; let q = '"'; let e = '\''; let u = '\u{a0}';
    fn f<'a>(s: &'a str) -> &'a str { s }
    let k = tr!("demo.card.title", name = "Plain Name");
    tracing::warn!("Could not save {}", path.display());
    self.log(&format!("Install HUD {name}"));
    debug_assert!(ok, "Never shown here");
    let raw = r#"Raw "quoted" text"#; let bytes = b"Byte text";
    let id = "huds-card-install"; let caps = "HUD"; let many = "HUDs"; let dir = "tf2tools-sounds";
    let word = "Dark"; let lower = "installed"; let esc = "Say \"Hello there\"";
    let allowed = "GameBanana"; let n = plural("demo.count", 2);
    match x { Some(v) => unreachable!("Bad {v}, {}", f("Still developer")), None => {} }
    let last = "Two words";"##;
        assert_eq!(
            english_literals(sample),
            [
                (3, "Delete HUD"),
                (5, "Plain Name"),
                (11, "Dark"),
                (11, r#"Say \"Hello there\""#),
                (12, "GameBanana"),
                (14, "Two words"),
            ]
        );
    }

    // Files whose shipped code shows no English; each page task appends its own.
    const SCANNED: &[&str] = &[
        "widgets/label_option.rs",
        "shell/route.rs",
        "widgets/class_icon.rs",
        "pages/huds.rs",
        "shell/external_huds.rs",
        // "./" keeps the key scan from reading this path as a `font_check` key.
        "./font_check.rs",
        "shell/font_check.rs",
        "widgets/carousel.rs",
        "pages/sounds.rs",
        "audio/player.rs",
        "widgets/profile_menu.rs",
        "pages/crosshairs.rs",
        "crosshair_sync.rs",
        "widgets/weapon_assigner.rs",
        "widgets/crosshair_picker.rs",
        "pages/profiles/mod.rs",
        "pages/profiles/model.rs",
        "pages/profiles/probe.rs",
        "pages/profiles/editor.rs",
        "pages/profiles/editor_model.rs",
        "pages/addons/mod.rs",
        "pages/addons/model.rs",
        "pages/addons/gamebanana.rs",
        "pages/maps/mod.rs",
        "pages/maps/model.rs",
        "pages/maps/gb_tab.rs",
        "widgets/sync_banner.rs",
        "widgets/store_error.rs",
        "pages/addon_store/card.rs",
        "store_grid/mod.rs",
        "store_grid/feed.rs",
        "pages/crosshair_store.rs",
        "pages/hud_store/bodies.rs",
        "pages/hud_store/curated.rs",
        "pages/hud_store/curated_tab.rs",
        "pages/hud_store/gamebanana_tab.rs",
        "pages/hud_store/mod.rs",
        "gb_card.rs",
        "pages/sound_store/body.rs",
        "pages/sound_store/card.rs",
        "pages/sound_store/model.rs",
        "pages/sound_store/mod.rs",
        "store_preview.rs",
        "pages/addon_store/mod.rs",
        "pages/addon_store/model.rs",
        "pages/addon_store/gamebanana_tab.rs",
        "gb_addon_installs.rs",
        "./store_detail.rs",
        "./store_download.rs",
        "store_messages.rs",
        "widgets/detail_header.rs",
        "./addon_detail.rs",
        "addon_updates.rs",
        "updater/mod.rs",
        "updater/view.rs",
        "updater/backend.rs",
        "updater/keys.rs",
        "shell/header.rs",
        "sync.rs",
        "widgets/sync_ring.rs",
        "widgets/dialog_frame.rs",
        "widgets/tip.rs",
        "widgets/card_grid.rs",
        "widgets/scroll.rs",
        "shell/toast.rs",
        "shell/mod.rs",
        "shell/confirm.rs",
        "shell/leave.rs",
        "shell/root.rs",
        "shell/error_view.rs",
        "crash/notice.rs",
        "old_v4.rs",
        "pages/placeholder.rs",
        "services.rs",
        "payload_checklist.rs",
        "pages/logs/mod.rs",
        "pages/logs/model.rs",
        "activity.rs",
        "pages/logs_tf/text.rs",
        "pages/logs_tf/tables.rs",
        "log_nicknames.rs",
        "logs_sync.rs",
        "logs_trim.rs",
        "pages/logs_tf/list.rs",
        "pages/logs_tf/sort.rs",
        "pages/logs_tf/head.rs",
        "pages/logs_tf/grid.rs",
        "pages/logs_tf/class_weapons.rs",
        "pages/logs_tf/detail/banner.rs",
        "pages/logs_tf/detail/footer.rs",
        "pages/logs_tf/detail/matrix.rs",
        "pages/logs_tf/detail/medics.rs",
        "pages/logs_tf/detail/mod.rs",
        "pages/logs_tf/detail/players.rs",
        "pages/logs_tf/detail/popover.rs",
        "pages/logs_tf/detail/rounds.rs",
        "pages/logs_tf/mod.rs",
    ];

    // (file under src, exact literal, why it is not translated).
    const ALLOWED: &[(&str, &str, &str)] = &[
        (
            "pages/huds.rs",
            "the HUD update task stopped unexpectedly",
            "developer detail of a stopped download",
        ),
        (
            "./font_check.rs",
            "no font host on this system",
            "developer detail: FontInstallError::Host reason, shown only in the row's details",
        ),
        (
            "pages/sounds.rs",
            "16-bit PCM, 44100Hz, Stereo",
            "technical format spec in the import row's details; no words to translate",
        ),
        (
            "pages/crosshairs.rs",
            "Dark",
            "the plain backdrop's Select value and row id; its label is crosshairs_page.background.dark",
        ),
        ("pages/maps/model.rs", "Name", SELECT_VALUE),
        ("pages/maps/model.rs", "Most played", SELECT_VALUE),
        ("pages/maps/model.rs", "Recently played", SELECT_VALUE),
        ("pages/maps/gb_tab.rs", "Most downloaded", SELECT_VALUE),
        ("pages/maps/gb_tab.rs", "Newest", SELECT_VALUE),
        ("pages/maps/gb_tab.rs", "All maps", SELECT_VALUE),
        ("pages/maps/gb_tab.rs", "Installed", SELECT_VALUE),
        ("store_grid/mod.rs", "Most Liked", SELECT_VALUE),
        ("store_grid/mod.rs", "Most Viewed", SELECT_VALUE),
        ("store_grid/mod.rs", "Newest", SELECT_VALUE),
        (
            "pages/addon_store/gamebanana_tab.rs",
            "Most downloaded",
            SELECT_VALUE,
        ),
        (
            "pages/addon_store/gamebanana_tab.rs",
            "Newest",
            SELECT_VALUE,
        ),
        (
            "pages/addon_store/gamebanana_tab.rs",
            "the add-on listing task stopped unexpectedly",
            "developer detail of a stopped listing",
        ),
        (
            "pages/maps/mod.rs",
            "GameBanana",
            "a proper noun, the source button",
        ),
        ("pages/hud_store/curated.rs", "Windows", OS_NAME),
        ("pages/hud_store/curated.rs", "Mac", OS_NAME),
        ("pages/hud_store/curated.rs", "Linux", OS_NAME),
        (
            "pages/hud_store/mod.rs",
            "GameBanana",
            "a proper noun, the source tab",
        ),
        (
            "pages/hud_store/bodies.rs",
            "GameBanana",
            "a proper noun, the link button",
        ),
        (
            "./store_detail.rs",
            "the file listing task stopped unexpectedly",
            "developer detail of a stopped listing",
        ),
        (
            "./store_download.rs",
            "the download task stopped unexpectedly",
            "developer detail of a stopped download",
        ),
        (
            "./store_download.rs",
            "dropping the subscriptions stops the grid redraws and the In Library flip",
            "developer detail: a must_use note for the compiler",
        ),
        (
            "store_messages.rs",
            "sound {} is no longer in the store",
            "developer detail of a missing preview",
        ),
        (
            "store_messages.rs",
            "the release has no file",
            "developer detail of an empty release",
        ),
        (
            "./addon_detail.rs",
            "the add-on profile task stopped unexpectedly",
            "developer detail of a stopped profile read",
        ),
        ("updater/mod.rs", "update check", STOPPED_LABEL),
        ("updater/mod.rs", "update download", STOPPED_LABEL),
        ("updater/mod.rs", "update install", STOPPED_LABEL),
        (
            "updater/keys.rs",
            "the update-test-key feature must never be in a release build",
            "developer detail: a build guard",
        ),
        (
            "updater/keys.rs",
            "{}/PhongGuy/TF2Tools/releases/latest",
            "developer detail: the release URL",
        ),
        (
            "sync.rs",
            "the sync stopped: {e}",
            "developer detail of a stopped sync",
        ),
        ("sync.rs", "Curated HUDs", STORE_FIELD),
        ("sync.rs", "GameBanana HUDs", STORE_FIELD),
        ("sync.rs", "Sounds", STORE_FIELD),
        ("sync.rs", "Crosshairs", STORE_FIELD),
        ("sync.rs", "Addons", STORE_FIELD),
        ("sync.rs", "Store", STORE_FIELD),
        ("shell/mod.rs", "Shell", "developer detail: a key context"),
        ("shell/confirm.rs", "Shrink canvas", DIALOG_ELSEWHERE),
        (
            "shell/confirm.rs",
            "Shrinking to 32x32 cuts off the edges of your drawing.",
            DIALOG_ELSEWHERE,
        ),
        ("shell/confirm.rs", "Shrink", DIALOG_ELSEWHERE),
        ("shell/confirm.rs", "Cancel", DIALOG_ELSEWHERE),
        (
            "shell/confirm.rs",
            "Delete this snapshot?",
            DIALOG_ELSEWHERE,
        ),
        (
            "shell/confirm.rs",
            "{name} will be gone for good. Your HUD stays as it is.",
            DIALOG_ELSEWHERE,
        ),
        ("shell/confirm.rs", "Delete", DIALOG_ELSEWHERE),
        ("shell/confirm.rs", "Keep", DIALOG_ELSEWHERE),
        ("shell/confirm.rs", "Reset all options?", DIALOG_ELSEWHERE),
        (
            "shell/confirm.rs",
            "Every option goes back to how the HUD came. Your other changes stay, and Undo brings the options back.",
            DIALOG_ELSEWHERE,
        ),
        ("shell/confirm.rs", "Reset all", DIALOG_ELSEWHERE),
        ("shell/confirm.rs", "Remove mastercomfig", DIALOG_ELSEWHERE),
        (
            "shell/confirm.rs",
            "This removes mastercomfig and the preset you picked from TF2. Your own config files stay.",
            DIALOG_ELSEWHERE,
        ),
        ("shell/confirm.rs", "Remove", DIALOG_ELSEWHERE),
        (
            "shell/confirm.rs",
            "Replace your modules.cfg?",
            DIALOG_ELSEWHERE,
        ),
        (
            "shell/confirm.rs",
            "TF2 already has a modules.cfg that TF2Tools didn't write. Your file is kept as a backup next to it.",
            DIALOG_ELSEWHERE,
        ),
        ("shell/confirm.rs", "Replace it", DIALOG_ELSEWHERE),
        (
            "shell/root.rs",
            "TF2Tools",
            "a proper noun, the loading screen's title",
        ),
        (
            "services.rs",
            "Could not open the database at {}: {message}",
            STARTUP_DETAIL,
        ),
        (
            "services.rs",
            "The database at {} is from a newer TF2Tools: {message}",
            STARTUP_DETAIL,
        ),
        (
            "services.rs",
            "Could not load the builtin crosshairs from {}: {message}",
            STARTUP_DETAIL,
        ),
        (
            "services.rs",
            "no TF2 folder is set",
            "developer detail: OpError's log text",
        ),
        (
            "services.rs",
            "unexpected error: {raw}",
            "developer detail: OpError's log text",
        ),
        ("pages/logs/model.rs", "All categories", SELECT_VALUE),
        ("pages/logs/model.rs", "Sounds", SELECT_VALUE),
        ("pages/logs/model.rs", "Crosshairs", SELECT_VALUE),
        ("pages/logs/model.rs", "Store", SELECT_VALUE),
        ("pages/logs/model.rs", "Settings", SELECT_VALUE),
        ("pages/logs/model.rs", "System", SELECT_VALUE),
        ("pages/logs/model.rs", "Profiles", SELECT_VALUE),
        ("pages/logs/model.rs", "Addons", SELECT_VALUE),
        ("pages/logs/model.rs", "All levels", SELECT_VALUE),
        ("pages/logs/model.rs", "Info", SELECT_VALUE),
        ("pages/logs/model.rs", "Warning", SELECT_VALUE),
        ("pages/logs/model.rs", "Error", SELECT_VALUE),
        ("pages/logs_tf/text.rs", "Kritzkrieg", WEAPON_NAME),
        ("pages/logs_tf/text.rs", "Quick-Fix", WEAPON_NAME),
        ("pages/logs_tf/text.rs", "Vaccinator", WEAPON_NAME),
        ("pages/logs_tf/text.rs", "Medigun", WEAPON_NAME),
        ("pages/logs_tf/sort.rs", "Team", ID_VALUE),
        ("pages/logs_tf/sort.rs", "Name", ID_VALUE),
        (
            "pages/logs_tf/sort.rs",
            "every class is in TfClass::ALL",
            "developer detail: an invariant's message",
        ),
        ("pages/logs_tf/sort.rs", "Class", ID_VALUE),
        ("pages/logs_tf/sort.rs", "Played", ID_VALUE),
        ("pages/logs_tf/sort.rs", "Logs", ID_VALUE),
        ("pages/logs_tf/sort.rs", "Kills/min", ID_VALUE),
        ("pages/logs_tf/sort.rs", "Assists/min", ID_VALUE),
        ("pages/logs_tf/sort.rs", "Deaths/min", ID_VALUE),
        ("pages/logs_tf/sort.rs", "Dmg/min", ID_VALUE),
        ("pages/logs_tf/sort.rs", "Heals/min", ID_VALUE),
        ("pages/logs_tf/sort.rs", "Ubers/10 min", ID_VALUE),
        ("pages/logs_tf/detail/banner.rs", "Kills", ID_VALUE),
        ("pages/logs_tf/detail/banner.rs", "Damage", ID_VALUE),
        ("pages/logs_tf/detail/banner.rs", "Charges", ID_VALUE),
        ("pages/logs_tf/detail/banner.rs", "Drops", ID_VALUE),
        ("pages/logs_tf/detail/banner.rs", "Caps", ID_VALUE),
        ("pages/logs_tf/detail/banner.rs", "Midfights", ID_VALUE),
        ("pages/logs_tf/detail/medics.rs", "Charges:{}", ID_VALUE),
        ("pages/logs_tf/detail/medics.rs", "Charges", ID_VALUE),
        ("pages/logs_tf/detail/medics.rs", "Healing", ID_VALUE),
        ("pages/logs_tf/detail/medics.rs", "Drops", ID_VALUE),
        (
            "pages/logs_tf/detail/medics.rs",
            "Avg time to build",
            ID_VALUE,
        ),
        (
            "pages/logs_tf/detail/medics.rs",
            "Avg time before using",
            ID_VALUE,
        ),
        (
            "pages/logs_tf/detail/medics.rs",
            "Near full charge deaths",
            ID_VALUE,
        ),
        (
            "pages/logs_tf/detail/medics.rs",
            "Avg uber length",
            ID_VALUE,
        ),
        (
            "pages/logs_tf/detail/medics.rs",
            "Deaths after charge",
            ID_VALUE,
        ),
        (
            "pages/logs_tf/detail/medics.rs",
            "Major advantages lost",
            ID_VALUE,
        ),
        (
            "pages/logs_tf/detail/medics.rs",
            "Biggest advantage lost",
            ID_VALUE,
        ),
        (
            "pages/logs_tf/detail/rounds.rs",
            "BLU K",
            LOGS_TF_ABBREVIATION,
        ),
        (
            "pages/logs_tf/detail/rounds.rs",
            "RED K",
            LOGS_TF_ABBREVIATION,
        ),
        (
            "pages/logs_tf/detail/rounds.rs",
            "BLU UC",
            LOGS_TF_ABBREVIATION,
        ),
        (
            "pages/logs_tf/detail/rounds.rs",
            "RED UC",
            LOGS_TF_ABBREVIATION,
        ),
        (
            "pages/logs_tf/detail/rounds.rs",
            "BLU DA",
            LOGS_TF_ABBREVIATION,
        ),
        (
            "pages/logs_tf/detail/rounds.rs",
            "RED DA",
            LOGS_TF_ABBREVIATION,
        ),
        (
            "pages/logs_tf/mod.rs",
            "the stored body is damaged",
            "developer detail of an unreadable stored log",
        ),
        (
            "pages/logs_tf/mod.rs",
            "no stored body",
            "developer detail of an unreadable stored log",
        ),
    ];

    const SELECT_VALUE: &str = "Select value and row id; its label is keyed";
    const OS_NAME: &str = "a proper noun, the OS filter's label";
    const WEAPON_NAME: &str = "weapon name, as the game and logs.tf write it";
    const STOPPED_LABEL: &str = "developer detail: a stopped task's log label";
    const STORE_FIELD: &str = "developer detail: the store's tracing field";
    const ID_VALUE: &str = "element id value; its label is keyed";
    const DIALOG_ELSEWHERE: &str = "dialog of an untranslated page";
    const LOGS_TF_ABBREVIATION: &str = "logs.tf abbreviation, as logs.tf prints it";
    const STARTUP_DETAIL: &str =
        "developer detail: StartupError's log text; the view reads describe_startup";

    #[test]
    fn library_pages_show_no_english_literal() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut used = vec![false; ALLOWED.len()];
        let mut found = Vec::new();
        for rel in SCANNED {
            let source = std::fs::read_to_string(src.join(rel)).unwrap();
            for (line, text) in english_literals(shipped_part(&source)) {
                match ALLOWED.iter().position(|(f, t, _)| f == rel && *t == text) {
                    Some(at) => used[at] = true,
                    None => found.push(format!("{rel}:{line}: {text:?}")),
                }
            }
        }
        assert!(found.is_empty(), "English on the library pages: {found:#?}");
        let stale: Vec<_> = ALLOWED
            .iter()
            .zip(used)
            .filter(|(_, u)| !u)
            .map(|(a, _)| a)
            .collect();
        assert!(
            stale.is_empty(),
            "allowlist entries no file needs: {stale:?}"
        );
    }

    // A token of Rust source, as far as the English scan needs one.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Tok<'a> {
        // A plain string literal: its opening quote's byte offset and the text between the quotes.
        Str(usize, &'a str),
        Ident(&'a str),
        Open(u8),
        Close(u8),
        Punct(u8),
    }

    // The tokens of `code`. Comments, char literals, lifetimes, byte, C and raw strings leave no
    // token, so a quote inside one of them never opens a string.
    fn tokens(code: &str) -> Vec<Tok<'_>> {
        let b = code.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            match c {
                b'/' if b.get(i + 1) == Some(&b'/') => {
                    i = code[i..].find('\n').map_or(b.len(), |n| i + n);
                }
                b'/' if b.get(i + 1) == Some(&b'*') => i = block_comment_end(b, i),
                b'"' => {
                    let end = string_end(b, i + 1);
                    out.push(Tok::Str(i, &code[i + 1..end]));
                    i = end + 1;
                }
                b'\'' => i = after_quote(code, i),
                b'(' | b'[' | b'{' => {
                    out.push(Tok::Open(c));
                    i += 1;
                }
                b')' | b']' | b'}' => {
                    out.push(Tok::Close(c));
                    i += 1;
                }
                _ if c.is_ascii_alphanumeric() || c == b'_' => {
                    let end = b[i..]
                        .iter()
                        .position(|x| !(x.is_ascii_alphanumeric() || *x == b'_'))
                        .map_or(b.len(), |n| i + n);
                    let word = &code[i..end];
                    i = match (word, b.get(end)) {
                        ("b" | "c", Some(b'"')) => string_end(b, end + 1) + 1,
                        ("b", Some(b'\'')) => after_quote(code, end),
                        ("r" | "br" | "cr", Some(b'"' | b'#')) => raw_string_end(code, end),
                        _ => {
                            out.push(Tok::Ident(word));
                            end
                        }
                    };
                }
                _ if c.is_ascii_whitespace() => i += 1,
                _ => {
                    out.push(Tok::Punct(c));
                    i += 1;
                }
            }
        }
        out
    }

    // The offset of the quote closing a string whose text starts at `i`; escapes never close it.
    fn string_end(b: &[u8], mut i: usize) -> usize {
        while i < b.len() && b[i] != b'"' {
            i += if b[i] == b'\\' { 2 } else { 1 };
        }
        i.min(b.len())
    }

    // Past the block comment opening at `i`; Rust block comments nest.
    fn block_comment_end(b: &[u8], mut i: usize) -> usize {
        let mut depth = 0usize;
        while i < b.len() {
            if b[i..].starts_with(b"/*") {
                depth += 1;
                i += 2;
            } else if b[i..].starts_with(b"*/") {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return i;
                }
            } else {
                i += 1;
            }
        }
        i
    }

    // Past the char literal whose quote is at `i`, or past the quote alone for a lifetime or label.
    fn after_quote(code: &str, i: usize) -> usize {
        let rest = &code[i + 1..];
        if rest.starts_with('\\') {
            // An escape ('\n', '\'', '\u{a0}') closes at the first quote after its first two chars.
            return rest[2..].find('\'').map_or(code.len(), |n| i + 4 + n);
        }
        let mut chars = rest.chars();
        match (chars.next(), chars.next()) {
            (Some(c), Some('\'')) => i + 2 + c.len_utf8(),
            _ => i + 1,
        }
    }

    // Past the raw string whose hashes or quote start at `at`; a raw identifier stops at its name.
    fn raw_string_end(code: &str, at: usize) -> usize {
        let hashes = code[at..].bytes().take_while(|c| *c == b'#').count();
        let open = at + hashes;
        if code.as_bytes().get(open) != Some(&b'"') {
            return open;
        }
        let close = format!("\"{}", "#".repeat(hashes));
        code[open + 1..]
            .find(&close)
            .map_or(code.len(), |n| open + 1 + n + close.len())
    }

    // Macros whose text is for developers: tracing lines, invariants and panics.
    const DEVELOPER_MACROS: [&str; 10] = [
        "trace",
        "debug",
        "info",
        "warn",
        "error",
        "unreachable",
        "panic",
        "debug_assert",
        "debug_assert_eq",
        "debug_assert_ne",
    ];

    // The index of the token closing the bracket opened at `open`.
    fn closing(toks: &[Tok<'_>], open: usize) -> usize {
        let mut depth = 0usize;
        for (at, tok) in toks.iter().enumerate().skip(open) {
            match tok {
                Tok::Open(_) => depth += 1,
                Tok::Close(_) => {
                    depth -= 1;
                    if depth == 0 {
                        return at;
                    }
                }
                Tok::Str(..) | Tok::Ident(_) | Tok::Punct(_) => {}
            }
        }
        toks.len()
    }

    // For each token, whether it sits inside a developer macro or a `.log(` call, matched by brackets.
    fn developer_text(toks: &[Tok<'_>]) -> Vec<bool> {
        let mut dev = vec![false; toks.len()];
        for at in 0..toks.len() {
            let open = match toks[at..] {
                [Tok::Ident(name), Tok::Punct(b'!'), Tok::Open(_), ..]
                    if DEVELOPER_MACROS.contains(&name) =>
                {
                    at + 2
                }
                [Tok::Punct(b'.'), Tok::Ident("log"), Tok::Open(b'('), ..] => at + 2,
                _ => continue,
            };
            let close = closing(toks, open);
            for flag in &mut dev[open..close] {
                *flag = true;
            }
        }
        dev
    }

    // Whether `text` reads as player prose: two ASCII words one space apart, or a word that opens
    // with an uppercase letter and a lowercase one. "HUD", "HUDs" and "tf2tools-sounds" do not.
    fn looks_like_prose(text: &str) -> bool {
        let b = text.as_bytes();
        let mut words = Vec::new();
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_alphabetic() {
                let end = b[i..]
                    .iter()
                    .position(|c| !c.is_ascii_alphabetic())
                    .map_or(b.len(), |n| i + n);
                words.push((i, end));
                i = end;
            } else {
                i += 1;
            }
        }
        let adjacent = words
            .windows(2)
            .any(|w| w[1].0 == w[0].1 + 1 && b[w[0].1] == b' ');
        let capitalised = words.iter().any(|&(s, e)| {
            e - s >= 2 && b[s].is_ascii_uppercase() && b[s + 1].is_ascii_lowercase()
        });
        adjacent || capitalised
    }

    // The literals of `code` that read as player prose, as (1-based line, text), outside
    // developer text.
    fn english_literals(code: &str) -> Vec<(usize, &str)> {
        let toks = tokens(code);
        let dev = developer_text(&toks);
        let mut found = Vec::new();
        for (at, tok) in toks.iter().enumerate() {
            if let Tok::Str(quote, text) = *tok
                && !dev[at]
                && looks_like_prose(text)
            {
                found.push((code[..quote].matches('\n').count() + 1, text));
            }
        }
        found
    }
}
