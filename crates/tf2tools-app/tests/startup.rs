use std::cell::RefCell;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, px, size};
use tf2tools_app::app::{Boot, detect_tf2, start};
use tf2tools_app::assets::AssetRoot;
use tf2tools_app::crash::{CRASH_FILE, PREVIOUS_FILE, crash_dir};
use tf2tools_app::ids;
use tf2tools_app::pages::huds::HudsPage;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::{Tf2Detector, Tf2State, tf2_state};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{Database, Provenance, Tf2Install};

use tf2tools_app::audio::RecordingAudio;

use crate::support;

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

#[gpui_kit::test]
fn the_loading_view_comes_first_then_the_shell(cx: &mut TestAppContext) {
    let env = support::env();
    let win = started(cx, boot(&env, support::no_steam()));
    // Before any background work runs, the window already shows v5's loading view.
    let loading = cx
        .update_window(win, |_, w, cx| {
            w.render_frame(cx);
            w.try_find(ids::STARTUP_LOADING).is_some()
        })
        .unwrap();
    assert!(loading);
    support::wait_real(cx, "the shell", |cx| {
        support::present(cx, win, ids::page(Page::Huds))
    });
    assert!(!support::present(cx, win, ids::STARTUP_LOADING));
    let viewport = cx.update_window(win, |_, w, _| w.viewport_size()).unwrap();
    assert_eq!(viewport, size(px(1200.), px(900.)));
}

#[gpui_kit::test]
fn a_saved_valid_path_shows_tf2_connected(cx: &mut TestAppContext) {
    let env = support::env();
    support::services(&env, support::no_steam())
        .db
        .set_tf2_path(Some(&env.game))
        .unwrap();
    let win = started(cx, boot(&env, support::detection_forbidden()));
    support::wait_real(cx, "the badge", |cx| {
        support::label_of(cx, win, ids::TF2_BADGE).as_deref() == Some("TF2 Connected")
    });
}

#[gpui_kit::test]
fn a_detection_miss_shows_tf2_not_found_without_a_toast(cx: &mut TestAppContext) {
    let env = support::env();
    let win = started(cx, boot(&env, support::no_steam()));
    support::wait_real(cx, "the badge", |cx| {
        support::label_of(cx, win, ids::TF2_BADGE).as_deref() == Some("TF2 Not Found")
    });
    assert!(!support::present(cx, win, ids::TOAST));
}

#[gpui_kit::test]
fn a_detection_hit_connects_and_saves_the_path(cx: &mut TestAppContext) {
    let env = support::env();
    let win = started(cx, boot(&env, support::steam_finds(env.game.clone())));
    support::wait_real(cx, "the badge", |cx| {
        support::label_of(cx, win, ids::TF2_BADGE).as_deref() == Some("TF2 Connected")
    });
    let db = Database::open(&env.data).unwrap();
    assert_eq!(db.settings().unwrap().tf2_path(), Some(env.game.as_path()));
}

#[gpui_kit::test]
fn no_asset_root_says_to_reinstall(cx: &mut TestAppContext) {
    let env = support::env();
    let nowhere = env.tmp.path().join("nowhere");
    let mut b = boot(&env, support::no_steam());
    b.assets = Err(AssetRoot::resolve_from(Some(nowhere), None, None).unwrap_err());
    let win = started(cx, b);
    support::wait_real(cx, "the error view", |cx| {
        support::present(cx, win, ids::STARTUP_ERROR)
    });
    let text = support::label_of(cx, win, ids::STARTUP_ERROR).unwrap();
    assert_eq!(
        text,
        "TF2Tools is missing some of its own files. Reinstall TF2Tools from the release page."
    );
    assert!(!support::present(cx, win, ids::SIDEBAR));
}

#[gpui_kit::test]
fn a_corrupt_database_reads_plainly_and_is_left_untouched(cx: &mut TestAppContext) {
    let env = support::env();
    std::fs::create_dir_all(env.data.root()).unwrap();
    let garbage = vec![0x5a_u8; 8192];
    std::fs::write(env.data.db_path(), &garbage).unwrap();
    let win = started(cx, boot(&env, support::no_steam()));
    support::wait_real(cx, "the error view", |cx| {
        support::present(cx, win, ids::STARTUP_ERROR)
    });
    let text = support::label_of(cx, win, ids::STARTUP_ERROR).unwrap();
    assert_eq!(
        text,
        "TF2Tools couldn't open your saved library. Restart your computer and try again. If it keeps happening, ask for help on the TF2Tools GitHub page."
    );
    assert_eq!(std::fs::read(env.data.db_path()).unwrap(), garbage);
}

#[gpui_kit::test]
fn a_newer_database_says_to_update_and_is_left_untouched(cx: &mut TestAppContext) {
    let env = support::env();
    drop(Database::open(&env.data).unwrap());
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    drop(conn);
    let win = started(cx, boot(&env, support::no_steam()));
    support::wait_real(cx, "the error view", |cx| {
        support::present(cx, win, ids::STARTUP_ERROR)
    });
    let text = support::label_of(cx, win, ids::STARTUP_ERROR).unwrap();
    assert_eq!(
        text,
        "Your library was saved by a newer TF2Tools. Install the latest TF2Tools from the release page to open it."
    );
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 99);
}

