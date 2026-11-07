//! Applying detected switches: exact bytes, moves, exclusivity, pristine when off, drops.

mod common;

use common::hud_switches::{DAMAGE_SWITCH, switch_files};
use tf2tools_core::{
    Change, EditRefused, HudFiles, HudOptions, OptionChange, OptionValue, Platform,
};

const GROUP: &str = "f:customizations/damage.res:0";

fn hud(id: &str, value: OptionValue) -> Change {
    Change::option(1, OptionChange::new(OptionChange::HUD, id, value))
}
fn text(files: &HudFiles, path: &str) -> Option<String> {
    files
        .read(path)
        .unwrap()
        .map(|f| String::from_utf8(f.into_bytes()).unwrap())
}

#[test]
fn a_pick_comments_the_others_and_uncomments_the_chosen_line() {
    let files = switch_files(&[]);
    let options = HudOptions::detected(&files, Platform::Linux);
    let switched = options
        .switch(
            &files,
            &[hud(
                GROUP,
                OptionValue::Pick("resource/ui/damage_big.res".into()),
            )],
        )
        .unwrap();
    let want = DAMAGE_SWITCH
        .replace(
            "\n#base \"../resource/ui/damage_medium.res\"",
            "\n//#base \"../resource/ui/damage_medium.res\"",
        )
        .replace(
            "//#base \"../resource/ui/damage_big.res\"",
            "#base \"../resource/ui/damage_big.res\"",
        );
    assert_eq!(
        text(switched.files(), "customizations/damage.res").unwrap(),
        want
    );
    assert_eq!(switched.dropped(), 0);
}

#[test]
fn the_pristine_value_writes_nothing() {
    let files = switch_files(&[]);
    let options = HudOptions::detected(&files, Platform::Linux);
    let choices = [
        hud(
            GROUP,
            OptionValue::Pick("resource/ui/damage_medium.res".into()),
        ),
        hud("b:#customization/bh_chat_top.res", OptionValue::Bool(true)),
    ];
    let switched = options.switch(&files, &choices).unwrap();
    assert_eq!(switched.files().switched_paths(), (Vec::new(), Vec::new()));
}

#[test]
fn an_enabled_slot_moves_in_and_a_shipped_one_moves_out() {
    let files = switch_files(&[]);
    let options = HudOptions::detected(&files, Platform::Linux);
    let switched = options
        .switch(
            &files,
            &[
                hud(
                    "b:#customization/bh_player_largehealth.res",
                    OptionValue::Bool(true),
                ),
                hud("b:#customization/bh_chat_top.res", OptionValue::Bool(false)),
            ],
        )
        .unwrap();
    let f = switched.files();
    assert!(f.contains("#customization/_enabled/bh_player_largehealth.res"));
    assert!(!f.contains("#customization/bh_player_largehealth.res"));
    assert!(f.contains("#customization/bh_chat_top.res"));
    assert!(!f.contains("#customization/_enabled/bh_chat_top.res"));
}

#[test]
fn a_rename_and_a_one_of_folder_move_whole_folders() {
    let files = switch_files(&[]);
    let options = HudOptions::detected(&files, Platform::Linux);
    let switched = options
        .switch(
            &files,
            &[
                hud("c:customizations/streamermode_", OptionValue::Bool(true)),
                hud(
                    "b:#customization/_disabled/! scoreboards",
                    OptionValue::Pick(
                        "#customization/_disabled/! scoreboards/32p scoreboard".into(),
                    ),
                ),
            ],
        )
        .unwrap();
    let f = switched.files();
    assert!(
        f.contains("customizations/streamermode/ammo.res")
            && !f.contains("customizations/streamermode_/ammo.res")
    );
    assert!(f.contains("#customization/32p scoreboard/scoreboard.res"));
    assert!(!f.contains("#customization/24p scoreboard/scoreboard.res"));
}

#[test]
fn a_move_onto_a_taken_path_is_dropped() {
    // Options read from the plain copy, applied over a version whose slot is already filled.
    let files = switch_files(&[(
        "#customization/_enabled/bh_player_largehealth.res",
        "\"Resource/UI/HudPlayerHealth.res\" { }\n",
    )]);
    let read_on_both = HudOptions::detected(&files, Platform::Linux);
    assert!(
        read_on_both
            .value("b:#customization/bh_player_largehealth.res", &[])
            .is_none()
    );
    let taken = HudOptions::detected(&switch_files(&[]), Platform::Linux)
        .switch(
            &files,
            &[hud(
                "b:#customization/bh_player_largehealth.res",
                OptionValue::Bool(true),
            )],
        )
        .unwrap();
    assert_eq!(taken.dropped(), 1);
    assert_eq!(taken.files().switched_paths(), (Vec::new(), Vec::new()));
}

