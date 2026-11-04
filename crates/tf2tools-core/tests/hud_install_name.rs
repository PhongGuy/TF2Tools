//! The install name: referenced folder, curated repo, a copy's recorded folder, library folder.

mod common;

use std::collections::BTreeMap;

use common::{LibraryEnv, library_env, write_file, zip_bytes};
use tf2tools_core::{CopyBase, HudEntry, LibraryKind, MakerBase, MakerFile, Provenance};

fn import(env: &LibraryEnv, root: &str, files: &[(&str, &str)]) -> HudEntry {
    let mut named = vec![(format!("{root}/info.vdf"), b"\"hud\" {}".to_vec())];
    named.extend(
        files
            .iter()
            .map(|(p, t)| (format!("{root}/{p}"), t.as_bytes().to_vec())),
    );
    let refs: Vec<(&str, &[u8])> = named
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let archive = write_file(
        &env.tmp.path().join(format!("in/{root}.zip")),
        &zip_bytes(&refs),
    );
    env.lib.import_hud(&archive, Provenance::Local).unwrap()
}

fn record(env: &LibraryEnv, hud: &HudEntry, maker: &[u8]) {
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(hud.folder_name());
    write_file(&dir.join(".tf2tools/maker.json"), maker);
}

fn copy_json(folder: &str) -> Vec<u8> {
    let base = CopyBase::new("h", "rayshud").with_install_folder(Some(folder));
    MakerFile::new(BTreeMap::new(), Vec::new())
        .with_base(MakerBase::Copy(base))
        .to_json()
}

#[test]
fn a_copy_without_references_installs_under_its_recorded_folder() {
    let env = library_env();
    let hud = import(&env, "my-copy-1a2b3c4d", &[]);
    record(&env, &hud, &copy_json("rayshud"));
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "rayshud"
    );
}

#[test]
fn a_referenced_folder_beats_the_recorded_one() {
    let env = library_env();
    let hud = import(
        &env,
        "my-copy-1a2b3c4d",
        &[("cfg/menu.cfg", "echo #base ../../custom/m0rehud/a.res")],
    );
    record(&env, &hud, &copy_json("rayshud"));
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "m0rehud"
    );
}

#[test]
fn an_unsafe_or_damaged_record_falls_back_to_the_library_folder() {
    let env = library_env();
    let hud = import(&env, "my-copy-1a2b3c4d", &[]);
    record(
        &env,
        &hud,
        br#"{"version":1,"base":{"kind":"copy","hud_id":"h","name":"x","install_folder":"con"}}"#,
    );
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "my-copy-1a2b3c4d"
    );
    record(&env, &hud, b"not json");
    assert_eq!(
        env.lib.install_hud(hud.id(), &env.tf2).unwrap().folder(),
        "my-copy-1a2b3c4d"
    );
}

#[test]
fn a_players_folder_holding_the_recorded_name_falls_back() {
    let env = library_env();
    let hud = import(&env, "my-copy-1a2b3c4d", &[]);
    record(&env, &hud, &copy_json("rayshud"));
    std::fs::create_dir_all(env.tf2.custom_dir().join("rayshud/my-stuff")).unwrap();
    let done = env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        (done.folder(), done.blocked_name()),
        ("my-copy-1a2b3c4d", Some("rayshud"))
    );
}
