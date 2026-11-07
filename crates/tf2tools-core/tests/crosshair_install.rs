//! Writing crosshair assignments into tf/custom, removing them, and the installed scan.

use crate::common;

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use common::{
    LibraryEnv, STOCK_WEAPON_TXT, crosshair_env, patched_stock_script, set_setting, setting, sql,
    tree, write_file,
};
use tf2tools_core::{
    BuiltinCrosshairs, CrosshairEntry, CrosshairRef, InstallError, Provenance, SkipReason,
    crosshair_scripts_need_repair,
};

const MATERIALS: &str = "tf2tools-crosshairs/materials/vgui/replay/thumbnails";
const SCRIPTS: &str = "tf2tools-crosshairs/scripts";

fn bundled() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/crosshairs")
}

fn catalog() -> BuiltinCrosshairs {
    BuiltinCrosshairs::load(&bundled()).unwrap()
}

fn import_png(env: &LibraryEnv, name: &str) -> CrosshairEntry {
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 255, 0, 255]));
    let mut bytes = Cursor::new(Vec::new());
    img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let src = write_file(
        &env.tmp.path().join(format!("in/{name}.png")),
        &bytes.into_inner(),
    );
    env.lib
        .import_crosshair(&src, name, Provenance::Local)
        .unwrap()
}

fn builtin(name: &str) -> CrosshairRef {
    CrosshairRef::Builtin(name.into())
}

#[test]
fn apply_writes_textures_and_scripts_for_builtin_and_library_crosshairs() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    let library_ref = CrosshairRef::Library(dot.id().clone());
    env.lib
        .assign("tf_weapon_scattergun", &builtin("cpma_9"))
        .unwrap();
    env.lib
        .assign("tf_weapon_rocketlauncher", &library_ref)
        .unwrap();
    env.lib
        .assign("tf_weapon_shotgun_primary", &library_ref)
        .unwrap();
    set_setting(&env.dir, "active_profile_id", "p1");

    let report = env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    assert_eq!(
        report.applied(),
        [
            "tf_weapon_rocketlauncher",
            "tf_weapon_scattergun",
            "tf_weapon_shotgun_primary"
        ]
    );
    assert!(report.skipped().is_empty());

    let f = dot.folder_name();
    let mut expected = vec![
        format!("{MATERIALS}/cpma_9.vmt"),
        format!("{MATERIALS}/cpma_9.vtf"),
        format!("{MATERIALS}/{f}.vmt"),
        format!("{MATERIALS}/{f}.vtf"),
        format!("{SCRIPTS}/tf_weapon_rocketlauncher.txt"),
        format!("{SCRIPTS}/tf_weapon_scattergun.txt"),
        format!("{SCRIPTS}/tf_weapon_shotgun_primary.txt"),
    ];
    expected.sort();
    let custom = env.tf2.custom_dir();
    assert_eq!(tree(&custom), expected);
    assert_eq!(
        fs::read(custom.join(MATERIALS).join("cpma_9.vtf")).unwrap(),
        fs::read(bundled().join("vtf/cpma_9.vtf")).unwrap()
    );
    assert_eq!(
        fs::read(custom.join(MATERIALS).join("cpma_9.vmt")).unwrap(),
        fs::read(bundled().join("vmt/cpma_9.vmt")).unwrap()
    );
    assert_eq!(
        fs::read_to_string(custom.join(SCRIPTS).join("tf_weapon_scattergun.txt")).unwrap(),
        patched_stock_script("cpma_9")
    );
    assert_eq!(
        fs::read_to_string(custom.join(SCRIPTS).join("tf_weapon_rocketlauncher.txt")).unwrap(),
        patched_stock_script(f)
    );
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some("p1")
    );
}

