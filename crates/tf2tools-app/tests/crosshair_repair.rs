//! Startup repairs the crosshair-only weapon scripts apply wrote before 6.0.2.

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};
use tf2tools_app::app::{Boot, start};
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::ids;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::{Tf2Detector, Tf2State, recheck_tf2, tf2_state};
use tf2tools_app::shell::toast::toast_count;
use tf2tools_core::{CrosshairRef, Tf2Install};

mod support;

const FIXED: &str = "Fixed your crosshairs for the latest TF2 update";

// The crosshair-only script apply wrote before 6.0.2, which stops the weapon from firing.
const OLD_SCRIPT: &str = "\"WeaponData\"\n{\n\t\"TextureData\"\n\t{\n\t\t\"crosshair\"\n\t\t{\n\t\t\t\"file\"\t\t\"vgui/replay/thumbnails/cpma_9\"\n\t\t\t\"x\"\t\t\"0\"\n\t\t\t\"y\"\t\t\"0\"\n\t\t\t\"width\"\t\t\"64\"\n\t\t\t\"height\"\t\"64\"\n\t\t}\n\t}\n}";

fn boot(env: &support::Env, detector: Tf2Detector) -> Boot {
    Boot {
        data: env.data.clone(),
        assets: Ok(support::repo_asset_root()),
        picker: Arc::new(ScriptedPicker::empty()),
        detector,
        audio: Arc::new(RecordingAudio::new()),
        endpoints: tf2tools_core::Endpoints::default(),
        auto_sync: false,
        offline: false,
        font_host: None,
        updates: tf2tools_app::updater::UpdateBoot::Off,
    }
}

fn started(cx: &mut TestAppContext, boot: Boot) -> AnyWindowHandle {
    support::init(cx);
    cx.update(|cx| start(boot, cx)).unwrap()
}

fn scattergun(game: &std::path::Path) -> PathBuf {
    game.join("tf/custom/tf2tools-crosshairs/scripts/tf_weapon_scattergun.txt")
}

// A library with the Scattergun on a builtin crosshair and the pre-6.0.2 script in the game;
// `saved` decides whether the game path is in the settings.
fn old_install(env: &support::Env, saved: bool) {
    let services = support::services(env, support::no_steam());
    services
        .library
        .assign(
            "tf_weapon_scattergun",
            &CrosshairRef::Builtin("cpma_9".into()),
        )
        .unwrap();
    if saved {
        services.db.set_tf2_path(Some(&env.game)).unwrap();
    }
    let script = scattergun(&env.game);
    std::fs::create_dir_all(script.parent().unwrap()).unwrap();
    std::fs::write(&script, OLD_SCRIPT).unwrap();
}

fn script(env: &support::Env) -> String {
    std::fs::read_to_string(scattergun(&env.game)).unwrap()
}

fn toasts(cx: &mut TestAppContext, win: AnyWindowHandle) -> usize {
    cx.update_window(win, |_, w, cx| toast_count(w, cx))
        .unwrap()
}

fn badge(cx: &mut TestAppContext, win: AnyWindowHandle, text: &str) {
    support::wait_real(cx, "the badge", |cx| {
        support::label_of(cx, win, ids::TF2_BADGE).as_deref() == Some(text)
    });
}

#[gpui_kit::test]
fn startup_repairs_an_old_script_once_with_one_toast(cx: &mut TestAppContext) {
    let env = support::env();
    old_install(&env, true);
    let win = started(cx, boot(&env, support::detection_forbidden()));
    support::wait_real(cx, "the repair toast", |cx| {
        support::label_of(cx, win, ids::TOAST).as_deref() == Some(FIXED)
    });
    assert!(script(&env).contains("\"printname\""));
    assert_eq!(toasts(cx, win), 1);

    // The repair is done for this start: the game coming back again does not apply again.
    std::fs::write(scattergun(&env.game), OLD_SCRIPT).unwrap();
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    cx.update(|cx| {
        let state = tf2_state(cx);
        state.update(cx, |s, cx| {
            *s = Tf2State::Missing {
                saved: Some(env.game.clone()),
            };
            cx.notify();
        });
        state.update(cx, |s, cx| {
            *s = Tf2State::Found(tf2);
            cx.notify();
        });
    });
    support::settle(cx, win);
    assert_eq!(script(&env), OLD_SCRIPT);
    assert_eq!(toasts(cx, win), 1);
}

#[gpui_kit::test]
fn full_scripts_start_without_a_repair(cx: &mut TestAppContext) {
    let env = support::env();
    old_install(&env, true);
    let services = support::services(&env, support::no_steam());
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    services
        .library
        .apply_crosshairs(&tf2, &services.builtins)
        .unwrap();
    let applied = script(&env);
    let win = started(cx, boot(&env, support::detection_forbidden()));
    badge(cx, win, "TF2 Connected");
    support::settle(cx, win);
    assert_eq!(script(&env), applied);
    assert_eq!(toasts(cx, win), 0);
}

#[gpui_kit::test]
fn no_repair_when_tf2_is_not_found(cx: &mut TestAppContext) {
    let env = support::env();
    old_install(&env, false);
    let win = started(cx, boot(&env, support::no_steam()));
    badge(cx, win, "TF2 Not Found");
    support::settle(cx, win);
    assert_eq!(script(&env), OLD_SCRIPT);
    assert_eq!(toasts(cx, win), 0);
}

#[gpui_kit::test]
fn a_recheck_that_finds_the_game_repairs(cx: &mut TestAppContext) {
    let env = support::env();
    old_install(&env, true);
    let away = env.tmp.path().join("moved away");
    std::fs::rename(&env.game, &away).unwrap();
    let win = started(cx, boot(&env, support::detection_forbidden()));
    badge(cx, win, "TF2 Not Found");
    support::settle(cx, win);
    assert_eq!(toasts(cx, win), 0);

    std::fs::rename(&away, &env.game).unwrap();
    cx.update(recheck_tf2);
    support::wait_real(cx, "the repair toast", |cx| {
        support::label_of(cx, win, ids::TOAST).as_deref() == Some(FIXED)
    });
    assert!(script(&env).contains("\"printname\""));
    assert_eq!(toasts(cx, win), 1);
}
