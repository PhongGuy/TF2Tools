//! The game's English text for `#token` labels, read from "lang" files.

use tf2tools_core::{GameText, HudFiles, Platform};

fn utf16le(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

fn utf16be(text: &str) -> Vec<u8> {
    let mut out = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

/// A synthetic lang file in the game's shape; no Valve bytes.
const LANG: &str = "\"lang\"\r\n{\r\n\"Language\" \"English\"\r\n\"Tokens\"\r\n{\r\n\
\"TF_Scoreboard_Name\"\t\t\"Name\"\r\n\
// a comment with \"quotes\" in it\r\n\
\"TF_Quote\"\t\"Turns out \\\"just bake it\\\" wasn't it\"\r\n\
\"TF_After\"\t\"Still in step\"\r\n\
\"TF_Escapes\"\t\"one\\ntwo\\tthree\\\\four\\qfive\"\r\n\
\"TF_Os\"\t\"Windows text\"\t[$WINDOWS]\r\n\
\"TF_Os\"\t\"Linux text\"\t[$LINUX]\r\n\
\"TF_Twice\"\t\"first\"\r\n\
\"TF_ClassMenu_Cancel\"\t\"CANCEL (&0)\"\r\n\
\"TF_Twice\"\t\"second\"\r\n\
\"TF_Unicode\"\t\"\u{dc}berCharge: %s1\"\r\n\
\"TF_Block\" { \"inner\" \"skipped\" }\r\n\
}\r\n}\r\n";

fn linux(bytes: &[u8]) -> GameText {
    GameText::parse(bytes, Platform::Linux)
}

#[test]
fn reads_a_utf16le_lang_file() {
    let text = linux(&utf16le(LANG));
    assert_eq!(text.get("TF_Scoreboard_Name"), Some("Name"));
    assert_eq!(text.get("TF_Unicode"), Some("\u{dc}berCharge: %s1"));
    assert_eq!(text.get("Language"), None, "only the Tokens block is read");
    assert_eq!(text.get("inner"), None, "nested blocks are skipped");
    assert_eq!(text.get("TF_Block"), None);
}

#[test]
fn reads_utf16be_and_utf8_too() {
    assert_eq!(linux(&utf16be(LANG)).get("TF_After"), Some("Still in step"));
    assert_eq!(
        linux(LANG.as_bytes()).get("TF_After"),
        Some("Still in step")
    );
    let mut bom = b"\xef\xbb\xbf".to_vec();
    bom.extend_from_slice(LANG.as_bytes());
    assert_eq!(linux(&bom).get("TF_Scoreboard_Name"), Some("Name"));
}

#[test]
fn lookup_ignores_ascii_case_and_takes_no_hash() {
    let text = linux(&utf16le(LANG));
    assert_eq!(text.get("tf_scoreboard_NAME"), Some("Name"));
    assert_eq!(text.get("#TF_Scoreboard_Name"), None);
    assert_eq!(text.get("TF_Missing"), None);
}

#[test]
fn escaped_quotes_keep_later_pairs_in_step() {
    let text = linux(&utf16le(LANG));
    assert_eq!(
        text.get("TF_Quote"),
        Some("Turns out \"just bake it\" wasn't it")
    );
    assert_eq!(text.get("TF_After"), Some("Still in step"));
    assert_eq!(text.get("a"), None, "a comment's quotes open no string");
}

#[test]
fn known_escapes_decode_and_others_stay() {
    let text = linux(&utf16le(LANG));
    assert_eq!(text.get("TF_Escapes"), Some("one\ntwo\tthree\\four\\qfive"));
}

#[test]
fn conditions_pick_the_platforms_pair_and_later_pairs_win() {
    let utf16 = utf16le(LANG);
    assert_eq!(linux(&utf16).get("TF_Os"), Some("Linux text"));
    let windows = GameText::parse(&utf16, Platform::Windows);
    assert_eq!(windows.get("TF_Os"), Some("Windows text"));
    assert_eq!(windows.get("TF_Twice"), Some("second"));
}

#[test]
fn show_resolves_known_tokens_and_keeps_everything_else() {
    let text = linux(&utf16le(LANG));
    assert_eq!(text.show("#tf_scoreboard_name"), "Name");
    assert_eq!(text.show("#TF_Missing"), "#TF_Missing");
    assert_eq!(text.show("TF_Scoreboard_Name"), "TF_Scoreboard_Name");
    assert_eq!(text.show("%playername%"), "%playername%");
    assert_eq!(text.show("#"), "#");
    assert_eq!(text.show(""), "");
}

#[test]
fn show_hides_the_hotkey_marker_but_get_keeps_the_stored_text() {
    let text = linux(&utf16le(LANG));
    assert_eq!(text.show("#TF_ClassMenu_Cancel"), "CANCEL (0)");
    assert_eq!(text.get("TF_ClassMenu_Cancel"), Some("CANCEL (&0)"));
    // A file's own labelText follows the same rule: `&&` draws one `&`.
    assert_eq!(text.show("&Fire"), "Fire");
    assert_eq!(text.show("Salt && &Pepper"), "Salt & Pepper");
    assert_eq!(text.show("&&&&"), "&&");
    assert_eq!(text.show("Ends with &"), "Ends with ");
    assert_eq!(text.show("&"), "");
    assert_eq!(text.show("No marker"), "No marker");
}

#[test]
fn malformed_input_reads_what_it_can() {
    assert!(linux(b"").is_empty());
    assert!(linux(b"garbage { } }}} \"").is_empty());
    assert!(linux(&utf16le("\"lang\" { \"Language\" \"English\" }")).is_empty());
    // Cut off mid-file: the pairs before the cut still count.
    let cut = LANG.split("\"TF_After\"").next().unwrap();
    let text = linux(&utf16le(cut));
    assert_eq!(text.get("TF_Scoreboard_Name"), Some("Name"));
    assert_eq!(text.get("TF_After"), None);
    assert_eq!(text.len(), 2);
}

#[test]
fn decoding_never_panics() {
    // Odd length, a lone high surrogate, a lone low surrogate.
    let mut odd = utf16le("\"lang\" { \"Tokens\" { \"TF_A\" \"x\" } }");
    odd.push(0x41);
    assert_eq!(linux(&odd).get("TF_A"), Some("x"));
    let mut lone = vec![0xFF, 0xFE];
    for unit in [0xD800u16, 0x0022, 0xDC00, 0x0022] {
        lone.extend_from_slice(&unit.to_le_bytes());
    }
    assert!(linux(&lone).is_empty());
}

#[test]
fn input_past_the_cap_is_ignored() {
    // Bytes past the size cap are never read, so a pair beyond it is absent.
    let mut big = "\"lang\" { \"Tokens\" { \"TF_A\" \"x\" "
        .as_bytes()
        .to_vec();
    big.resize(17 * 1024 * 1024, b' ');
    big.extend_from_slice(b"\"TF_B\" \"y\" } }");
    let text = linux(&big);
    assert_eq!(text.get("TF_A"), Some("x"));
    assert_eq!(text.get("TF_B"), None);
}

fn lang(pairs: &str) -> Vec<u8> {
    utf16le(&format!(
        "\"lang\" {{ \"Language\" \"English\" \"Tokens\" {{ {pairs} }} }}"
    ))
}

fn files(layers: Vec<Vec<(&str, Vec<u8>)>>) -> HudFiles {
    HudFiles::from_memory(
        layers
            .into_iter()
            .map(|l| l.into_iter().map(|(p, b)| (p.to_owned(), b)).collect())
            .collect(),
    )
}

#[test]
fn load_merges_gameui_then_tf_with_tf_winning() {
    let files = files(vec![vec![
        (
            "resource/tf_english.txt",
            lang("\"TF_Only\" \"tf\" \"Both\" \"from tf\""),
        ),
        (
            "resource/gameui_english.txt",
            lang("\"GameUI_Only\" \"gameui\" \"Both\" \"from gameui\""),
        ),
        ("resource/chat_english.txt", lang("\"Chat_Only\" \"chat\"")),
    ]]);
    let text = GameText::load(&files, Platform::Linux);
    assert_eq!(text.get("TF_Only"), Some("tf"));
    assert_eq!(text.get("GameUI_Only"), Some("gameui"));
    assert_eq!(text.get("Both"), Some("from tf"));
    assert_eq!(text.get("Chat_Only"), None, "only FILES are read");
}

#[test]
fn load_finds_files_ignoring_case_from_the_top_layer() {
    let files = files(vec![
        vec![("Resource/TF_English.txt", lang("\"TF_A\" \"top\""))],
        vec![(
            "resource/tf_english.txt",
            lang("\"TF_A\" \"below\" \"TF_B\" \"below\""),
        )],
    ]);
    let text = GameText::load(&files, Platform::Linux);
    assert_eq!(text.get("TF_A"), Some("top"));
    assert_eq!(
        text.get("TF_B"),
        None,
        "the top layer's file hides the one below"
    );
}

#[test]
fn load_without_lang_files_is_empty() {
    let files = files(vec![vec![(
        "resource/clientscheme.res",
        b"\"Scheme\" { }".to_vec(),
    )]]);
    assert!(GameText::load(&files, Platform::Linux).is_empty());
}
