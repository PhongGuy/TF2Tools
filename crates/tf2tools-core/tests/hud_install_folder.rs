//! Which tf/custom folder a library HUD installs into.

use crate::common;

use std::fs;

use common::{
    LibraryEnv, builtin_catalog, gb, library_env, set_setting, setting, tree, write_file, zip_bytes,
};
use tf2tools_core::{
    ApplyStep, DraftSlot, HudEntry, HudStatus, LibraryKind, ProfileDraft, Provenance,
};

const HASH: &str = "4f00092c3b5e8a1d2f6e7a8b9c0d1e2f3a4b5c6d";

/// Imports a HUD whose archive root folder is `root`, holding an `info.vdf` plus `files`
/// (path under the root, text).
fn import_hud(
    env: &LibraryEnv,
    root: &str,
    files: &[(&str, &str)],
    provenance: Provenance,
) -> HudEntry {
    let mut entries: Vec<(String, Vec<u8>)> =
        vec![(format!("{root}/info.vdf"), b"\"hud\" {}".to_vec())];
    for (rel, text) in files {
        entries.push((format!("{root}/{rel}"), text.as_bytes().to_vec()));
    }
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let archive = write_file(
        &env.tmp.path().join(format!("in/{root}.zip")),
        &zip_bytes(&refs),
    );
    env.lib.import_hud(&archive, provenance).unwrap()
}

/// A `#base` line pointing into `tf/custom/<name>/`, as m0rehud and hexhud write them.
fn base_ref(name: &str) -> String {
    format!("#base \"../../custom/{name}/resource/ui/base.res\"\n")
}

#[test]
fn a_hud_installs_under_the_folder_its_files_reference() {
    let env = library_env();
    let root = format!("m0rehud-{HASH}");
    let hud = import_hud(
        &env,
        &root,
        &[("resource/ui/hudlayout.res", base_ref("m0rehud").as_str())],
        Provenance::Local,
    );
    assert_eq!(hud.folder_name(), root);
    let installed = env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(installed.folder(), "m0rehud");
    assert_eq!(installed.blocked_name(), None);
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        ["m0rehud/info.vdf", "m0rehud/resource/ui/hudlayout.res"]
    );
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some("m0rehud")
    );
}

#[test]
fn the_most_referenced_name_wins_and_only_hud_text_files_count() {
    let env = library_env();
    let two_refs = format!("{}{}", base_ref("jofrehud-main"), base_ref("jofrehud-main"));
    let readme = "custom/otherhud/ custom/otherhud/ custom/otherhud/ custom/otherhud/\n";
    let hud = import_hud(
        &env,
        &format!("JofreHUD-{HASH}"),
        &[
            ("resource/ui/a.res", two_refs.as_str()),
            ("cfg/jofrehud/x.cfg", "// tf/custom/jofrehud-main/cfg/\n"),
            (
                "resource/ui/b.res",
                "//#base \"../../../custom/resource/ui/econ/x.res\"\n",
            ),
            ("README.md", readme),
            ("materials/x.vmt", readme),
        ],
        Provenance::Local,
    );
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "jofrehud-main"
    );
}

#[test]
fn paths_that_only_contain_custom_are_not_references() {
    let env = library_env();
    let text = "\"image\" \"replay/thumbnails/custom/speedo_g/a\"\n\
                #base \"../../custom/resource/ui/x.res\"\n\
                #base \"users/custom/stuff/x.res\"\n\
                \"file\" \"../../custom/.hidden/x.res\"\n\
                \"half\" \"../../custom/nofolder\"\n";
    let hud = import_hud(
        &env,
        "speedhud-master",
        &[("resource/ui/s.res", text)],
        Provenance::Local,
    );
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "speedhud-master"
    );
}

#[test]
fn a_tie_goes_to_the_smaller_name() {
    let env = library_env();
    let text = format!("{}{}", base_ref("bhud"), base_ref("ahud"));
    let hud = import_hud(
        &env,
        "tiehud",
        &[("resource/ui/t.res", text.as_str())],
        Provenance::Local,
    );
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "ahud"
    );
}

#[test]
fn reference_matching_ignores_case_of_custom_and_tf() {
    let env = library_env();
    let hud = import_hud(
        &env,
        "casehud-main",
        &[("resource/ui/c.res", "\"x\" \"TF/Custom/casehud/x.res\"\n")],
        Provenance::Local,
    );
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "casehud"
    );
}