#[test]
fn a_builtin_without_a_vmt_gets_the_template() {
    let env = crosshair_env();
    env.lib
        .assign("tf_weapon_scattergun", &builtin("Crossdot"))
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    let vmt =
        fs::read_to_string(env.tf2.custom_dir().join(MATERIALS).join("Crossdot.vmt")).unwrap();
    assert_eq!(
        vmt,
        "\"UnlitGeneric\"\n{\n\t\"$translucent\" \"1\"\n\t\"$basetexture\" \"vgui/replay/thumbnails/Crossdot\"\n\t\"$vertexcolor\" \"1\"\n\t\"$no_fullbright\" \"1\"\n\t\"$ignorez\" \"1\"\n}"
    );
}

// A v5 library can hold a folder whose name has a quote (v5 only replaced spaces); v6
// import never makes one. Windows cannot name such a file, so this runs on Unix only.
#[cfg(unix)]
#[test]
fn a_quote_in_a_library_folder_name_installs_under_a_safe_name() {
    const QUOTED: &str = "q\"t-1a2b3c4d";
    const SAFE: &str = "q-t-1a2b3c4d";
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    let library = env.dir.library_dir(tf2tools_core::LibraryKind::Crosshairs);
    let old = library.join(dot.folder_name());
    fs::rename(
        old.join(format!("{}.vtf", dot.folder_name())),
        old.join(format!("{QUOTED}.vtf")),
    )
    .unwrap();
    fs::remove_file(old.join(format!("{}.vmt", dot.folder_name()))).unwrap();
    // v5's template with the raw name: the library's own VMT is already broken.
    fs::write(
        old.join(format!("{QUOTED}.vmt")),
        format!("\"UnlitGeneric\"\n{{\n\t\"$basetexture\" \"vgui/replay/thumbnails/{QUOTED}\"\n}}"),
    )
    .unwrap();
    fs::rename(&old, library.join(QUOTED)).unwrap();
    sql(&env.dir)
        .execute(
            "UPDATE library_crosshairs SET folder_name = ?1 WHERE id = ?2",
            [QUOTED, dot.id().as_str()],
        )
        .unwrap();
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Library(dot.id().clone()),
        )
        .unwrap();

    let report = env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    assert_eq!(report.applied(), ["tf_weapon_scattergun"]);
    let custom = env.tf2.custom_dir();
    assert_eq!(
        tree(&custom),
        [
            format!("{MATERIALS}/{SAFE}.vmt"),
            format!("{MATERIALS}/{SAFE}.vtf"),
            format!("{SCRIPTS}/tf_weapon_scattergun.txt"),
        ]
    );
    assert_eq!(
        fs::read_to_string(custom.join(MATERIALS).join(format!("{SAFE}.vmt"))).unwrap(),
        format!(
            "\"UnlitGeneric\"\n{{\n\t\"$translucent\" \"1\"\n\t\"$basetexture\" \"vgui/replay/thumbnails/{SAFE}\"\n\t\"$vertexcolor\" \"1\"\n\t\"$no_fullbright\" \"1\"\n\t\"$ignorez\" \"1\"\n}}"
        )
    );
    let script = fs::read_to_string(custom.join(SCRIPTS).join("tf_weapon_scattergun.txt")).unwrap();
    assert!(
        script.contains(&format!("\"file\"\t\t\"vgui/replay/thumbnails/{SAFE}\"")),
        "{script}"
    );
}

