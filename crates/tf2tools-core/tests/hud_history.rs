//! A made HUD's history snapshots, kept in `.tf2tools/history/` inside its library folder.

mod common;

use common::hud_stock::stock_files;
use common::{LibraryEnv, library_env, tree};
use std::path::PathBuf;
use tf2tools_core::{
    Change, HistoryError, HudId, LibraryKind, MAX_SNAPSHOTS, MakeTarget, Platform, SavedHud, SetKey,
};

fn red_health(group: u32, colour: &str) -> Change {
    Change::set(
        group,
        SetKey::new(
            "resource/ui/hudplayerhealth.res",
            &["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
            "fgcolor",
            None,
            Some("TanDark"),
            colour,
        ),
    )
}

fn made(env: &LibraryEnv) -> SavedHud {
    env.lib
        .save_made_hud(
            &MakeTarget::New,
            "My HUD",
            &stock_files(),
            Platform::Linux,
            &[red_health(1, "255 0 0 255")],
        )
        .unwrap()
}

fn folder(env: &LibraryEnv, saved: &SavedHud) -> PathBuf {
    env.dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name())
}

fn history(env: &LibraryEnv, saved: &SavedHud) -> PathBuf {
    folder(env, saved).join(".tf2tools/history")
}

/// A snapshot file as a hand would write it, taken at `at`.
fn snapshot_json(name: &str, at: &str, changes: &str) -> String {
    format!(
        r#"{{ "version": 1, "kind": "snapshot", "name": {name:?}, "at": {at:?},
            "changes": [{changes}] }}"#
    )
}

const RED: &str = r#"{ "group": 1, "set": { "file": "resource/ui/hudplayerhealth.res",
    "block": ["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
    "key": "fgcolor", "cond": null, "old": "TanDark", "new": "255 0 0 255" } }"#;

#[test]
fn a_saved_snapshot_lists_and_reads_back() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    let changes = [red_health(1, "1 2 3 255"), red_health(2, "4 5 6 255")];
    let entry = env
        .lib
        .save_hud_snapshot(id, "  Before\u{200B} blue ", &changes)
        .unwrap();
    assert_eq!((entry.name(), entry.changes()), ("Before blue", 2));
    assert_eq!(entry.at().len(), 32, "{}", entry.at());
    assert_eq!(
        env.lib.list_hud_snapshots(id).unwrap(),
        std::slice::from_ref(&entry)
    );
    assert_eq!(
        tree(&history(&env, &hud)),
        [entry.file()],
        "no temporary file is left"
    );
    let text = std::fs::read_to_string(history(&env, &hud).join(entry.file())).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["version"], 1);
    assert_eq!(json["kind"], "snapshot");
    assert_eq!(json["name"], "Before blue");
    assert_eq!(json["at"], entry.at());
    assert_eq!(json["changes"][0]["set"]["new"], "1 2 3 255");
    assert_eq!(json["changes"][1]["group"], 2);
}

#[test]
fn names_are_cleaned_and_a_blank_one_is_refused() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    for blank in ["", "   ", "\u{200B}\t"] {
        assert!(matches!(
            env.lib.save_hud_snapshot(id, blank, &[]),
            Err(HistoryError::NameBlank)
        ));
    }
    let long = "x".repeat(100);
    let entry = env.lib.save_hud_snapshot(id, &long, &[]).unwrap();
    assert_eq!(entry.name().chars().count(), 80);
    // Names may repeat; each snapshot is its own file.
    let again = env.lib.save_hud_snapshot(id, &long, &[]).unwrap();
    assert_ne!(entry.file(), again.file());
    assert_eq!(env.lib.list_hud_snapshots(id).unwrap().len(), 2);
}

#[test]
fn the_list_is_newest_first() {
    let env = library_env();
    let hud = made(&env);
    let dir = history(&env, &hud);
    std::fs::create_dir_all(&dir).unwrap();
    let write = |file: &str, name: &str, at: &str| {
        std::fs::write(dir.join(file), snapshot_json(name, at, RED)).unwrap();
    };
    write("a.json", "Old", "2026-10-01T10:00:00+00:00");
    write("b.json", "New", "2026-10-03T10:00:00+00:00");
    // Another offset is read as the same instant in UTC.
    write("c.json", "Middle", "2026-10-02T12:00:00+02:00");
    write("d.json", "Also new", "2026-10-03T10:00:00Z");
    let listed = env.lib.list_hud_snapshots(hud.entry().id()).unwrap();
    let names: Vec<(&str, &str, usize)> = (listed.iter())
        .map(|e| (e.name(), e.at(), e.changes()))
        .collect();
    assert_eq!(
        names,
        [
            ("New", "2026-10-03T10:00:00.000000+00:00", 1),
            ("Also new", "2026-10-03T10:00:00.000000+00:00", 1),
            ("Middle", "2026-10-02T10:00:00.000000+00:00", 1),
            ("Old", "2026-10-01T10:00:00.000000+00:00", 1),
        ]
    );
}

