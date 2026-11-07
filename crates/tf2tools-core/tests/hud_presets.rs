//! Presets kept under the data dir.

use crate::common;

use common::library_env;
use tf2tools_core::{Change, OptionChange, OptionValue, PresetError, SetKey};

fn option(group: u32, id: &str, value: OptionValue) -> Change {
    Change::option(group, OptionChange::new(OptionChange::BUILTIN, id, value))
}

fn changes() -> Vec<Change> {
    vec![
        Change::set(
            0,
            SetKey::new(
                "scripts/hudlayout.res",
                &["R", "X"],
                "xpos",
                None,
                None,
                "1",
            ),
        ),
        option(1, "killfeed_rows", OptionValue::Int(6)),
    ]
}

#[test]
fn a_saved_preset_lists_and_loads() {
    let env = library_env();
    let entry = env.lib.save_preset("  Six rows ", &changes()).unwrap();
    assert_eq!(entry.name(), "Six rows");
    assert_eq!(env.lib.list_presets(), std::slice::from_ref(&entry));
    let recipe = env.lib.load_preset(entry.file()).unwrap();
    assert_eq!(
        recipe.changes(),
        [option(1, "killfeed_rows", OptionValue::Int(6))]
    );
    let dir = env.dir.hud_presets_dir();
    let names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "no temporary file is left");
}

#[test]
fn names_must_be_given_and_unique() {
    let env = library_env();
    env.lib.save_preset("Six rows", &changes()).unwrap();
    assert!(matches!(
        env.lib.save_preset("six ROWS", &changes()),
        Err(PresetError::NameTaken { .. })
    ));
    assert!(matches!(
        env.lib.save_preset("   ", &changes()),
        Err(PresetError::NameBlank)
    ));
    assert!(matches!(
        env.lib.save_preset("Nothing", &changes()[..1]),
        Err(PresetError::NoOptions)
    ));
}

#[test]
fn listing_skips_what_is_not_a_preset() {
    let env = library_env();
    assert!(env.lib.list_presets().is_empty(), "no folder yet");
    env.lib.save_preset("b", &changes()).unwrap();
    env.lib.save_preset("A", &changes()).unwrap();
    let dir = env.dir.hud_presets_dir();
    std::fs::write(dir.join("junk.json"), b"{").unwrap();
    std::fs::write(dir.join("notes.txt"), b"hello").unwrap();
    let names: Vec<String> = env
        .lib
        .list_presets()
        .iter()
        .map(|p| p.name().to_owned())
        .collect();
    assert_eq!(names, ["A", "b"]);
}

#[test]
fn a_deleted_preset_is_gone() {
    let env = library_env();
    let entry = env.lib.save_preset("Gone", &changes()).unwrap();
    env.lib.delete_preset(entry.file()).unwrap();
    assert!(env.lib.list_presets().is_empty());
    assert!(matches!(
        env.lib.load_preset(entry.file()),
        Err(PresetError::NotFound)
    ));
}

#[test]
fn preset_files_are_plain_names() {
    let env = library_env();
    std::fs::write(env.tmp.path().join("x.json"), b"{}").unwrap();
    for file in ["../x.json", "/etc/passwd", "a/b.json", "", "x.txt"] {
        assert!(
            matches!(env.lib.load_preset(file), Err(PresetError::NotFound)),
            "{file}"
        );
        assert!(
            matches!(env.lib.delete_preset(file), Err(PresetError::NotFound)),
            "{file}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_symlinked_preset_is_never_followed() {
    let env = library_env();
    let real = env.lib.save_preset("Mine", &changes()).unwrap();
    let dir = env.dir.hud_presets_dir();
    let outside = env.tmp.path().join("outside.json");
    std::fs::copy(dir.join(real.file()), &outside).unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("link.json")).unwrap();

    let names: Vec<String> = env
        .lib
        .list_presets()
        .iter()
        .map(|p| p.file().to_owned())
        .collect();
    assert_eq!(names, [real.file()]);
    assert!(matches!(
        env.lib.load_preset("link.json"),
        Err(PresetError::NotFound)
    ));
    // Deleting removes the link itself and leaves its target alone.
    env.lib.delete_preset("link.json").unwrap();
    assert!(std::fs::symlink_metadata(dir.join("link.json")).is_err());
    assert!(outside.is_file());
}

#[cfg(unix)]
#[test]
fn a_pipe_or_folder_named_like_a_preset_is_skipped() {
    let env = library_env();
    env.lib.save_preset("Mine", &changes()).unwrap();
    let dir = env.dir.hud_presets_dir();
    let made = std::process::Command::new("mkfifo")
        .arg(dir.join("pipe.json"))
        .status()
        .unwrap();
    assert!(made.success());
    std::fs::create_dir(dir.join("folder.json")).unwrap();

    // Opening a pipe would wait for a writer forever, so the calls run on a timed thread.
    let (tx, rx) = std::sync::mpsc::channel();
    let lib = env.lib.clone();
    std::thread::spawn(move || {
        let names: Vec<String> = lib
            .list_presets()
            .iter()
            .map(|p| p.name().to_owned())
            .collect();
        let pipe = matches!(lib.load_preset("pipe.json"), Err(PresetError::NotFound));
        let folder = matches!(lib.load_preset("folder.json"), Err(PresetError::NotFound));
        let _ = tx.send((names, pipe, folder));
    });
    let (names, pipe, folder) = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("listing must not block on a pipe");
    assert_eq!(names, ["Mine"]);
    assert!(pipe && folder);
    assert!(matches!(
        env.lib.delete_preset("folder.json"),
        Err(PresetError::NotFound)
    ));
    assert!(dir.join("folder.json").is_dir());
}

#[test]
fn listing_reads_at_most_a_thousand_entries() {
    let env = library_env();
    let first = env.lib.save_preset("Seed", &changes()).unwrap();
    let dir = env.dir.hud_presets_dir();
    let bytes = std::fs::read(dir.join(first.file())).unwrap();
    for i in 0..1004 {
        std::fs::write(dir.join(format!("copy-{i}.json")), &bytes).unwrap();
    }
    assert_eq!(env.lib.list_presets().len(), 1000);
}

#[cfg(unix)]
#[test]
fn a_picked_recipe_file_reads_and_a_pipe_never_blocks() {
    use tf2tools_core::{Recipe, read_recipe_file};

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("mine.json");
    std::fs::write(&file, Recipe::preset("Mine", &changes()).to_json()).unwrap();
    let (recipe, skipped) = read_recipe_file(&file).unwrap();
    assert_eq!((recipe.name(), skipped), ("Mine", 0));

    std::fs::write(dir.path().join("junk.json"), b"not json").unwrap();
    assert!(matches!(
        read_recipe_file(&dir.path().join("junk.json")),
        Err(PresetError::Recipe(_))
    ));
    assert!(matches!(
        read_recipe_file(&dir.path().join("gone.json")),
        Err(PresetError::NotFound)
    ));
    let pipe = dir.path().join("pipe.json");
    let made = std::process::Command::new("mkfifo")
        .arg(&pipe)
        .status()
        .unwrap();
    assert!(made.success());
    // Opening a pipe would wait for a writer forever, so the read runs on a timed thread.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(matches!(
            read_recipe_file(&pipe),
            Err(PresetError::NotFound)
        ));
    });
    let refused = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("reading must not block on a pipe");
    assert!(refused);
}
