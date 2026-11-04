//! Lossless KeyValues documents: an untouched file writes back byte-identical.

use std::path::{Path, PathBuf};
use tf2tools_core::Platform;
use tf2tools_core::{KvDocument, KvParseError};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hud/kv")
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(name)).unwrap()
}

fn round_trips(bytes: &[u8]) {
    let doc = KvDocument::parse(bytes).unwrap();
    assert_eq!(doc.as_bytes(), bytes);
}

#[test]
fn every_fixture_round_trips_byte_identical() {
    let mut seen = 0;
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "res") {
            round_trips(&std::fs::read(&path).unwrap());
            seen += 1;
        }
    }
    assert_eq!(seen, 3);
}

#[test]
fn quirks_round_trip_byte_identical() {
    // BOM and CRLF; an unterminated #base (tf2-recording-hud); unquoted `|`, `&` and `r640+15`
    // (colourhud); a `//` inside a bare token, typo and `_minbad` keys; conditionals before and
    // after values and one cut by a newline; stray braces and a dangling key; Latin-1 and NUL.
    const QUIRKS: &[&[u8]] = &[
        b"",
        b"\xef\xbb\xbf\"Scheme\"\r\n{\r\n\t\"Colors\" { \"A\" \"1 2 3 4\" } // tail\r\n}\r\n",
        b"#base \"../../#customization/Damage.res",
        b"\"settings\" { \"bind\" ammo|health&uber \"x\" r640+15 }",
        b"\"P\" { \"tall\" 15//p0.043\n \"paintboder\" \"0\" \"xpos_minbad\" \"9\" }",
        b"\"P\" { \"wide\" \"10\" [$_disabled_] \"tall\" [$WIN32] \"5\" [$X\n }",
        b"}}} \"stray\" { \"k\" } \"dangling\"",
        b"\"latin1\" \"caf\xe9\" \"nul\" \"a\x00b\"",
        b"\t\n  \x0b\x0c",
    ];
    for bytes in QUIRKS {
        round_trips(bytes);
    }
}

fn nested(levels: usize) -> Vec<u8> {
    let mut text = String::new();
    for _ in 0..levels {
        text.push_str("\"k\" { ");
    }
    for _ in 0..levels {
        text.push_str("} ");
    }
    text.into_bytes()
}

#[test]
fn oversized_and_overdeep_files_are_refused() {
    let big = vec![b' '; 8 * 1024 * 1024 + 1];
    assert!(matches!(
        KvDocument::parse(&big),
        Err(KvParseError::TooLarge { bytes }) if bytes == 8 * 1024 * 1024 + 1
    ));
    assert!(KvDocument::parse(&vec![b' '; 8 * 1024 * 1024]).is_ok());
    assert!(KvDocument::parse(&nested(64)).is_ok());
    assert!(matches!(
        KvDocument::parse(&nested(65)),
        Err(KvParseError::TooDeep)
    ));
}

#[test]
fn utf16_files_are_refused_as_utf16() {
    // Byte-lexing UTF-16 finds spurious braces in CJK code units, so it is named instead.
    let le: Vec<u8> = "\u{feff}\"a\" { }"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert!(matches!(KvDocument::parse(&le), Err(KvParseError::Utf16)));
    let be: Vec<u8> = "\u{feff}x"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    assert!(matches!(KvDocument::parse(&be), Err(KvParseError::Utf16)));
}

#[test]
fn documents_cross_threads() {
    fn assert_send_sync<T: Send + Sync + std::fmt::Debug + 'static>() {}
    assert_send_sync::<KvDocument>();
    assert_send_sync::<KvParseError>();
}

/// Run by hand: `TF2TOOLS_HUD_CORPUS=~/.cache/tf2tools-hud-research cargo test -p tf2tools-core
/// --test hud_document -- --ignored --nocapture`.
#[test]
#[ignore = "needs the HUD research corpus in TF2TOOLS_HUD_CORPUS"]
fn the_research_corpus_round_trips() {
    let root = std::env::var_os("TF2TOOLS_HUD_CORPUS").expect("set TF2TOOLS_HUD_CORPUS");
    let (mut checked, mut too_large, mut too_deep, mut utf16) = (0, 0, 0, 0);
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.unwrap();
        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if !entry.file_type().is_file()
            || !["res", "txt", "vmt", "vdf"]
                .iter()
                .any(|x| ext.eq_ignore_ascii_case(x))
        {
            continue;
        }
        let bytes = std::fs::read(entry.path()).unwrap();
        match KvDocument::parse(&bytes) {
            Ok(doc) => assert_eq!(doc.as_bytes(), bytes, "{}", entry.path().display()),
            Err(KvParseError::TooLarge { .. }) => too_large += 1,
            Err(KvParseError::TooDeep) => too_deep += 1,
            Err(KvParseError::Utf16) => utf16 += 1,
            Err(e) => panic!("{}: {e}", entry.path().display()),
        }
        checked += 1;
    }
    eprintln!(
        "{checked} files, {too_large} over the size limit, {too_deep} too deep, {utf16} UTF-16"
    );
    assert!(checked > 20_000, "only {checked} files under {root:?}");
}