#[test]
fn missing_and_unsafe_assignments_are_skipped_and_reported() {
    let env = crosshair_env();
    env.lib
        .assign("tf_weapon_scattergun", &builtin("Dot_Tob"))
        .unwrap();
    env.lib
        .assign("tf_weapon_rocketlauncher", &builtin("cpma_9"))
        .unwrap();
    sql(&env.dir)
        .execute_batch(
            "INSERT INTO crosshair_assignments (weapon_script, crosshair_source, crosshair_ref) VALUES
                ('tf_weapon_pistol', 'library', 'gone-12345678'),
                ('tf_weapon_smg', 'library', '../escape'),
                ('../evil', 'builtin', 'cpma_9');",
        )
        .unwrap();

    let report = env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    assert_eq!(report.applied(), ["tf_weapon_rocketlauncher"]);
    let skipped: Vec<(&str, &str, SkipReason)> = report
        .skipped()
        .iter()
        .map(|s| (s.weapon_script(), s.reference(), s.reason()))
        .collect();
    assert_eq!(
        skipped,
        [
            ("../evil", "cpma_9", SkipReason::InvalidWeaponScript),
            (
                "tf_weapon_pistol",
                "gone-12345678",
                SkipReason::MissingLibraryCrosshair
            ),
            (
                "tf_weapon_scattergun",
                "Dot_Tob",
                SkipReason::UnknownBuiltin
            ),
            (
                "tf_weapon_smg",
                "../escape",
                SkipReason::MissingLibraryCrosshair
            ),
        ]
    );
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        [
            format!("{MATERIALS}/cpma_9.vmt"),
            format!("{MATERIALS}/cpma_9.vtf"),
            format!("{SCRIPTS}/tf_weapon_rocketlauncher.txt"),
        ]
    );
    assert!(!env.tf2.tf_dir().join("evil.txt").exists());
}

#[test]
fn a_weapon_without_a_readable_stock_script_is_skipped_and_never_written() {
    let env = crosshair_env();
    for weapon in [
        "tf_weapon_scattergun",
        "tf_weapon_bat",
        "tf_weapon_pistol_scout",
    ] {
        env.lib.assign(weapon, &builtin("cpma_9")).unwrap();
    }
    // The bat's stock script has no crosshair block to replace; the pistol's is not in the pak.
    let paths = [
        "scripts/tf_weapon_scattergun.ctx",
        "scripts/tf_weapon_bat.ctx",
    ];
    let flat: &[u8] = b"WeaponData { TextureData { \"ammo\" { } } }";
    write_file(
        &env.tf2.tf_dir().join("tf2_misc_dir.vpk"),
        &common::vpk_bytes(&[(paths[0], common::STOCK_WEAPON_CTX), (paths[1], flat)]),
    );

    let report = env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    assert_eq!(report.applied(), ["tf_weapon_scattergun"]);
    let skipped: Vec<(&str, SkipReason)> = report
        .skipped()
        .iter()
        .map(|s| (s.weapon_script(), s.reason()))
        .collect();
    assert_eq!(
        skipped,
        [
            ("tf_weapon_bat", SkipReason::NoStockScript),
            ("tf_weapon_pistol_scout", SkipReason::NoStockScript),
        ]
    );
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        [
            format!("{MATERIALS}/cpma_9.vmt"),
            format!("{MATERIALS}/cpma_9.vtf"),
            format!("{SCRIPTS}/tf_weapon_scattergun.txt"),
        ]
    );
}

#[test]
fn without_the_game_pak_every_weapon_is_skipped() {
    let env = crosshair_env();
    fs::remove_file(env.tf2.tf_dir().join("tf2_misc_dir.vpk")).unwrap();
    env.lib
        .assign("tf_weapon_scattergun", &builtin("cpma_9"))
        .unwrap();

    let report = env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    assert!(report.applied().is_empty());
    assert_eq!(report.skipped()[0].reason(), SkipReason::NoStockScript);
    assert!(!env.tf2.custom_dir().join("tf2tools-crosshairs").exists());
}

#[test]
fn apply_replaces_an_old_crosshair_only_script_with_the_full_one() {
    let env = crosshair_env();
    let script = env
        .tf2
        .custom_dir()
        .join(SCRIPTS)
        .join("tf_weapon_scattergun.txt");
    write_file(
        &script,
        b"\"WeaponData\"\n{\n\t\"TextureData\"\n\t{\n\t\t\"crosshair\"\n\t\t{\n\t\t\t\"file\"\t\t\"vgui/replay/thumbnails/cpma_9\"\n\t\t}\n\t}\n}",
    );
    env.lib
        .assign("tf_weapon_scattergun", &builtin("cpma_9"))
        .unwrap();

    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    let written = fs::read_to_string(&script).unwrap();
    assert_eq!(written, patched_stock_script("cpma_9"));
    assert!(written.contains("\"TimeFireDelay\""));
    assert_ne!(written.as_bytes(), STOCK_WEAPON_TXT);
}

