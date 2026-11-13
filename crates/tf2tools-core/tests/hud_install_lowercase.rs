//! On Linux a HUD installs in lowercase, with its own file references lowercased to match,
//! because TF2 there ignores a HUD whose folder or files carry capitals.

use crate::common;

use std::fs;

use common::{LibraryEnv, library_env, write_file, zip_bytes};
#[cfg(target_os = "linux")]
use common::{setting, tree};
#[cfg(target_os = "linux")]
use tf2tools_core::{FontIssue, HudStatus, LibraryKind, Platform, Reinstall, scan_hud_fonts};
use tf2tools_core::{HudEntry, Provenance};

const ROOT: &str = "TF2HUDsArchive-KN-Edit-55d1256";

const SCHEME: &str = "#base \"scheme/ClientScheme_Fonts.res\"\n\
#base \"../../custom/KN-Edit/resource/scheme/ClientScheme_Colors.res\"\n\
\"Scheme\"\n{\n\t\"Colors\" { \"TanLight\" \"235 226 202 255\" }\n\
\t\"Fonts\" { \"HudFontBig\" { \"1\" { \"name\" \"TF2 Build\" } } }\n}\n\
\"CustomFontFiles\"\n{\n\t\"1\" { \"font\" \"resource/fonts/TF2Build.ttf\" \"name\" \"TF2 Build\" }\n}\n";

const HEALTH: &str = "\"Resource/UI/HudPlayerHealth.res\"\n{\n\
\t\"PlayerStatusHealthValue\" { \"ControlName\" \"CExLabel\" \"fieldName\" \"PlayerStatusHealthValue\" \
\"font\" \"HudFontBig\" \"labelText\" \"%Health%\" }\n}\n";

/// Windows-1252 rather than UTF-8, with a `#base` that must still follow the renames.
const ODD: &[u8] = b"#base \"Odd_Base.res\" \xff\n";

/// Imports KN Edit's shape: capitals in the archive folder, file names and references.
fn import_kn_edit(env: &LibraryEnv, extra: &[(&str, &[u8])]) -> HudEntry {
    let font = common::sfnt::sfnt_bytes(&[(1, "TF2 Build")], false);
    let mut files: Vec<(&str, &[u8])> = vec![
        ("info.vdf", b"\"KN Edit\" { \"ui_version\" \"3\" }"),
        ("resource/ClientScheme.res", SCHEME.as_bytes()),
        ("resource/scheme/ClientScheme_Fonts.res", b"\"Scheme\" {}"),
        ("resource/scheme/ClientScheme_Colors.res", b"\"Scheme\" {}"),
        ("resource/ui/HudPlayerHealth.res", HEALTH.as_bytes()),
        ("resource/ui/Odd.res", ODD),
        ("resource/fonts/TF2Build.ttf", &font),
    ];
    files.extend_from_slice(extra);
    let named: Vec<(String, &[u8])> = files
        .iter()
        .map(|(rel, bytes)| (format!("{ROOT}/{rel}"), *bytes))
        .collect();
    let refs: Vec<(&str, &[u8])> = named.iter().map(|(p, b)| (p.as_str(), *b)).collect();
    let archive = write_file(
        &env.tmp.path().join(format!("in/{ROOT}.zip")),
        &zip_bytes(&refs),
    );
    env.lib.import_hud(&archive, Provenance::Local).unwrap()
}

fn read(env: &LibraryEnv, rel: &str) -> String {
    fs::read_to_string(env.tf2.custom_dir().join(rel)).unwrap()
}

