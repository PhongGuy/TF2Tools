mod support;

use gpui_kit::TestAppContext;
use tf2tools_app::pages::hud_store::HudStoreTab;
use tf2tools_app::remote_images::decode::decode_image;
use tf2tools_app::remote_images::queue::IMAGE_BUDGET;
use tf2tools_app::remote_images::{ImageKey, RemoteImages};
use tf2tools_app::timing::cases::{self, CaseConfig, CaseReport};
use tf2tools_app::timing::{Case, FrameStats};

fn quick() -> CaseConfig {
    CaseConfig {
        intervals: 60,
        scroll_step: 120.0,
        ..CaseConfig::default()
    }
}

/// The image grids end with the image cache's decoded bytes and foreground decodes.
fn assert_image_notes(report: &CaseReport) {
    support::note(report, "image_bytes").parse::<u64>().unwrap();
    support::note(report, "foreground_decodes")
        .parse::<u64>()
        .unwrap();
}

#[gpui_kit::test]
fn sound_store_scroll_pages_while_it_scrolls(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _runs) = support::store_shell_window(cx, &env, |env| support::seed_n_sounds(env, 600));
    let report = support::run_frame_case(cx, &s, Case::SoundStoreScroll, quick()).unwrap();
    assert_eq!(report.actions, 60);
    assert!(report.max_scroll_px > 0.0, "{report:?}");
    assert!(
        support::note(&report, "cards").parse::<usize>().unwrap() > 60,
        "{report:?}"
    );
}

#[gpui_kit::test]
fn gamebanana_scroll_runs_on_the_gamebanana_tab(cx: &mut TestAppContext) {
    let env = support::env();
    let names: Vec<(i64, String)> = (1..=200).map(|i| (i, format!("Hud {i}"))).collect();
    let (s, _runs) = support::store_shell_window(cx, &env, |env| {
        let rows: Vec<(i64, &str)> = names.iter().map(|(i, n)| (*i, n.as_str())).collect();
        support::seed_gb_huds(env, &rows);
    });
    let report = support::run_frame_case(cx, &s, Case::GameBananaScroll, quick()).unwrap();
    assert!(report.max_scroll_px > 0.0, "{report:?}");
    assert_image_notes(&report);
    let page = cx.update(|cx| s.shell.read(cx).hud_store_page()).unwrap();
    assert_eq!(cx.update(|cx| page.read(cx).tab()), HudStoreTab::GameBanana);
}

#[gpui_kit::test]
fn curated_scroll_moves_the_curated_grid(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _runs) = support::store_shell_window(cx, &env, |env| {
        for i in 0..60 {
            let id = format!("hud{i:02}");
            support::seed_curated(
                env,
                &id,
                &id,
                "vex",
                None,
                r#"{"traits":{"os":["windows","mac","linux"]}}"#,
            );
        }
    });
    let report = support::run_frame_case(cx, &s, Case::CuratedScroll, quick()).unwrap();
    assert!(report.max_scroll_px > 0.0, "{report:?}");
    assert_image_notes(&report);
}

#[test]
fn store_cases_are_skipped_by_all() {
    assert!(Case::SoundStoreScroll.needs_store() && Case::CuratedScroll.needs_store());
    assert!(!Case::HudGridScroll.needs_store() && !Case::Idle.needs_store());
}

#[test]
fn notes_follow_the_standard_fields() {
    let stats = FrameStats::from_intervals(&[std::time::Duration::from_millis(16)]).unwrap();
    let report = CaseReport {
        stats,
        actions: 1,
        max_scroll_px: 0.0,
        notes: vec![("cards", "3".into())],
    };
    let line = cases::report_line(Case::SoundStoreScroll, &Ok(report));
    assert!(
        line.starts_with("frames case=sound-store-scroll "),
        "{line}"
    );
    assert!(line.ends_with(" max_scroll_px=0 cards=3"), "{line}");
}

#[gpui_kit::test]
fn the_image_notes_read_the_image_cache_stats(cx: &mut TestAppContext) {
    support::init(cx);
    // Fetches cross the tokio runtime, so the test executor must be allowed to park.
    cx.executor().allow_parking();
    let images = cx.update(|cx| {
        RemoteImages::install_with(cx, support::thumbnail_fetch(), IMAGE_BUDGET, decode_image)
    });
    let keys = (1..=3).map(|i| ImageKey::card(format!("u/{i}"))).collect();
    cx.update(|cx| images.update(cx, |i, cx| i.want("grid", keys, cx)));
    support::wait_real(cx, "three decodes", |cx| {
        cx.update(|cx| images.read(cx).stats().decodes == 3)
    });
    let stats = cx.update(|cx| images.read(cx).stats());
    assert!(stats.bytes > 0, "{stats:?}");
    assert_eq!(
        cx.update(|cx| cases::image_notes(cx)),
        [
            ("image_bytes", stats.bytes.to_string()),
            ("foreground_decodes", stats.foreground_decodes.to_string()),
        ]
    );
}