/// Removes a weapon's assignment row directly, as a profile apply that omits it would.
fn drop_assignment(env: &LibraryEnv, weapon_script: &str) {
    sql(&env.dir)
        .execute(
            "DELETE FROM crosshair_assignments WHERE weapon_script = ?1",
            [weapon_script],
        )
        .unwrap();
}

#[test]
fn apply_rebuilds_the_folder_so_cleared_assignments_disappear() {
    let env = crosshair_env();
    let custom = env.tf2.custom_dir();
    env.lib
        .assign("tf_weapon_scattergun", &builtin("cpma_9"))
        .unwrap();
    env.lib
        .assign("tf_weapon_rocketlauncher", &builtin("cpma_9"))
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    write_file(&custom.join("tf2tools-crosshairs/stale.txt"), b"old");

    drop_assignment(&env, "tf_weapon_rocketlauncher");
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    assert_eq!(
        tree(&custom),
        [
            format!("{MATERIALS}/cpma_9.vmt"),
            format!("{MATERIALS}/cpma_9.vtf"),
            format!("{SCRIPTS}/tf_weapon_scattergun.txt"),
        ]
    );

    drop_assignment(&env, "tf_weapon_scattergun");
    let report = env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    assert!(report.applied().is_empty());
    assert!(!custom.join("tf2tools-crosshairs").exists());
}

#[test]
fn remove_crosshairs_deletes_only_the_managed_folder() {
    let env = crosshair_env();
    let custom = env.tf2.custom_dir();
    env.lib
        .assign("tf_weapon_scattergun", &builtin("cpma_9"))
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    write_file(&custom.join("other/scripts/tf_weapon_scattergun.txt"), b"x");
    set_setting(&env.dir, "active_profile_id", "p1");
    env.lib.remove_crosshairs(&env.tf2).unwrap();
    assert_eq!(tree(&custom), ["other/scripts/tf_weapon_scattergun.txt"]);
    assert_eq!(env.lib.assignments().unwrap().len(), 1);
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some("p1")
    );
    env.lib.remove_crosshairs(&env.tf2).unwrap();
}

#[test]
fn installed_crosshairs_counts_scripts_and_reports_conflicts() {
    let env = crosshair_env();
    let custom = env.tf2.custom_dir();
    let empty = env.lib.installed_crosshairs(&env.tf2).unwrap();
    assert!(!empty.folder_exists());
    assert_eq!(empty.script_count(), 0);
    assert!(empty.conflicts().is_empty());

    env.lib
        .assign("tf_weapon_scattergun", &builtin("cpma_9"))
        .unwrap();
    env.lib
        .assign("tf_weapon_rocketlauncher", &builtin("cpma_9"))
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    write_file(&custom.join("zhud/scripts/tf_weapon_scattergun.txt"), b"x");
    write_file(&custom.join("ahud/scripts/tf_weapon_pistol.TXT"), b"x");
    write_file(&custom.join("bhud/scripts/readme.txt"), b"x");
    write_file(&custom.join("chud/scripts/sub/tf_weapon_smg.txt"), b"x");
    let scan = env.lib.installed_crosshairs(&env.tf2).unwrap();
    assert!(scan.folder_exists());
    assert_eq!(scan.script_count(), 2);
    assert_eq!(scan.conflicts(), ["ahud", "zhud"]);
}

/// Assigns library crosshair `dot` to the scattergun and the builtin to the rocket launcher.
fn assign_both(env: &LibraryEnv, dot: &CrosshairEntry) {
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Library(dot.id().clone()),
        )
        .unwrap();
    env.lib
        .assign("tf_weapon_rocketlauncher", &builtin("cpma_9"))
        .unwrap();
}

