//! The pure parts of the setting pictures shooter: cfg text, log markers, launch and action lists.

use tf2tools_core::PictureManifest;
use tf2tools_core::shoot::{
    ShotAction, join_commands, launch_args, marker_seen, rmse, setting_actions, shot_name,
    start_cfg, step_cfg, value_actions,
};

const MANIFEST: &str = r#"{
  "version": 1, "mastercomfig": "9.100.1", "size": [960, 540], "map": "ctf_2fort",
  "base": { "gibs": "low", "shadows": "high" },
  "settings": [
    { "module": "shadows", "setpos": [-1888.0, -1280.5, 321.0], "setang": [0.0, -30.0, 0.0],
      "setup": ["thirdperson"], "values": [{ "value": "off" }] },
    { "module": "gibs", "map": "cp_badlands", "reload": "map", "pause": false,
      "steps": [{ "run": "kill", "wait_ms": 400 }], "values": [{ "value": "high" }] },
    { "module": "lod", "reload": "materials", "values": [{ "value": "low" }] },
    { "module": "anti_aliasing", "reload": "restart", "values": [{ "value": "msaa_8x" }] }
  ]
}"#;

fn manifest() -> PictureManifest {
    PictureManifest::parse(MANIFEST.as_bytes()).unwrap()
}

fn run(cmds: &[&str]) -> ShotAction {
    ShotAction::Run(cmds.iter().map(|c| (*c).to_owned()).collect())
}

#[test]
fn the_start_cfg_arms_the_step_loop() {
    assert_eq!(
        start_cfg(),
        "echo TFS_START\nsv_cheats 1\ncl_disablehtmlmotd 1\nalias tfs_loop \"tfs_poll\"\n\
         alias tfs_poll \"exec tfs/step_1; wait 30; tfs_loop\"\ntfs_loop\n"
    );
}

#[test]
fn a_step_cfg_points_the_loop_at_the_next_step_and_ends_with_its_marker() {
    let cmds = vec![
        "shadows_low".to_owned(),
        "jpeg tfs_shadows_low 95".to_owned(),
    ];
    assert_eq!(
        step_cfg(7, &cmds),
        "alias tfs_poll \"exec tfs/step_8; wait 30; tfs_loop\"\nshadows_low\njpeg tfs_shadows_low 95\necho TFS_DONE_7\n"
    );
}

#[test]
fn a_marker_matches_its_own_step_only() {
    let log = "Unknown command \"x\"\nTFS_DONE_12 \n'tfs/step_13' not present; not executing.\n";
    assert!(marker_seen(log, 12));
    assert!(!marker_seen(log, 1));
    assert!(!marker_seen(log, 2));
    assert!(!marker_seen("echo TFS_DONE_3\n", 3));
}

#[test]
fn the_launch_is_a_private_local_server_on_the_map() {
    let args = launch_args("ctf_2fort");
    let joined = args.join(" ");
    assert!(joined.starts_with("-applaunch 440 "), "{joined}");
    for needed in [
        "-insecure",
        "+sv_lan 1",
        "+maxplayers 1",
        "+map ctf_2fort",
        "+exec tfs/start",
        "-condebug",
        "-windowed",
        "-w 1440 -h 810",
        "+volume 0",
    ] {
        assert!(joined.contains(needed), "{needed} missing from {joined}");
    }
    assert!(!joined.contains("connect"), "{joined}");
}

#[test]
fn joining_hides_the_hud_and_viewmodel() {
    assert_eq!(
        join_commands(),
        [
            "jointeam blue",
            "joinclass scout",
            "cl_drawhud 0",
            "r_drawviewmodel 0"
        ]
    );
}

#[test]
fn shot_names_carry_module_and_value() {
    assert_eq!(shot_name("3dsky", "on"), "tfs_3dsky_on");
}

#[test]
fn a_setting_starts_from_the_base_without_its_own_module() {
    let m = manifest();
    let shadows = m.setting("shadows").unwrap();
    assert_eq!(
        setting_actions(&m, shadows),
        vec![
            run(&["gibs_low"]),
            run(&["setpos -1888 -1280.5 321", "setang 0 -30 0", "thirdperson"]),
            ShotAction::Sleep(1000),
        ]
    );
}

#[test]
fn a_live_value_is_applied_paused_and_shot() {
    let m = manifest();
    let shadows = m.setting("shadows").unwrap();
    assert_eq!(
        value_actions(&m, shadows, "off"),
        vec![
            run(&["shadows_off"]),
            ShotAction::Sleep(1000),
            run(&["setpause"]),
            ShotAction::Jpeg("tfs_shadows_off".to_owned()),
            run(&["unpause"]),
        ]
    );
}

#[test]
fn a_map_reload_value_rejoins_and_sets_up_again_on_the_settings_map() {
    let m = manifest();
    let gibs = m.setting("gibs").unwrap();
    assert_eq!(
        value_actions(&m, gibs, "high"),
        vec![
            run(&["gibs_high"]),
            run(&["map cp_badlands"]),
            ShotAction::Rejoin,
            run(&["shadows_high"]),
            run(&[]),
            ShotAction::Sleep(1000),
            run(&["gibs_high"]),
            ShotAction::Sleep(1000),
            run(&["kill"]),
            ShotAction::Sleep(400),
            ShotAction::Jpeg("tfs_gibs_high".to_owned()),
        ]
    );
}

#[test]
fn a_materials_value_reloads_materials_with_the_alias() {
    let m = manifest();
    let lod = m.setting("lod").unwrap();
    assert_eq!(
        value_actions(&m, lod, "low")[0],
        run(&["lod_low", "mat_reloadallmaterials"])
    );
}

#[test]
fn a_restart_value_restarts_after_the_alias() {
    let m = manifest();
    let aa = m.setting("anti_aliasing").unwrap();
    let actions = value_actions(&m, aa, "msaa_8x");
    assert_eq!(actions[0], run(&["anti_aliasing_msaa_8x"]));
    assert_eq!(actions[1], ShotAction::Restart);
    assert_eq!(actions[2], run(&["gibs_low", "shadows_high"]));
    assert_eq!(actions.last(), Some(&run(&["unpause"])));
}

#[test]
fn rmse_is_zero_for_equal_pixels_and_none_for_different_sizes() {
    assert_eq!(rmse(&[10, 20, 30], &[10, 20, 30]), Some(0.0));
    assert_eq!(rmse(&[0, 0, 0, 0], &[2, 2, 2, 2]), Some(2.0));
    assert_eq!(rmse(&[0, 0, 0], &[0, 0]), None);
    assert_eq!(rmse(&[], &[]), None);
}
