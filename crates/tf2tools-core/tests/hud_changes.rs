//! Semantic changes replayed as token edits: one value span, insertion, order, and what drops.

mod common;

use common::hud_stock::{HEALTH, stock_files};
use tf2tools_core::{
    Change, HiddenBy, HudModel, KvDocument, PartKey, Platform, Screen, SetKey, check_hud_path,
};

const HP: &str = "resource/ui/hudplayerhealth.res";
const HV_BLOCK: [&str; 2] = ["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"];
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn set(group: u32, file: &str, block: &[&str], key: &str, cond: Option<&str>, new: &str) -> Change {
    Change::set(group, SetKey::new(file, block, key, cond, None, new))
}

fn model() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}

#[test]
fn insert_text_adds_a_line_before_the_closing_brace() {
    let mut doc =
        KvDocument::parse(b"\"R\"\n{\n\t\"A\"\n\t{\n\t\t\"xpos\"\t\"1\"\n\t}\n}\n").unwrap();
    assert!(doc.insert_text(&["R", "A"], "visible", "0", None));
    assert!(doc.insert_text(&["r", "a"], "ypos", "9999", Some("$WIN32")));
    assert_eq!(doc.as_bytes(), b"\"R\"\n{\n\t\"A\"\n\t{\n\t\t\"xpos\"\t\"1\"\n\t\t\"visible\"\t\"0\"\n\t\t\"ypos\"\t\"9999\"\t[$WIN32]\n\t}\n}\n");
    let mut crlf = KvDocument::parse(b"\"R\"\r\n{\r\n}\r\n").unwrap();
    assert!(crlf.insert_text(&["R"], "k", "v", None));
    assert_eq!(crlf.as_bytes(), b"\"R\"\r\n{\r\n\t\"k\"\t\"v\"\r\n}\r\n");
    let mut inline = KvDocument::parse(b"\"R\" { \"a\" \"1\" }").unwrap();
    assert!(inline.insert_text(&["R"], "b", "2", None));
    assert_eq!(inline.as_bytes(), b"\"R\" { \"a\" \"1\" \"b\" \"2\" }");
    assert!(!inline.insert_text(&["R", "Missing"], "b", "2", None));
    let mut glued = KvDocument::parse(b"\"R\" { \"a\" \"1\"}").unwrap();
    assert!(glued.insert_text(&["R"], "b", "2", None));
    assert_eq!(glued.as_bytes(), b"\"R\" { \"a\" \"1\" \"b\" \"2\" }");
}

#[test]
fn a_set_matches_an_existing_conditional_in_any_case() {
    let block = ["Resource/UI/HudPlayerHealth.res", "HudPlayerHealth"];
    let edited = model().with_changes(&[set(1, HP, &block, "xpos", Some("$win32"), "40")]);
    let text = std::str::from_utf8(edited.model().document(HP).unwrap().as_bytes())
        .unwrap()
        .to_owned();
    assert!(text.contains("xpos 40 [$WIN32]"), "{text}");
    assert!(!text.contains("[$win32]"), "{text}");
}

#[test]
fn insert_text_refuses_quotes_and_line_breaks() {
    let mut doc = KvDocument::parse(b"\"R\"\n{\n}\n").unwrap();
    let bad = [
        ("k\"", "v", None),
        ("k\nx", "v", None),
        ("k", "v\nx", None),
        ("k", "v\r", None),
        ("k", "a \"b\"", None),
        ("k", "v", Some("$WIN32]\n\"x")),
        ("k", "v", Some("$X\"360")),
    ];
    for (key, value, cond) in bad {
        assert!(
            !doc.insert_text(&["R"], key, value, cond),
            "{key:?} {value:?} {cond:?}"
        );
    }
    assert_eq!(doc.as_bytes(), b"\"R\"\n{\n}\n");
}

#[test]
fn a_set_changes_one_value_span_and_nothing_else() {
    let pristine = model();
    let edited = pristine.with_changes(&[set(1, HP, &HV_BLOCK, "fgcolor", None, "255 80 80 255")]);
    assert_eq!((edited.dropped(), edited.applied().len()), (0, 1));
    assert_eq!(edited.touched(), [HP]);
    let want = HEALTH.replace("fgcolor TanDark", "fgcolor \"255 80 80 255\"");
    assert_eq!(
        edited.model().document(HP).unwrap().as_bytes(),
        want.as_bytes()
    );
    let part = edited.model().part(&PartKey::new(HV)).unwrap();
    assert_eq!(part.value("fgcolor").unwrap().text(), "255 80 80 255");
    assert_eq!(
        pristine.document(HP).unwrap().as_bytes(),
        HEALTH.as_bytes(),
        "the pristine model is untouched"
    );
}