#[test]
fn unknown_ids_keys_and_sources_are_dropped_and_counted() {
    let files = switch_files(&[]);
    let options = HudOptions::detected(&files, Platform::Linux);
    let choices = [
        hud("c:customizations/gone_", OptionValue::Bool(true)),
        hud(
            GROUP,
            OptionValue::Pick("resource/ui/damage_huge.res".into()),
        ),
        Change::option(
            1,
            OptionChange::new(
                OptionChange::SCHEMA,
                GROUP,
                OptionValue::Pick("resource/ui/damage_big.res".into()),
            ),
        ),
        hud(GROUP, OptionValue::Bool(true)),
    ];
    let switched = options.switch(&files, &choices).unwrap();
    assert_eq!(switched.dropped(), 4);
    assert_eq!(
        options.value(GROUP, &choices),
        Some(OptionValue::Pick("resource/ui/damage_medium.res".into()))
    );
}

#[test]
fn set_and_reset_follow_the_built_in_rules() {
    let files = switch_files(&[]);
    let options = HudOptions::detected(&files, Platform::Linux);
    let big = OptionValue::Pick("resource/ui/damage_big.res".into());
    let changes = options.set(GROUP, big.clone(), &[], 3).unwrap();
    assert_eq!(
        changes,
        [Change::option(
            3,
            OptionChange::new(OptionChange::HUD, GROUP, big.clone())
        )]
    );
    assert_eq!(options.value(GROUP, &changes), Some(big));
    let back = options
        .set(
            GROUP,
            OptionValue::Pick("resource/ui/damage_medium.res".into()),
            &changes,
            4,
        )
        .unwrap();
    assert!(back.is_empty());
    assert_eq!(
        options.set("c:nope", OptionValue::Bool(true), &[], 1),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        options.set(GROUP, OptionValue::Pick("x".into()), &[], 1),
        Err(EditRefused::BadValue)
    );
    assert!(options.reset_group("Damage", &changes).is_empty());
}

#[test]
fn crlf_lines_keep_their_endings() {
    let crlf = DAMAGE_SWITCH.replace('\n', "\r\n");
    let files = switch_files(&[("customizations/damage.res", &crlf)]);
    let options = HudOptions::detected(&files, Platform::Linux);
    let switched = options
        .switch(
            &files,
            &[hud(
                GROUP,
                OptionValue::Pick("resource/ui/damage_small.res".into()),
            )],
        )
        .unwrap();
    let out = text(switched.files(), "customizations/damage.res").unwrap();
    assert!(out.contains("#base \"../resource/ui/damage_small.res\"\t\t// SMALL\r\n//#base \"../resource/ui/damage_medium.res\""));
    assert_eq!(out.matches('\n').count(), out.matches("\r\n").count());
}

/// A renamed folder of `files` files, the bulk of `bytes_each` bytes with extension `ext`.
fn streamer_folder(files: usize, bytes_each: usize, ext: &str) -> Vec<(String, String)> {
    let mut extra = vec![(
        "customizations/streamermode_/ammo.res".to_owned(),
        "\"Resource/UI/HudAmmoWeapons.res\" { }\n".to_owned(),
    )];
    for i in 1..files {
        extra.push((
            format!("customizations/streamermode_/f{i}.{ext}"),
            "x".repeat(bytes_each),
        ));
    }
    extra
}
fn switched_with(extra: &[(String, String)]) -> tf2tools_core::Switched {
    let refs: Vec<(&str, &str)> = extra
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let files = switch_files(&refs);
    let options = HudOptions::detected(&files, Platform::Linux);
    // The choice must be one the options offer, so a drop below is the cap's doing.
    assert!(
        options
            .value("c:customizations/streamermode_", &[])
            .is_some(),
        "the rename was not detected"
    );
    options
        .switch(
            &files,
            &[hud(
                "c:customizations/streamermode_",
                OptionValue::Bool(true),
            )],
        )
        .unwrap()
}

#[test]
fn a_move_up_to_the_file_cap_applies() {
    let switched = switched_with(&streamer_folder(2048, 1, "res"));
    assert_eq!(
        (
            switched.dropped(),
            switched.files().switched_paths().0.len()
        ),
        (0, 2048)
    );
}

#[test]
fn a_move_past_the_file_cap_is_dropped_and_counted() {
    let switched = switched_with(&streamer_folder(2049, 1, "res"));
    assert_eq!(switched.dropped(), 1);
    assert_eq!(switched.files().switched_paths(), (Vec::new(), Vec::new()));
}

#[test]
fn a_move_past_the_byte_cap_is_dropped_and_counted() {
    // Three files of 22 MiB: 66 MiB, past the 64 MiB layer cap. They are textures, which
    // detection never reads, so its own byte cap leaves the rename detected.
    let switched = switched_with(&streamer_folder(4, 22 * 1024 * 1024, "vtf"));
    assert_eq!(switched.dropped(), 1);
    assert_eq!(switched.files().switched_paths(), (Vec::new(), Vec::new()));
}