#[test]
fn uninstalling_one_crosshair_keeps_the_others_in_the_game() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    assign_both(&env, &dot);
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();

    env.lib.uninstall_crosshair(dot.id(), &env.tf2).unwrap();
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        [
            format!("{MATERIALS}/cpma_9.vmt"),
            format!("{MATERIALS}/cpma_9.vtf"),
            format!("{SCRIPTS}/tf_weapon_rocketlauncher.txt"),
        ]
    );
    assert_eq!(env.lib.assignments().unwrap().len(), 2, "only files change");
}

#[test]
fn uninstalling_the_only_crosshair_leaves_no_managed_folder() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Library(dot.id().clone()),
        )
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();

    env.lib.uninstall_crosshair(dot.id(), &env.tf2).unwrap();
    assert!(!env.tf2.custom_dir().join("tf2tools-crosshairs").exists());
}

#[test]
fn uninstalling_a_crosshair_not_in_the_game_installs_nothing() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    assign_both(&env, &dot);

    env.lib.uninstall_crosshair(dot.id(), &env.tf2).unwrap();
    assert!(tree(&env.tf2.custom_dir()).is_empty());
}

#[test]
fn uninstalling_a_crosshair_reassigned_since_its_install_removes_its_files() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    assign_both(&env, &dot);
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    env.lib
        .assign("tf_weapon_scattergun", &builtin("cpma_9"))
        .unwrap();

    env.lib.uninstall_crosshair(dot.id(), &env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    let files = tree(&custom);
    assert!(
        !files.iter().any(|f| f.contains(dot.folder_name())),
        "{files:?}"
    );
    let script = custom.join(SCRIPTS).join("tf_weapon_scattergun.txt");
    assert!(!script.exists(), "it named the removed texture");
}

#[test]
fn uninstalling_an_unknown_crosshair_is_not_in_library() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    env.lib.delete_crosshair(dot.id()).unwrap();
    let err = env.lib.uninstall_crosshair(dot.id(), &env.tf2).unwrap_err();
    assert!(matches!(err, InstallError::NotInLibrary { .. }), "{err:?}");
}

// It locks a folder with Unix permissions, which Windows does not have.
#[cfg(unix)]
#[test]
fn a_failed_uninstall_keeps_the_crosshair_and_its_assignments() {
    use std::os::unix::fs::PermissionsExt as _;

    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    assign_both(&env, &dot);
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();

    let custom = env.tf2.custom_dir();
    let scripts = custom.join(SCRIPTS);
    let mode = |m| fs::set_permissions(&scripts, fs::Permissions::from_mode(m)).unwrap();
    mode(0o555);
    let failed = env.lib.uninstall_crosshair(dot.id(), &env.tf2);
    mode(0o755);
    assert!(matches!(failed, Err(InstallError::Io { .. })), "{failed:?}");
    let scan = env.lib.installed_crosshairs(&env.tf2).unwrap();
    assert!(
        scan.is_in_game(dot.id()),
        "a retry still finds it installed"
    );
    assert_eq!(env.lib.list_crosshairs().unwrap().len(), 1);
    assert_eq!(env.lib.assignments().unwrap().len(), 2);
}

#[test]
fn deleting_an_assigned_crosshair_clears_the_active_profile() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    assign_both(&env, &dot);
    set_setting(&env.dir, "active_profile_id", "p1");
    env.lib.delete_crosshair(dot.id()).unwrap();
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
}

#[test]
fn deleting_an_unused_crosshair_keeps_the_active_profile() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    set_setting(&env.dir, "active_profile_id", "p1");
    env.lib.delete_crosshair(dot.id()).unwrap();
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some("p1")
    );
}