#[test]
fn entries_keep_conditionals_lines_and_bare_words() {
    let doc = KvDocument::parse(&fixture("jofrehud-basechat.res")).unwrap();
    let bases: Vec<&str> = doc
        .entries()
        .iter()
        .filter(|e| e.key().eq_ignore_ascii_case("#base"))
        .filter_map(|e| e.text())
        .collect();
    assert_eq!(
        bases,
        [
            "../../../../cfg/jp_chat_wide.txt",
            "../../../../cfg/jp_chat_tall.txt",
            "../../../../cfg/jp_chat_xpos.txt",
            "../../../../cfg/jp_chat_ypos.txt",
            "_tf2hud/basechat.res",
        ]
    );
    let wide = doc.find(&["Resource/UI/BaseChat.res", "HudChat", "wide"]);
    assert_eq!(wide.len(), 1);
    assert_eq!((wide[0].key(), wide[0].text()), ("WIDE", Some("140")));
    assert_eq!((wide[0].cond(), wide[0].line()), (Some("$LINUX"), 17));
    assert!(wide[0].holds(Platform::Linux));
    assert!(!wide[0].holds(Platform::Windows));
    let field = doc.find(&["resource/ui/basechat.res", "chatinputline", "fieldName"]);
    assert_eq!(
        (field[0].text(), field[0].line()),
        (Some("ChatInputLine"), 31)
    );
}

#[test]
fn duplicate_keys_are_all_found_in_file_order() {
    let doc = KvDocument::parse(&fixture("jofrehud-huddamageaccount.res")).unwrap();
    let alpha = doc.find(&[
        "Resource/UI/HudDamageAccount.res",
        "CDamageAccountPanel",
        "alpha",
    ]);
    let got: Vec<(Option<&str>, u32)> = alpha.iter().map(|e| (e.text(), e.line())).collect();
    assert_eq!(got, [(Some("255"), 16), (Some("125"), 22)]);
}

#[test]
fn set_text_changes_only_that_value() {
    let original = fixture("jofrehud-huddamageaccount.res");
    let path = [
        "Resource/UI/HudDamageAccount.res",
        "DamageAccountLabel",
        "xpos",
    ];
    let mut doc = KvDocument::parse(&original).unwrap();
    let id = doc.find(&path)[0].value_id();
    assert!(doc.set_text(id, "c-60"));
    let expected = String::from_utf8(original).unwrap().replacen(
        "\"xpos\"\t\t\t\"c-50\"",
        "\"xpos\"\t\t\t\"c-60\"",
        1,
    );
    assert_eq!(doc.as_bytes(), expected.as_bytes());
    assert_eq!(doc.find(&path)[0].text(), Some("c-60"));
}

#[test]
fn set_text_quotes_only_when_needed() {
    let mut doc = KvDocument::parse(b"\"P\" { \"a\" bare \"b\" \"q\" }").unwrap();
    let a = doc.find(&["P", "a"])[0].value_id();
    let b = doc.find(&["P", "b"])[0].value_id();
    assert!(doc.set_text(a, "word"));
    assert!(doc.set_text(b, "two words"));
    assert_eq!(doc.as_bytes(), b"\"P\" { \"a\" word \"b\" \"two words\" }");
    assert!(doc.set_text(a, "has space"));
    assert!(doc.set_text(b, ""));
    assert!(!doc.set_text(a, "say \"hi\""), "a quote cannot be written");
    let block = doc.find(&["P"])[0].value_id();
    assert!(!doc.set_text(block, "x"), "a block is not text");
    assert_eq!(doc.as_bytes(), b"\"P\" { \"a\" \"has space\" \"b\" \"\" }");

    let mut open = KvDocument::parse(b"#base \"x").unwrap();
    let id = open.entries()[0].value_id();
    assert!(open.set_text(id, "y.res"));
    assert_eq!(open.as_bytes(), b"#base \"y.res\"");
}
