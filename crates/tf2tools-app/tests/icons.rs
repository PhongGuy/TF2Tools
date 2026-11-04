use std::collections::BTreeSet;
use std::path::Path;

use gpui_kit::AssetSource as _;
use gpui_kit::assets::IconName;
use tf2tools_app::assets::AppAssets;

// Collects every `IconName::<Name>` token written in the Rust files under `dir`.
fn names_in(dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            names_in(&path, out);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (at, prefix) in text.match_indices("IconName::") {
            let name: String = text[at + prefix.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            out.insert(name);
        }
    }
}

fn variant(name: &str) -> Option<IconName> {
    IconName::ALL
        .iter()
        .copied()
        .find(|icon| format!("{icon:?}") == name)
}

#[test]
fn every_icon_the_app_names_is_bundled() {
    let mut names = BTreeSet::new();
    names_in(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut names,
    );
    let unknown: Vec<&String> = names.iter().filter(|n| variant(n).is_none()).collect();
    assert!(unknown.is_empty(), "not IconName variants: {unknown:?}");
    assert!(names.len() >= 30, "the scan found only {names:?}");
    let missing: Vec<String> = names
        .iter()
        .filter_map(|n| variant(n))
        .map(|icon| icon.path().to_string())
        .filter(|path| AppAssets.load(path).unwrap().is_none())
        .collect();
    assert!(missing.is_empty(), "not bundled: {missing:?}");
}
