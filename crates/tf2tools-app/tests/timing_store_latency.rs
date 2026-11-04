mod support;

use std::time::{Duration, Instant};

use gpui_kit::TestAppContext;
use support::note;
use tf2tools_app::store_grid::SEARCH_DEBOUNCE;
use tf2tools_app::timing::Case;
use tf2tools_app::timing::cases::{self, CaseConfig};

fn sounds(n: i64) -> impl FnOnce(&support::Env) {
    move |env| support::seed_n_sounds(env, n)
}

#[gpui_kit::test]
fn nav_reports_one_sample_per_visit(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _runs) = support::store_shell_window(cx, &env, sounds(120));
    let config = CaseConfig {
        intervals: 4,
        ..CaseConfig::default()
    };
    let report = support::run_frame_case(cx, &s, Case::SoundStoreNav, config).unwrap();
    assert_eq!((report.actions, report.stats.count), (4, 4), "{report:?}");
    assert!(note(&report, "max_frames").parse::<usize>().unwrap() >= 1);
}

#[gpui_kit::test]
fn search_reports_one_sample_per_search(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _runs) = support::store_shell_window(cx, &env, sounds(120));
    let config = CaseConfig {
        intervals: 4,
        ..CaseConfig::default()
    };
    let report = support::run_frame_case_ticking(cx, &s, Case::SoundStoreSearch, config).unwrap();
    assert_eq!((report.actions, report.stats.count), (4, 4), "{report:?}");
    let page = cx.update(|cx| s.shell.read(cx).sound_store_page()).unwrap();
    let firsts = cx.update(|cx| page.read(cx).grid().read(cx).first_pages());
    assert_eq!(firsts, 5, "the open plus four searches");
}

#[gpui_kit::test]
fn search_reports_each_term_apart(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _runs) = support::store_shell_window(cx, &env, sounds(120));
    let config = CaseConfig {
        intervals: 8,
        ..CaseConfig::default()
    };
    let report = support::run_frame_case_ticking(cx, &s, Case::SoundStoreSearch, config).unwrap();
    // Two samples per term, each term's numbers under its own keys.
    for term in ["zq", "sound_1", "sound_7", "reset"] {
        for stat in ["median_ms", "p95_ms", "max_ms"] {
            let key = format!("search_{term}_{stat}");
            let ms: f64 = note(&report, &key).parse().unwrap();
            assert!(ms > 0.0 && ms <= report.stats.max_ms, "{key} in {report:?}");
        }
        assert_eq!(note(&report, &format!("search_{term}_n")), "2");
    }
}

#[gpui_kit::test]
fn walk_passes_2000_cards_and_reports_memory(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _runs) = support::store_shell_window(cx, &env, sounds(2_200));
    let config = CaseConfig {
        scroll_step: 2_000.0,
        ..CaseConfig::default()
    };
    let report = support::run_frame_case(cx, &s, Case::SoundStoreWalk, config).unwrap();
    assert!(
        note(&report, "passed_cards").parse::<usize>().unwrap() >= 2_000,
        "{report:?}"
    );
    // Memory comes from /proc, which only Linux has.
    #[cfg(target_os = "linux")]
    assert!(note(&report, "rss_kb").parse::<u64>().unwrap() > 0);
    assert_eq!(note(&report, "foreground_decodes"), "0");
}

// Memory comes from /proc, which only Linux has.
#[cfg(target_os = "linux")]
#[test]
fn memory_reads_this_process() {
    let m = tf2tools_app::timing::memory().expect("Linux /proc");
    assert!(m.rss_kb > 0 && m.pss_kb > 0, "{}/{}", m.rss_kb, m.pss_kb);
}

#[test]
fn a_search_sample_starts_at_the_query_start() {
    let acted = Instant::now();
    // Neither the act nor the act plus the debounce, so only the query start matches it.
    let query = acted + SEARCH_DEBOUNCE + Duration::from_millis(250);
    let start = |case, query| cases::sample_start(case, acted, query);
    assert_eq!(start(Case::SoundStoreSearch, Some(query)), query);
    assert_eq!(start(Case::SoundStoreNav, Some(query)), acted);
    assert_eq!(start(Case::SoundStoreSearch, None), acted);
}