#[test]
fn a_hud_keeps_at_most_fifty_snapshots() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    assert_eq!(MAX_SNAPSHOTS, 50);
    for i in 0..MAX_SNAPSHOTS {
        env.lib
            .save_hud_snapshot(id, &format!("S{i}"), &[])
            .unwrap();
    }
    assert!(matches!(
        env.lib.save_hud_snapshot(id, "One more", &[]),
        Err(HistoryError::Full { limit: 50 })
    ));
    assert_eq!(env.lib.list_hud_snapshots(id).unwrap().len(), 50);
}

#[test]
fn too_many_changes_are_refused() {
    let env = library_env();
    let hud = made(&env);
    let many = vec![red_health(1, "1 2 3 255"); 10_001];
    assert!(matches!(
        env.lib.save_hud_snapshot(hud.entry().id(), "Big", &many),
        Err(HistoryError::TooLarge)
    ));
    assert!(!history(&env, &hud).exists());
}

#[test]
fn only_a_made_hud_keeps_history() {
    let env = library_env();
    let gone = HudId::new("no-such-hud");
    assert!(matches!(
        env.lib.save_hud_snapshot(&gone, "Mine", &[]),
        Err(HistoryError::NotMade { .. })
    ));
    assert!(matches!(
        env.lib.list_hud_snapshots(&gone),
        Err(HistoryError::NotMade { .. })
    ));
    let hud = made(&env);
    assert!(
        env.lib
            .list_hud_snapshots(hud.entry().id())
            .unwrap()
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn links_are_never_followed() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    let real = env.lib.save_hud_snapshot(id, "Mine", &[]).unwrap();
    let dir = history(&env, &hud);
    let outside = env.tmp.path().join("outside.json");
    std::fs::copy(dir.join(real.file()), &outside).unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("link.json")).unwrap();
    let files: Vec<String> = (env.lib.list_hud_snapshots(id).unwrap().iter())
        .map(|e| e.file().to_owned())
        .collect();
    assert_eq!(files, [real.file()]);

    // A history folder that is a link is refused, so no write leaves the HUD's folder.
    let elsewhere = env.tmp.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &dir).unwrap();
    assert!(matches!(
        env.lib.save_hud_snapshot(id, "Out", &[]),
        Err(HistoryError::Io { .. })
    ));
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
}

#[test]
fn a_snapshot_reads_back_whole() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    let changes = [red_health(1, "1 2 3 255"), red_health(2, "4 5 6 255")];
    let entry = env.lib.save_hud_snapshot(id, "Two", &changes).unwrap();
    let loaded = env.lib.load_hud_snapshot(id, entry.file()).unwrap();
    assert_eq!((loaded.name(), loaded.at()), ("Two", entry.at()));
    assert_eq!(loaded.changes(), changes);
    assert_eq!(loaded.skipped(), 0);
    assert!(loaded.assets().is_empty());
}

#[test]
fn a_deleted_snapshot_is_gone() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    let entry = env.lib.save_hud_snapshot(id, "Mine", &[]).unwrap();
    env.lib.delete_hud_snapshot(id, entry.file()).unwrap();
    assert!(env.lib.list_hud_snapshots(id).unwrap().is_empty());
    assert!(matches!(
        env.lib.delete_hud_snapshot(id, entry.file()),
        Err(HistoryError::NotFound)
    ));
    assert!(matches!(
        env.lib.load_hud_snapshot(id, entry.file()),
        Err(HistoryError::NotFound)
    ));
    // With one deleted, a full HUD takes another.
    for i in 0..MAX_SNAPSHOTS {
        env.lib
            .save_hud_snapshot(id, &format!("S{i}"), &[])
            .unwrap();
    }
    let first = env.lib.list_hud_snapshots(id).unwrap()[0].file().to_owned();
    env.lib.delete_hud_snapshot(id, &first).unwrap();
    env.lib.save_hud_snapshot(id, "One more", &[]).unwrap();
}

