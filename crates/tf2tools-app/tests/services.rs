use std::sync::{Arc, OnceLock};

use gpui_kit::TestAppContext;
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::local_messages::INTERNAL;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::{
    OpError, Services, Tf2Detector, Tf2State, install_tf2_state, require_tf2, resolve_tf2,
    run_blocking,
};
use tf2tools_core::{Database, Endpoints, Tf2Install};

use crate::support;

#[test]
fn services_are_clone_send_sync() {
    fn check<T: Clone + Send + Sync + 'static>() {}
    check::<Services>();
}

#[test]
fn open_builds_the_store_client_on_the_callers_endpoints() {
    let env = support::env();
    let server = support::StoreServer::start();
    let open = |offline| {
        Services::open(
            env.data.clone(),
            support::repo_asset_root(),
            Arc::new(ScriptedPicker::empty()),
            support::no_steam(),
            Arc::new(RecordingAudio::new()),
            Endpoints::loopback(&server.uri()),
            offline,
        )
        .unwrap()
    };
    // The default endpoints refuse a loopback image, so a fetch proves the client uses ours.
    let rt = tokio::runtime::Runtime::new().unwrap();
    for (offline, name) in [(true, "a.jpg"), (false, "b.jpg")] {
        let store = open(offline).store;
        assert_eq!(store.is_loopback_only(), offline);
        let url = server.mount_image(name, b"pixels".to_vec());
        assert_eq!(rt.block_on(store.fetch_image(&url)).unwrap(), b"pixels");
    }
}

#[test]
fn a_saved_valid_path_is_found_without_detection() {
    let env = support::env();
    let s = support::services(&env, support::detection_forbidden());
    s.db.set_tf2_path(Some(&env.game)).unwrap();
    assert_eq!(
        resolve_tf2(&s),
        (
            Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
            None
        )
    );
}

#[test]
fn a_saved_path_that_is_no_longer_tf2_is_missing_and_kept() {
    let env = support::env();
    let gone = env.tmp.path().join("gone");
    let s = support::services(&env, support::no_steam());
    s.db.set_tf2_path(Some(&gone)).unwrap();
    assert_eq!(
        resolve_tf2(&s),
        (
            Tf2State::Missing {
                saved: Some(gone.clone())
            },
            None
        )
    );
    assert_eq!(s.db.settings().unwrap().tf2_path(), Some(gone.as_path()));
}

#[test]
fn with_nothing_saved_a_detection_hit_is_saved() {
    let env = support::env();
    let s = support::services(&env, support::steam_finds(env.game.clone()));
    let (state, error) = resolve_tf2(&s);
    assert_eq!(error, None);
    assert_eq!(
        state.install().map(|i| i.game_dir()),
        Some(env.game.as_path())
    );
    assert_eq!(
        s.db.settings().unwrap().tf2_path(),
        Some(env.game.as_path())
    );
}

#[test]
fn a_detection_miss_leaves_the_path_unset() {
    let env = support::env();
    let s = support::services(&env, support::no_steam());
    assert_eq!(resolve_tf2(&s), (Tf2State::Missing { saved: None }, None));
    assert_eq!(s.db.settings().unwrap().tf2_path(), None);
}

#[test]
fn a_detection_error_counts_as_a_miss() {
    let env = support::env();
    let s = support::services(&env, support::steam_finds(env.tmp.path().join("not-tf2")));
    assert_eq!(resolve_tf2(&s), (Tf2State::Missing { saved: None }, None));
}

#[test]
fn a_detection_hit_survives_a_failed_save() {
    let env = support::env();
    let s = support::services(&env, support::steam_finds(env.game.clone()));
    // A trigger refusing every settings write stands in for a read-only database.
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER refuse_insert BEFORE INSERT ON settings BEGIN SELECT RAISE(ABORT, 'read-only'); END;
         CREATE TRIGGER refuse_update BEFORE UPDATE ON settings BEGIN SELECT RAISE(ABORT, 'read-only'); END;",
    )
    .unwrap();
    let (state, error) = resolve_tf2(&s);
    assert_eq!(
        state,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
    );
    assert_eq!(
        error.as_ref().map(OpError::description),
        Some("TF2Tools couldn't read or save its settings. Restart TF2Tools and try again.")
    );
    assert!(matches!(
        error,
        Some(OpError::Core {
            action: "Save TF2 path",
            ..
        })
    ));
}

#[test]
fn op_errors_read_as_a_toast_title_and_description() {
    assert_eq!(OpError::NoTf2.title(), "TF2 not found");
    assert_eq!(
        OpError::NoTf2.description(),
        "Set the TF2 folder in Settings, then try again."
    );
    assert_eq!(
        OpError::NoTf2.to_string(),
        "TF2 not found: Set the TF2 folder in Settings, then try again."
    );
    let e = OpError::core("Install HUD", "disk full");
    assert_eq!((e.title(), e.description()), ("Install HUD", "disk full"));
    assert_eq!(e.to_string(), "Install HUD: disk full");
    let p = OpError::Internal("boom".into());
    assert_eq!(
        (p.title(), p.description()),
        ("Something went wrong", INTERNAL)
    );
    assert_eq!(p.to_string(), "Something went wrong: boom");
}

