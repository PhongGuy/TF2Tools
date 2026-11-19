//! The pure parts of the setting pictures shooter: cfg text, log markers, launch and action lists.

use tf2tools_core::PictureManifest;
use tf2tools_core::shoot::{
    ShotAction, join_commands, kick_seen, launch_args, marker_seen, modules_cfg, reset_actions,
    rmse, setting_actions, shot_name, start_cfg, step_cfg, value_actions, zoom_geometry,
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
        "echo TFS_START\nsv_cheats 1\nsv_quota_stringcmdspersecond 1000\n\
         mp_teams_unbalance_limit 0\nmp_autoteambalance 0\nmp_disable_respawn_times 1\nr_drawviewmodel 1\n\
         cl_disablehtmlmotd 1\n\
         alias tfs_loop \"tfs_poll\"\nalias tfs_poll \"exec tfs/step_1; wait 30; tfs_loop\"\ntfs_loop\n"
    );
}

#[test]
fn a_kick_is_seen_in_the_log() {
    assert!(kick_seen(
        "x\nDisconnect: #GameUI_Disconnect_TooManyCommands.\n"
    ));
    assert!(kick_seen("Issued too many commands to server.\n"));
    assert!(!kick_seen(
        "TFS_DONE_3 \n'tfs/step_4' not present; not executing.\n"
    ));
    assert!(!kick_seen("echo Disconnect: is a word\n"));
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
        "+sv_quota_stringcmdspersecond 1000",
        "+map ctf_2fort",
        "+exec tfs/start",
        "-condebug",
        "-windowed -noborder",
        "-sdl_displayindex 1",
        "-w 1440 -h 810",
        "+volume 0",
    ] {
        assert!(joined.contains(needed), "{needed} missing from {joined}");
    }
    assert!(!joined.contains("connect"), "{joined}");
}

#[test]
fn joining_hides_the_hud_and_shows_the_viewmodel() {
    assert_eq!(
        join_commands(),
        [
            "jointeam blue",
            "joinclass soldier",
            "cl_drawhud 0",
            "r_drawviewmodel 1"
        ]
    );
}

#[test]
fn shot_names_carry_module_and_value() {
    assert_eq!(shot_name("3dsky", "on"), "tfs_3dsky_on");
}

#[test]
fn a_setting_starts_from_its_position_and_setup() {
    let m = manifest();
    let shadows = m.setting("shadows").unwrap();
    assert_eq!(
        setting_actions(&m, shadows),
        vec![
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
fn a_restart_value_never_runs_its_alias_live() {
    let m = manifest();
    let aa = m.setting("anti_aliasing").unwrap();
    assert_eq!(
        value_actions(&m, aa, "msaa_8x"),
        vec![
            ShotAction::Restart,
            run(&[]),
            ShotAction::Sleep(1000),
            ShotAction::Sleep(1000),
            run(&["setpause"]),
            ShotAction::Jpeg("tfs_anti_aliasing_msaa_8x".to_owned()),
            run(&["unpause"]),
        ]
    );
}

#[test]
fn a_live_setting_resets_its_module_to_the_base_afterwards() {
    let m = manifest();
    assert_eq!(
        reset_actions(&m, m.setting("shadows").unwrap()),
        vec![run(&["shadows_high"])]
    );
    assert_eq!(reset_actions(&m, m.setting("lod").unwrap()), vec![]);
    assert_eq!(
        reset_actions(&m, m.setting("anti_aliasing").unwrap()),
        vec![]
    );
}

#[test]
fn modules_cfg_lists_the_base_in_mastercomfig_syntax() {
    let m = manifest();
    assert_eq!(modules_cfg(&m, None), "gibs=low\nshadows=high\n");
}

#[test]
fn modules_cfg_puts_the_shot_value_in_place_of_its_base() {
    let m = manifest();
    assert_eq!(
        modules_cfg(&m, Some(("shadows", "off"))),
        "gibs=low\nshadows=off\n"
    );
    assert_eq!(
        modules_cfg(&m, Some(("lighting", "ultra"))),
        "gibs=low\nshadows=high\nlighting=ultra\n"
    );
}

#[test]
fn rmse_is_the_root_of_the_mean_square() {
    let got = rmse(&[0, 0], &[3, 4]).unwrap();
    assert!((got - 12.5_f64.sqrt()).abs() < 1e-9, "{got}");
}

#[test]
fn rmse_is_zero_for_equal_pixels_and_none_for_different_sizes() {
    assert_eq!(rmse(&[10, 20, 30], &[10, 20, 30]), Some(0.0));
    assert_eq!(rmse(&[0, 0, 0, 0], &[2, 2, 2, 2]), Some(2.0));
    assert_eq!(rmse(&[0, 0, 0], &[0, 0]), None);
    assert_eq!(rmse(&[], &[]), None);
}

#[test]
fn the_zoom_is_cut_from_the_full_size_frame_at_twice_the_crop() {
    let crop: tf2tools_core::Crop =
        serde_json::from_str(r#"{"x": 100, "y": 50, "w": 320, "h": 180}"#).unwrap();
    let g = zoom_geometry(crop, [960, 540], (1440, 810));
    assert_eq!(
        (g.x, g.y, g.w, g.h, g.out_w, g.out_h),
        (150, 75, 480, 270, 640, 360)
    );
}
