use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use gpui_kit::{AnyWindowHandle, TestAppContext};
use tf2tools_app::app::{Boot, start};
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::ids;
use tf2tools_app::leftovers::{MAX_PART_AGE, freed_entry};
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::{Services, Tf2Detector, Tf2State, tf2_state};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{Freed, LogEntry, LogLevel, NewLogEntry};

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
        locales: Vec::new(),
    }
}

fn started(cx: &mut TestAppContext, boot: Boot) -> AnyWindowHandle {
    support::init(cx);
    let win = cx.update(|cx| start(boot, cx)).unwrap();
    support::wait_real(cx, "the shell", |cx| {
        support::present(cx, win, ids::page(Page::Huds))
    });
    win
}

fn freed_rows(services: &Services) -> Vec<LogEntry> {
    support::log_rows(services)
        .into_iter()
        .filter(|row| row.category() == "system" && row.action() == "remove")
        .collect()
}

fn age(path: &Path, by: Duration) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::now() - by)
        .unwrap();
}

#[test]
fn nothing_freed_logs_nothing() {
    assert_eq!(freed_entry(&Freed::default()), None);
}

#[test]
fn a_sweep_that_freed_files_is_one_system_row_with_its_size_and_paths() {
    let a = PathBuf::from("/data/WebKitCache");
    let b = PathBuf::from("/data/cookies");
    let freed = Freed::new(3 * 1024 * 1024, vec![a.clone(), b.clone()]);
    assert_eq!(
        freed_entry(&freed),
        Some(
            NewLogEntry::new(
                LogLevel::Info,
                "system",
                "remove",
                "Freed 3.0 MB of leftover files"
            )
            .with_details(format!("{}\n{}", a.display(), b.display()))
        )
    );
}

#[test]
fn a_removed_link_alone_still_logs_a_row() {
    let freed = Freed::new(0, vec![PathBuf::from("/data/WebKitCache")]);
    let entry = freed_entry(&freed).unwrap();
    assert_eq!(
        entry,
        NewLogEntry::new(
            LogLevel::Info,
            "system",
            "remove",
            "Freed 0 B of leftover files"
        )
        .with_details("/data/WebKitCache")
    );
}

#[gpui_kit::test]
fn a_start_removes_v5_web_view_data_and_logs_one_row(cx: &mut TestAppContext) {
    let env = support::env();
    let cache = env.data.root().join("WebKitCache");
    std::fs::create_dir_all(cache.join("blobs")).unwrap();
    std::fs::write(cache.join("blobs/0"), vec![1u8; 2048]).unwrap();
    let _win = started(cx, boot(&env, support::no_steam()));
    let services = cx.update(|cx| cx.global::<Services>().clone());
    support::wait_real(cx, "the leftover row", |_| freed_rows(&services).len() == 1);
    assert!(!cache.exists());
    let rows = freed_rows(&services);
    assert_eq!(rows[0].level(), LogLevel::Info);
    assert_eq!(rows[0].message(), "Freed 2.0 KB of leftover files");
    assert_eq!(
        rows[0].details(),
        Some(cache.display().to_string().as_str())
    );
}

#[gpui_kit::test]
fn finding_tf2_removes_an_aged_download_part_from_custom(cx: &mut TestAppContext) {
    let env = support::env();
    let part = env
        .game
        .join("tf")
        .join("custom")
        .join(".tf2tools-AbC123.part");
    std::fs::write(&part, [0u8; 64]).unwrap();
    age(&part, MAX_PART_AGE * 2);
    let _win = started(cx, boot(&env, support::steam_finds(env.game.clone())));
    let services = cx.update(|cx| cx.global::<Services>().clone());
    support::wait_real(cx, "the part to go", |_| !part.exists());
    support::wait_real(cx, "its row", |_| freed_rows(&services).len() == 1);
    assert_eq!(
        freed_rows(&services)[0].details(),
        Some(part.display().to_string().as_str())
    );
}

// The game sweep runs inside detection, so once TF2 reads Found a part
// younger than a day has had its chance to go.
#[gpui_kit::test]
fn finding_tf2_keeps_a_download_part_younger_than_a_day(cx: &mut TestAppContext) {
    let env = support::env();
    let part = env
        .game
        .join("tf")
        .join("custom")
        .join(".tf2tools-AbC123.part");
    std::fs::write(&part, [0u8; 64]).unwrap();
    age(&part, MAX_PART_AGE - Duration::from_secs(60 * 60));
    let _win = started(cx, boot(&env, support::steam_finds(env.game.clone())));
    let services = cx.update(|cx| cx.global::<Services>().clone());
    support::wait_real(cx, "TF2 to be found", |cx| {
        cx.update(|cx| matches!(*tf2_state(cx).read(cx), Tf2State::Found(_)))
    });
    assert!(part.exists());
    assert!(freed_rows(&services).is_empty());
}