#[gpui_kit::test]
fn a_save_during_detection_keeps_the_saved_state(cx: &mut TestAppContext) {
    let env = support::env();
    let detected = env.tmp.path().join("Detected TF2");
    std::fs::create_dir_all(detected.join("tf/custom")).unwrap();
    let s = support::shell_window_with(
        cx,
        &env,
        Tf2State::Detecting,
        Arc::new(ScriptedPicker::empty()),
        support::steam_finds(detected),
    );
    let user = Tf2Install::from_path(&env.game).unwrap();
    // Detection starts, then Settings' Save sets the state before the detector's result is back.
    cx.update(|cx| {
        detect_tf2(s.win, cx);
        s.tf2.update(cx, |tf2, cx| {
            *tf2 = Tf2State::Found(user.clone());
            cx.notify();
        });
    });
    support::settle(cx, s.win);
    assert_eq!(
        cx.update(|cx| s.tf2.read(cx).clone()),
        Tf2State::Found(user)
    );
}

/// A minimal HUD archive, `<name>/info.vdf` plus a layout file, written into `dir`.
fn hud_zip(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{name}/info.vdf"), options).unwrap();
    write!(zip, "\"{name}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n").unwrap();
    zip.start_file(format!("{name}/resource/ui/hudlayout.res"), options)
        .unwrap();
    write!(zip, "\"Resource/HudLayout.res\"\n{{\n}}\n").unwrap();
    zip.finish().unwrap();
    path
}

#[gpui_kit::test]
fn the_first_huds_page_is_born_with_the_library_loaded(cx: &mut TestAppContext) {
    let env = support::env();
    let services = support::services(&env, support::no_steam());
    let archive = hud_zip(env.tmp.path(), "budhud");
    services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    drop(services);
    support::init(cx);
    // Record each HUDs page as it is created, before any of its own loads can land.
    let born = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&born);
    let _watch = cx.update(|cx| {
        cx.observe_new(move |page: &mut HudsPage, _, _| {
            seen.borrow_mut().push((page.is_loaded(), page.hud_count()));
        })
    });
    let win = cx
        .update(|cx| start(boot(&env, support::no_steam()), cx))
        .unwrap();
    support::wait_real(cx, "the shell", |cx| {
        support::present(cx, win, ids::page(Page::Huds))
    });
    assert_eq!(born.borrow().first(), Some(&(true, 1)));
}

#[gpui_kit::test]
fn a_failed_preload_falls_back_to_the_pages_own_load(cx: &mut TestAppContext) {
    let env = support::env();
    drop(support::services(&env, support::no_steam()));
    // Without its table the HUD list fails while the database itself still opens.
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    conn.execute_batch("DROP TABLE library_huds").unwrap();
    drop(conn);
    support::init(cx);
    let pages: Rc<RefCell<Vec<Entity<HudsPage>>>> = Rc::default();
    let seen = Rc::clone(&pages);
    let _watch = cx.update(|cx| {
        cx.observe_new(move |_: &mut HudsPage, _, cx| seen.borrow_mut().push(cx.entity()))
    });
    let win = cx
        .update(|cx| start(boot(&env, support::no_steam()), cx))
        .unwrap();
    support::wait_real(cx, "the load error toast", |cx| {
        support::toast_labels(cx, win)
            .iter()
            .any(|t| t.starts_with("Failed to load HUDs"))
    });
    support::wait_real(cx, "detection and every load to settle", |cx| {
        cx.update(|cx| {
            !matches!(*tf2_state(cx).read(cx), Tf2State::Detecting)
                && pages.borrow().iter().all(|p| p.read(cx).is_idle())
        })
    });
    let failures = support::toast_labels(cx, win)
        .iter()
        .filter(|t| t.starts_with("Failed to load HUDs"))
        .count();
    assert_eq!(failures, 1, "one toast for one broken library");
}

#[gpui_kit::test]
fn a_preloaded_start_still_shows_the_installed_hud(cx: &mut TestAppContext) {
    let env = support::env();
    let services = support::services(&env, support::no_steam());
    let archive = hud_zip(env.tmp.path(), "rayshud");
    let hud = services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    services.db.set_tf2_path(Some(&env.game)).unwrap();
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    services.library.install_hud(hud.id(), &tf2).unwrap();
    drop(services);
    let win = started(cx, boot(&env, support::detection_forbidden()));
    support::wait_real(cx, "the installed HUD", |cx| {
        support::label_of(cx, win, ids::HUDS_INSTALLED_NAME).as_deref() == Some("rayshud")
    });
}