#[test]
fn uninstalling_a_crosshair_leaves_pending_changes_uninstalled() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    assign_both(&env, &dot);
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    let custom = env.tf2.custom_dir();
    let rocket = custom.join(SCRIPTS).join("tf_weapon_rocketlauncher.txt");
    let installed = fs::read_to_string(&rocket).unwrap();
    // Neither of these has been installed, so neither may reach the game.
    let builtins = catalog();
    let other = builtins
        .all()
        .iter()
        .find(|b| b.name() != "cpma_9")
        .unwrap();
    env.lib
        .assign("tf_weapon_rocketlauncher", &builtin(other.name()))
        .unwrap();
    env.lib
        .assign("tf_weapon_minigun", &builtin("cpma_9"))
        .unwrap();

    env.lib.uninstall_crosshair(dot.id(), &env.tf2).unwrap();
    assert!(!custom.join(SCRIPTS).join("tf_weapon_minigun.txt").exists());
    assert_eq!(fs::read_to_string(&rocket).unwrap(), installed);
}

#[test]
fn deleting_a_library_crosshair_keeps_a_builtin_assignment_of_the_same_name() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    env.lib
        .assign("tf_weapon_scattergun", &builtin(dot.folder_name()))
        .unwrap();
    env.lib.delete_crosshair(dot.id()).unwrap();
    assert_eq!(env.lib.assignments().unwrap().len(), 1);
}

#[test]
fn uninstalling_a_crosshair_keeps_a_script_for_a_longer_texture_name() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Library(dot.id().clone()),
        )
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    let custom = env.tf2.custom_dir();
    let scattergun = custom.join(SCRIPTS).join("tf_weapon_scattergun.txt");
    let longer = fs::read_to_string(&scattergun)
        .unwrap()
        .replace(dot.folder_name(), &format!("{}2", dot.folder_name()));
    let pistol = custom.join(SCRIPTS).join("tf_weapon_pistol.txt");
    fs::write(&pistol, longer).unwrap();

    env.lib.uninstall_crosshair(dot.id(), &env.tf2).unwrap();
    assert!(pistol.exists());
    assert!(!scattergun.exists());
}

#[test]
fn the_scan_tells_which_library_crosshairs_are_in_the_game() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    let ring = import_png(&env, "Ring");
    assert!(
        !env.lib
            .installed_crosshairs(&env.tf2)
            .unwrap()
            .is_in_game(dot.id())
    );
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Library(dot.id().clone()),
        )
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    env.lib
        .assign(
            "tf_weapon_minigun",
            &CrosshairRef::Library(ring.id().clone()),
        )
        .unwrap();
    let scan = env.lib.installed_crosshairs(&env.tf2).unwrap();
    assert!(scan.is_in_game(dot.id()));
    assert!(!scan.is_in_game(ring.id()), "assigned, not installed");
}

#[test]
fn uninstalling_a_crosshair_keeps_a_script_for_a_texture_ending_the_same() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Library(dot.id().clone()),
        )
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    let custom = env.tf2.custom_dir();
    let scattergun = custom.join(SCRIPTS).join("tf_weapon_scattergun.txt");
    let other = fs::read_to_string(&scattergun).unwrap().replace(
        &format!("thumbnails/{}", dot.folder_name()),
        &format!("thumbnails/x{}", dot.folder_name()),
    );
    let pistol = custom.join(SCRIPTS).join("tf_weapon_pistol.txt");
    fs::write(&pistol, other).unwrap();

    env.lib.uninstall_crosshair(dot.id(), &env.tf2).unwrap();
    assert!(pistol.exists());
    assert!(!scattergun.exists());
}