#[test]
fn a_hand_imported_zip_without_references_keeps_its_folder_name() {
    let env = library_env();
    let hud = import_hud(
        &env,
        "budhud-master",
        &[(
            "resource/ui/hudlayout.res",
            "\"Resource/HudLayout.res\" {}\n",
        )],
        Provenance::Local,
    );
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "budhud-master"
    );
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        [
            "budhud-master/info.vdf",
            "budhud-master/resource/ui/hudlayout.res"
        ]
    );
}

/// A curated store row, as the hud-db sync writes it.
fn seed_store_row(env: &LibraryEnv, hud_id: &str, repo: &str, hash: &str) {
    common::sql(&env.dir)
        .execute(
            "INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
             VALUES (?1, ?1, 'someone', ?2, ?3, '{}', 'x')",
            [hud_id, repo, hash],
        )
        .unwrap();
}

fn curated(hud_id: &str) -> Provenance {
    Provenance::Curated {
        hud_id: hud_id.to_owned(),
    }
}

#[test]
fn a_store_hud_without_references_installs_under_its_repo_name() {
    let env = library_env();
    seed_store_row(
        &env,
        "m0rehud",
        "https://github.com/mtxfellen/m0rehud",
        "abc123",
    );
    let hud = import_hud(&env, "m0rehud-abc123", &[], curated("m0rehud"));
    assert_eq!(hud.folder_name(), "m0rehud-abc123");
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "m0rehud"
    );
    assert_eq!(tree(&env.tf2.custom_dir()), ["m0rehud/info.vdf"]);
}

#[test]
fn the_repo_name_drops_a_trailing_slash_and_git_suffix() {
    let env = library_env();
    seed_store_row(
        &env,
        "flawhud",
        "https://github.com/CriticalFlaw/flawhud.git/",
        HASH,
    );
    let hud = import_hud(&env, &format!("flawhud-{HASH}"), &[], curated("flawhud"));
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "flawhud"
    );
}

#[test]
fn a_store_hud_whose_store_row_is_gone_drops_the_commit_hash() {
    let env = library_env();
    let hud = import_hud(&env, &format!("rayshud-{HASH}"), &[], curated("rayshud"));
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "rayshud"
    );
}

#[test]
fn a_repo_name_that_is_not_a_plain_folder_falls_back_to_the_stripped_folder() {
    let env = library_env();
    seed_store_row(&env, "oddhud", "https://github.com/someone/..", HASH);
    let hud = import_hud(&env, &format!("oddhud-{HASH}"), &[], curated("oddhud"));
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "oddhud"
    );
}

#[test]
fn references_beat_the_repo_name() {
    let env = library_env();
    seed_store_row(
        &env,
        "cosmoshud",
        "https://github.com/Zeesastrous/CosmosHUD",
        HASH,
    );
    let hud = import_hud(
        &env,
        &format!("CosmosHUD-{HASH}"),
        &[("resource/ui/hudlayout.res", base_ref("cosmoshud").as_str())],
        curated("cosmoshud"),
    );
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "cosmoshud"
    );
}

#[test]
fn gamebanana_and_local_huds_keep_a_hex_looking_suffix() {
    let env = library_env();
    let hud = import_hud(&env, "cool-hud-1234567", &[], gb(7));
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "cool-hud-1234567"
    );
    let hud = import_hud(&env, "plain-hud-abcdef0", &[], Provenance::Local);
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "plain-hud-abcdef0"
    );
}

#[test]
fn a_non_hud_folder_with_the_expected_name_is_left_alone() {
    let env = library_env();
    let root = format!("m0rehud-{HASH}");
    let hud = import_hud(
        &env,
        &root,
        &[("resource/ui/hudlayout.res", base_ref("m0rehud").as_str())],
        Provenance::Local,
    );
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("m0rehud/notes.txt"), b"mine");
    let installed = env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(installed.folder(), root);
    assert_eq!(installed.blocked_name(), Some("m0rehud"));
    assert_eq!(fs::read(custom.join("m0rehud/notes.txt")).unwrap(), b"mine");
    assert!(!custom.join("m0rehud/info.vdf").exists());
    assert!(custom.join(&root).join("info.vdf").is_file());
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some(root.as_str())
    );
}

/// A HUD that expects `tf/custom/m0rehud`, kept in the library under `m0rehud-<hash>`.
fn m0rehud(env: &LibraryEnv) -> HudEntry {
    import_hud(
        env,
        &format!("m0rehud-{HASH}"),
        &[("resource/ui/hudlayout.res", base_ref("m0rehud").as_str())],
        Provenance::Local,
    )
}

