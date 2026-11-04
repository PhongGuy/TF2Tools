//! Byte-exact removal of one entry and insertion of a whole block.

use tf2tools_core::{KvDocument, KvValueId};

fn doc(text: &str) -> KvDocument {
    KvDocument::parse(text.as_bytes()).unwrap()
}

/// The value id of the `nth` direct entry named `key` in the last block on `path`.
fn id(d: &KvDocument, path: &[&str], key: &str, nth: usize) -> KvValueId {
    d.find(path)
        .into_iter()
        .rfind(|e| e.text().is_none())
        .unwrap()
        .children()
        .iter()
        .filter(|e| e.key().eq_ignore_ascii_case(key))
        .nth(nth)
        .unwrap()
        .value_id()
}

fn text(d: &KvDocument) -> &str {
    std::str::from_utf8(d.as_bytes()).unwrap()
}

#[test]
fn an_entry_alone_on_its_line_takes_the_line() {
    let mut d = doc("\"R\"\n{\n\t\"a\"\t\"1\"\n\t\"b\"\t\"2\"\t[$WIN32]\n\t\"c\"\t\"3\"\n}\n");
    let b = id(&d, &["R"], "b", 0);
    assert!(d.remove_entry(b));
    assert_eq!(text(&d), "\"R\"\n{\n\t\"a\"\t\"1\"\n\t\"c\"\t\"3\"\n}\n");
    let mut crlf = doc("\"R\"\r\n{\r\n\t\"a\"\t\"1\"\r\n\t\"b\"\t\"2\"\r\n}\r\n");
    let b = id(&crlf, &["R"], "b", 0);
    assert!(crlf.remove_entry(b));
    assert_eq!(text(&crlf), "\"R\"\r\n{\r\n\t\"a\"\t\"1\"\r\n}\r\n");
}

#[test]
fn an_entry_sharing_its_line_leaves_the_rest() {
    let mut d = doc("R { a 1 b 2 c 3 }");
    let b = id(&d, &["R"], "b", 0);
    assert!(d.remove_entry(b));
    assert_eq!(text(&d), "R { a 1 c 3 }");
    let mut last = doc("R { a 1 b 2 }");
    let b = id(&last, &["R"], "b", 0);
    assert!(last.remove_entry(b));
    assert_eq!(text(&last), "R { a 1 }");
}

#[test]
fn a_trailing_comment_stays() {
    let mut d = doc("\"R\"\n{\n\t\"b\" \"2\" // note\n\t\"c\" \"3\"\n}\n");
    let b = id(&d, &["R"], "b", 0);
    assert!(d.remove_entry(b));
    assert_eq!(text(&d), "\"R\"\n{\n\t// note\n\t\"c\" \"3\"\n}\n");
}

#[test]
fn a_leading_conditional_goes_with_its_entry() {
    let mut d = doc("\"R\"\n{\n\t\"b\" [$X360] \"2\"\n\t\"c\" \"3\"\n}\n");
    let b = id(&d, &["R"], "b", 0);
    assert!(d.remove_entry(b));
    assert_eq!(text(&d), "\"R\"\n{\n\t\"c\" \"3\"\n}\n");
}

#[test]
fn blocks_and_unknown_ids_are_refused() {
    let mut d = doc("R { S { k v } }");
    let s = d.find(&["R", "S"])[0].value_id();
    assert!(!d.remove_entry(s));
    let k = id(&d, &["R", "S"], "k", 0);
    let mut other = doc("Q { k v x y z w }");
    let far = id(&other, &["Q"], "z", 0);
    assert!(
        !d.remove_entry(far),
        "an id no text entry of this document owns"
    );
    assert_eq!(text(&d), "R { S { k v } }");
    assert!(d.remove_entry(k));
    assert_eq!(text(&d), "R { S { } }");
    assert!(other.remove_entry(far));
}

#[test]
fn insert_block_lays_out_like_insert_text() {
    let mut d = doc("\"S\"\n{\n\t\"Fonts\"\n\t{\n\t\t\"A\"\n\t\t{\n\t\t}\n\t}\n}\n");
    assert!(d.insert_block(
        &["s", "fonts"],
        "3",
        Some("$WINDOWS"),
        &[("name", "TF2", None), ("tall", "12", Some("$LINUX"))],
    ));
    assert_eq!(
        text(&d),
        "\"S\"\n{\n\t\"Fonts\"\n\t{\n\t\t\"A\"\n\t\t{\n\t\t}\n\t\t\"3\"\t[$WINDOWS]\n\t\t{\n\t\t\t\"name\"\t\"TF2\"\n\t\t\t\"tall\"\t\"12\"\t[$LINUX]\n\t\t}\n\t}\n}\n"
    );
    let block = d.find(&["S", "Fonts", "3"]);
    assert_eq!(block.len(), 1);
    assert_eq!(block[0].cond(), Some("$WINDOWS"));
    let keys: Vec<(&str, Option<&str>, Option<&str>)> = block[0]
        .children()
        .iter()
        .map(|e| (e.key(), e.text(), e.cond()))
        .collect();
    assert_eq!(
        keys,
        [
            ("name", Some("TF2"), None),
            ("tall", Some("12"), Some("$LINUX"))
        ]
    );
}

#[test]
fn insert_block_into_an_empty_or_inline_block() {
    let mut empty = doc("\"S\"\r\n{\r\n}\r\n");
    assert!(empty.insert_block(&["S"], "B", None, &[]));
    assert_eq!(text(&empty), "\"S\"\r\n{\r\n\t\"B\"\r\n\t{\r\n\t}\r\n}\r\n");
    let mut inline = doc("\"S\" { \"Fonts\" { } }");
    assert!(inline.insert_block(&["S", "Fonts"], "X", None, &[("k", "v", None)]));
    assert_eq!(
        text(&inline),
        "\"S\" { \"Fonts\" { \"X\" { \"k\" \"v\" } } }"
    );
}

#[test]
fn insert_block_refuses_bad_text_and_missing_parents() {
    let mut d = doc("S { }");
    assert!(!d.insert_block(&["Missing"], "B", None, &[]));
    assert!(!d.insert_block(&["S"], "B\"", None, &[]));
    assert!(!d.insert_block(&["S"], "B", Some("$X]"), &[]));
    assert!(!d.insert_block(&["S"], "B", None, &[("k", "line\nbreak", None)]));
    assert_eq!(text(&d), "S { }");
}

#[test]
fn neighbours_glued_to_a_removed_entry_stay_apart() {
    let mut d = doc("R { \"a\" 1\"b\" \"2\"c 3 }");
    let b = id(&d, &["R"], "b", 0);
    assert!(d.remove_entry(b));
    assert_eq!(text(&d), "R { \"a\" 1 c 3 }");
    let a = d.find(&["R", "a"]);
    assert_eq!(a[0].text(), Some("1"), "a keeps its value");
}

#[test]
fn insert_block_refuses_an_empty_name_or_a_blank_cond() {
    let mut d = doc("S { }");
    assert!(!d.insert_block(&["S"], "", None, &[]));
    assert!(!d.insert_block(&["S"], " ", None, &[]));
    assert!(!d.insert_block(&["S"], "B", Some(""), &[]));
    assert!(!d.insert_block(&["S"], "B", Some(" \t"), &[]));
    assert!(!d.insert_block(&["S"], "B", None, &[("k", "v", Some(""))]));
    assert_eq!(text(&d), "S { }");
}
