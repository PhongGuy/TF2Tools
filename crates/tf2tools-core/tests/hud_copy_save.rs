//! Saving a copy in place: kept originals, base bytes on reset, untouched files left alone, crash repair.

mod common;

use std::fs::read_to_string;
use std::path::PathBuf;

use common::hud_copy::{game_for, import_stock_hud, snapshot};
use common::hud_stock::{AMMO, HEALTH};
use common::{LibraryEnv, library_env};
use tf2tools_core::{Change, HudEntry, LibraryKind, MakeTarget, Platform, Provenance, SetKey};

const H: &str = "resource/ui/hudplayerhealth.res";
const A: &str = "resource/ui/hudammoweapons.res";

fn red() -> Change {
    Change::set(
        1,
        SetKey::new(
            H,
            &["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
            "fgcolor",
            None,
            Some("TanDark"),
            "255 80 80 255",
        ),
    )
}
fn ammo() -> Change {
    Change::set(
        2,
        SetKey::new(
            A,
            &["Resource/UI/HudAmmoWeapons.res", "AmmoInClip"],
            "xpos",
            None,
            Some("10"),
            "20",
        ),
    )
}
fn dir(env: &LibraryEnv, hud: &HudEntry) -> PathBuf {
    env.dir
        .library_dir(LibraryKind::Huds)
        .join(hud.folder_name())
}
fn save(env: &LibraryEnv, tf: &std::path::Path, copy: &HudEntry, changes: &[Change]) {
    let files = env.lib.made_hud_files(copy.id(), tf).unwrap();
    env.lib
        .save_made_hud(
            &MakeTarget::Existing(copy.id().clone()),
            copy.name(),
            &files,
            Platform::Linux,
            changes,
        )
        .unwrap();
}

#[test]
fn saving_keeps_the_original_reopens_over_it_and_leaves_the_base_alone() {
    let env = library_env();
    let tf = game_for(&env, &[]);
    let base = import_stock_hud(&env, "somehud", &[("README.txt", "hi")], Provenance::Local);
    let base_before = snapshot(&dir(&env, &base));
    let copy = env.lib.copy_hud(base.id()).unwrap();
    save(&env, &tf, &copy, &[red()]);
    let d = dir(&env, &copy);
    assert!(read_to_string(d.join(H)).unwrap().contains("255 80 80 255"));
    assert_eq!(
        read_to_string(d.join(".tf2tools/original").join(H)).unwrap(),
        HEALTH
    );
    assert_eq!(snapshot(&dir(&env, &base)), base_before);
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let made = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap();
    assert_eq!(
        (made.changes().len(), made.dropped(), made.tf2_updated()),
        (1, 0, false)
    );
    assert_eq!(
        made.pristine().document(H).unwrap().as_bytes(),
        HEALTH.as_bytes()
    );
    save(&env, &tf, &copy, &[]);
    assert_eq!(read_to_string(d.join(H)).unwrap(), HEALTH);
    assert!(!d.join(".tf2tools/original").join(H).exists());
}

#[cfg(unix)]
#[test]
fn untouched_files_are_never_rewritten() {
    use std::os::unix::fs::MetadataExt;
    let env = library_env();
    let tf = game_for(&env, &[]);
    let base = import_stock_hud(&env, "somehud", &[("README.txt", "hi")], Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let d = dir(&env, &copy);
    let stamp = |rel: &str| {
        let m = std::fs::metadata(d.join(rel)).unwrap();
        (m.ino(), m.mtime(), m.mtime_nsec())
    };
    let before = (stamp("README.txt"), stamp("scripts/hudlayout.res"));
    save(&env, &tf, &copy, &[red()]);
    assert_eq!(
        (stamp("README.txt"), stamp("scripts/hudlayout.res")),
        before
    );
}

#[test]
fn a_file_the_maker_adds_over_a_game_file_stays_out_of_the_base() {
    let env = library_env();
    let tf = game_for(&env, &[(A, AMMO.as_bytes())]);
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    std::fs::remove_file(dir(&env, &base).join(A)).unwrap();
    let copy = env.lib.copy_hud(base.id()).unwrap();
    save(&env, &tf, &copy, &[ammo()]);
    assert!(
        dir(&env, &copy).join(A).is_file()
            && !dir(&env, &copy).join(".tf2tools/original").join(A).exists()
    );
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let made = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap();
    assert_eq!(
        made.pristine().document(A).unwrap().as_bytes(),
        AMMO.as_bytes()
    );
}

#[test]
fn a_crash_after_any_rename_step_reopens_and_repairs() {
    // Going from S1 (red health) to S2 (ammo moved, health reset) renames, in order: the ammo
    // original, the edited ammo file, the restored health file, maker.json. A twin copy saved
    // to S2 supplies each step's bytes; a crash after step k is the S1 folder with steps 1..=k.
    let env = library_env();
    let tf = game_for(&env, &[]);
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    let twin = env.lib.copy_hud(base.id()).unwrap();
    save(&env, &tf, &twin, &[red()]);
    save(&env, &tf, &twin, &[ammo()]);
    let s2 = dir(&env, &twin);
    let steps = [
        format!(".tf2tools/original/{A}"),
        A.to_owned(),
        H.to_owned(),
        ".tf2tools/maker.json".to_owned(),
    ];
    for k in 1..=steps.len() {
        let copy = env.lib.copy_hud(base.id()).unwrap();
        save(&env, &tf, &copy, &[red()]);
        let d = dir(&env, &copy);
        for rel in &steps[..k] {
            std::fs::create_dir_all(d.join(rel).parent().unwrap()).unwrap();
            std::fs::copy(s2.join(rel), d.join(rel)).unwrap();
        }
        let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
        let made = env
            .lib
            .open_made_hud(copy.id(), &files, Platform::Linux)
            .unwrap();
        assert_eq!(
            made.pristine().document(H).unwrap().as_bytes(),
            HEALTH.as_bytes(),
            "step {k}"
        );
        assert_eq!(
            made.pristine().document(A).unwrap().as_bytes(),
            AMMO.as_bytes(),
            "step {k}"
        );
        save(&env, &tf, &copy, &[ammo()]);
        assert_eq!(snapshot(&d), snapshot(&s2), "step {k}");
    }
}

#[test]
fn a_crash_while_adding_a_game_file_reopens_on_the_game_s_bytes_and_repairs() {
    // Going from S1 (red health) to S2 (ammo moved, health reset) with the ammo file only in
    // the game: the list of paths being added, the added ammo file, the restored health file,
    // maker.json. A crash after step k is the S1 folder with steps 1..=k.
    let env = library_env();
    let tf = game_for(&env, &[(A, AMMO.as_bytes())]);
    let base = import_stock_hud(&env, "somehud", &[], Provenance::Local);
    std::fs::remove_file(dir(&env, &base).join(A)).unwrap();
    let twin = env.lib.copy_hud(base.id()).unwrap();
    save(&env, &tf, &twin, &[red()]);
    save(&env, &tf, &twin, &[ammo()]);
    let s2 = dir(&env, &twin);
    assert!(!s2.join(".tf2tools/adding.txt").exists());
    for k in 1..=4 {
        let copy = env.lib.copy_hud(base.id()).unwrap();
        save(&env, &tf, &copy, &[red()]);
        let d = dir(&env, &copy);
        std::fs::write(d.join(".tf2tools/adding.txt"), format!("{A}\n")).unwrap();
        for rel in [A, H, ".tf2tools/maker.json"].iter().take(k - 1) {
            std::fs::create_dir_all(d.join(rel).parent().unwrap()).unwrap();
            std::fs::copy(s2.join(rel), d.join(rel)).unwrap();
        }
        let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
        let made = env
            .lib
            .open_made_hud(copy.id(), &files, Platform::Linux)
            .unwrap();
        assert_eq!(
            made.pristine().document(H).unwrap().as_bytes(),
            HEALTH.as_bytes(),
            "step {k}"
        );
        assert_eq!(
            made.pristine().document(A).unwrap().as_bytes(),
            AMMO.as_bytes(),
            "step {k}"
        );
        save(&env, &tf, &copy, &[ammo()]);
        assert_eq!(snapshot(&d), snapshot(&s2), "step {k}");
        // Resetting must drop the added file rather than keep it as part of the base.
        save(&env, &tf, &copy, &[]);
        assert!(!d.join(A).exists(), "step {k}");
    }
}
