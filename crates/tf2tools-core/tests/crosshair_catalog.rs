//! The builtin crosshair catalog: manifest entries that have a bundled VTF.

use std::fs;
use std::path::{Path, PathBuf};
use tf2tools_core::{BuiltinCrosshairs, CatalogError};

fn bundled() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/crosshairs")
}

#[test]
fn the_bundled_catalog_is_the_manifest_and_vtf_intersection() {
    let dir = bundled();
    let catalog = BuiltinCrosshairs::load(&dir).unwrap();
    assert_eq!(catalog.len(), 405);
    assert!(!catalog.is_empty());
    assert_eq!(catalog.missing().len(), 17);
    assert!(catalog.missing().iter().any(|m| m == "Dot_Tob"));
    assert_eq!(catalog.all()[0].name(), "default seeker outline");
    assert_eq!(catalog.all()[404].name(), "yzwings");

    let brackets = catalog.get("Brackets_TobYZ").unwrap();
    assert_eq!(brackets.display_name(), "Brackets TobYZ");
    assert_eq!(
        brackets.vtf_path(),
        dir.join("vtf").join("Brackets_TobYZ.vtf")
    );

    let cpma = catalog.get("cpma_9").unwrap();
    assert_eq!(
        cpma.vmt_path(),
        Some(dir.join("vmt").join("cpma_9.vmt").as_path())
    );
    assert_eq!(catalog.get("Crossdot").unwrap().vmt_path(), None);
    let without_vmt: Vec<&str> = catalog
        .all()
        .iter()
        .filter(|c| c.vmt_path().is_none())
        .map(|c| c.name())
        .collect();
    assert_eq!(without_vmt, ["Crossdot", "cross_NO", "crosshair1811"]);

    assert!(catalog.get("crossdot").is_none(), "names match exactly");
    assert!(catalog.get("Dot_Tob").is_none());
}

fn synthetic(manifest: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("vtf")).unwrap();
    fs::create_dir_all(tmp.path().join("vmt")).unwrap();
    fs::write(tmp.path().join("crosshairs.json"), manifest).unwrap();
    tmp
}

#[test]
fn a_synthetic_catalog_skips_and_reports_missing_vtfs() {
    let tmp = synthetic(
        r#"[{"filename":"a","display_name":"A"},{"filename":"b","display_name":"B"},{"filename":"c","display_name":"C"}]"#,
    );
    for file in ["vtf/a.vtf", "vtf/c.vtf", "vtf/extra.vtf", "vmt/a.vmt"] {
        fs::write(tmp.path().join(file), b"x").unwrap();
    }
    let catalog = BuiltinCrosshairs::load(tmp.path()).unwrap();
    let names: Vec<&str> = catalog.all().iter().map(|c| c.name()).collect();
    assert_eq!(names, ["a", "c"]);
    assert_eq!(catalog.missing(), ["b".to_string()]);
    assert!(catalog.get("a").unwrap().vmt_path().is_some());
    assert!(catalog.get("c").unwrap().vmt_path().is_none());
    assert_eq!(catalog.get("c").unwrap().display_name(), "C");
}

#[test]
fn a_missing_manifest_or_vtf_dir_is_an_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    let err = BuiltinCrosshairs::load(tmp.path()).unwrap_err();
    assert!(matches!(err, CatalogError::Io { .. }), "{err:?}");

    let tmp = synthetic("[]");
    fs::remove_dir(tmp.path().join("vtf")).unwrap();
    let err = BuiltinCrosshairs::load(tmp.path()).unwrap_err();
    assert!(matches!(err, CatalogError::Io { .. }), "{err:?}");
}

#[test]
fn a_malformed_manifest_is_reported() {
    for manifest in [
        "not json",
        r#"[{"filename":"a"}]"#,
        r#"{"filename":"a","display_name":"A"}"#,
    ] {
        let tmp = synthetic(manifest);
        let err = BuiltinCrosshairs::load(tmp.path()).unwrap_err();
        assert!(
            matches!(err, CatalogError::Manifest { ref path, .. } if path.ends_with("crosshairs.json")),
            "{manifest}: {err:?}"
        );
    }
}