#[test]
fn a_failed_vmt_removal_keeps_the_vtf_so_a_retry_can_finish() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Library(dot.id().clone()),
        )
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    let materials = env.tf2.custom_dir().join(MATERIALS);
    let vtf = materials.join(format!("{}.vtf", dot.folder_name()));
    let vmt = materials.join(format!("{}.vmt", dot.folder_name()));
    // A non-empty folder under the VMT's name is something a file removal cannot take.
    fs::remove_file(&vmt).unwrap();
    write_file(&vmt.join("held.txt"), b"x");

    let failed = env.lib.uninstall_crosshair(dot.id(), &env.tf2);
    assert!(matches!(failed, Err(InstallError::Io { .. })), "{failed:?}");
    assert!(vtf.exists());
    assert!(
        env.lib
            .installed_crosshairs(&env.tf2)
            .unwrap()
            .is_in_game(dot.id())
    );

    fs::remove_dir_all(&vmt).unwrap();
    env.lib.uninstall_crosshair(dot.id(), &env.tf2).unwrap();
    assert!(!vtf.exists());
}

#[test]
fn a_folder_name_climbing_out_of_the_managed_folder_deletes_nothing() {
    let env = crosshair_env();
    let dot = import_png(&env, "Dot");
    env.lib
        .assign("tf_weapon_rocketlauncher", &builtin("cpma_9"))
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    // Six levels up from the thumbnails folder is the tf folder, beside tf/custom.
    let climbing = format!("{}outside", "../".repeat(6));
    sql(&env.dir)
        .execute(
            "UPDATE library_crosshairs SET folder_name = ?1 WHERE id = ?2",
            [climbing.as_str(), dot.id().as_str()],
        )
        .unwrap();
    let tf = env.tf2.custom_dir().parent().unwrap().to_path_buf();
    let vtf = write_file(&tf.join("outside.vtf"), b"keep");
    let vmt = write_file(&tf.join("outside.vmt"), b"keep");

    env.lib.uninstall_crosshair(dot.id(), &env.tf2).unwrap();
    assert!(vtf.exists());
    assert!(vmt.exists());
    let scan = env.lib.installed_crosshairs(&env.tf2).unwrap();
    assert!(!scan.is_in_game(dot.id()));
}

// The crosshair-only script apply wrote before 6.0.2, which stops the weapon from firing.
const OLD_SCRIPT: &str = "\"WeaponData\"\n{\n\t\"TextureData\"\n\t{\n\t\t\"crosshair\"\n\t\t{\n\t\t\t\"file\"\t\t\"vgui/replay/thumbnails/cpma_9\"\n\t\t\t\"x\"\t\t\"0\"\n\t\t\t\"y\"\t\t\"0\"\n\t\t\t\"width\"\t\t\"64\"\n\t\t\t\"height\"\t\"64\"\n\t\t}\n\t}\n}";

#[test]
fn an_old_crosshair_only_script_needs_repair_and_apply_repairs_it() {
    let env = crosshair_env();
    env.lib
        .assign("tf_weapon_scattergun", &builtin("cpma_9"))
        .unwrap();
    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    assert!(!crosshair_scripts_need_repair(&env.tf2));

    let custom = env.tf2.custom_dir();
    write_file(
        &custom.join(SCRIPTS).join("tf_weapon_pistol_scout.txt"),
        OLD_SCRIPT.as_bytes(),
    );
    assert!(crosshair_scripts_need_repair(&env.tf2));

    env.lib.apply_crosshairs(&env.tf2, &catalog()).unwrap();
    assert!(!crosshair_scripts_need_repair(&env.tf2));
}

#[test]
fn no_repair_without_old_weapon_scripts() {
    let env = crosshair_env();
    assert!(!crosshair_scripts_need_repair(&env.tf2));

    let custom = env.tf2.custom_dir();
    fs::create_dir_all(custom.join(SCRIPTS)).unwrap();
    assert!(!crosshair_scripts_need_repair(&env.tf2));

    // Only weapon scripts count, and only those in the managed folder.
    write_file(
        &custom.join(SCRIPTS).join("readme.txt"),
        OLD_SCRIPT.as_bytes(),
    );
    write_file(
        &custom.join("other-mod/scripts/tf_weapon_scattergun.txt"),
        OLD_SCRIPT.as_bytes(),
    );
    assert!(!crosshair_scripts_need_repair(&env.tf2));
}