#[test]
fn reading_is_strict_about_shape_and_lenient_per_change() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    let dir = history(&env, &hud);
    std::fs::create_dir_all(&dir).unwrap();
    let at = "2026-10-01T10:00:00+00:00";
    // An unknown kind, a path outside the HUD and a malformed body are each skipped.
    let odd = format!(
        r#"{RED}, {{ "group": 2, "future": {{}} }},
           {{ "group": 3, "set": {{ "file": "../../evil.res", "block": ["X"], "key": "k",
              "cond": null, "old": null, "new": "v" }} }},
           {{ "group": 4, "set": 7 }}"#
    );
    std::fs::write(dir.join("odd.json"), snapshot_json("Odd", at, &odd)).unwrap();
    let loaded = env.lib.load_hud_snapshot(id, "odd.json").unwrap();
    assert_eq!((loaded.changes().len(), loaded.skipped()), (1, 3));

    let cases: [(&str, &[u8]); 5] = [
        ("junk.json", b"\x00 not json"),
        (
            "recipe.json",
            br#"{ "version": 1, "kind": "recipe", "name": "R", "changes": [] }"#,
        ),
        (
            "when.json",
            br#"{ "version": 1, "kind": "snapshot", "name": "W", "at": "soon", "changes": [] }"#,
        ),
        (
            "noversion.json",
            br#"{ "kind": "snapshot", "name": "N", "at": "2026-10-01T10:00:00Z", "changes": [] }"#,
        ),
        (
            "newer.json",
            br#"{ "version": 2, "kind": "snapshot", "name": "N", "at": "2026-10-01T10:00:00Z", "changes": [] }"#,
        ),
    ];
    for (file, bytes) in cases {
        std::fs::write(dir.join(file), bytes).unwrap();
    }
    for file in ["junk.json", "recipe.json", "when.json", "noversion.json"] {
        assert!(
            matches!(
                env.lib.load_hud_snapshot(id, file),
                Err(HistoryError::Damaged { .. })
            ),
            "{file}"
        );
    }
    assert!(matches!(
        env.lib.load_hud_snapshot(id, "newer.json"),
        Err(HistoryError::Newer)
    ));
    std::fs::write(dir.join("huge.json"), vec![b' '; 32 * 1024 * 1024 + 1]).unwrap();
    assert!(matches!(
        env.lib.load_hud_snapshot(id, "huge.json"),
        Err(HistoryError::Damaged { .. })
    ));
    // A temporary file left by a crash and every unreadable file stay out of the list.
    std::fs::write(dir.join("left.json.tmp"), snapshot_json("Tmp", at, RED)).unwrap();
    let names: Vec<String> = (env.lib.list_hud_snapshots(id).unwrap().iter())
        .map(|e| e.name().to_owned())
        .collect();
    assert_eq!(names, ["Odd"]);
}

#[test]
fn snapshot_files_are_plain_names() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    env.lib.save_hud_snapshot(id, "Mine", &[]).unwrap();
    for bad in [
        "../maker.json",
        "/etc/passwd",
        "a/b.json",
        "",
        "..",
        "x.txt",
        "x.json.tmp",
    ] {
        assert!(
            matches!(
                env.lib.load_hud_snapshot(id, bad),
                Err(HistoryError::NotFound)
            ),
            "{bad}"
        );
        assert!(
            matches!(
                env.lib.delete_hud_snapshot(id, bad),
                Err(HistoryError::NotFound)
            ),
            "{bad}"
        );
    }
    assert!(folder(&env, &hud).join(".tf2tools/maker.json").is_file());
}

#[cfg(unix)]
#[test]
fn a_linked_snapshot_is_never_read_and_deleting_removes_only_the_link() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    let real = env.lib.save_hud_snapshot(id, "Mine", &[]).unwrap();
    let dir = history(&env, &hud);
    let outside = env.tmp.path().join("outside.json");
    std::fs::copy(dir.join(real.file()), &outside).unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("link.json")).unwrap();
    assert!(matches!(
        env.lib.load_hud_snapshot(id, "link.json"),
        Err(HistoryError::NotFound)
    ));
    env.lib.delete_hud_snapshot(id, "link.json").unwrap();
    assert!(std::fs::symlink_metadata(dir.join("link.json")).is_err());
    assert!(outside.is_file());
}