#[test]
fn an_empty_manifest_is_an_empty_catalog() {
    let tmp = synthetic("[]");
    let catalog = BuiltinCrosshairs::load(tmp.path()).unwrap();
    assert!(catalog.is_empty());
    assert!(catalog.missing().is_empty());
}

#[test]
fn every_bundled_builtin_has_its_preview_png() {
    let dir = bundled();
    let catalog = BuiltinCrosshairs::load(&dir).unwrap();
    for c in catalog.all() {
        let png = dir.join("png").join(format!("{}.png", c.name()));
        assert_eq!(c.preview_png_path(), Some(png.as_path()), "{}", c.name());
    }
}

#[test]
fn a_builtin_without_a_png_has_no_preview_path() {
    let tmp =
        synthetic(r#"[{"filename":"a","display_name":"A"},{"filename":"b","display_name":"B"}]"#);
    fs::create_dir_all(tmp.path().join("png")).unwrap();
    for file in ["vtf/a.vtf", "vtf/b.vtf", "png/a.png", "png/B.png"] {
        fs::write(tmp.path().join(file), b"x").unwrap();
    }
    let catalog = BuiltinCrosshairs::load(tmp.path()).unwrap();
    assert_eq!(
        catalog.get("a").unwrap().preview_png_path(),
        Some(tmp.path().join("png").join("a.png").as_path())
    );
    assert_eq!(
        catalog.get("b").unwrap().preview_png_path(),
        None,
        "names match exactly"
    );
}

#[test]
fn a_catalog_without_a_png_dir_still_loads() {
    let tmp = synthetic(r#"[{"filename":"a","display_name":"A"}]"#);
    fs::write(tmp.path().join("vtf/a.vtf"), b"x").unwrap();
    let catalog = BuiltinCrosshairs::load(tmp.path()).unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog.get("a").unwrap().preview_png_path(), None);
}

/// Bundled files the catalog can never serve, as `<sub>/<file name>`, sorted: a wrong
/// extension, a stem no manifest entry names, or a VMT whose VTF is not in `vtf/`.
fn unreachable_files(dir: &Path) -> Vec<String> {
    let manifest: Vec<serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(dir.join("crosshairs.json")).unwrap()).unwrap();
    let names: std::collections::HashSet<&str> = manifest
        .iter()
        .map(|entry| entry["filename"].as_str().unwrap())
        .collect();
    let listing = |sub: &str| -> Vec<String> {
        fs::read_dir(dir.join(sub))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    };
    let vtfs: std::collections::HashSet<String> = listing("vtf").into_iter().collect();
    let mut stray = Vec::new();
    for sub in ["vtf", "vmt"] {
        for file in listing(sub) {
            let path = Path::new(&file);
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let reachable = path.extension().and_then(|e| e.to_str()) == Some(sub)
                && names.contains(stem)
                && vtfs.contains(&format!("{stem}.vtf"));
            if !reachable {
                stray.push(format!("{sub}/{file}"));
            }
        }
    }
    stray.sort();
    stray
}

#[test]
fn every_bundled_vtf_and_vmt_belongs_to_a_manifest_entry() {
    let stray = unreachable_files(&bundled());
    assert!(stray.is_empty(), "unreachable crosshair files: {stray:?}");
}

#[test]
fn a_vmt_without_its_vtf_is_unreachable() {
    let tmp =
        synthetic(r#"[{"filename":"a","display_name":"A"},{"filename":"b","display_name":"B"}]"#);
    for file in [
        "vtf/a.vtf",
        "vtf/B.vtf",
        "vmt/a.vmt",
        "vmt/b.vmt",
        "vmt/c.vmt",
    ] {
        fs::write(tmp.path().join(file), b"x").unwrap();
    }
    assert_eq!(
        unreachable_files(tmp.path()),
        ["vmt/b.vmt", "vmt/c.vmt", "vtf/B.vtf"]
    );
}
