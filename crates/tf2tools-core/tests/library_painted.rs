//! Painted crosshairs: the maker's save and reopen contract over a temp library.

use crate::common;

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use common::{LibraryEnv, builtin_catalog, crosshair_env, import_png_as, library_env, sql, tree};
use tf2tools_core::{
    CrosshairEntry, CrosshairFormat, CrosshairId, CrosshairRef, LibraryError, LibraryKind,
    PaintError, PaintTarget, Provenance, RgbaImage, decode_vtf, rgba_to_vtf,
};

#[test]
fn a_painted_row_lists_as_painted_and_an_unknown_type_as_png() {
    let env = library_env();
    sql(&env.dir)
        .execute_batch(
            "INSERT INTO library_crosshairs (id, folder_name, name, source_type, source, created_at)
             VALUES ('p1', 'p-p1', 'Painted', 'painted', 'local', '2026-10-21T00:00:00.000Z'),
                    ('z1', 'z-z1', 'Zed', 'zzz', 'local', '2026-10-21T00:00:00.000Z');",
        )
        .unwrap();
    let formats: Vec<(String, CrosshairFormat)> = env
        .lib
        .list_crosshairs()
        .unwrap()
        .iter()
        .map(|e| (e.name().to_owned(), e.format()))
        .collect();
    assert_eq!(
        formats,
        [
            ("Painted".to_owned(), CrosshairFormat::Painted),
            ("Zed".to_owned(), CrosshairFormat::Png)
        ]
    );
}

/// A square drawing with opaque texels that all differ and some erased ones, so a crop,
/// flip or channel swap shows.
fn drawing(side: u32) -> RgbaImage {
    let pixels = (0..side * side)
        .flat_map(|i| {
            if i % 5 == 0 {
                [0, 0, 0, 0]
            } else {
                [i as u8, (i / 7) as u8, (i * 3) as u8, 255]
            }
        })
        .collect();
    RgbaImage::from_pixels(side, side, pixels).unwrap()
}

/// Each texel of `small` as a 2x2 block.
fn doubled(small: &RgbaImage) -> Vec<u8> {
    let side = small.width() as usize;
    let mut out = Vec::with_capacity(side * side * 16);
    for y in 0..side * 2 {
        for x in 0..side * 2 {
            let at = ((y / 2) * side + x / 2) * 4;
            out.extend_from_slice(&small.pixels()[at..at + 4]);
        }
    }
    out
}

fn save_new(env: &LibraryEnv, name: &str, drawing: &RgbaImage) -> CrosshairEntry {
    env.lib
        .save_painted_crosshair(&PaintTarget::New, name, drawing)
        .unwrap()
}

fn dir_of(env: &LibraryEnv, entry: &CrosshairEntry) -> PathBuf {
    env.dir
        .library_dir(LibraryKind::Crosshairs)
        .join(entry.folder_name())
}

fn vtf_of(env: &LibraryEnv, entry: &CrosshairEntry) -> Vec<u8> {
    fs::read(dir_of(env, entry).join(format!("{}.vtf", entry.folder_name()))).unwrap()
}

fn png_pixels(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .unwrap()
        .to_rgba8();
    (img.width(), img.height(), img.into_raw())
}

