//! Tree hashes, borrowing and card pictures of HUDs whose library copy is packed.

use crate::common;

use common::hud_copy::{empty_game, store_item};
use common::packed::{convert, hud_dir, loose_hud};
use common::{library_env, sql};
use tf2tools_core::{Provenance, RgbaImage, rgba_to_vtf};

const FILES: &[(&str, &[u8])] = &[
    ("resource/clientscheme.res", b"\"Scheme\" {}"),
    ("resource/ui/hudlayout.res", b"\"layout\" {}"),
];

#[test]
fn a_packed_hud_has_the_tree_hash_of_its_loose_form_and_is_cached() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let loose = env.lib.hud_tree_hash(hud.id()).unwrap();
    convert(&env);
    let packed = env.lib.hud_tree_hash(hud.id()).unwrap();
    assert_eq!(packed.hash(), loose.hash());
    assert!(packed.files_read() > 0);
    let again = env.lib.hud_tree_hash(hud.id()).unwrap();
    assert_eq!((again.hash(), again.files_read()), (loose.hash(), 0));
}

#[test]
fn a_replaced_zip_is_hashed_again() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", FILES, store_item());
    convert(&env);
    let first = env.lib.hud_tree_hash(hud.id()).unwrap();
    let newer = loose_hud(
        &env,
        "rayshud",
        &[("resource/clientscheme.res", b"\"v2\" {}")],
        store_item(),
    );
    assert_eq!(newer.folder_name(), hud.folder_name());
    convert(&env);
    let second = env.lib.hud_tree_hash(newer.id()).unwrap();
    assert!(second.files_read() > 0);
    assert_ne!(second.hash(), first.hash());

    // Same entry sizes, other bytes: only the zip's modified time can tell the cache.
    let zip = hud_dir(&env, &newer).join(".tf2tools/files.zip");
    let stamp = std::fs::metadata(&zip).unwrap().modified().unwrap();
    let same_size = loose_hud(
        &env,
        "rayshud",
        &[("resource/clientscheme.res", b"\"v3\" {}")],
        store_item(),
    );
    convert(&env);
    let later = stamp + std::time::Duration::from_secs(10);
    std::fs::File::options()
        .write(true)
        .open(&zip)
        .unwrap()
        .set_modified(later)
        .unwrap();
    let third = env.lib.hud_tree_hash(same_size.id()).unwrap();
    assert!(
        third.files_read() > 0,
        "a rewritten zip with equal entry sizes must be hashed again"
    );
    assert_ne!(third.hash(), second.hash());
}

#[test]
fn a_packed_hud_can_be_borrowed_from() {
    let env = library_env();
    let (_game, tf) = empty_game();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    convert(&env);
    let listed: Vec<_> = (env.lib.list_borrowable(None).unwrap().iter())
        .map(|h| h.id().clone())
        .collect();
    assert_eq!(listed, [hud.id().clone()]);
    let (_, files) = env.lib.borrow_files(hud.id(), &tf).unwrap();
    let file = files.read("resource/clientscheme.res").unwrap().unwrap();
    assert_eq!(file.bytes(), b"\"Scheme\" {}");
}

#[test]
fn a_borrow_opened_before_conversion_still_reads_after_it() {
    let env = library_env();
    let (_game, tf) = empty_game();
    let hud = loose_hud(&env, "rayshud", FILES, Provenance::Local);
    let (_, files) = env.lib.borrow_files(hud.id(), &tf).unwrap();
    convert(&env);
    assert!(
        !hud_dir(&env, &hud)
            .join("resource/ui/hudlayout.res")
            .exists()
    );
    let file = files.read("resource/ui/hudlayout.res").unwrap().unwrap();
    assert_eq!(file.bytes(), b"\"layout\" {}");
}

#[test]
fn the_thumbnail_backfill_reads_a_packed_hud() {
    let env = library_env();
    let red =
        rgba_to_vtf(&RgbaImage::from_pixels(64, 32, [220u8, 20, 20, 255].repeat(64 * 32)).unwrap())
            .unwrap();
    let hud = loose_hud(
        &env,
        "rayshud",
        &[("materials/console/background_upward_widescreen.vtf", &red)],
        Provenance::Local,
    );
    convert(&env);
    let picture = hud_dir(&env, &hud).join(".tf2tools/thumbnail.jpg");
    let _ = std::fs::remove_file(&picture);
    let db = sql(&env.dir);
    db.execute("UPDATE library_huds SET has_thumbnail = 0", [])
        .unwrap();
    db.execute("DELETE FROM settings WHERE key = 'hud_thumbnails_made'", [])
        .unwrap();
    assert_eq!(env.lib.make_missing_hud_thumbnails().unwrap(), 1);
    assert!(picture.is_file());
}