#[test]
fn a_file_with_the_expected_name_is_left_alone() {
    let env = library_env();
    let hud = m0rehud(&env);
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("m0rehud"), b"mine");
    let installed = env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(installed.folder(), hud.folder_name());
    assert_eq!(installed.blocked_name(), Some("m0rehud"));
    assert_eq!(fs::read(custom.join("m0rehud")).unwrap(), b"mine");
    assert!(custom.join(hud.folder_name()).join("info.vdf").is_file());
}

#[cfg(unix)]
#[test]
fn a_symlink_with_the_expected_name_is_left_alone() {
    let env = library_env();
    let hud = m0rehud(&env);
    let custom = env.tf2.custom_dir();
    let elsewhere = env.tmp.path().join("elsewhere");
    write_file(&elsewhere.join("notes.txt"), b"mine");
    fs::create_dir_all(&custom).unwrap();
    std::os::unix::fs::symlink(&elsewhere, custom.join("m0rehud")).unwrap();
    let installed = env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(installed.folder(), hud.folder_name());
    assert_eq!(installed.blocked_name(), Some("m0rehud"));
    assert_eq!(tree(&elsewhere), ["notes.txt"]);
    assert!(custom.join(hud.folder_name()).join("info.vdf").is_file());
}

#[cfg(unix)]
#[test]
fn a_dangling_symlink_with_the_expected_name_is_left_alone() {
    let env = library_env();
    let hud = m0rehud(&env);
    let custom = env.tf2.custom_dir();
    fs::create_dir_all(&custom).unwrap();
    let link = custom.join("m0rehud");
    std::os::unix::fs::symlink(env.tmp.path().join("gone"), &link).unwrap();
    let installed = env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(installed.blocked_name(), Some("m0rehud"));
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
}

#[test]
fn switching_huds_leaves_only_the_new_one() {
    let env = library_env();
    let first = import_hud(
        &env,
        &format!("m0rehud-{HASH}"),
        &[("resource/ui/hudlayout.res", base_ref("m0rehud").as_str())],
        Provenance::Local,
    );
    let second = import_hud(&env, "rayshud", &[], Provenance::Local);
    env.lib.install_hud(first.id(), &env.tf2).unwrap();
    env.lib.install_hud(second.id(), &env.tf2).unwrap();
    assert_eq!(tree(&env.tf2.custom_dir()), ["rayshud/info.vdf"]);
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some("rayshud")
    );
}

#[test]
fn reinstalling_moves_an_old_install_to_the_expected_folder() {
    let env = library_env();
    let root = format!("m0rehud-{HASH}");
    let hud = import_hud(
        &env,
        &root,
        &[("resource/ui/hudlayout.res", base_ref("m0rehud").as_str())],
        Provenance::Local,
    );
    // An install made before this change sits under the library folder name.
    let library = env.dir.library_dir(LibraryKind::Huds).join(&root);
    for rel in tree(&library) {
        let bytes = fs::read(library.join(&rel)).unwrap();
        write_file(&env.tf2.custom_dir().join(&root).join(&rel), &bytes);
    }
    set_setting(&env.dir, "installed_hud_folder", &root);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        ["m0rehud/info.vdf", "m0rehud/resource/ui/hudlayout.res"]
    );
}

fn referencing_hud(env: &LibraryEnv) -> HudEntry {
    import_hud(
        env,
        &format!("m0rehud-{HASH}"),
        &[("resource/ui/hudlayout.res", base_ref("m0rehud").as_str())],
        Provenance::Local,
    )
}

#[test]
fn status_maps_the_expected_folder_back_to_its_library_hud() {
    let env = library_env();
    let hud = referencing_hud(&env);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        setting(&env.dir, "installed_hud_id").as_deref(),
        Some(hud.id().as_str())
    );
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Managed {
            folder_name: "m0rehud".into(),
            id: hud.id().clone()
        }
    );
}

#[test]
fn an_install_under_the_library_folder_name_stays_managed() {
    let env = library_env();
    let hud = referencing_hud(&env);
    let root = hud.folder_name().to_owned();
    write_file(&env.tf2.custom_dir().join(&root).join("info.vdf"), b"old");
    set_setting(&env.dir, "installed_hud_folder", &root);
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Managed {
            folder_name: root,
            id: hud.id().clone()
        }
    );
}

#[test]
fn a_deleted_library_hud_shows_as_unmanaged() {
    let env = library_env();
    let hud = referencing_hud(&env);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    env.lib.delete_hud(hud.id()).unwrap();
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Unmanaged {
            folder_name: "m0rehud".into()
        }
    );
}