#[cfg(target_os = "linux")]
#[test]
fn a_mixed_case_hud_installs_in_lowercase_with_its_references_to_match() {
    let env = library_env();
    let hud = import_kn_edit(&env, &[]);
    let done = env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(done.folder(), "kn-edit");
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some("kn-edit")
    );
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        [
            "kn-edit/info.vdf",
            "kn-edit/resource/clientscheme.res",
            "kn-edit/resource/fonts/tf2build.ttf",
            "kn-edit/resource/scheme/clientscheme_colors.res",
            "kn-edit/resource/scheme/clientscheme_fonts.res",
            "kn-edit/resource/ui/hudplayerhealth.res",
            "kn-edit/resource/ui/odd.res",
        ]
    );
    assert_eq!(
        read(&env, "kn-edit/resource/clientscheme.res"),
        SCHEME
            .replace(
                "scheme/ClientScheme_Fonts.res",
                "scheme/clientscheme_fonts.res"
            )
            .replace(
                "../../custom/KN-Edit/resource/scheme/ClientScheme_Colors.res",
                "../../custom/kn-edit/resource/scheme/clientscheme_colors.res"
            )
            .replace("resource/fonts/TF2Build.ttf", "resource/fonts/tf2build.ttf")
    );
    assert_eq!(
        read(&env, "kn-edit/resource/ui/hudplayerhealth.res"),
        HEALTH.replace(
            "Resource/UI/HudPlayerHealth.res",
            "resource/ui/hudplayerhealth.res"
        ),
        "labels, control and font names stay as written"
    );
    assert_eq!(
        fs::read(env.tf2.custom_dir().join("kn-edit/resource/ui/odd.res")).unwrap(),
        b"#base \"odd_base.res\" \xff\n",
        "a file that is not UTF-8 is still rewritten"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn references_to_the_huds_own_materials_follow_the_renamed_files() {
    let env = library_env();
    let hud = import_kn_edit(
        &env,
        &[
            (
                "materials/vgui/Replay/Thumbnails/Foo.vmt",
                b"\"UnlitGeneric\" { \"$baseTexture\" \"VGUI/Replay/Thumbnails/Foo\" }",
            ),
            ("materials/vgui/Replay/Thumbnails/Foo.vtf", b"VTF\0"),
            (
                "resource/ui/Images.res",
                b"\"image\" \"Replay/Thumbnails/Foo\"\n\"image\" \"Stock/Thing\"\n",
            ),
        ],
    );
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        read(&env, "kn-edit/materials/vgui/replay/thumbnails/foo.vmt"),
        "\"UnlitGeneric\" { \"$baseTexture\" \"vgui/replay/thumbnails/foo\" }"
    );
    assert_eq!(
        read(&env, "kn-edit/resource/ui/images.res"),
        "\"image\" \"replay/thumbnails/foo\"\n\"image\" \"Stock/Thing\"\n",
        "a stock image the HUD does not ship stays as written"
    );
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn the_library_copy_keeps_its_capitals() {
    let env = library_env();
    let hud = import_kn_edit(&env, &[]);
    let library = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(hud.folder_name());
    let before = fs::read(library.join("resource/ClientScheme.res")).unwrap();
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        fs::read(library.join("resource/ClientScheme.res")).unwrap(),
        before
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_case_collision_keeps_the_lowercase_file_and_still_installs() {
    let env = library_env();
    let hud = import_kn_edit(
        &env,
        &[
            ("resource/ui/HudLayout.res", b"upper"),
            ("resource/ui/hudlayout.res", b"lower"),
        ],
    );
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(read(&env, "kn-edit/resource/ui/hudlayout.res"), "lower");
    assert!(
        !env.tf2
            .custom_dir()
            .join("kn-edit/resource/ui/HudLayout.res")
            .exists()
    );
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn a_lowercased_install_is_still_the_libraries_own() {
    let env = library_env();
    let hud = import_kn_edit(&env, &[]);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
    assert!(matches!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Managed { folder_name, id } if folder_name == "kn-edit" && &id == hud.id()
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn a_lowercased_install_without_its_fingerprint_matches_the_library_by_bytes() {
    let env = library_env();
    let hud = import_kn_edit(
        &env,
        &[("resource/ui/A.res", b"a"), ("resource/ui/a.res", b"b")],
    );
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    common::sql(&env.dir)
        .execute(
            "DELETE FROM settings WHERE key = 'installed_hud_fingerprint'",
            [],
        )
        .unwrap();
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
    write_file(
        &env.tf2
            .custom_dir()
            .join("kn-edit/resource/clientscheme.res"),
        b"tweaked",
    );
    let found = env.lib.external_huds(&env.tf2).unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].has_changes());
}

#[cfg(target_os = "linux")]
#[test]
fn a_live_update_stays_lowercase_and_uninstall_and_reinstall_work() {
    let env = library_env();
    let hud = import_kn_edit(&env, &[]);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let library = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(hud.folder_name());
    write_file(
        &library.join("resource/ui/HudPlayerHealth.res"),
        b"#base \"../../custom/KN-Edit/resource/ui/Base.res\"\n",
    );
    let done = env.lib.reinstall_hud(hud.id(), &env.tf2).unwrap();
    assert!(matches!(done, Reinstall::Updated { .. }), "{done:?}");
    assert_eq!(
        read(&env, "kn-edit/resource/ui/hudplayerhealth.res"),
        "#base \"../../custom/kn-edit/resource/ui/base.res\"\n"
    );
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());

    env.lib.uninstall_hud(&env.tf2).unwrap();
    assert!(tree(&env.tf2.custom_dir()).is_empty());
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some("kn-edit")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn the_font_check_finds_no_capitals_in_an_installed_hud() {
    let env = library_env();
    let hud = import_kn_edit(&env, &[]);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let fonts = scan_hud_fonts(&env.tf2.custom_dir().join("kn-edit"), Platform::Linux).unwrap();
    assert_eq!(fonts.fonts().len(), 1, "{:?}", fonts.fonts());
    assert!(
        !fonts.issues().iter().any(|issue| matches!(
            issue,
            FontIssue::UppercaseFileName { .. } | FontIssue::PathCaseMismatch { .. }
        )),
        "{:?}",
        fonts.issues()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_live_update_of_an_install_made_before_lowercasing_keeps_its_capitals() {
    let env = library_env();
    let hud = import_kn_edit(&env, &[]);
    let library = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(hud.folder_name());
    let installed = env.tf2.custom_dir().join("KN-Edit");
    let copy_library = || {
        for rel in tree(&library)
            .iter()
            .filter(|rel| !rel.starts_with(".tf2tools/"))
        {
            write_file(&installed.join(rel), &fs::read(library.join(rel)).unwrap());
        }
    };
    copy_library();
    common::set_setting(&env.dir, "installed_hud_folder", "KN-Edit");
    common::set_setting(&env.dir, "installed_hud_id", hud.id().as_str());
    // The first update records the install's fingerprint, as an install made before would have.
    env.lib.reinstall_hud(hud.id(), &env.tf2).unwrap();

    write_file(
        &library.join("resource/ui/HudPlayerHealth.res"),
        b"#base \"../../custom/KN-Edit/resource/ui/Base.res\"\n",
    );
    let done = env.lib.reinstall_hud(hud.id(), &env.tf2).unwrap();
    assert!(matches!(done, Reinstall::Updated { .. }), "{done:?}");
    let snapshot = |root: &std::path::Path| -> Vec<(String, Vec<u8>)> {
        tree(root)
            .into_iter()
            .filter(|rel| !rel.starts_with(".tf2tools/"))
            .map(|rel| {
                let bytes = fs::read(root.join(&rel)).unwrap();
                (rel, bytes)
            })
            .collect()
    };
    assert_eq!(snapshot(&installed), snapshot(&library));
}

#[cfg(not(target_os = "linux"))]
#[test]
fn elsewhere_a_hud_installs_as_its_files_spell_it() {
    let env = library_env();
    let hud = import_kn_edit(&env, &[]);
    let done = env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(done.folder(), "KN-Edit");
    assert_eq!(read(&env, "KN-Edit/resource/ClientScheme.res"), SCHEME);
}