fn row(env: &LibraryEnv, id: &CrosshairId) -> (String, String, String, Option<i64>) {
    sql(&env.dir)
        .query_row(
            "SELECT name, source_type, source, store_gb_id FROM library_crosshairs WHERE id = ?1",
            [id.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
}

fn crosshair_folders(env: &LibraryEnv) -> usize {
    fs::read_dir(env.dir.library_dir(LibraryKind::Crosshairs)).map_or(0, |d| d.count())
}

#[test]
fn a_new_64_drawing_saves_source_preview_vtf_and_a_painted_row() {
    let env = library_env();
    let art = drawing(64);
    let entry = save_new(&env, "  My Dot ", &art);
    let name = format!("my-dot-{}", &entry.id().as_str()[..8]);
    assert_eq!(entry.folder_name(), name);
    assert_eq!(entry.name(), "My Dot");
    assert_eq!(entry.format(), CrosshairFormat::Painted);
    assert_eq!(entry.provenance(), &Provenance::Local);
    let dir = dir_of(&env, &entry);
    assert_eq!(
        tree(&dir),
        [
            format!("{name}.vmt"),
            format!("{name}.vtf"),
            "preview.png".into(),
            "source.png".into()
        ]
    );
    let source = fs::read(dir.join("source.png")).unwrap();
    assert_eq!(png_pixels(&source), (64, 64, art.pixels().to_vec()));
    assert_eq!(fs::read(dir.join("preview.png")).unwrap(), source);
    assert!(
        vtf_of(&env, &entry) == rgba_to_vtf(&art).unwrap(),
        "the VTF is not the drawing"
    );
    let vmt = fs::read_to_string(dir.join(format!("{name}.vmt"))).unwrap();
    assert!(vmt.contains("\"$vertexcolor\" \"1\""), "{vmt}");
    assert_eq!(
        row(&env, entry.id()),
        ("My Dot".into(), "painted".into(), "local".into(), None)
    );
    assert_eq!(env.lib.list_crosshairs().unwrap(), vec![entry]);
}

#[test]
fn a_32_drawing_keeps_its_size_in_source_and_exports_a_doubled_64_vtf() {
    let env = library_env();
    let art = drawing(32);
    let entry = save_new(&env, "Small", &art);
    let dir = dir_of(&env, &entry);
    assert_eq!(
        png_pixels(&fs::read(dir.join("source.png")).unwrap()),
        (32, 32, art.pixels().to_vec())
    );
    let vtf = vtf_of(&env, &entry);
    assert_eq!(vtf.len(), 16_464);
    let exported = decode_vtf(&vtf).unwrap();
    assert_eq!((exported.width(), exported.height()), (64, 64));
    assert!(
        exported.pixels() == doubled(&art).as_slice(),
        "the VTF is not the drawing doubled"
    );
}

#[test]
fn a_bad_size_or_a_blank_name_is_refused_before_any_file() {
    let env = library_env();
    for (w, h) in [(16, 16), (64, 32), (128, 128)] {
        let odd = RgbaImage::from_pixels(w, h, vec![0; (w * h * 4) as usize]).unwrap();
        let err = env
            .lib
            .save_painted_crosshair(&PaintTarget::New, "Odd", &odd)
            .unwrap_err();
        assert!(
            matches!(err, PaintError::InvalidDrawing { width, height } if (width, height) == (w, h)),
            "{err:?}"
        );
    }
    let err = env
        .lib
        .save_painted_crosshair(&PaintTarget::New, " \t ", &drawing(64))
        .unwrap_err();
    assert!(matches!(err, PaintError::EmptyName), "{err:?}");
    assert_eq!(crosshair_folders(&env), 0);
    assert!(env.lib.list_crosshairs().unwrap().is_empty());
}

#[test]
fn a_name_another_crosshair_has_in_any_case_is_taken_for_painted_saves_only() {
    let env = library_env();
    import_png_as(&env, "Dot", Provenance::Local);
    save_new(&env, "Éclair", &drawing(64));
    for name in ["dot", " DOT ", "ÉCLAIR"] {
        let err = env
            .lib
            .save_painted_crosshair(&PaintTarget::New, name, &drawing(32))
            .unwrap_err();
        assert!(
            matches!(&err, PaintError::NameTaken { name: taken } if taken == name.trim()),
            "{name}: {err:?}"
        );
    }
    assert_eq!(crosshair_folders(&env), 2);
    import_png_as(&env, "éclair", Provenance::Local);
    assert_eq!(
        env.lib.list_crosshairs().unwrap().len(),
        3,
        "imports keep today's rule"
    );
}

#[test]
fn a_failed_insert_removes_the_new_folder() {
    let env = library_env();
    sql(&env.dir)
        .execute_batch(
            "CREATE TRIGGER no_crosshairs BEFORE INSERT ON library_crosshairs
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    let err = env
        .lib
        .save_painted_crosshair(&PaintTarget::New, "Dot", &drawing(64))
        .unwrap_err();
    assert!(matches!(err, PaintError::Database { .. }), "{err:?}");
    assert_eq!(crosshair_folders(&env), 0);
}

#[cfg(unix)]
#[test]
fn an_unwritable_library_is_an_io_error_with_no_row() {
    use std::os::unix::fs::PermissionsExt;
    let env = library_env();
    let lib_dir = env.dir.library_dir(LibraryKind::Crosshairs);
    fs::create_dir_all(&lib_dir).unwrap();
    fs::set_permissions(&lib_dir, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(lib_dir.join("probe"), b"").is_ok() {
        fs::set_permissions(&lib_dir, fs::Permissions::from_mode(0o755)).unwrap();
        return; // running as root: permissions are not enforced
    }
    let result = env
        .lib
        .save_painted_crosshair(&PaintTarget::New, "Dot", &drawing(64));
    fs::set_permissions(&lib_dir, fs::Permissions::from_mode(0o755)).unwrap();
    let err = result.unwrap_err();
    assert!(matches!(err, PaintError::Io { .. }), "{err:?}");
    assert_eq!(crosshair_folders(&env), 0);
    assert!(env.lib.list_crosshairs().unwrap().is_empty());
}

#[test]
fn apply_installs_a_painted_crosshair_like_any_other() {
    let env = crosshair_env();
    let entry = save_new(&env, "Painted", &drawing(32));
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Library(entry.id().clone()),
        )
        .unwrap();
    let report = env
        .lib
        .apply_crosshairs(&env.tf2, &builtin_catalog())
        .unwrap();
    assert_eq!(report.applied(), ["tf_weapon_scattergun"]);
    let f = entry.folder_name();
    let managed = env.tf2.custom_dir().join("tf2tools-crosshairs");
    let installed = managed.join(format!("materials/vgui/replay/thumbnails/{f}.vtf"));
    assert!(fs::read(installed).unwrap() == vtf_of(&env, &entry));
    let script = fs::read_to_string(managed.join("scripts/tf_weapon_scattergun.txt")).unwrap();
    assert!(
        script.contains(&format!("vgui/replay/thumbnails/{f}")),
        "{script}"
    );
}

fn png_bytes(image: &image::RgbaImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn damage_source(dir: &Path, how: &str) {
    let source = dir.join("source.png");
    match how {
        "missing" => fs::remove_file(&source).unwrap(),
        "garbage" => fs::write(&source, b"not a png").unwrap(),
        "wrong size" => fs::write(&source, png_bytes(&image::RgbaImage::new(16, 16))).unwrap(),
        other => panic!("unknown damage {other}"),
    }
}

#[test]
fn a_saved_drawing_reopens_identical_at_its_own_size() {
    let env = library_env();
    for side in [32, 64] {
        let art = drawing(side);
        let entry = save_new(&env, &format!("Art {side}"), &art);
        let source = env.lib.painted_source(entry.id()).unwrap();
        assert!(source.drawing() == &art, "{side}");
        assert!(!source.recovered(), "{side}");
    }
}

#[test]
fn a_damaged_source_png_is_rebuilt_from_the_vtf() {
    let env = library_env();
    for how in ["missing", "garbage", "wrong size"] {
        let art = drawing(64);
        let entry = save_new(&env, &format!("Big {how}"), &art);
        damage_source(&dir_of(&env, &entry), how);
        let source = env.lib.painted_source(entry.id()).unwrap();
        assert!(source.recovered(), "{how}");
        assert!(
            source.drawing() == &art,
            "{how}: a 64x64 drawing comes back exact"
        );
    }
    let small = drawing(32);
    let entry = save_new(&env, "Small", &small);
    damage_source(&dir_of(&env, &entry), "garbage");
    let source = env.lib.painted_source(entry.id()).unwrap();
    assert!(source.recovered());
    assert_eq!(
        (source.drawing().width(), source.drawing().height()),
        (64, 64)
    );
    assert!(source.drawing().pixels() == doubled(&small).as_slice());
}

#[test]
fn a_damaged_source_and_vtf_are_unreadable() {
    let env = library_env();
    let entry = save_new(&env, "Gone", &drawing(64));
    let dir = dir_of(&env, &entry);
    damage_source(&dir, "garbage");
    fs::write(dir.join(format!("{}.vtf", entry.folder_name())), b"garbage").unwrap();
    let err = env.lib.painted_source(entry.id()).unwrap_err();
    assert!(
        matches!(&err, PaintError::Unreadable { id } if id == entry.id().as_str()),
        "{err:?}"
    );
}

#[test]
fn only_a_painted_crosshair_has_a_painted_source() {
    let env = library_env();
    let png = import_png_as(&env, "Imported", Provenance::Local);
    let err = env.lib.painted_source(png.id()).unwrap_err();
    assert!(
        matches!(&err, PaintError::NotPainted { id } if id == png.id().as_str()),
        "{err:?}"
    );
    let err = env
        .lib
        .painted_source(&CrosshairId::new("no-such-id"))
        .unwrap_err();
    assert!(
        matches!(&err, PaintError::NotFound { id } if id == "no-such-id"),
        "{err:?}"
    );
}

fn resave(
    env: &LibraryEnv,
    id: &CrosshairId,
    name: &str,
    drawing: &RgbaImage,
) -> Result<CrosshairEntry, PaintError> {
    env.lib
        .save_painted_crosshair(&PaintTarget::Existing(id.clone()), name, drawing)
}

fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    tree(dir)
        .into_iter()
        .map(|f| {
            let bytes = fs::read(dir.join(&f)).unwrap();
            (f, bytes)
        })
        .collect()
}

#[test]
fn a_resave_rewrites_the_files_in_place_and_renames_the_row() {
    let env = library_env();
    let first = save_new(&env, "Dot", &drawing(64));
    let dir = dir_of(&env, &first);
    let art = drawing(32);
    let second = resave(&env, first.id(), " Ring ", &art).unwrap();
    assert_eq!(second.id(), first.id());
    assert_eq!(second.folder_name(), first.folder_name());
    assert_eq!(second.name(), "Ring");
    assert_eq!(second.created_at(), first.created_at());
    assert_eq!(second.format(), CrosshairFormat::Painted);
    let f = first.folder_name();
    assert_eq!(
        tree(&dir),
        [
            format!("{f}.vmt"),
            format!("{f}.vtf"),
            "preview.png".into(),
            "source.png".into()
        ]
    );
    let source = fs::read(dir.join("source.png")).unwrap();
    assert_eq!(png_pixels(&source), (32, 32, art.pixels().to_vec()));
    assert_eq!(fs::read(dir.join("preview.png")).unwrap(), source);
    assert!(decode_vtf(&vtf_of(&env, &second)).unwrap().pixels() == doubled(&art).as_slice());
    assert_eq!(row(&env, first.id()).0, "Ring");
    assert_eq!(env.lib.list_crosshairs().unwrap(), vec![second]);
}

#[test]
fn a_resave_may_keep_its_own_name_but_not_take_another() {
    let env = library_env();
    let dot = save_new(&env, "Dot", &drawing(64));
    import_png_as(&env, "Ring", Provenance::Local);
    let kept = resave(&env, dot.id(), "DOT", &drawing(64)).unwrap();
    assert_eq!(kept.name(), "DOT");
    let err = resave(&env, dot.id(), "ring", &drawing(64)).unwrap_err();
    assert!(
        matches!(&err, PaintError::NameTaken { name } if name == "ring"),
        "{err:?}"
    );
    let err = resave(&env, dot.id(), "  ", &drawing(64)).unwrap_err();
    assert!(matches!(err, PaintError::EmptyName), "{err:?}");
    assert_eq!(row(&env, dot.id()).0, "DOT");
}

#[test]
fn a_resave_of_an_imported_or_deleted_crosshair_is_refused() {
    let env = library_env();
    let png = import_png_as(&env, "Imported", Provenance::Local);
    let err = resave(&env, png.id(), "Imported 2", &drawing(64)).unwrap_err();
    assert!(
        matches!(&err, PaintError::NotPainted { id } if id == png.id().as_str()),
        "{err:?}"
    );
    let dot = save_new(&env, "Dot", &drawing(64));
    env.lib.delete_crosshair(dot.id()).unwrap();
    let err = resave(&env, dot.id(), "Dot", &drawing(64)).unwrap_err();
    assert!(
        matches!(&err, PaintError::NotFound { id } if id == dot.id().as_str()),
        "{err:?}"
    );
}

#[cfg(unix)]
#[test]
fn an_unwritable_folder_leaves_the_old_files_and_name() {
    use std::os::unix::fs::PermissionsExt;
    let env = library_env();
    let dot = save_new(&env, "Dot", &drawing(64));
    let dir = dir_of(&env, &dot);
    let before = snapshot(&dir);
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(dir.join("probe"), b"").is_ok() {
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        return; // running as root: permissions are not enforced
    }
    let result = resave(&env, dot.id(), "Ring", &drawing(32));
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    let err = result.unwrap_err();
    assert!(matches!(err, PaintError::Io { .. }), "{err:?}");
    assert!(snapshot(&dir) == before, "the old files changed");
    assert_eq!(row(&env, dot.id()).0, "Dot");
}

#[test]
fn a_failed_swap_puts_every_old_file_back() {
    let env = library_env();
    let dot = save_new(&env, "Dot", &drawing(64));
    let dir = dir_of(&env, &dot);
    // A non-empty directory where the VMT goes cannot be backed up, so the swap fails after
    // source.png and preview.png were already replaced.
    let vmt = dir.join(format!("{}.vmt", dot.folder_name()));
    fs::remove_file(&vmt).unwrap();
    fs::create_dir(&vmt).unwrap();
    fs::write(vmt.join("keep"), b"x").unwrap();
    let before = snapshot(&dir);
    let err = resave(&env, dot.id(), "Ring", &drawing(32)).unwrap_err();
    assert!(matches!(err, PaintError::Io { .. }), "{err:?}");
    assert!(
        snapshot(&dir) == before,
        "an old file was not restored, or a temporary or backup file was left"
    );
    assert_eq!(row(&env, dot.id()).0, "Dot");
}

#[test]
fn a_failed_rename_in_the_database_keeps_the_new_files_under_the_old_name() {
    let env = library_env();
    let dot = save_new(&env, "Dot", &drawing(64));
    sql(&env.dir)
        .execute_batch(
            "CREATE TRIGGER no_renames BEFORE UPDATE ON library_crosshairs
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    let art = drawing(32);
    let err = resave(&env, dot.id(), "Ring", &art).unwrap_err();
    assert!(matches!(err, PaintError::Database { .. }), "{err:?}");
    let dir = dir_of(&env, &dot);
    assert_eq!(
        png_pixels(&fs::read(dir.join("source.png")).unwrap()),
        (32, 32, art.pixels().to_vec())
    );
    assert!(decode_vtf(&vtf_of(&env, &dot)).unwrap().pixels() == doubled(&art).as_slice());
    assert!(
        !tree(&dir)
            .iter()
            .any(|f| f.ends_with(".tmp") || f.ends_with(".bak"))
    );
    assert_eq!(row(&env, dot.id()).0, "Dot");
}

// A row whose folder is not one plain name is never joined into a path, even when the
// path it would form reaches real files.
#[test]
fn a_row_whose_folder_is_not_a_plain_name_is_never_followed() {
    let env = library_env();
    let dot = save_new(&env, "Dot", &drawing(64));
    sql(&env.dir)
        .execute(
            "UPDATE library_crosshairs SET folder_name = ?1 WHERE id = ?2",
            [format!("{}/.", dot.folder_name()), dot.id().to_string()],
        )
        .unwrap();
    let err = env.lib.painted_source(dot.id()).unwrap_err();
    assert!(
        matches!(&err, PaintError::Unreadable { id } if id == dot.id().as_str()),
        "{err:?}"
    );
    let err = resave(&env, dot.id(), "Ring", &drawing(32)).unwrap_err();
    assert!(
        matches!(&err, PaintError::NotFound { id } if id == dot.id().as_str()),
        "{err:?}"
    );
}

/// Every alpha from 0 to 255 under changing colours, so a lost, rounded or premultiplied alpha
/// shows. Alpha 0 falls on texels whose colour is 0 too, since an encoder may zero it.
fn see_through(side: u32) -> RgbaImage {
    let pixels = (0..side * side)
        .flat_map(|i| {
            [
                (i * 7) as u8,
                (i * 11) as u8,
                (i * 13) as u8,
                (i % 256) as u8,
            ]
        })
        .collect();
    RgbaImage::from_pixels(side, side, pixels).unwrap()
}

#[test]
fn see_through_pixels_keep_their_alpha_through_save_reopen_and_the_vtf() {
    let env = library_env();
    for side in [32, 64] {
        let art = see_through(side);
        let entry = save_new(&env, &format!("Soft {side}"), &art);
        let source = env.lib.painted_source(entry.id()).unwrap();
        assert!(!source.recovered(), "{side}");
        assert!(
            source.drawing() == &art,
            "{side}: source.png keeps every alpha"
        );
        let want = if side == 64 {
            art.pixels().to_vec()
        } else {
            doubled(&art)
        };
        let vtf = decode_vtf(&vtf_of(&env, &entry)).unwrap();
        assert!(
            vtf.pixels() == want.as_slice(),
            "{side}: the VTF keeps every alpha"
        );
        damage_source(&dir_of(&env, &entry), "garbage");
        let rebuilt = env.lib.painted_source(entry.id()).unwrap();
        assert!(rebuilt.recovered(), "{side}");
        assert!(
            rebuilt.drawing().pixels() == want.as_slice(),
            "{side}: a drawing rebuilt from the VTF keeps every alpha"
        );
    }
}

#[test]
fn crosshair_drawing_gives_a_painted_crosshair_at_its_own_size() {
    let env = library_env();
    let mut pixels = vec![0u8; 32 * 32 * 4];
    pixels[4..8].copy_from_slice(&[255, 0, 0, 128]);
    let drawing = RgbaImage::from_pixels(32, 32, pixels).unwrap();
    let entry = env
        .lib
        .save_painted_crosshair(&PaintTarget::New, "Small", &drawing)
        .unwrap();
    let got = env
        .lib
        .crosshair_drawing(
            &CrosshairRef::Library(entry.id().clone()),
            &builtin_catalog(),
        )
        .unwrap();
    assert_eq!(got, drawing);
}

#[test]
fn crosshair_drawing_decodes_builtins_and_imports_from_their_vtf() {
    let env = library_env();
    let builtins = builtin_catalog();
    let cpma = CrosshairRef::Builtin("cpma_9".to_owned());
    let got = env.lib.crosshair_drawing(&cpma, &builtins).unwrap();
    assert_eq!((got.width(), got.height()), (32, 32));
    assert_eq!(got, env.lib.crosshair_preview(&cpma, &builtins).unwrap());
    let green = import_png_as(&env, "Green", Provenance::Local);
    let green = CrosshairRef::Library(green.id().clone());
    assert_eq!(
        env.lib.crosshair_drawing(&green, &builtins).unwrap(),
        env.lib.crosshair_preview(&green, &builtins).unwrap()
    );
}

#[test]
fn crosshair_drawing_of_an_unknown_crosshair_is_not_found() {
    let env = library_env();
    let builtins = builtin_catalog();
    for missing in [
        CrosshairRef::Builtin("no-such-builtin".to_owned()),
        CrosshairRef::Library(CrosshairId::new("no-such-id")),
    ] {
        let got = env.lib.crosshair_drawing(&missing, &builtins);
        assert!(matches!(got, Err(LibraryError::NotFound { .. })), "{got:?}");
    }
}