#[test]
fn a_hand_placed_folder_is_not_claimed_by_the_last_install() {
    let env = library_env();
    let hud = referencing_hud(&env);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    fs::remove_dir_all(custom.join("m0rehud")).unwrap();
    write_file(&custom.join("manual/info.vdf"), b"x");
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Unmanaged {
            folder_name: "manual".into()
        }
    );
}

#[test]
fn uninstall_clears_the_installed_id() {
    let env = library_env();
    let hud = referencing_hud(&env);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    env.lib.uninstall_hud(&env.tf2).unwrap();
    assert_eq!(setting(&env.dir, "installed_hud_id").as_deref(), Some(""));
}

#[test]
fn save_current_keeps_a_hud_installed_under_its_expected_folder() {
    let env = library_env();
    let hud = referencing_hud(&env);
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let p = env.lib.save_current_profile("Now", &env.tf2).unwrap();
    assert_eq!(p.hud().unwrap().folder_name(), hud.folder_name());
}

#[test]
fn a_referenced_name_up_to_64_bytes_is_used() {
    let env = library_env();
    let name = "a".repeat(64);
    let hud = import_hud(
        &env,
        "longhud-main",
        &[("resource/ui/l.res", base_ref(&name).as_str())],
        Provenance::Local,
    );
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        name
    );
}

#[test]
fn a_referenced_name_over_64_bytes_keeps_the_library_folder() {
    let env = library_env();
    let hud = import_hud(
        &env,
        "longhud-main",
        &[("resource/ui/l.res", base_ref(&"a".repeat(65)).as_str())],
        Provenance::Local,
    );
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "longhud-main"
    );
}

#[test]
fn text_files_over_4_mib_are_not_scanned() {
    let env = library_env();
    let hud = import_hud(&env, "bighud-main", &[], Provenance::Local);
    let mut big = base_ref("bighud").into_bytes();
    big.resize(4 * 1024 * 1024 + 1, b' ');
    let lib = env.dir.library_dir(LibraryKind::Huds).join("bighud-main");
    write_file(&lib.join("resource/ui/items.txt"), &big);
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "bighud-main"
    );
}

#[test]
fn a_store_hud_off_github_does_not_take_its_repo_name() {
    let env = library_env();
    seed_store_row(&env, "gbhud", "https://gamebanana.com/mods/461758", HASH);
    let hud = import_hud(&env, &format!("gbhud-{HASH}"), &[], curated("gbhud"));
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "gbhud"
    );
}

// An unreadable non-text file passes the scan, then fails the copy after the old HUD went.
#[cfg(unix)]
#[test]
fn a_failed_copy_leaves_no_hud_recorded_as_installed() {
    use std::os::unix::fs::PermissionsExt;
    let env = library_env();
    let old = referencing_hud(&env);
    env.lib.install_hud(old.id(), &env.tf2).unwrap();
    let new = import_hud(
        &env,
        "newhud",
        &[("materials/n.vtf", "x")],
        Provenance::Local,
    );
    let locked = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join("newhud/materials/n.vtf");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let result = env.lib.install_hud(new.id(), &env.tf2);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(result.is_err());
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some("")
    );
    assert_eq!(setting(&env.dir, "installed_hud_id").as_deref(), Some(""));
}

fn profile_with(env: &LibraryEnv, hud: &HudEntry) -> tf2tools_core::Profile {
    let draft = ProfileDraft::new("P").with_hud(DraftSlot::Item(hud.id().clone()));
    env.lib.create_profile(&draft).unwrap()
}

#[test]
fn applying_a_profile_installs_its_hud_under_the_expected_folder() {
    let env = library_env();
    let hud = referencing_hud(&env);
    let p = profile_with(&env, &hud);
    let report = env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap();
    assert!(report.failed().is_empty());
    assert!(report.applied().contains(&ApplyStep::Hud));
    assert_eq!(report.hud_blocked_name(), None);
    assert!(env.tf2.custom_dir().join("m0rehud/info.vdf").is_file());
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Managed {
            folder_name: "m0rehud".into(),
            id: hud.id().clone()
        }
    );
}

#[test]
fn applying_a_profile_reports_a_blocked_hud_name() {
    let env = library_env();
    let hud = referencing_hud(&env);
    let p = profile_with(&env, &hud);
    write_file(&env.tf2.custom_dir().join("m0rehud/notes.txt"), b"mine");
    let report = env
        .lib
        .apply_profile(p.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap();
    assert!(report.failed().is_empty());
    assert_eq!(report.hud_blocked_name(), Some("m0rehud"));
}
