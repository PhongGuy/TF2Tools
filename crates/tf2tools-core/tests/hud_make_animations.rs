//! Saving and reopening a made HUD with animation changes.

mod common;

use common::hud_anims::{MANIFEST, anim_files};
use common::{library_env, tree};
use tf2tools_core::{AnimationAppend, AnimationChange, Change, LibraryKind, MakeTarget, Platform};

const SCRIPT: &str = "scripts/hudanimations_tf2tools.txt";
const MANIFEST_PATH: &str = "scripts/hudanimations_manifest.txt";

fn anim(group: u32, event: &str, body: &str) -> Change {
    Change::animation(group, AnimationChange::new(event, body))
}

#[test]
fn the_script_is_written_and_listed_first() {
    let env = library_env();
    let changes = [
        anim(1, "HudHealthDyingPulse", "RunEvent A 0"),
        anim(2, "HudLowAmmoPulse", ""),
        anim(3, "hudhealthdyingpulse", "RunEvent B 0.5"),
        Change::animation_append(
            4,
            AnimationAppend::new("HudHealthDyingPulse", "RunEvent Flash 0"),
        ),
    ];
    let saved = env
        .lib
        .save_made_hud(
            &MakeTarget::New,
            "Pulse",
            &anim_files(&[]),
            Platform::Linux,
            &changes,
        )
        .unwrap();
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name());
    assert!(tree(&dir).contains(&SCRIPT.to_owned()));
    assert_eq!(
        std::fs::read_to_string(dir.join(SCRIPT)).unwrap(),
        "event HudHealthDyingPulse\n{\n\tRunEvent \"B\" 0.5\n\tRunEvent \"Flash\" 0\n}\n\nevent HudLowAmmoPulse\n{\n}\n"
    );
    let manifest = std::fs::read_to_string(dir.join(MANIFEST_PATH)).unwrap();
    assert_eq!(
        manifest,
        MANIFEST.replacen(
            "{\n",
            "{\n\t\"file\"\t\"scripts/hudanimations_tf2tools.txt\"\n",
            1
        )
    );
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join(".tf2tools/maker.json")).unwrap()).unwrap();
    assert!(json["sources"][MANIFEST_PATH].is_string());
}

#[test]
fn a_reopened_hud_plays_its_changed_events() {
    let env = library_env();
    let changes = [anim(1, "HudMedicCharged", "RunEvent Z 0")];
    let files = anim_files(&[]);
    let saved = env
        .lib
        .save_made_hud(&MakeTarget::New, "Re", &files, Platform::Linux, &changes)
        .unwrap();
    let made = env
        .lib
        .open_made_hud(saved.entry().id(), &files, Platform::Linux)
        .unwrap();
    let edited = made.pristine().with_changes(made.changes());
    assert_eq!(
        edited
            .model()
            .animations()
            .event("HudMedicCharged")
            .unwrap()
            .body(),
        "RunEvent \"Z\" 0"
    );
}

#[test]
fn without_a_manifest_animation_changes_are_left_out() {
    let env = library_env();
    let files = common::hud_stock::stock_files();
    let saved = env
        .lib
        .save_made_hud(
            &MakeTarget::New,
            "None",
            &files,
            Platform::Linux,
            &[anim(1, "A", "RunEvent B 0")],
        )
        .unwrap();
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name());
    assert!(!tree(&dir).contains(&SCRIPT.to_owned()));
    assert_eq!(saved.dropped(), 1);
}

#[test]
fn a_saved_animation_does_not_read_as_a_game_update() {
    let env = library_env();
    let files = anim_files(&[]);
    let changes = [anim(1, "HudMedicCharged", "RunEvent Z 0")];
    let saved = env
        .lib
        .save_made_hud(&MakeTarget::New, "Same", &files, Platform::Linux, &changes)
        .unwrap();
    let made = env
        .lib
        .open_made_hud(saved.entry().id(), &files, Platform::Linux)
        .unwrap();
    assert!(
        !made.tf2_updated(),
        "the manifest's hash matches the files it was saved from"
    );
}

/// Every file the saved HUD holds, read as text.
fn texts(dir: &std::path::Path) -> String {
    tree(dir)
        .iter()
        .map(|rel| String::from_utf8_lossy(&std::fs::read(dir.join(rel)).unwrap()).into_owned())
        .collect()
}

#[test]
fn a_smuggled_console_command_is_never_saved() {
    let env = library_env();
    let probe = "SetVisible \"x 1 0\nFireCommand 0 quit\nSetVisible y\" 1 0";
    let changes = [
        anim(1, "Sneaky", probe),
        Change::animation_append(2, AnimationAppend::new("DamagedPlayer", probe)),
    ];
    let saved = env
        .lib
        .save_made_hud(
            &MakeTarget::New,
            "Probe",
            &anim_files(&[]),
            Platform::Linux,
            &changes,
        )
        .unwrap();
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name());
    assert!(!texts(&dir).contains("FireCommand"));
    assert_eq!(saved.dropped(), 2);
}

#[test]
fn a_manifest_changed_after_saving_reopens_as_a_game_update() {
    let env = library_env();
    let changes = [anim(1, "HudMedicCharged", "RunEvent Z 0")];
    let saved = env
        .lib
        .save_made_hud(
            &MakeTarget::New,
            "Moved",
            &anim_files(&[]),
            Platform::Linux,
            &changes,
        )
        .unwrap();
    let newer = format!("{MANIFEST}// a newer game\n");
    let files = anim_files(&[(MANIFEST_PATH, Some(&newer))]);
    let made = env
        .lib
        .open_made_hud(saved.entry().id(), &files, Platform::Linux)
        .unwrap();
    assert!(made.tf2_updated());
}

#[test]
fn a_manifest_without_a_block_drops_the_changes_it_cannot_list() {
    let env = library_env();
    let files = anim_files(&[(MANIFEST_PATH, Some("just words"))]);
    let changes = [anim(1, "HudMedicCharged", "RunEvent Z 0")];
    let saved = env
        .lib
        .save_made_hud(&MakeTarget::New, "Flat", &files, Platform::Linux, &changes)
        .unwrap();
    let dir = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name());
    assert!(!tree(&dir).contains(&SCRIPT.to_owned()));
    assert_eq!(saved.dropped(), 1);
    let json = std::fs::read_to_string(dir.join(".tf2tools/maker.json")).unwrap();
    assert!(
        !json.contains("\"animation\""),
        "a change never written is not listed"
    );
}
