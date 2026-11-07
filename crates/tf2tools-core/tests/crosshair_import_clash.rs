//! A crosshair imported under a name the library already uses: both are kept, and the
//! newcomer takes the first free numbered name.

use crate::common;

use std::fs;
use std::io::Cursor;

use common::{LibraryEnv, library_env, write_file};
use tf2tools_core::{CrosshairEntry, GbId, LibraryKind, Provenance};

fn png(colour: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(32, 32, image::Rgba(colour));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn import(env: &LibraryEnv, name: &str, colour: [u8; 4], p: Provenance) -> CrosshairEntry {
    let src = write_file(&env.tmp.path().join(format!("in/{name}.png")), &png(colour));
    env.lib.import_crosshair(&src, name, p).unwrap()
}

fn preview(env: &LibraryEnv, entry: &CrosshairEntry) -> Vec<u8> {
    let dir = env.dir.library_dir(LibraryKind::Crosshairs);
    fs::read(dir.join(entry.folder_name()).join("preview.png")).unwrap()
}

/// Each library crosshair's name, sorted.
fn names(env: &LibraryEnv) -> Vec<String> {
    let mut names: Vec<_> = env
        .lib
        .list_crosshairs()
        .unwrap()
        .iter()
        .map(|c| c.name().to_owned())
        .collect();
    names.sort();
    names
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

#[test]
fn repeated_imports_of_one_name_take_numbered_names() {
    let env = library_env();
    let first = import(&env, "wide", RED, Provenance::Local);
    assert_eq!(first.wanted_name(), None);
    let second = import(&env, "wide", BLUE, Provenance::Local);
    let third = import(&env, "wide", GREEN, Provenance::Local);

    assert_eq!(second.name(), "wide (2)");
    assert_eq!(second.wanted_name(), Some("wide"));
    assert_eq!(third.name(), "wide (3)");
    assert_eq!(third.wanted_name(), Some("wide"));
    assert_eq!(names(&env), ["wide", "wide (2)", "wide (3)"]);

    // Each keeps its own safe folder, and the first is untouched.
    let folders = [&first, &second, &third].map(|c| c.folder_name().to_owned());
    assert!(
        folders
            .iter()
            .all(|f| !f.contains(['/', '\\', ' ', '(', ')']))
    );
    assert_ne!(folders[0], folders[1]);
    assert_ne!(folders[1], folders[2]);
    assert_eq!(preview(&env, &first), png(RED));
    assert_eq!(preview(&env, &second), png(BLUE));
    let listed = env.lib.list_crosshairs().unwrap();
    let kept = listed.iter().find(|c| c.id() == first.id()).unwrap();
    assert_eq!(kept.name(), "wide");
    assert_eq!(kept.folder_name(), first.folder_name());
}

#[test]
fn a_name_taken_in_another_case_counts_as_taken() {
    let env = library_env();
    import(&env, "Wide", RED, Provenance::Local);
    let second = import(&env, "wide", BLUE, Provenance::Local);
    assert_eq!(second.name(), "wide (2)");
}

#[test]
fn a_freed_number_is_taken_again_first() {
    let env = library_env();
    import(&env, "wide", RED, Provenance::Local);
    let second = import(&env, "wide", BLUE, Provenance::Local);
    import(&env, "wide", GREEN, Provenance::Local);
    env.lib.delete_crosshair(second.id()).unwrap();

    let again = import(&env, "wide", BLUE, Provenance::Local);
    assert_eq!(again.name(), "wide (2)");
}

#[test]
fn a_store_download_of_a_taken_name_is_numbered_too() {
    let env = library_env();
    let from_gb = || Provenance::GameBanana {
        gb_id: GbId::new(42),
        author: String::new(),
    };
    import(&env, "Dot", RED, from_gb());
    let again = import(&env, "Dot", RED, from_gb());

    assert_eq!(again.name(), "Dot (2)");
    assert_eq!(again.provenance(), &from_gb());
    assert_eq!(names(&env), ["Dot", "Dot (2)"]);
}
