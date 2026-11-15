//! The app's translations: the offered languages, the process-wide locale and plural forms.

use std::sync::{Mutex, MutexGuard, PoisonError};

use gpui_kit::SharedString;

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
    let code = current();
    let form = format!("{key}.{}", plural_form(code, n).suffix());
    let other = format!("{key}.other");
    let text = crate::_rust_i18n_try_translate(code, &form)
        .or_else(|| crate::_rust_i18n_try_translate(code, &other))
        .map_or(other.clone(), |t| t.into_owned());
    rust_i18n::replace_patterns(&text, &["count"], &[n.to_string()]).into()
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

    use super::{LANGUAGES, PluralForm, current, plural_form};

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

    // Every literal "<surface>.<key>" in `source` whose body is key-shaped up to its closing quote.
    fn key_literals(source: &str, surfaces: &BTreeSet<String>) -> Vec<String> {
        let mut found = Vec::new();
        for surface in surfaces {
            let opener = format!("\"{surface}.");
            let mut rest = source;
            while let Some(at) = rest.find(&opener) {
                let body = &rest[at + 1..];
                let len = body
                    .find(|c: char| {
                        !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
                    })
                    .unwrap_or(body.len());
                if body[len..].starts_with('"') {
                    found.push(body[..len].to_owned());
                }
                rest = &body[len..];
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
}
