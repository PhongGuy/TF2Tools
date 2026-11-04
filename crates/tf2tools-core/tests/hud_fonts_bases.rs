//! In a copy, a part's own font and an added font file work when the fonts live only in
//! `#base` files: the clone and the font entry land in the HUD's own clientscheme.res.

mod common;

use common::hud_copy::{game_for, import_stock_hud};
use common::hud_stock::stock_files_with;
use common::library_env;
use tf2tools_core::{
    AddBlock, AssetFile, BlockLine, Change, FontEdit, HudModel, LibraryKind, MakeTarget, PartKey,
    Platform, Provenance, SetKey,
};

const CS: &str = "resource/clientscheme.res";
const FONTS_FILE: &str = "resource/clientscheme_fonts.res";
const MORE_FILE: &str = "resource/clientscheme_more.res";
const DAMAGE: &str = "CDamageAccountPanel";
const DMG_FILE: &str = "resource/ui/huddamageaccount.res";

/// The base that defines the damage font, with a line under the font and platform variants.
const BASE_FONTS: &str = r#""Scheme"
{
	"Fonts"
	{
		"HudFontMedium"
		{
			"isproportional"	"only"	[$WINDOWS]
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
	}
	"CustomFontFiles"
	{
		"1"	"resource/fonts/base.ttf"
	}
}
"#;

/// A later base: its `tall` loses to the first base's, its `weight` is merged in.
const BASE_MORE: &str =
    r#""Scheme" { "Fonts" { "HudFontMedium" { "1" { "tall" "99" "weight" "500" } } } }"#;

/// Own clientscheme forms: Fonts without the font, a root without Fonts, and a stub.
const WITH_FONTS: &str = "#base \"clientscheme_fonts.res\"\n\"Scheme\" { \"Fonts\" { \"Default\" { \"1\" { \"name\" \"Verdana\" \"tall\" \"12\" } } } }\n";
const NO_FONTS: &str =
    "#base \"clientscheme_fonts.res\"\n\"Scheme\" { \"Colors\" { \"TanLight\" \"1 2 3 255\" } }\n";
const STUB: &str = "#base \"clientscheme_fonts.res\"\n";

fn copy_over(own: &str, extra: &[(&str, Option<&str>)]) -> HudModel {
    let mut files = vec![(CS, Some(own)), (FONTS_FILE, Some(BASE_FONTS))];
    files.extend_from_slice(extra);
    let files = stock_files_with(&files).with_install_folder("myhud");
    HudModel::load(&files, Platform::Linux).unwrap()
}

fn line(key: &str, value: &str, cond: Option<&str>) -> BlockLine {
    BlockLine::new(key, value, cond)
}

fn text(m: &HudModel, path: &str) -> String {
    String::from_utf8(m.document(path).unwrap().as_bytes().to_vec()).unwrap()
}

fn outline(m: &HudModel) -> Vec<Change> {
    m.edit_font(
        &PartKey::new(DAMAGE),
        "delta_item_font",
        FontEdit::Outline(true),
        1080,
        3,
    )
    .unwrap()
}

#[test]
fn a_base_only_font_clones_into_the_own_scheme_with_its_conditionals() {
    for own in [WITH_FONTS, NO_FONTS, STUB] {
        let m = copy_over(own, &[]);
        let changes = outline(&m);
        let fonts = ["Scheme", "Fonts"];
        let clone = ["Scheme", "Fonts", "TF2Tools_1"];
        let want = vec![
            Change::add_block(
                3,
                AddBlock::new(
                    CS,
                    &fonts,
                    "TF2Tools_1",
                    None,
                    vec![line("isproportional", "only", Some("$WINDOWS"))],
                ),
            ),
            Change::add_block(
                3,
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
                3,
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
                3,
                SetKey::new(
                    DMG_FILE,
                    &["Resource/UI/HudDamageAccount.res", DAMAGE],
                    "delta_item_font",
                    None,
                    Some("HudFontMedium"),
                    "TF2Tools_1",
                ),
            ),
        ];
        assert_eq!(changes, want, "{own}");
        let edited = m.with_changes(&changes);
        assert_eq!((edited.applied().len(), edited.dropped()), (4, 0), "{own}");
        assert_eq!(edited.touched(), [CS, DMG_FILE], "{own}");
        let after = edited.model();
        let scheme = text(after, CS);
        assert!(
            scheme.starts_with("#base \"clientscheme_fonts.res\""),
            "{scheme}"
        );
        assert!(
            scheme.contains("TF2Tools_1") && scheme.contains("Fonts"),
            "{scheme}"
        );
        let font = after.scheme().font("TF2Tools_1", 1080).unwrap();
        assert_eq!(
            (font.family(), font.tall(), font.outline()),
            ("TF2 Build", 16, true)
        );
        let base = after.scheme().font("HudFontMedium", 1080).unwrap();
        assert!(!base.outline(), "the base font stays");
    }
}

#[test]
fn the_clone_merges_every_base_the_first_winning() {
    let own = format!("#base \"clientscheme_more.res\"\n{WITH_FONTS}");
    let own = own.replacen(
        "#base \"clientscheme_more.res\"\n#base \"clientscheme_fonts.res\"",
        "#base \"clientscheme_fonts.res\"\n#base \"clientscheme_more.res\"",
        1,
    );
    let m = copy_over(&own, &[(MORE_FILE, Some(BASE_MORE))]);
    let before = m.scheme().font("HudFontMedium", 1080).unwrap().clone();
    let after = m.with_changes(&outline(&m)).into_model();
    let font = after.scheme().font("TF2Tools_1", 1080).unwrap();
    assert_eq!(
        (font.family(), font.tall(), font.weight()),
        (before.family(), before.tall(), before.weight())
    );
    assert!(font.outline());
}

#[test]
fn a_font_file_adds_to_a_scheme_whose_fonts_live_in_a_base() {
    let bytes = common::sfnt::sfnt_bytes(&[(1, "Big Numbers")], false);
    for own in [WITH_FONTS, NO_FONTS, STUB] {
        let m = copy_over(own, &[]);
        let (file, changes) = m.add_font_file("Big Numbers.ttf", &bytes, 2).unwrap();
        let path = "resource/fonts/tf2tools/big_numbers.ttf";
        assert_eq!(
            changes,
            [
                Change::asset(2, AssetFile::new(path, file.source())),
                Change::add_block(
                    2,
                    AddBlock::new(
                        CS,
                        &["Scheme"],
                        "CustomFontFiles",
                        None,
                        vec![line("2", path, None)]
                    ),
                ),
            ],
            "{own}"
        );
        let edited = m.with_changes(&changes);
        assert_eq!(edited.dropped(), 0, "{own}");
        assert_eq!(edited.touched(), [CS], "{own}");
        let files = edited.model().scheme().font_files().to_vec();
        assert!(files.iter().any(|f| f == path), "{own}: {files:?}");
        assert!(
            files.iter().any(|f| f == "resource/fonts/base.ttf"),
            "{own}: {files:?}"
        );
        assert!(text(edited.model(), CS).starts_with("#base"), "{own}");
    }
}

#[test]
fn saving_a_clone_leaves_the_base_file_byte_identical() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let hud = import_stock_hud(
        &env,
        "fonthud",
        &[(CS, STUB), (FONTS_FILE, BASE_FONTS)],
        Provenance::Local,
    );
    let copy = env.lib.copy_hud(hud.id()).unwrap();
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name());
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let made = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap();
    let changes = outline(made.pristine());
    env.lib
        .save_made_hud(
            &MakeTarget::Existing(copy.id().clone()),
            copy.name(),
            &files,
            Platform::Linux,
            &changes,
        )
        .unwrap();
    assert_eq!(
        std::fs::read(dir.join(FONTS_FILE)).unwrap(),
        BASE_FONTS.as_bytes()
    );
    let own = std::fs::read_to_string(dir.join(CS)).unwrap();
    assert!(own.starts_with(STUB) && own.contains("TF2Tools_1"), "{own}");
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let reopened = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap();
    // A reopened copy is its kept original with the saved changes replayed over it.
    assert_eq!((reopened.changes().len(), reopened.dropped()), (4, 0));
    let replayed = reopened.pristine().with_changes(reopened.changes());
    let font = replayed.model().scheme().font("TF2Tools_1", 1080).unwrap();
    assert!(font.outline());
}
