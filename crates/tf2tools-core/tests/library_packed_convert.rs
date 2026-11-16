//! Converting HUD copies to packed zips: what is packed, what stays loose, leftovers, failures
//! and the copies lock.

use crate::common;

use std::sync::mpsc;
use std::time::Duration;

use common::packed::{
    convert, hud_dir, library_files, loose_files, loose_hud, packed_files, zip_path,
};
use common::{builtin_catalog, library_env, write_file, zip_bytes};
use tf2tools_core::{DraftSlot, ProfileDraft, Provenance};

const FILES: &[(&str, &[u8])] = &[
    (
        "resource/clientscheme.res",
        b"\"Scheme\" { \"Colors\" { } }",
    ),
    ("materials/vgui/bg.vtf", &[7u8; 8192]),
];

#[test]
fn a_loose_hud_is_packed_with_its_metadata_kept() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let before = library_files(&env, &hud);
    write_file(
        &hud_dir(&env, &hud).join(".tf2tools/thumbnail.jpg"),
        b"jpeg",
    );
    let report = convert(&env);
    assert_eq!((report.huds().packed(), report.huds().failed()), (1, 0));
    assert_eq!(loose_files(&env, &hud), Vec::<String>::new());
    assert_eq!(packed_files(&env, &hud), before);
    assert!(
        hud_dir(&env, &hud)
            .join(".tf2tools/thumbnail.jpg")
            .is_file()
    );
    let bytes: u64 = before.iter().map(|(_, b)| b.len() as u64).sum();
    let zip_len = std::fs::metadata(zip_path(&env, &hud)).unwrap().len();
    assert_eq!(
        (report.huds().bytes_before(), report.huds().bytes_after()),
        (bytes, zip_len)
    );
    assert!(zip_len < bytes, "the VTF bytes must compress");
}

#[test]
fn a_second_pass_changes_nothing() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    convert(&env);
    let stamp = std::fs::metadata(zip_path(&env, &hud))
        .unwrap()
        .modified()
        .unwrap();
    let again = convert(&env);
    let h = again.huds();
    assert_eq!((h.packed(), h.failed(), h.cleaned()), (0, 0, 0));
    assert_eq!(
        std::fs::metadata(zip_path(&env, &hud))
            .unwrap()
            .modified()
            .unwrap(),
        stamp
    );
}

#[test]
fn made_huds_stay_loose() {
    let env = library_env();
    let base = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    convert(&env);
    assert!(zip_path(&env, &base).is_file());
    assert!(!zip_path(&env, &copy).exists());
    assert!(!loose_files(&env, &copy).is_empty());
}

#[test]
fn a_made_hud_caught_mid_save_is_left_alone() {
    let env = library_env();
    let base = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let copy = env.lib.copy_hud(base.id()).unwrap();
    let live = hud_dir(&env, &copy);
    let old = live.with_file_name(format!("{}.old", copy.folder_name()));
    std::fs::rename(&live, &old).unwrap();
    let report = convert(&env);
    assert_eq!(report.huds().failed(), 0);
    assert!(
        !live.exists(),
        "conversion must not recreate a folder a save moved"
    );
    assert!(!old.join(".tf2tools/files.zip").exists());
}

#[test]
fn leftovers_from_a_crash_are_cleaned() {
    let env = library_env();
    let packed = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    convert(&env);
    let before = packed_files(&env, &packed);
    write_file(
        &hud_dir(&env, &packed).join("resource/clientscheme.res"),
        b"stale",
    );
    let other = loose_hud(&env, "flawhud", FILES, Provenance::Local);
    let tmp = hud_dir(&env, &other).join(".tf2tools/files.zip.tmp");
    write_file(&tmp, b"half a zip");
    let report = convert(&env);
    assert_eq!((report.huds().cleaned(), report.huds().packed()), (1, 1));
    assert_eq!(loose_files(&env, &packed), Vec::<String>::new());
    assert_eq!(packed_files(&env, &packed), before);
    assert!(!tmp.exists());
    assert!(zip_path(&env, &other).is_file());
}

#[cfg(unix)]
#[test]
fn a_hud_that_cannot_be_packed_stays_loose_and_installs() {
    use std::os::unix::fs::PermissionsExt;
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let locked = hud_dir(&env, &hud).join("resource/clientscheme.res");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let report = convert(&env);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!((report.huds().packed(), report.huds().failed()), (0, 1));
    assert!(!zip_path(&env, &hud).exists());
    assert!(!hud_dir(&env, &hud).join(".tf2tools/files.zip.tmp").exists());
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert!(
        env.tf2
            .custom_dir()
            .join("rayshud/resource/clientscheme.res")
            .is_file()
    );
}

/// Holds a copies read guard on another thread until the returned sender is used.
fn hold_read(env: &common::LibraryEnv) -> (mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let lib = env.lib.clone();
    let holder = std::thread::spawn(move || {
        lib.with_copies_read(|| {
            held_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
    });
    held_rx.recv().unwrap();
    (release_tx, holder)
}

#[test]
fn a_reader_holding_the_copies_lock_delays_conversion() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let (release, holder) = hold_read(&env);
    let lib = env.lib.clone();
    let pass = std::thread::spawn(move || lib.convert_library());
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !zip_path(&env, &hud).exists(),
        "conversion must wait for the reader"
    );
    release.send(()).unwrap();
    holder.join().unwrap();
    assert_eq!(pass.join().unwrap().huds().packed(), 1);
    assert!(zip_path(&env, &hud).is_file());
}

