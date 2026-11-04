//! Game files through ordered layers: VPKs in gameinfo order, loose fonts, memory.

mod common;

use tf2tools_core::{GameText, HudFiles, HudFilesError, Platform, normalise_game_path};

// A trimmed copy of TF2's tf/gameinfo.txt (SteamDatabase/GameTracking-TF2, 2026-10-21).
const GAMEINFO: &str = r#""GameInfo"
{
	game	"Team Fortress 2"
	FileSystem
	{
		SteamAppId				440
		//
		// |gameinfo_path| points at the directory where gameinfo.txt is.
		// |all_source_engine_paths| points at the directory cintaining HL2 shared content.
		//
		SearchPaths
		{
			// First, mount all user customizations.
			game+mod+custom_mod	tf/custom/*
			game_lv				tf/tf2_lv.vpk
			game+mod			tf/tf2_textures.vpk
			game+mod			tf/tf2_sound_vo_english.vpk
			game+mod			tf/tf2_sound_misc.vpk
			game+mod+vgui		tf/tf2_misc.vpk
			game				|all_source_engine_paths|hl2/hl2_textures.vpk
			game				|all_source_engine_paths|hl2/hl2_sound_vo_english.vpk
			game				|all_source_engine_paths|hl2/hl2_sound_misc.vpk
			game+vgui			|all_source_engine_paths|hl2/hl2_misc.vpk
			platform+vgui			|all_source_engine_paths|platform/platform_misc.vpk
			// Now search loose files.
			mod+mod_write+default_write_path		|gameinfo_path|.
			game+game_write		tf
			gamebin				tf/bin
			game				|all_source_engine_paths|hl2
			platform			|all_source_engine_paths|platform
			game+download	tf/download
		}
	}
}
"#;

fn read(files: &HudFiles, path: &str) -> Option<Vec<u8>> {
    files.read(path).unwrap().map(|f| f.into_bytes())
}

#[test]
fn game_files_come_from_vpks_in_gameinfo_order_and_loose_fonts() {
    let (_tmp, game) = common::temp_tf2_tree();
    let tf = game.join("tf");
    common::write_file(&tf.join("gameinfo.txt"), GAMEINFO.as_bytes());
    common::write_file(&tf.join("tf2_lv_dir.vpk"), b"not a vpk");
    common::write_file(&tf.join("tf2_sound_misc_dir.vpk"), b"not a vpk");
    common::write_file(&tf.join("tf2_sound_vo_english_dir.vpk"), b"not a vpk");
    common::write_file(&game.join("hl2/resource/ui/loose.res"), b"loose hl2");
    common::write_file(
        &tf.join("download/resource/ui/downloaded.res"),
        b"downloaded",
    );
    common::write_file(
        &tf.join("tf2_textures_dir.vpk"),
        &common::vpk_bytes(&[("materials/hud/health_bg.vtf", b"VTF")]),
    );
    common::write_file(
        &tf.join("tf2_misc_dir.vpk"),
        &common::vpk_bytes(&[
            ("resource/ui/hudplayerhealth.res", b"tf2"),
            ("Resource/ui/x.res", b"upper"),
            ("resource/ui/x.res", b"lower"),
        ]),
    );
    common::write_file(
        &game.join("hl2/hl2_misc_dir.vpk"),
        &common::vpk_bytes(&[
            ("resource/ui/basechat.res", b"hl2"),
            ("resource/ui/hudplayerhealth.res", b"hl2 loses"),
        ]),
    );
    common::write_file(
        &game.join("platform/platform_misc_dir.vpk"),
        &common::vpk_bytes(&[("resource/sourcescheme.res", b"platform")]),
    );
    common::write_file(&tf.join("resource/tf2.ttf"), b"font");
    common::write_file(&tf.join("resource/readme.txt"), b"not a font");

    let files = HudFiles::open_game(&tf).unwrap();
    assert_eq!(
        files.layer_count(),
        5,
        "textures, misc, hl2 misc, platform, fonts"
    );
    assert_eq!(
        read(&files, "resource/ui/hudplayerhealth.res").unwrap(),
        b"tf2"
    );
    assert_eq!(read(&files, "Resource\\UI\\BaseChat.res").unwrap(), b"hl2");
    assert_eq!(
        files
            .read("resource/ui/basechat.res")
            .unwrap()
            .unwrap()
            .layer(),
        2
    );
    assert_eq!(read(&files, "materials/hud/health_bg.vtf").unwrap(), b"VTF");
    assert_eq!(
        read(&files, "resource/sourcescheme.res").unwrap(),
        b"platform"
    );
    assert_eq!(read(&files, "resource/TF2.ttf").unwrap(), b"font");
    assert_eq!(read(&files, "resource/readme.txt"), None);
    assert_eq!(
        read(&files, "resource/ui/loose.res"),
        None,
        "loose directory lines are ignored"
    );
    assert_eq!(read(&files, "resource/ui/downloaded.res"), None);
    assert_eq!(read(&files, "RESOURCE/UI/X.RES").unwrap(), b"upper");
    assert_eq!(files.collisions(), ["resource/ui/x.res"]);
}

#[test]
fn a_corrupt_listed_vpk_fails_and_a_folder_without_gameinfo_is_refused() {
    let (_tmp, game) = common::temp_tf2_tree();
    let tf = game.join("tf");
    let err = HudFiles::open_game(&tf).unwrap_err();
    assert!(matches!(err, HudFilesError::NoGameInfo { .. }), "{err:?}");
    common::write_file(&tf.join("gameinfo.txt"), GAMEINFO.as_bytes());
    common::write_file(&tf.join("tf2_misc_dir.vpk"), b"not a vpk");
    let err = HudFiles::open_game(&tf).unwrap_err();
    assert!(matches!(err, HudFilesError::Vpk(_)), "{err:?}");
}

#[test]
fn memory_layers_stack_top_first_and_report_case_collisions() {
    let layer = |entries: &[(&str, &[u8])]| -> Vec<(String, Vec<u8>)> {
        entries
            .iter()
            .map(|(p, b)| (p.to_string(), b.to_vec()))
            .collect()
    };
    let files = HudFiles::from_memory(vec![
        layer(&[
            ("resource/fonts/a.ttf", b"lower"),
            ("resource/Fonts/a.ttf", b"upper"),
            ("scripts/hudlayout.res", b"top"),
        ]),
        layer(&[
            ("Scripts\\HudLayout.res", b"bottom"),
            ("resource/clientscheme.res", b"scheme"),
        ]),
    ]);
    let got = files.read("resource/fonts/A.TTF").unwrap().unwrap();
    assert_eq!(
        (got.bytes(), got.layer(), got.name()),
        (&b"upper"[..], 0, "resource/Fonts/a.ttf")
    );
    assert_eq!(read(&files, "scripts/hudlayout.res").unwrap(), b"top");
    assert_eq!(
        files
            .read("./resource//clientscheme.res")
            .unwrap()
            .unwrap()
            .layer(),
        1
    );
    assert_eq!(read(&files, "resource/missing.res"), None);
    assert!(files.contains("SCRIPTS/hudlayout.res"));
    assert_eq!(files.collisions(), ["resource/fonts/a.ttf"]);
    assert_eq!(
        files.list("Resource/"),
        ["resource/clientscheme.res", "resource/fonts/a.ttf"]
    );
    assert_eq!(files.layer_count(), 2);
}

#[test]
fn game_paths_normalise_separators_and_dots_and_refuse_escapes() {
    let n = |p: &str| normalise_game_path(p);
    assert_eq!(
        n("materials\\vgui\\..\\hud\\X.vmt").as_deref(),
        Some("materials/hud/X.vmt")
    );
    assert_eq!(
        n("./resource//ui/./a.res").as_deref(),
        Some("resource/ui/a.res")
    );
    assert_eq!(
        n("/scripts/hudlayout.res").as_deref(),
        Some("scripts/hudlayout.res")
    );
    assert_eq!(n("../cfg/x.txt"), None);
    assert_eq!(n("a/../../b"), None);
    assert_eq!(n("a/.."), None);
    assert_eq!(n(""), None);
}

#[test]
fn hud_files_cross_threads() {
    fn assert_send_sync<T: Send + Sync + std::fmt::Debug + 'static>() {}
    assert_send_sync::<HudFiles>();
    assert_send_sync::<tf2tools_core::HudFile>();
    assert_send_sync::<HudFilesError>();
}

#[test]
fn entries_over_64_mib_are_refused_before_reading() {
    let (_dir, tf) = common::game_with_vpk(&common::vpk_claiming(
        "resource/huge.res",
        64 * 1024 * 1024 + 1,
    ));
    let files = HudFiles::open_game(&tf).unwrap();
    let err = files.read("resource/huge.res").unwrap_err();
    assert!(
        matches!(&err, HudFilesError::TooLarge { path, bytes }
            if path == "resource/huge.res" && *bytes == 64 * 1024 * 1024 + 1),
        "{err:?}"
    );
}

#[test]
fn list_prefixes_normalise_like_paths() {
    let files = HudFiles::from_memory(vec![vec![
        ("resource/ui/a.res".to_owned(), vec![]),
        ("resource/b.res".to_owned(), vec![]),
        ("resourcex/c.res".to_owned(), vec![]),
        ("scripts/d.txt".to_owned(), vec![]),
    ]]);
    let resource = ["resource/b.res", "resource/ui/a.res"];
    assert_eq!(files.list("resource/"), resource);
    assert_eq!(files.list("./Resource//"), resource);
    assert_eq!(files.list("scripts/../resource/."), resource);
    assert_eq!(files.list("resource\\ui\\"), ["resource/ui/a.res"]);
    assert_eq!(files.list("resource/ui/a"), ["resource/ui/a.res"]);
    assert_eq!(files.list("./").len(), 4);
    assert_eq!(files.list("").len(), 4);
    assert!(files.list("../").is_empty());
}

fn utf16le(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

fn lang(pairs: &str) -> Vec<u8> {
    utf16le(&format!(
        "\"lang\" {{ \"Language\" \"English\" \"Tokens\" {{ {pairs} }} }}"
    ))
}

#[test]
fn the_games_loose_english_text_joins_below_the_vpks() {
    let (_tmp, game) = common::temp_tf2_tree();
    let tf = game.join("tf");
    common::write_file(&tf.join("gameinfo.txt"), GAMEINFO.as_bytes());
    common::write_file(
        &tf.join("tf2_misc_dir.vpk"),
        &common::vpk_bytes(&[("resource/ui/hudplayerhealth.res", b"tf2")]),
    );
    common::write_file(
        &tf.join("resource/tf_english.txt"),
        &lang("\"TF_Ball\" \"BALL\""),
    );
    common::write_file(
        &game.join("hl2/resource/gameui_english.txt"),
        &lang("\"GameUI_Only\" \"gameui\""),
    );
    common::write_file(
        &tf.join("resource/closecaption_english.txt"),
        &lang("\"Caption\" \"never read\""),
    );

    let files = HudFiles::open_game(&tf).unwrap();
    assert_eq!(files.layer_count(), 2, "misc, then the loose English text");
    let text = GameText::load(&files, Platform::Linux);
    assert_eq!(text.show("#TF_Ball"), "BALL");
    assert_eq!(text.get("GameUI_Only"), Some("gameui"));
    assert_eq!(read(&files, "resource/closecaption_english.txt"), None);
}

#[test]
fn a_packed_english_text_wins_over_a_loose_one() {
    let (_tmp, game) = common::temp_tf2_tree();
    let tf = game.join("tf");
    common::write_file(&tf.join("gameinfo.txt"), GAMEINFO.as_bytes());
    let packed = lang("\"TF_Ball\" \"packed\"");
    common::write_file(
        &tf.join("tf2_misc_dir.vpk"),
        &common::vpk_bytes(&[("resource/tf_english.txt", packed.as_slice())]),
    );
    common::write_file(
        &tf.join("resource/tf_english.txt"),
        &lang("\"TF_Ball\" \"loose\""),
    );
    let files = HudFiles::open_game(&tf).unwrap();
    assert_eq!(
        GameText::load(&files, Platform::Linux).get("TF_Ball"),
        Some("packed")
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_english_text_is_skipped() {
    let (_tmp, game) = common::temp_tf2_tree();
    let tf = game.join("tf");
    common::write_file(&tf.join("gameinfo.txt"), GAMEINFO.as_bytes());
    common::write_file(
        &tf.join("tf2_misc_dir.vpk"),
        &common::vpk_bytes(&[("resource/ui/hudplayerhealth.res", b"tf2")]),
    );
    let elsewhere = common::write_file(&game.join("elsewhere.txt"), &lang("\"TF_Ball\" \"BALL\""));
    std::fs::create_dir_all(tf.join("resource")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, tf.join("resource/tf_english.txt")).unwrap();

    let files = HudFiles::open_game(&tf).unwrap();
    assert_eq!(files.layer_count(), 1, "the symlink adds no layer");
    assert_eq!(GameText::load(&files, Platform::Linux).get("TF_Ball"), None);
}