#[gpui_kit::test]
async fn run_blocking_returns_the_value(cx: &mut TestAppContext) {
    support::init(cx);
    let env = support::env();
    support::install_services(cx, support::services(&env, support::no_steam()));
    let task = cx.update(|cx| run_blocking(cx, |s| Ok(s.db.schema_version())));
    assert_eq!(task.await, Ok(17));
}

#[gpui_kit::test]
async fn run_blocking_passes_errors_through(cx: &mut TestAppContext) {
    support::init(cx);
    let env = support::env();
    support::install_services(cx, support::services(&env, support::no_steam()));
    let task = cx.update(|cx| run_blocking(cx, |_| Err::<(), _>(OpError::NoTf2)));
    assert_eq!(task.await, Err(OpError::NoTf2));
}

#[gpui_kit::test]
async fn a_panic_becomes_an_internal_error(cx: &mut TestAppContext) {
    support::init(cx);
    let env = support::env();
    support::install_services(cx, support::services(&env, support::no_steam()));
    let task = cx.update(|cx| run_blocking(cx, |_| -> Result<(), OpError> { panic!("boom") }));
    match task.await {
        Err(OpError::Internal(message)) => assert!(message.contains("boom"), "{message}"),
        other => panic!("expected Internal, got {other:?}"),
    }
}

#[gpui_kit::test]
fn require_tf2_follows_the_state(cx: &mut TestAppContext) {
    support::init(cx);
    let env = support::env();
    let state = cx.update(|cx| install_tf2_state(cx, Tf2State::Detecting));
    cx.update(|cx| assert_eq!(require_tf2(cx), Err(OpError::NoTf2)));
    let found = Tf2Install::from_path(&env.game).unwrap();
    cx.update(|cx| {
        state.update(cx, |s, cx| {
            *s = Tf2State::Found(found.clone());
            cx.notify();
        })
    });
    cx.update(|cx| assert_eq!(require_tf2(cx), Ok(found.clone())));
    cx.update(|cx| state.update(cx, |s, _| *s = Tf2State::Missing { saved: None }));
    cx.update(|cx| assert_eq!(require_tf2(cx), Err(OpError::NoTf2)));
}

#[test]
fn startup_saves_the_first_of_several_detected_installs() {
    let env = support::env();
    let second = env.tmp.path().join("Flatpak TF2");
    std::fs::create_dir_all(second.join("tf/custom")).unwrap();
    let services = support::services(
        &env,
        support::steam_finds_all(vec![second.clone(), env.game.clone()]),
    );
    let (state, error) = resolve_tf2(&services);
    assert!(error.is_none());
    let Tf2State::Found(install) = &state else {
        panic!("expected Found, got {state:?}")
    };
    assert_eq!(install.game_dir(), second);
    assert_eq!(
        services.db.settings().unwrap().tf2_path(),
        Some(second.as_path())
    );
}

#[test]
fn a_path_saved_during_detection_wins() {
    let env = support::env();
    let detected = env.tmp.path().join("Detected TF2");
    std::fs::create_dir_all(detected.join("tf/custom")).unwrap();
    let user = env.game.clone();
    // The detector stands in for a slow Steam scan: the user's Save lands while it runs.
    let db: Arc<OnceLock<Database>> = Arc::new(OnceLock::new());
    let detector: Tf2Detector = {
        let (db, user, detected) = (Arc::clone(&db), user.clone(), detected.clone());
        Arc::new(move || {
            db.get().unwrap().set_tf2_path(Some(&user)).unwrap();
            Tf2Install::from_path(&detected).map(|install| vec![install])
        })
    };
    let services = support::services(&env, detector);
    assert!(db.set(services.db.clone()).is_ok());
    let (state, error) = resolve_tf2(&services);
    assert!(error.is_none());
    assert_eq!(
        services.db.settings().unwrap().tf2_path(),
        Some(user.as_path())
    );
    assert_eq!(
        state,
        Tf2State::Found(Tf2Install::from_path(&user).unwrap())
    );
}

#[test]
fn services_carry_the_audio_out() {
    let env = support::env();
    let audio = Arc::new(RecordingAudio::new());
    let s = support::services_with_audio(&env, Arc::clone(&audio));
    let _playback = s.audio.play(vec![1], 0.5);
    assert_eq!(audio.plays(), vec![(vec![1], 0.5)]);
}

#[test]
fn opening_the_services_retires_override_addons_from_the_library() {
    let env = support::env();
    let addons = env.data.library_dir(tf2tools_core::LibraryKind::Addons);
    std::fs::create_dir_all(&addons).unwrap();
    std::fs::write(addons.join("mastercomfig-addon-override-1.vpk"), b"o").unwrap();
    std::fs::write(addons.join("mastercomfig-addon-flat-mouse.vpk"), b"a").unwrap();
    let _services = support::services(&env, support::no_steam());
    assert!(!addons.join("mastercomfig-addon-override-1.vpk").exists());
    assert!(addons.join("mastercomfig-addon-flat-mouse.vpk").exists());
}