#[test]
fn absent_keys_are_inserted_and_cond_picks_the_platform_line() {
    let block = ["Resource/UI/HudPlayerHealth.res", "HudPlayerHealth"];
    let edited = model().with_changes(&[
        set(1, HP, &HV_BLOCK, "visible", None, "0"),
        set(2, HP, &block, "xpos", Some("$X360"), "40"),
    ]);
    let text = std::str::from_utf8(edited.model().document(HP).unwrap().as_bytes())
        .unwrap()
        .to_owned();
    assert!(text.contains("    labelText %Health%\n    \"visible\"\t\"0\"\n  }"));
    assert!(text.contains("xpos 0 [$WIN32]") && text.contains("xpos 40 [$X360]"));
    let health = edited
        .model()
        .part(&PartKey::new("HudPlayerStatus/HudPlayerHealth"))
        .unwrap();
    assert_eq!(health.value("xpos").unwrap().text(), "0");
}

#[test]
fn later_changes_to_the_same_key_win() {
    let edited = model().with_changes(&[
        set(1, HP, &HV_BLOCK, "fgcolor", None, "1 2 3 255"),
        set(2, HP, &HV_BLOCK, "fgcolor", None, "TanLight"),
    ]);
    let part = edited.model().part(&PartKey::new(HV)).unwrap();
    assert_eq!(part.value("fgcolor").unwrap().text(), "TanLight");
}

#[test]
fn changes_that_do_not_resolve_are_dropped_and_counted() {
    let edited = model().with_changes(&[
        set(
            1,
            "resource/ui/scoreboard.res",
            &["Resource/UI/Scoreboard.res", "x"],
            "xpos",
            None,
            "1",
        ),
        set(
            2,
            HP,
            &["Resource/UI/HudPlayerHealth.res", "Gone"],
            "xpos",
            None,
            "1",
        ),
        set(3, "../../cfg/autoexec.cfg", &["R"], "k", None, "v"),
        set(4, HP, &HV_BLOCK, "labelText", None, "say \"hi\""),
        set(6, HP, &HV_BLOCK, "labelText", None, "two\nlines"),
        set(7, HP, &HV_BLOCK, "note\"", None, "x"),
        set(5, HP, &HV_BLOCK, "zpos", None, "9"),
    ]);
    assert_eq!(edited.dropped(), 6);
    assert_eq!(
        edited
            .applied()
            .iter()
            .map(Change::group)
            .collect::<Vec<_>>(),
        [5]
    );
}

#[test]
fn a_hide_by_change_shows_in_the_layout() {
    let edited = model().with_changes(&[set(
        1,
        "scripts/hudlayout.res",
        &["Resource/HudLayout.res", "CHealthAccountPanel"],
        "ypos",
        None,
        "9999",
    )]);
    let layout = edited.model().layout(Screen::new(1920, 1080));
    assert_eq!(
        layout
            .get(&PartKey::new("CHealthAccountPanel"))
            .unwrap()
            .hidden(),
        Some(HiddenBy::OffScreen)
    );
}

#[test]
fn the_path_rule() {
    let allowed = [
        (
            "resource/ui/HudPlayerHealth.res",
            "resource/ui/hudplayerhealth.res",
        ),
        ("resource\\ui\\a.res", "resource/ui/a.res"),
        ("./scripts/hudlayout.res", "scripts/hudlayout.res"),
        ("resource/console.res", "resource/console.res"),
        ("resource/comic.res", "resource/comic.res"),
        ("resource/nullable.res", "resource/nullable.res"),
        ("resource/com10.res", "resource/com10.res"),
        ("resource/lpt.res", "resource/lpt.res"),
    ];
    for (path, want) in allowed {
        assert_eq!(check_hud_path(path).as_deref(), Some(want), "{path}");
    }
    let refused = [
        "",
        "../x.res",
        "a/../../b.res",
        "/etc/passwd",
        "\\x.res",
        "C:/x.res",
        "cfg/autoexec.cfg",
        "CFG/x.txt",
        "resource/../cfg/x.txt",
        "resource/x.cfg",
        "a.BAT",
        "a.ps1",
        "a.exe",
        "a.dll",
        "pak01_dir.vpk",
        "resource/ui/a\0.res",
        "x.cfg.",
        "x.cfg ",
        "pak01_dir.vpk.",
        "x.exe ",
        "cfg./x.txt",
        "cfg /x.txt",
        ".. /x",
        "...",
        "con.res",
        "NUL",
        "aux/x.res",
        "COM0",
        "com0.res",
        "LPT0.txt",
        "com9 .res",
        "COM¹",
        "com².res",
        "resource/com³",
        "LPT¹.res",
        "lpt²",
        "lpt³.txt",
        "CONIN$",
        "conin$.res",
        "ConOut$.txt",
        "conout$",
        "clock$",
        "CLOCK$ .res",
    ];
    for path in refused {
        assert_eq!(check_hud_path(path), None, "{path:?}");
    }
}