#[gpui_kit::test]
fn startup_fonts_cover_the_weights_the_first_frame_draws(cx: &mut TestAppContext) {
    support::init(cx);
    cx.update(|cx| {
        // A family other than the bundled one proves the prewarm follows the theme.
        gpui_kit::component::Theme::global_mut(cx).font_family = "Prewarm Probe".into();
        let family = gpui_kit::component::Theme::global(cx).font_family.clone();
        let fonts = tf2tools_app::app::startup_fonts(cx);
        let weights: Vec<_> = fonts.iter().map(|f| f.weight).collect();
        assert_eq!(
            weights,
            [
                gpui_kit::FontWeight::NORMAL,
                gpui_kit::FontWeight::MEDIUM,
                gpui_kit::FontWeight::SEMIBOLD,
                gpui_kit::FontWeight::BOLD,
            ]
        );
        assert!(fonts.iter().all(|f| f.family == family));
        assert!(fonts.iter().all(|f| f.style == gpui_kit::FontStyle::Normal));
    });
}

#[gpui_kit::test]
fn the_boot_font_host_reaches_the_services(cx: &mut TestAppContext) {
    let env = support::env();
    let host: Arc<dyn tf2tools_core::FontHost> =
        Arc::new(support::fonts::FakeFontHost::new(&[], env.tmp.path()));
    let boot = Boot {
        font_host: Some(host),
        updates: tf2tools_app::updater::UpdateBoot::Off,
        ..boot(&env, support::no_steam())
    };
    let win = started(cx, boot);
    support::wait_real(cx, "the shell", |cx| {
        support::present(cx, win, ids::page(Page::Huds))
    });
    let has_host = cx.update(|cx| {
        cx.global::<tf2tools_app::services::Services>()
            .font_host
            .is_some()
    });
    assert!(has_host);
}

#[gpui_kit::test]
fn a_crash_last_time_shows_the_banner_and_moves_the_file_aside(cx: &mut TestAppContext) {
    let env = support::env();
    let dir = crash_dir(env.data.root());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(CRASH_FILE), "Message: boom\n").unwrap();
    let win = started(cx, boot(&env, support::no_steam()));
    support::wait_real(cx, "the crash banner", |cx| {
        support::present(cx, win, ids::CRASH_BANNER)
    });
    assert!(!dir.join(CRASH_FILE).exists());
    assert_eq!(
        std::fs::read_to_string(dir.join(PREVIOUS_FILE)).unwrap(),
        "Message: boom\n"
    );
}

#[gpui_kit::test]
fn a_clean_start_shows_no_crash_banner(cx: &mut TestAppContext) {
    let env = support::env();
    let win = started(cx, boot(&env, support::no_steam()));
    support::wait_real(cx, "the shell", |cx| {
        support::present(cx, win, ids::page(Page::Huds))
    });
    assert!(!support::present(cx, win, ids::CRASH_BANNER));
    assert!(!crash_dir(env.data.root()).exists(), "nothing is created");
}

// A window closed before the opening task lands never shows the banner, so the file must stay.
#[gpui_kit::test]
fn a_window_closed_during_startup_keeps_the_crash_for_next_time(cx: &mut TestAppContext) {
    let env = support::env();
    let dir = crash_dir(env.data.root());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(CRASH_FILE), "Message: boom\n").unwrap();
    let win = started(cx, boot(&env, support::no_steam()));
    cx.update_window(win, |_, window, _| window.remove_window())
        .unwrap();
    // The opening task creates the library database in its single poll; once it exists,
    // the task has finished and its result has met the closed window.
    support::wait_real(cx, "the opening task", |_| env.data.db_path().exists());
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(dir.join(CRASH_FILE)).unwrap(),
        "Message: boom\n"
    );
    assert!(!dir.join(PREVIOUS_FILE).exists());
}

#[gpui_kit::test]
fn startup_puts_back_a_hud_an_interrupted_reinstall_set_aside(cx: &mut TestAppContext) {
    let env = support::env();
    let services = support::services(&env, support::no_steam());
    let archive = hud_zip(env.tmp.path(), "rayshud");
    let hud = services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    services.db.set_tf2_path(Some(&env.game)).unwrap();
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    services.library.install_hud(hud.id(), &tf2).unwrap();
    drop(services);
    // A crash between the reinstall's two renames: the HUD sits aside, nothing in its place.
    let custom = env.game.join("tf/custom");
    let staging = custom.join(".tf2tools-reinstall");
    std::fs::create_dir_all(staging.join("old")).unwrap();
    std::fs::write(staging.join("swapping"), "rayshud").unwrap();
    std::fs::rename(custom.join("rayshud"), staging.join("old/rayshud")).unwrap();

    let win = started(cx, boot(&env, support::detection_forbidden()));
    // Detection, and the recovery with it, runs after the shell's first frame.
    support::wait_real(cx, "the badge", |cx| {
        support::label_of(cx, win, ids::TF2_BADGE).as_deref() == Some("TF2 Connected")
    });
    support::wait_real(cx, "the HUD put back", |_| {
        custom.join("rayshud/info.vdf").is_file() && !staging.exists()
    });
    support::wait_real(cx, "the installed HUD", |cx| {
        support::label_of(cx, win, ids::HUDS_INSTALLED_NAME).as_deref() == Some("rayshud")
    });
}
