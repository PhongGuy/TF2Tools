//! Crosshair library: PNG and VTF import, previews, and per-weapon assignments.

mod common;

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use common::{LibraryEnv, library_env, set_setting, setting, sql, tree, write_file};
use tf2tools_core::{
    BuiltinCrosshairs, CrosshairEntry, CrosshairFormat, CrosshairId, CrosshairRef, GbId,
    ImportError, LibraryError, LibraryKind, Provenance, VtfError, decode_vtf,
};

fn bundled() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/crosshairs")
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn import_png(env: &LibraryEnv, name: &str) -> CrosshairEntry {
    let src = write_file(&env.tmp.path().join(format!("in/{name}.png")), &png(32, 32));
    env.lib
        .import_crosshair(&src, name, Provenance::Local)
        .unwrap()
}

fn folder(env: &LibraryEnv, entry: &CrosshairEntry) -> PathBuf {
    env.dir
        .library_dir(LibraryKind::Crosshairs)
        .join(entry.folder_name())
}

fn crosshairs_dir_is_empty(env: &LibraryEnv) -> bool {
    fs::read_dir(env.dir.library_dir(LibraryKind::Crosshairs))
        .unwrap()
        .count()
        == 0
}

fn pairs(env: &LibraryEnv) -> Vec<(String, CrosshairRef)> {
    env.lib
        .assignments()
        .unwrap()
        .iter()
        .map(|a| (a.weapon_script().to_owned(), a.crosshair().clone()))
        .collect()
}

