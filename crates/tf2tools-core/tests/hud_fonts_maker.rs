//! Stage 2 fonts: the scheme over the edited clientscheme.res, per-part font clones and added
//! font files.

mod common;

use common::hud_stock::{HEALTH, stock_files_with};
use tf2tools_core::{
    AddBlock, AssetFile, BlockLine, Change, ChangeTarget, EditRefused, FontEdit, HudFiles,
    HudModel, PartKey, Platform, Scheme, SetKey, font_families, font_family,
};

const CS: &str = "resource/clientscheme.res";

const OWN: &str = r#"#base "clientscheme_base.res"
"Scheme"
{
	"Colors"
	{
		"TanDark"	"1 2 3 255"
	}
	"Fonts"
	{
		"Own"
		{
			"1"
			{
				"name"	"A"
				"tall"	"10"
			}
		}
	}
}
"#;

const BASE: &str = r#""Scheme" { Colors { TanDark "9 9 9 255" BaseOnly "0 255 0 255" }
  Fonts { Own { "1" { name B tall 99 weight 5 } } BaseFont { "1" { name C tall 7 } } } }"#;

fn files_with(scheme: &str, extra: &[(&str, Option<&str>)]) -> HudFiles {
    let mut overrides = vec![(CS, Some(scheme))];
    overrides.extend_from_slice(extra);
    stock_files_with(&overrides)
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn the_scheme_follows_the_edited_document() {
    let base = [("resource/clientscheme_base.res", Some(BASE))];
    let m = HudModel::load(&files_with(OWN, &base), Platform::Linux).unwrap();
    assert!(m.document(CS).is_some());
    let set = SetKey::new(
        CS,
        &["Scheme", "Colors"],
        "TanDark",
        None,
        Some("1 2 3 255"),
        "4 5 6 255",
    );
    let edited = m.with_changes(&[Change::set(1, set)]);
    assert_eq!(edited.touched(), [CS]);
    let bytes =
        String::from_utf8(edited.model().document(CS).unwrap().as_bytes().to_vec()).unwrap();
    let want = Scheme::load(&files_with(&bytes, &base), Platform::Linux).unwrap();
    assert_eq!(edited.model().scheme(), &want);
    let tan = edited.model().scheme().named_colour("TanDark").unwrap();
    assert_eq!(tan.to_array(), [4, 5, 6, 255]);
    let own = edited.model().scheme().font("Own", 1080).unwrap();
    assert_eq!(
        (own.family(), own.tall(), own.weight()),
        ("A", 10, 5),
        "merged key by key"
    );
    assert_eq!(
        m.scheme().named_colour("TanDark").unwrap().to_array(),
        [1, 2, 3, 255]
    );
}

fn tan(value: &str) -> Change {
    let set = SetKey::new(
        CS,
        &["Scheme", "Colors"],
        "TanDark",
        None,
        Some("1 2 3 255"),
        value,
    );
    Change::set(1, set)
}

#[test]
fn replays_with_the_same_scheme_bytes_share_one_scheme() {
    let m = HudModel::load(&files_with(OWN, &[]), Platform::Linux).unwrap();
    let a = m.with_changes(&[tan("4 5 6 255")]);
    let b = m.with_changes(&[tan("4 5 6 255")]);
    assert!(
        std::ptr::eq(a.model().scheme(), b.model().scheme()),
        "rebuilt once"
    );
    let c = m.with_changes(&[tan("7 7 7 255")]);
    assert_eq!(
        c.model()
            .scheme()
            .named_colour("TanDark")
            .unwrap()
            .to_array(),
        [7, 7, 7, 255]
    );
    let back = m.with_changes(&[tan("1 2 3 255")]);
    assert!(
        std::ptr::eq(back.model().scheme(), m.scheme()),
        "the loaded bytes again"
    );
}

#[test]
fn a_replay_that_misses_the_scheme_keeps_the_loaded_one() {
    let m = HudModel::load(&files_with(OWN, &[]), Platform::Linux).unwrap();
    let block = ["Resource/UI/HudDamageAccount.res", "CDamageAccountPanel"];
    let set = SetKey::new(
        "resource/ui/huddamageaccount.res",
        &block,
        "zpos",
        None,
        None,
        "3",
    );
    let edited = m.with_changes(&[Change::set(1, set)]);
    assert_eq!(edited.touched(), ["resource/ui/huddamageaccount.res"]);
    assert!(std::ptr::eq(edited.model().scheme(), m.scheme()));
    assert!(std::ptr::eq(
        m.with_changes(&[]).model().scheme(),
        m.scheme()
    ));
}

#[test]
fn an_unparsable_or_rootless_scheme_is_absent_without_a_problem() {
    let deep = format!("\"Scheme\" {}", "{ a ".repeat(70));
    for scheme in [deep.as_str(), r#""Scheme" "text""#] {
        let m = HudModel::load(&files_with(scheme, &[]), Platform::Linux).unwrap();
        assert!(m.document(CS).is_none(), "{scheme:.20}");
        assert!(m.problems().is_empty(), "{scheme:.20}");
        assert!(
            m.with_changes(&[tan("4 5 6 255")])
                .model()
                .scheme()
                .font_names()
                .is_empty()
        );
    }
}

#[test]
fn an_added_font_block_reaches_the_scheme() {
    let m = HudModel::load(&files_with(OWN, &[]), Platform::Linux).unwrap();
    let add = AddBlock::new(CS, &["Scheme", "Fonts"], "TF2Tools_1", None, Vec::new());
    let variant = AddBlock::new(
        CS,
        &["Scheme", "Fonts", "TF2Tools_1"],
        "1",
        None,
        vec![
            BlockLine::new("name", "Z", None),
            BlockLine::new("tall", "20", None),
        ],
    );
    let edited = m.with_changes(&[Change::add_block(1, add), Change::add_block(1, variant)]);
    assert_eq!(edited.dropped(), 0);
    let font = edited.model().scheme().font("tf2tools_1", 1080).unwrap();
    assert_eq!((font.family(), font.tall()), ("Z", 20));
    assert!(m.scheme().font("TF2Tools_1", 1080).is_none());
}

#[test]
fn a_hud_without_a_clientscheme_still_loads() {
    let m = HudModel::load(&stock_files_with(&[(CS, None)]), Platform::Linux).unwrap();
    assert!(m.document(CS).is_none());
    assert!(m.problems().is_empty());
    assert!(
        m.part(&k("HudPlayerStatus/HudPlayerHealth"))
            .unwrap()
            .is_readable()
    );
}

const DAMAGE: &str = "CDamageAccountPanel";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const DMG_FILE: &str = "resource/ui/huddamageaccount.res";

pub const FONTS: &str = r#""Scheme"
{
	"Colors"
	{
		"TanLight"	"235 226 202 255"
	}
	"Fonts"
	{
		"HudFontMedium"
		{
			"1"
			{
				"name"	"TF2 Build"
				"tall"	"14"	[$WINDOWS]
				"tall"	"16"	[$LINUX]
				"outline"	"0"
			}
			"2"	[$WINDOWS]
			{
				"name"	"TF2 Build"
				"tall"	"20"
			}
		}
		"Default"
		{
			"1"
			{
				"name"	"Verdana"
				"tall"	"12"
			}
		}
		"TF2Tools_4"
		{
			"1"
			{
				"name"	"Verdana"
				"tall"	"9"
			}
		}
	}
}
"#;

/// FONTS over the stock files, the health value without a `font` line of its own.
fn fonts_model() -> HudModel {
    let health = HEALTH.replace("    font HudClassHealth\n", "");
    let extra = [("resource/ui/hudplayerhealth.res", Some(health.as_str()))];
    HudModel::load(&files_with(FONTS, &extra), Platform::Linux).unwrap()
}

fn line(key: &str, value: &str, cond: Option<&str>) -> BlockLine {
    BlockLine::new(key, value, cond)
}

#[test]
fn a_clone_copies_every_platform_line() {
    let m = fonts_model();
    let changes = m
        .edit_font(
            &k(DAMAGE),
            "delta_item_font",
            FontEdit::Outline(true),
            1080,
            5,
        )
        .unwrap();
    let fonts = ["Scheme", "Fonts"];
    let clone = ["Scheme", "Fonts", "TF2Tools_5"];
    let want = vec![
        Change::add_block(5, AddBlock::new(CS, &fonts, "TF2Tools_5", None, Vec::new())),
        Change::add_block(
            5,
            AddBlock::new(
                CS,
                &clone,
                "1",
                None,
                vec![
                    line("name", "TF2 Build", None),
                    line("tall", "14", Some("$WINDOWS")),
                    line("tall", "16", Some("$LINUX")),
                    line("outline", "1", None),
                ],
            ),
        ),
        Change::add_block(
            5,
            AddBlock::new(
                CS,
                &clone,
                "2",
                Some("$WINDOWS"),
                vec![
                    line("name", "TF2 Build", None),
                    line("tall", "20", None),
                    line("outline", "1", None),
                ],
            ),
        ),
        Change::set(
            5,
            SetKey::new(
                DMG_FILE,
                &["Resource/UI/HudDamageAccount.res", DAMAGE],
                "delta_item_font",
                None,
                Some("HudFontMedium"),
                "TF2Tools_5",
            ),
        ),
    ];
    assert_eq!(changes, want);
    let after = m.with_changes(&changes);
    let after = after.model();
    let part = after.part(&k(DAMAGE)).unwrap();
    assert_eq!(
        part.value("delta_item_font").map(|l| l.text()),
        Some("TF2Tools_5")
    );
    let font = after.scheme().font("TF2Tools_5", 1080).unwrap();
    assert_eq!(
        (font.family(), font.tall(), font.outline()),
        ("TF2 Build", 16, true)
    );
    assert!(
        !after
            .scheme()
            .font("HudFontMedium", 1080)
            .unwrap()
            .outline(),
        "the original stays"
    );
}

/// A `Set` as (block, cond, old, value).
type SetRow = (Vec<String>, Option<String>, Option<String>, String);

#[test]
fn a_clone_is_edited_in_place_and_sizes_scale_together() {
    let m = fonts_model();
    let first = m
        .edit_font(
            &k(DAMAGE),
            "delta_item_font",
            FontEdit::Outline(true),
            1080,
            1,
        )
        .unwrap();
    let m2 = m.with_changes(&first).into_model();
    let size = m2
        .edit_font(&k(DAMAGE), "delta_item_font", FontEdit::Size(32), 1080, 2)
        .unwrap();
    let rows: Vec<SetRow> = size
        .iter()
        .map(|c| {
            let ChangeTarget::Set(s) = c.target() else {
                panic!("an in-place edit only sets")
            };
            assert_eq!((s.file(), s.key()), (CS, "tall"));
            (
                s.block().to_vec(),
                s.cond().map(str::to_owned),
                s.old().map(str::to_owned),
                s.value().to_owned(),
            )
        })
        .collect();
    let path = |v: &str| {
        vec![
            "Scheme".to_owned(),
            "Fonts".to_owned(),
            "TF2Tools_5".to_owned(),
            v.to_owned(),
        ]
    };
    let some = |s: &str| Some(s.to_owned());
    assert_eq!(
        rows,
        [
            (path("1"), some("$WINDOWS"), some("14"), "28".to_owned()),
            (path("1"), some("$LINUX"), some("16"), "32".to_owned()),
            (path("2"), None, some("20"), "40".to_owned()),
        ]
    );
    let after = m
        .with_changes(&[first.clone(), size.clone()].concat())
        .into_model();
    assert_eq!(after.scheme().font("TF2Tools_5", 1080).unwrap().tall(), 32);
}

#[test]
fn a_part_without_a_font_clones_the_rows_default() {
    let m = fonts_model();
    let changes = m
        .edit_font(&k(HV), "font", FontEdit::Shadow(true), 1080, 3)
        .unwrap();
    let ChangeTarget::AddBlock(variant) = changes[1].target() else {
        panic!("a variant")
    };
    assert_eq!(
        variant.lines(),
        [
            line("name", "Verdana", None),
            line("tall", "12", None),
            line("dropshadow", "1", None)
        ]
    );
    let ChangeTarget::Set(set) = changes.last().unwrap().target() else {
        panic!("the part's font")
    };
    assert_eq!(
        (set.key(), set.old(), set.value()),
        ("font", None, "TF2Tools_5")
    );
}

#[test]
fn the_scheme_lists_its_families() {
    let m = fonts_model();
    assert_eq!(m.scheme().families(), ["TF2 Build", "Verdana"]);
    let changes = m
        .edit_font(
            &k(DAMAGE),
            "delta_item_font",
            FontEdit::Family("verdana".into()),
            1080,
            1,
        )
        .unwrap();
    let after = m.with_changes(&changes).into_model();
    assert_eq!(
        after.scheme().font("TF2Tools_5", 1080).unwrap().family(),
        "verdana"
    );
}

#[test]
fn font_edits_refuse_what_they_cannot_write() {
    let m = fonts_model();
    let edit = |part: &str, key: &str, e: FontEdit| m.edit_font(&k(part), key, e, 1080, 1);
    assert_eq!(
        edit("Nope", "font", FontEdit::Outline(true)),
        Err(EditRefused::UnknownPart)
    );
    assert_eq!(
        edit(HV, "labelText", FontEdit::Outline(true)),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        edit(DAMAGE, "delta_item_font", FontEdit::Family(String::new())),
        Err(EditRefused::BadValue)
    );
    assert_eq!(
        edit(DAMAGE, "delta_item_font", FontEdit::Family("a\"b".into())),
        Err(EditRefused::BadValue)
    );
    assert!(
        edit(
            DAMAGE,
            "delta_item_font",
            FontEdit::Family("Comic Sans".into())
        )
        .is_ok()
    );
    assert_eq!(
        edit(DAMAGE, "delta_item_font", FontEdit::Size(0)),
        Err(EditRefused::BadValue)
    );
    assert_eq!(
        edit(DAMAGE, "delta_item_font", FontEdit::Size(257)),
        Err(EditRefused::BadValue)
    );
    let based = r#"#base "clientscheme_fonts.res"
"Scheme" { "Fonts" { } }"#;
    let fonts_file =
        r#""Scheme" { "Fonts" { "HudFontMedium" { "1" { "name" "X" "tall" "9" } } } }"#;
    let files = files_with(
        based,
        &[("resource/clientscheme_fonts.res", Some(fonts_file))],
    );
    let shared = HudModel::load(&files, Platform::Linux).unwrap();
    assert!(shared.scheme().font("HudFontMedium", 1080).is_some());
    assert_eq!(
        shared.edit_font(
            &k(DAMAGE),
            "delta_item_font",
            FontEdit::Outline(true),
            1080,
            1
        ),
        Err(EditRefused::ReadOnly)
    );
    let none = HudModel::load(&stock_files_with(&[(CS, None)]), Platform::Linux).unwrap();
    assert_eq!(
        none.edit_font(
            &k(DAMAGE),
            "delta_item_font",
            FontEdit::Outline(true),
            1080,
            1
        ),
        Err(EditRefused::ReadOnly)
    );
}

#[test]
fn reset_drops_the_parts_clone_and_keeps_anothers() {
    let m = fonts_model();
    let first = m
        .edit_font(
            &k(DAMAGE),
            "delta_item_font",
            FontEdit::Outline(true),
            1080,
            1,
        )
        .unwrap();
    let m2 = m.with_changes(&first).into_model();
    let second = m2
        .edit_font(&k(HV), "font", FontEdit::Shadow(true), 1080, 2)
        .unwrap();
    let ChangeTarget::Set(set) = second.last().unwrap().target() else {
        panic!("the part's font")
    };
    assert_eq!(set.value(), "TF2Tools_6");
    let all: Vec<Change> = first.iter().chain(&second).cloned().collect();
    let m3 = m.with_changes(&all).into_model();
    let kept = m3.reset_property(&k(DAMAGE), &["delta_item_font"], &all);
    assert_eq!(kept, second);
    let after = m.with_changes(&kept).into_model();
    assert!(
        after.scheme().font("TF2Tools_5", 1080).is_none(),
        "no orphan clone"
    );
}

#[test]
fn reset_keeps_a_clone_another_part_was_set_to() {
    let m = fonts_model();
    let first = m
        .edit_font(
            &k(DAMAGE),
            "delta_item_font",
            FontEdit::Outline(true),
            1080,
            1,
        )
        .unwrap();
    let m2 = m.with_changes(&first).into_model();
    let shared = m2.set_property(&k(HV), "font", "tf2tools_5", 2).unwrap();
    let m3 = m
        .with_changes(&[first.clone(), shared.clone()].concat())
        .into_model();
    let in_place = m3
        .edit_font(&k(HV), "font", FontEdit::Shadow(true), 1080, 3)
        .unwrap();
    let all: Vec<Change> = [first.clone(), shared.clone(), in_place.clone()].concat();
    let m4 = m.with_changes(&all).into_model();
    let kept = m4.reset_property(&k(DAMAGE), &["delta_item_font"], &all);
    let want: Vec<Change> = [&first[..first.len() - 1], &shared, &in_place].concat();
    assert_eq!(kept, want, "HV still uses the clone");
    let after = m.with_changes(&kept).into_model();
    let font = after.scheme().font("TF2Tools_5", 1080).unwrap();
    assert!(font.dropshadow() && font.outline());
}

fn ttf(family: &str) -> Vec<u8> {
    common::sfnt::sfnt_bytes(&[(1, family), (4, &format!("{family} Regular"))], false)
}

#[test]
fn a_font_file_becomes_an_asset_and_a_custom_font_entry() {
    let m = fonts_model();
    let (file, changes) = m
        .add_font_file("Big Numbers.TTF", &ttf("Big Numbers"), 4)
        .unwrap();
    let path = "resource/fonts/tf2tools/big_numbers.ttf";
    let source = ".tf2tools/fonts/big_numbers.ttf";
    assert_eq!(
        (file.family(), file.path(), file.source()),
        ("Big Numbers", path, source)
    );
    assert_eq!(
        changes,
        [
            Change::asset(4, AssetFile::new(path, source)),
            Change::add_block(
                4,
                AddBlock::new(
                    CS,
                    &["Scheme"],
                    "CustomFontFiles",
                    None,
                    vec![line("1", path, None)]
                ),
            ),
        ]
    );
    let m2 = m.with_changes(&changes).into_model();
    assert_eq!(m2.scheme().font_files(), [path]);
    assert_eq!(m2.scheme().next_font_slot(), 2);
    let (again, more) = m2
        .add_font_file("Big Numbers.ttf", &ttf("Big Numbers"), 5)
        .unwrap();
    assert_eq!(again.path(), "resource/fonts/tf2tools/big_numbers-2.ttf");
    let ChangeTarget::Set(set) = more[1].target() else {
        panic!("the block exists now")
    };
    assert_eq!(
        (set.block(), set.key(), set.value()),
        (
            ["Scheme".to_owned(), "CustomFontFiles".to_owned()].as_slice(),
            "2",
            again.path()
        )
    );
}

#[test]
fn slots_number_above_the_highest() {
    let scheme = FONTS.replacen(
        "\t\"Fonts\"",
        "\t\"CustomFontFiles\"\n\t{\n\t\t\"1\"\t\"resource/tf.ttf\"\n\t\t\"7\"\n\t\t{\n\t\t\t\"font\"\t\"resource/x.ttf\"\n\t\t\t\"name\"\t\"X\"\n\t\t}\n\t}\n\t\"Fonts\"",
        1,
    );
    let m = HudModel::load(&files_with(&scheme, &[]), Platform::Linux).unwrap();
    assert_eq!(m.scheme().next_font_slot(), 8);
    let (_, changes) = m.add_font_file("a.otf", &ttf("A Font"), 1).unwrap();
    let ChangeTarget::Set(set) = changes[1].target() else {
        panic!("a set into the block")
    };
    assert_eq!(
        (set.key(), set.value()),
        ("8", "resource/fonts/tf2tools/a.otf")
    );
}

#[test]
fn odd_file_names_stay_in_the_fonts_folder() {
    let m = fonts_model();
    for (name, want) in [
        ("../../evil.ttf", "resource/fonts/tf2tools/evil.ttf"),
        ("con.ttf", "resource/fonts/tf2tools/con_.ttf"),
        (".ttf", "resource/fonts/tf2tools/font.ttf"),
        (
            "\u{dc}n\u{ef}code F\u{f6}nt.otf",
            "resource/fonts/tf2tools/_n_code_f_nt.otf",
        ),
    ] {
        let (file, _) = m.add_font_file(name, &ttf("Odd"), 1).unwrap();
        assert_eq!(file.path(), want, "{name}");
    }
    let (file, _) = m.add_font_file("../../evil.ttf", &ttf("Odd"), 1).unwrap();
    assert!(tf2tools_core::check_hud_path(file.path()).is_some());
    assert!(tf2tools_core::check_hud_path(file.source()).is_some());
}

#[test]
fn files_that_are_not_usable_fonts_are_refused() {
    let m = fonts_model();
    assert_eq!(
        m.add_font_file("a.woff", &ttf("A"), 1).unwrap_err(),
        EditRefused::BadValue
    );
    assert_eq!(
        m.add_font_file("a.ttf", b"not a font", 1).unwrap_err(),
        EditRefused::BadValue
    );
    assert_eq!(
        m.add_font_file("a.ttf", &[], 1).unwrap_err(),
        EditRefused::BadValue
    );
    assert_eq!(
        m.add_font_file("arial.ttf", &ttf("Arial"), 1).unwrap_err(),
        EditRefused::BadValue
    );
    let none = HudModel::load(&stock_files_with(&[(CS, None)]), Platform::Linux).unwrap();
    assert_eq!(
        none.add_font_file("a.ttf", &ttf("A"), 1).unwrap_err(),
        EditRefused::ReadOnly
    );
    assert_eq!(
        font_family(&ttf("Big Numbers")).as_deref(),
        Some("Big Numbers")
    );
    assert_eq!(font_family(b"\0\x01\0\0garbage"), None);
}

#[test]
fn a_font_answers_to_every_family_it_carries() {
    let two = common::sfnt::sfnt_bytes(&[(1, "Team Fortress"), (1, "Counter-Strike")], false);
    assert_eq!(font_families(&two), ["Team Fortress", "Counter-Strike"]);
    assert_eq!(font_family(&two).as_deref(), Some("Team Fortress"));
    assert!(font_families(b"\0\x01\0\0garbage").is_empty());
    // A protected second family keeps the file out, as the font check would.
    let m = fonts_model();
    let hidden = common::sfnt::sfnt_bytes(&[(1, "Big Numbers"), (16, "Marlett")], false);
    assert_eq!(
        m.add_font_file("big.ttf", &hidden, 1).unwrap_err(),
        EditRefused::BadValue
    );
}

#[test]
fn a_font_install_would_skip_is_never_added() {
    let m = fonts_model();
    let full = common::sfnt::sfnt_bytes(&[(1, "Big Numbers"), (4, "Marlett")], false);
    assert_eq!(
        m.add_font_file("full.ttf", &full, 1).unwrap_err(),
        EditRefused::BadValue
    );
    // A ShiftJIS family could name anything to the system, so install treats it as protected.
    let big: Vec<u8> = "Big".encode_utf16().flat_map(u16::to_be_bytes).collect();
    let hidden =
        common::sfnt::sfnt_raw_bytes(&[([3, 1, 0x0409, 1], &big), ([3, 2, 0x0411, 1], b"Marlett")]);
    assert_eq!(
        m.add_font_file("hidden.ttf", &hidden, 1).unwrap_err(),
        EditRefused::BadValue
    );
}

/// The name `edit_font` gives a new clone of HudFontMedium in a model over `scheme`.
fn clone_name(scheme: &str) -> String {
    let m = HudModel::load(&files_with(scheme, &[]), Platform::Linux).unwrap();
    let changes = m
        .edit_font(
            &k(DAMAGE),
            "delta_item_font",
            FontEdit::Outline(true),
            1080,
            1,
        )
        .unwrap();
    let ChangeTarget::AddBlock(add) = changes[0].target() else {
        panic!("a clone starts with its block")
    };
    add.name().to_owned()
}

#[test]
fn clone_numbers_count_clones_on_every_platform() {
    let windows_only = "\t\t\"TF2Tools_9\"\t[$WINDOWS]\n\t\t{\n\t\t\t\"1\"\n\t\t\t{\n\t\t\t\t\"name\"\t\"V\"\n\t\t\t}\n\t\t}\n\t\t\"Default\"";
    let scheme = FONTS.replacen("\t\t\"Default\"", windows_only, 1);
    assert_eq!(clone_name(&scheme), "TF2Tools_10");
    let windows_fonts =
        "\t\"Fonts\"\t[$WINDOWS]\n\t{\n\t\t\"TF2Tools_12\"\n\t\t{\n\t\t}\n\t}\n\t\"Fonts\"";
    let scheme = FONTS.replacen("\t\"Fonts\"", windows_fonts, 1);
    assert_eq!(clone_name(&scheme), "TF2Tools_13");
}

#[test]
fn a_clone_copies_lines_under_the_font_and_refuses_nested_blocks() {
    let loose = FONTS.replacen(
        "\t\t\"HudFontMedium\"\n\t\t{\n",
        "\t\t\"HudFontMedium\"\n\t\t{\n\t\t\t\"isproportional\"\t\"only\"\t[$WINDOWS]\n",
        1,
    );
    let m = HudModel::load(&files_with(&loose, &[]), Platform::Linux).unwrap();
    let changes = m
        .edit_font(
            &k(DAMAGE),
            "delta_item_font",
            FontEdit::Outline(true),
            1080,
            1,
        )
        .unwrap();
    let fonts = ["Scheme", "Fonts"];
    let lines = vec![line("isproportional", "only", Some("$WINDOWS"))];
    assert_eq!(
        changes[0],
        Change::add_block(1, AddBlock::new(CS, &fonts, "TF2Tools_5", None, lines))
    );
    let after = m.with_changes(&changes).into_model();
    assert!(after.scheme().font("TF2Tools_5", 1080).unwrap().outline());

    let nested = FONTS.replacen(
        "\t\t\t\t\"outline\"\t\"0\"\n",
        "\t\t\t\t\"outline\"\t\"0\"\n\t\t\t\t\"extra\"\n\t\t\t\t{\n\t\t\t\t\t\"a\"\t\"1\"\n\t\t\t\t}\n",
        1,
    );
    assert_ne!(nested, FONTS);
    let m = HudModel::load(&files_with(&nested, &[]), Platform::Linux).unwrap();
    assert_eq!(
        m.edit_font(
            &k(DAMAGE),
            "delta_item_font",
            FontEdit::Outline(true),
            1080,
            1
        ),
        Err(EditRefused::ReadOnly)
    );
}

#[test]
fn an_added_font_skips_names_the_hud_already_ships() {
    let shipped = [
        ("resource/fonts/tf2tools/Big_Numbers.ttf", Some("x")),
        (".tf2tools/fonts/big_numbers-2.ttf", Some("x")),
    ];
    let m = HudModel::load(&files_with(FONTS, &shipped), Platform::Linux).unwrap();
    let (file, _) = m
        .add_font_file("Big Numbers.ttf", &ttf("Big Numbers"), 1)
        .unwrap();
    assert_eq!(file.path(), "resource/fonts/tf2tools/big_numbers-3.ttf");
}

#[test]
fn windows_families_have_open_stand_ins() {
    use tf2tools_core::stand_in_families;
    assert_eq!(
        stand_in_families("verdana"),
        ["DejaVu Sans", "Bitstream Vera Sans", "Noto Sans"]
    );
    assert_eq!(stand_in_families("Arial")[0], "Liberation Sans");
    assert_eq!(stand_in_families("Courier New")[0], "Liberation Mono");
    assert!(stand_in_families("TF2 Build").is_empty());
    assert!(stand_in_families("").is_empty());
}