#[test]
fn entry_points_finish_while_a_conversion_waits() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let other = loose_hud(&env, "flawhud", FILES, Provenance::Local);
    let draft = ProfileDraft::new("P").with_hud(DraftSlot::Item(other.id().clone()));
    let profile = env.lib.create_profile(&draft).unwrap();
    let archive = write_file(
        &env.tmp.path().join("in/budhud.zip"),
        &zip_bytes(&[("budhud/info.vdf", b"\"budhud\" {}".as_slice())]),
    );
    let (release, holder) = hold_read(&env);
    let lib = env.lib.clone();
    let pass = std::thread::spawn(move || lib.convert_library());
    std::thread::sleep(Duration::from_millis(100));
    let (done_tx, done_rx) = mpsc::channel();
    let (lib, tf2) = (env.lib.clone(), env.tf2.clone());
    let (hud_id, profile_id) = (hud.id().clone(), profile.id().clone());
    let worker = std::thread::spawn(move || {
        lib.install_hud(&hud_id, &tf2).unwrap();
        lib.external_huds(&tf2).unwrap();
        lib.apply_profile(&profile_id, &tf2, &builtin_catalog(), false)
            .unwrap();
        lib.uninstall_hud(&tf2).unwrap();
        lib.import_hud(&archive, Provenance::Local).unwrap();
        done_tx.send(()).unwrap();
    });
    std::thread::sleep(Duration::from_millis(100));
    release.send(()).unwrap();
    done_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("an entry point deadlocked behind the waiting conversion");
    worker.join().unwrap();
    holder.join().unwrap();
    assert_eq!(pass.join().unwrap().huds().failed(), 0);
}

/// Converts once and checks the HUD failed and kept every loose file as it was.
fn assert_fails_and_stays_loose(env: &common::LibraryEnv, hud: &tf2tools_core::HudEntry) {
    let before = loose_files(env, hud);
    let report = convert(env);
    let tally = (report.huds().packed(), report.huds().failed());
    assert_eq!(tally, (0, 1), "the HUD must count as failed, not packed");
    assert_eq!(
        loose_files(env, hud),
        before,
        "no loose file may be removed"
    );
    let tmp = hud_dir(env, hud).join(".tf2tools/files.zip.tmp");
    assert!(!tmp.exists(), "a failed pack must leave no tmp zip");
}

#[cfg(unix)]
#[test]
fn a_file_under_a_drive_like_folder_is_never_packed() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    write_file(&hud_dir(&env, &hud).join("a:b/c.res"), b"\"c\" {}");
    assert_fails_and_stays_loose(&env, &hud);
    assert!(!zip_path(&env, &hud).exists());
}

#[cfg(unix)]
#[test]
fn a_linked_zip_does_not_clear_the_loose_tree() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let elsewhere = write_file(
        &env.tmp.path().join("elsewhere/files.zip"),
        &zip_bytes(&[("info.vdf", b"\"x\" {}".as_slice())]),
    );
    std::os::unix::fs::symlink(&elsewhere, zip_path(&env, &hud)).unwrap();
    assert_fails_and_stays_loose(&env, &hud);
}

#[test]
fn a_damaged_zip_does_not_clear_the_loose_tree() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    write_file(&zip_path(&env, &hud), b"not a zip at all");
    assert_fails_and_stays_loose(&env, &hud);
}

#[test]
fn a_conversion_whose_zip_does_not_read_back_leaves_the_hud_loose() {
    let mut env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let before = library_files(&env, &hud);
    env.lib.set_pack_rename(common::packed::rename_then_damage);
    let report = convert(&env);
    assert_eq!((report.huds().packed(), report.huds().failed()), (0, 1));
    assert!(!zip_path(&env, &hud).exists());
    assert_eq!(library_files(&env, &hud), before);
    assert_eq!(loose_files(&env, &hud).len(), before.len());
}

#[test]
fn an_item_that_keeps_failing_is_new_only_the_first_time() {
    use tf2tools_core::{Database, Library};
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let key = format!("huds/{}", hud.folder_name());
    std::fs::create_dir_all(zip_path(&env, &hud)).unwrap();
    let first = convert(&env);
    assert_eq!(first.huds().failed(), 1);
    assert_eq!(first.new_failures(), std::slice::from_ref(&key));
    let second = convert(&env);
    assert_eq!(second.huds().failed(), 1);
    assert!(second.new_failures().is_empty());
    // The memory outlives the process: a fresh Library over the same data is quiet too.
    let restarted = Library::new(Database::open(&env.dir).unwrap(), env.dir.clone());
    assert!(restarted.convert_library().new_failures().is_empty());
    // Once it packs, a later break is new again.
    std::fs::remove_dir(zip_path(&env, &hud)).unwrap();
    let healed = convert(&env);
    assert_eq!((healed.huds().packed(), healed.huds().failed()), (1, 0));
    env.lib.unpack_hud_for_tests(hud.id()).unwrap();
    std::fs::create_dir_all(zip_path(&env, &hud)).unwrap();
    assert_eq!(convert(&env).new_failures(), [key]);
}

#[test]
fn a_damaged_failure_memory_counts_as_empty() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let key = format!("huds/{}", hud.folder_name());
    common::set_setting(&env.dir, "library_pack_failed", "[not json");
    std::fs::create_dir_all(zip_path(&env, &hud)).unwrap();
    assert_eq!(convert(&env).new_failures(), std::slice::from_ref(&key));
    // Conversion still packs once the item heals, and rewrites the memory as valid JSON.
    std::fs::remove_dir(zip_path(&env, &hud)).unwrap();
    let healed = convert(&env);
    assert_eq!((healed.huds().packed(), healed.huds().failed()), (1, 0));
    assert_eq!(
        common::setting(&env.dir, "library_pack_failed").as_deref(),
        Some("[]")
    );
}