#[test]
fn a_png_becomes_a_vtf_a_vmt_and_a_preview() {
    let env = library_env();
    let src = write_file(&env.tmp.path().join("in/My Cross.png"), &png(32, 32));
    let entry = env
        .lib
        .import_crosshair(&src, "My Cross", Provenance::Local)
        .unwrap();
    let id8 = &entry.id().as_str()[..8];
    let name = format!("my-cross-{id8}");
    assert_eq!(entry.folder_name(), name);
    assert_eq!(entry.format(), CrosshairFormat::Png);
    let dir = folder(&env, &entry);
    assert_eq!(
        tree(&dir),
        [
            format!("{name}.vmt"),
            format!("{name}.vtf"),
            "preview.png".into()
        ]
    );
    let vmt = fs::read_to_string(dir.join(format!("{name}.vmt"))).unwrap();
    assert!(
        vmt.contains(&format!("\"vgui/replay/thumbnails/{name}\"")),
        "{vmt}"
    );
    let decoded = decode_vtf(&fs::read(dir.join(format!("{name}.vtf"))).unwrap()).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (64, 64));
    assert_eq!(
        fs::read(dir.join("preview.png")).unwrap(),
        fs::read(&src).unwrap()
    );
    let row: (String, String, Option<i64>) = sql(&env.dir)
        .query_row(
            "SELECT source_type, source, store_gb_id FROM library_crosshairs WHERE id = ?1",
            [entry.id().as_str()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(row, ("png".into(), "local".into(), None));
    assert_eq!(env.lib.list_crosshairs().unwrap(), vec![entry]);
}

#[test]
fn a_vtf_is_copied_with_a_decoded_preview() {
    let env = library_env();
    let original = fs::read(bundled().join("vtf/Brackets_TobYZ.vtf")).unwrap();
    let src = write_file(&env.tmp.path().join("in/b.vtf"), &original);
    let gb = Provenance::GameBanana {
        gb_id: GbId::new(7),
        author: "a".into(),
    };
    let entry = env.lib.import_crosshair(&src, "Brackets", gb).unwrap();
    assert_eq!(entry.format(), CrosshairFormat::Vtf);
    assert_eq!(
        entry.provenance(),
        &Provenance::GameBanana {
            gb_id: GbId::new(7),
            author: String::new()
        }
    );
    let dir = folder(&env, &entry);
    let copied = fs::read(dir.join(format!("{}.vtf", entry.folder_name()))).unwrap();
    assert_eq!(copied, original);
    let expected = decode_vtf(&original).unwrap();
    let preview = image::load_from_memory(&fs::read(dir.join("preview.png")).unwrap()).unwrap();
    assert_eq!(
        (preview.width(), preview.height()),
        (expected.width(), expected.height())
    );
    let row: (String, String, Option<i64>) = sql(&env.dir)
        .query_row(
            "SELECT source_type, source, store_gb_id FROM library_crosshairs WHERE id = ?1",
            [entry.id().as_str()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(row, ("vtf".into(), "store".into(), Some(7)));
}

#[test]
fn an_undecodable_vtf_still_imports_without_a_preview() {
    let env = library_env();
    let src = write_file(&env.tmp.path().join("in/bad.vtf"), b"garbage");
    let entry = env
        .lib
        .import_crosshair(&src, "Bad", Provenance::Local)
        .unwrap();
    let name = entry.folder_name().to_owned();
    assert_eq!(
        tree(&folder(&env, &entry)),
        [format!("{name}.vmt"), format!("{name}.vtf")]
    );
}

#[test]
fn a_png_that_does_not_decode_is_refused() {
    let env = library_env();
    let src = write_file(&env.tmp.path().join("in/bad.png"), b"nope");
    let err = env
        .lib
        .import_crosshair(&src, "Bad", Provenance::Local)
        .unwrap_err();
    assert!(
        matches!(err, ImportError::Vtf(VtfError::InvalidPng { .. })),
        "{err:?}"
    );
    assert!(crosshairs_dir_is_empty(&env));
    assert!(env.lib.list_crosshairs().unwrap().is_empty());
}

#[test]
fn other_extensions_and_curated_provenance_are_refused() {
    let env = library_env();
    let jpg = write_file(&env.tmp.path().join("in/x.jpg"), b"jpg");
    let err = env
        .lib
        .import_crosshair(&jpg, "X", Provenance::Local)
        .unwrap_err();
    assert!(
        matches!(err, ImportError::UnsupportedFile { .. }),
        "{err:?}"
    );
    let ok = write_file(&env.tmp.path().join("in/ok.png"), &png(8, 8));
    let curated = Provenance::Curated { hud_id: "h".into() };
    let err = env.lib.import_crosshair(&ok, "Ok", curated).unwrap_err();
    assert!(matches!(err, ImportError::UnsupportedProvenance), "{err:?}");
    assert!(crosshairs_dir_is_empty(&env));
}

#[test]
fn assign_upserts_and_clears_the_active_profile() {
    let env = library_env();
    set_setting(&env.dir, "active_profile_id", "p1");
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Builtin("cpma_9".into()),
        )
        .unwrap();
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));

    let dot = import_png(&env, "Dot");
    let library_ref = CrosshairRef::Library(dot.id().clone());
    env.lib
        .assign("tf_weapon_scattergun", &library_ref)
        .unwrap();
    env.lib
        .assign(
            "tf_weapon_rocketlauncher",
            &CrosshairRef::Builtin("ELAdot".into()),
        )
        .unwrap();
    assert_eq!(
        pairs(&env),
        [
            (
                "tf_weapon_rocketlauncher".to_owned(),
                CrosshairRef::Builtin("ELAdot".into())
            ),
            ("tf_weapon_scattergun".to_owned(), library_ref),
        ]
    );
    let raw: (String, String) = sql(&env.dir)
        .query_row(
            "SELECT crosshair_source, crosshair_ref FROM crosshair_assignments
             WHERE weapon_script = 'tf_weapon_scattergun'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(raw, ("library".into(), dot.folder_name().to_owned()));
}

#[test]
fn invalid_weapon_scripts_and_unknown_library_crosshairs_are_refused() {
    let env = library_env();
    let builtin = CrosshairRef::Builtin("cpma_9".into());
    for bad in [
        "../evil",
        "tf_weapon_a/b",
        "scattergun",
        "tf_weapon_",
        "TF_WEAPON_X",
    ] {
        let err = env.lib.assign(bad, &builtin).unwrap_err();
        assert!(
            matches!(err, LibraryError::InvalidWeaponScript { .. }),
            "{bad}: {err:?}"
        );
    }
    let missing = CrosshairRef::Library(CrosshairId::new("nope"));
    let err = env
        .lib
        .assign("tf_weapon_scattergun", &missing)
        .unwrap_err();
    assert!(matches!(err, LibraryError::NotFound { .. }), "{err:?}");
    assert!(pairs(&env).is_empty());
}

#[test]
fn assign_many_writes_every_weapon_or_none() {
    let env = library_env();
    let eladot = CrosshairRef::Builtin("ELAdot".into());
    let cpma = CrosshairRef::Builtin("cpma_9".into());
    env.lib.assign("tf_weapon_bat", &eladot).unwrap();
    set_setting(&env.dir, "active_profile_id", "p1");
    // The second row of the batch fails, as a disk that fills mid-write would.
    sql(&env.dir)
        .execute_batch(
            "CREATE TRIGGER fail_pistol BEFORE INSERT ON crosshair_assignments
             WHEN NEW.weapon_script = 'tf_weapon_pistol_scout'
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    let batch = [
        "tf_weapon_scattergun",
        "tf_weapon_pistol_scout",
        "tf_weapon_bat",
    ];
    let err = env.lib.assign_many(&batch, &cpma).unwrap_err();
    assert!(matches!(err, LibraryError::Database { .. }), "{err:?}");
    assert_eq!(pairs(&env), [("tf_weapon_bat".to_owned(), eladot)]);
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some("p1")
    );

    sql(&env.dir)
        .execute_batch("DROP TRIGGER fail_pistol;")
        .unwrap();
    env.lib.assign_many(&batch, &cpma).unwrap();
    assert_eq!(
        pairs(&env),
        [
            ("tf_weapon_bat".to_owned(), cpma.clone()),
            ("tf_weapon_pistol_scout".to_owned(), cpma.clone()),
            ("tf_weapon_scattergun".to_owned(), cpma),
        ]
    );
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
}

#[test]
fn assign_many_refuses_a_bad_batch_before_writing() {
    let env = library_env();
    let cpma = CrosshairRef::Builtin("cpma_9".into());
    set_setting(&env.dir, "active_profile_id", "p1");
    let err = env
        .lib
        .assign_many(&["tf_weapon_scattergun", "../evil", "tf_weapon_bat"], &cpma)
        .unwrap_err();
    assert!(
        matches!(err, LibraryError::InvalidWeaponScript { ref name, .. } if name == "../evil"),
        "{err:?}"
    );
    let missing = CrosshairRef::Library(CrosshairId::new("nope"));
    let err = env
        .lib
        .assign_many(&["tf_weapon_scattergun", "tf_weapon_bat"], &missing)
        .unwrap_err();
    assert!(matches!(err, LibraryError::NotFound { .. }), "{err:?}");
    env.lib.assign_many(&[], &cpma).unwrap();
    assert!(pairs(&env).is_empty());
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some("p1")
    );
}

