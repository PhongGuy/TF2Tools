//! The licence check: every file under `licenses/` ships inside the app, and every bundled
//! third-party asset folder is covered by one of them or explicitly set aside.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use tf2tools_app::notices;

/// Who a bundled asset folder belongs to, and so which notice it needs.
enum Owner {
    /// Third-party work shipped under the named file in `licenses/`.
    Notice(&'static str),
    /// Made for TF2Tools, under the app's own licence.
    Own,
    /// Taken or derived from Team Fortress 2. Whether these ship at all is still undecided, so
    /// they get no notice here.
    ValveDerived,
}

/// Every folder the app bundles: the packaged `assets/` resources and the files compiled into
/// the binary from the app crate's `assets/` and the core crate's `data/`. A new folder fails the check until it is listed.
const BUNDLED: &[(&str, Owner)] = &[
    ("assets/backgrounds", Owner::ValveDerived),
    ("assets/classes", Owner::ValveDerived),
    ("assets/crosshairs", Owner::ValveDerived),
    ("assets/weapon-icons", Owner::ValveDerived),
    (
        "crates/tf2tools-app/assets/fonts",
        Owner::Notice("Inter-OFL.txt"),
    ),
    ("crates/tf2tools-app/assets/theme", Owner::Own),
    (
        "crates/tf2tools-core/data/hud-schemas",
        Owner::Notice("HUD-option-schemas-MIT.txt"),
    ),
];

/// Third-party work compiled in from a dependency rather than a folder of ours.
const COMPILED_IN: &[(&str, &str)] = &[("Lucide icons via gpui-kit", "Lucide-ISC.txt")];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn licence_files() -> BTreeSet<String> {
    std::fs::read_dir(repo().join("licenses"))
        .expect("read licenses/")
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().unwrap().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn every_licence_file_ships_inside_the_app_word_for_word() {
    let shipped = notices::all();
    let names: BTreeSet<String> = shipped.iter().map(|n| n.file().to_owned()).collect();
    assert_eq!(
        names,
        licence_files(),
        "licenses/ and the app's notices differ"
    );
    for notice in &shipped {
        let on_disk = std::fs::read_to_string(repo().join("licenses").join(notice.file())).unwrap();
        assert_eq!(notice.text(), on_disk, "{} is stale", notice.file());
        assert!(!notice.title().is_empty());
    }
}

#[test]
fn every_bundled_folder_has_a_notice_or_is_set_aside() {
    let files = licence_files();
    let mut found = BTreeSet::new();
    for root in [
        "assets",
        "crates/tf2tools-app/assets",
        "crates/tf2tools-core/data",
    ] {
        for entry in std::fs::read_dir(repo().join(root)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                found.insert(format!("{root}/{}", entry.file_name().to_string_lossy()));
            }
        }
    }
    let unlisted: Vec<&String> = found
        .iter()
        .filter(|dir| !BUNDLED.iter().any(|(listed, _)| listed == dir))
        .collect();
    assert!(
        unlisted.is_empty(),
        "bundled folders without a notice (add the licence to licenses/ and list them in BUNDLED): {unlisted:?}"
    );
    let gone: Vec<&str> = BUNDLED
        .iter()
        .map(|(dir, _)| *dir)
        .filter(|dir| !found.contains(*dir))
        .collect();
    assert!(
        gone.is_empty(),
        "BUNDLED lists folders that no longer exist: {gone:?}"
    );
    let needed = BUNDLED
        .iter()
        .filter_map(|(dir, owner)| match owner {
            Owner::Notice(file) => Some((*dir, *file)),
            Owner::Own | Owner::ValveDerived => None,
        })
        .chain(COMPILED_IN.iter().copied());
    for (what, file) in needed {
        assert!(files.contains(file), "{what} needs licenses/{file}");
    }
}

#[test]
fn known_notices_get_plain_titles() {
    assert_eq!(notices::title_for("Inter-OFL.txt"), "Inter font");
    assert_eq!(notices::title_for("Lucide-ISC.txt"), "Lucide icons");
    // The slot Stage 9 fills when it adds the HUD option schemas and their licence.
    assert_eq!(
        notices::title_for("HUD-option-schemas-MIT.txt"),
        "HUD option schemas (MIT)"
    );
    assert_eq!(
        notices::title_for("mastercomfig-MIT.txt"),
        "mastercomfig (MIT)"
    );
    assert_eq!(
        notices::title_for("hud-db-MIT.txt"),
        "hud-db HUD list (MIT)"
    );
    assert_eq!(notices::title_for("GameBanana-notice.txt"), "GameBanana");
    assert_eq!(notices::title_for("serveme-notice.txt"), "serveme.tf");
    assert_eq!(notices::title_for("logstf-notice.txt"), "logs.tf");
    assert_eq!(
        notices::title_for("Valve-notice.txt"),
        "Valve and Team Fortress 2"
    );
    assert_eq!(
        notices::title_for("rars-Apache-2.0.txt"),
        "rars RAR reader (Apache-2.0)"
    );
    assert_eq!(
        notices::title_for("crosshairs-notice.txt"),
        "Built-in crosshairs"
    );
    assert_eq!(
        notices::title_for("comfig-app-MIT.txt"),
        "mastercomfig app preset text (MIT)"
    );
    // A notice nobody titled still shows, named after its file.
    assert_eq!(notices::title_for("Some-Thing-MIT.txt"), "Some Thing MIT");
}

/// Every outside source the app takes content from, its notice file, and the line that notice
/// must carry: the upstream copyright line where there is a licence, else the source's name.
const SOURCES: &[(&str, &str, &str)] = &[
    (
        "mastercomfig addons",
        "mastercomfig-MIT.txt",
        "Copyright (c) 2016-2026 mastercoms",
    ),
    (
        "hud-db HUD list",
        "hud-db-MIT.txt",
        "Copyright (c) 2024-2026 mastercomfig",
    ),
    ("GameBanana stores", "GameBanana-notice.txt", "GameBanana"),
    ("Maps page list", "serveme-notice.txt", "serveme.tf"),
    ("logs.tf page", "logstf-notice.txt", "logs.tf"),
    ("game artwork", "Valve-notice.txt", "Valve Corporation"),
    ("built-in crosshairs", "crosshairs-notice.txt", "Kenney"),
    (
        "Graphics preset text",
        "comfig-app-MIT.txt",
        "Copyright (c) 2016-2026 mastercoms",
    ),
];

#[test]
fn every_outside_source_has_a_notice_that_disclaims_affiliation() {
    let shipped = notices::all();
    for (what, file, line) in SOURCES {
        let notice = shipped
            .iter()
            .find(|n| n.file() == *file)
            .unwrap_or_else(|| panic!("{what} needs licenses/{file}"));
        let flat = notice
            .text()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(flat.contains(line), "{file} lacks {line:?}");
        assert!(
            flat.contains(
                "TF2Tools is not made, sponsored, endorsed or licensed by, or affiliated with"
            ),
            "{file} lacks the not-affiliated statement"
        );
    }
}