#[cfg(unix)]
#[test]
fn a_pipe_or_folder_named_like_a_snapshot_is_skipped() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id().clone();
    env.lib.save_hud_snapshot(&id, "Mine", &[]).unwrap();
    let dir = history(&env, &hud);
    let made_pipe = std::process::Command::new("mkfifo")
        .arg(dir.join("pipe.json"))
        .status()
        .unwrap();
    assert!(made_pipe.success());
    std::fs::create_dir(dir.join("folder.json")).unwrap();
    // Opening a pipe would wait for a writer forever, so the calls run on a timed thread.
    let (tx, rx) = std::sync::mpsc::channel();
    let lib = env.lib.clone();
    std::thread::spawn(move || {
        let names: Vec<String> = (lib.list_hud_snapshots(&id).unwrap().iter())
            .map(|e| e.name().to_owned())
            .collect();
        let pipe = matches!(
            lib.load_hud_snapshot(&id, "pipe.json"),
            Err(HistoryError::NotFound)
        );
        let folder = matches!(
            lib.load_hud_snapshot(&id, "folder.json"),
            Err(HistoryError::NotFound)
        );
        let kept = matches!(
            lib.delete_hud_snapshot(&id, "folder.json"),
            Err(HistoryError::NotFound)
        );
        let _ = tx.send((names, pipe, folder, kept));
    });
    let (names, pipe, folder, kept) = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("listing must not block on a pipe");
    assert_eq!(names, ["Mine"]);
    assert!(pipe && folder && kept);
    assert!(dir.join("folder.json").is_dir());
}

#[cfg(unix)]
#[test]
fn a_linked_history_folder_is_never_read() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    let elsewhere = env.tmp.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let at = "2026-10-01T10:00:00+00:00";
    std::fs::write(elsewhere.join("x.json"), snapshot_json("Planted", at, RED)).unwrap();
    std::os::unix::fs::symlink(&elsewhere, history(&env, &hud)).unwrap();
    assert!(matches!(
        env.lib.list_hud_snapshots(id),
        Err(HistoryError::Io { .. })
    ));
    assert!(matches!(
        env.lib.load_hud_snapshot(id, "x.json"),
        Err(HistoryError::Io { .. })
    ));
}

#[test]
fn history_survives_a_resave_but_not_save_as_new() {
    let env = library_env();
    let hud = made(&env);
    let id = hud.entry().id();
    let entry = env.lib.save_hud_snapshot(id, "Keep me", &[]).unwrap();
    let changes = [red_health(1, "9 9 9 255")];
    env.lib
        .save_made_hud(
            &MakeTarget::Existing(id.clone()),
            "My HUD",
            &stock_files(),
            Platform::Linux,
            &changes,
        )
        .unwrap();
    assert_eq!(env.lib.list_hud_snapshots(id).unwrap(), [entry]);
    let copy = env
        .lib
        .save_made_hud(
            &MakeTarget::New,
            "My copy",
            &stock_files(),
            Platform::Linux,
            &changes,
        )
        .unwrap();
    assert!(
        env.lib
            .list_hud_snapshots(copy.entry().id())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_snapshot_brings_the_kept_copy_of_its_fonts() {
    use tf2tools_core::{HudAssets, HudModel};

    let env = library_env();
    let bytes = common::sfnt::sfnt_bytes(&[(1, "Big Numbers")], false);
    let model = HudModel::load(&stock_files(), Platform::Linux).unwrap();
    let (file, changes) = model.add_font_file("Big Numbers.ttf", &bytes, 1).unwrap();
    let mut assets = HudAssets::new();
    assets.insert(file.source(), bytes.clone());
    let hud = env
        .lib
        .save_made_hud_with_assets(
            &MakeTarget::New,
            "Fonty",
            &stock_files(),
            Platform::Linux,
            &changes,
            &assets,
        )
        .unwrap();
    let id = hud.entry().id();
    let entry = env
        .lib
        .save_hud_snapshot(id, "With font", &changes)
        .unwrap();
    let loaded = env.lib.load_hud_snapshot(id, entry.file()).unwrap();
    assert_eq!(loaded.assets().get(file.source()), Some(bytes.as_slice()));
    // A kept copy that is gone leaves the changes as they were, with no bytes.
    std::fs::remove_file(folder(&env, &hud).join(file.source())).unwrap();
    let loaded = env.lib.load_hud_snapshot(id, entry.file()).unwrap();
    assert!(loaded.assets().is_empty());
    assert_eq!(loaded.changes(), changes);
}