#[test]
fn deleting_a_crosshair_removes_its_files_row_and_assignments() {
    let env = library_env();
    let dot = import_png(&env, "Dot");
    env.lib
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Library(dot.id().clone()),
        )
        .unwrap();
    env.lib
        .assign(
            "tf_weapon_rocketlauncher",
            &CrosshairRef::Builtin("x".into()),
        )
        .unwrap();
    env.lib.delete_crosshair(dot.id()).unwrap();
    assert_eq!(
        pairs(&env),
        [(
            "tf_weapon_rocketlauncher".to_owned(),
            CrosshairRef::Builtin("x".into())
        )]
    );
    assert!(env.lib.list_crosshairs().unwrap().is_empty());
    assert!(crosshairs_dir_is_empty(&env));
    let err = env.lib.delete_crosshair(dot.id()).unwrap_err();
    assert!(matches!(err, LibraryError::NotFound { .. }), "{err:?}");
}

#[test]
fn preview_decodes_builtin_and_library_vtfs() {
    let env = library_env();
    let catalog = BuiltinCrosshairs::load(&bundled()).unwrap();
    let builtin = env
        .lib
        .crosshair_preview(&CrosshairRef::Builtin("cpma_9".into()), &catalog)
        .unwrap();
    assert_eq!((builtin.width(), builtin.height()), (32, 32));

    let dot = import_png(&env, "Dot");
    let library = env
        .lib
        .crosshair_preview(&CrosshairRef::Library(dot.id().clone()), &catalog)
        .unwrap();
    assert_eq!((library.width(), library.height()), (64, 64));

    let err = env
        .lib
        .crosshair_preview(&CrosshairRef::Builtin("Dot_Tob".into()), &catalog)
        .unwrap_err();
    assert!(matches!(err, LibraryError::NotFound { .. }), "{err:?}");
}
