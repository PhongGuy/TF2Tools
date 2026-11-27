//! The Demo Viewer's bar, with a map loaded, in the languages with its longest hint, on the
//! narrowest page (an 800x600 window) and the default 944 px one.

use gpui_kit::{TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::shell::route::Page;

use crate::LocaleGuard;
use crate::support;
use support::demo_viewer::{inside, open_with_tracks};

const ENGLISH_HINT: &str =
    "Drag to look, WASD to move, Shift to slow down, Space to follow a player.";

fn bar_fits(lang: &str, width: f32, height: f32, cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let w = open_with_tracks(cx, &env, None);
    cx.simulate_window_resize(w.s.win, size(px(width), px(height)));
    support::settle(cx, w.s.win);
    // The hint must be the translated one, or the widths below prove nothing about it.
    let hint = support::label_of(cx, w.s.win, ids::DEMO_VIEWER_HINT);
    assert!(
        hint.is_some_and(|h| h != ENGLISH_HINT),
        "{lang}: the hint is not translated"
    );
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET_NEXT);
    let page = support::bounds_of(cx, w.s.win, ids::page(Page::DemosTf)).unwrap();
    let bar = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_BAR).unwrap();
    assert!(inside(bar, page), "{lang} {width}: {bar:?} leaves {page:?}");
    for id in [
        ids::DEMO_VIEWER_HINT,
        ids::DEMO_VIEWER_OPEN,
        ids::DEMO_VIEWER_BAR_COLLAPSE,
        ids::DEMO_VIEWER_PLAY,
        ids::DEMO_VIEWER_STEP_BACK,
        ids::DEMO_VIEWER_STEP_FORWARD,
        ids::DEMO_VIEWER_TIMELINE,
        ids::DEMO_VIEWER_TIME,
        ids::DEMO_VIEWER_SPEED,
        ids::DEMO_VIEWER_CAMERA_MODE,
        ids::DEMO_VIEWER_CAMERA_TARGET_PREV,
        ids::DEMO_VIEWER_CAMERA_TARGET_NEXT,
        ids::DEMO_VIEWER_CAMERA_TARGET,
    ] {
        let part = support::bounds_of(cx, w.s.win, id).unwrap();
        assert!(
            inside(part, bar) && inside(part, page),
            "{lang} {width}: {id} at {part:?} leaves the bar {bar:?} or the page {page:?}"
        );
    }
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_narrowest_page_in_german(cx: &mut TestAppContext) {
    bar_fits("de", 800., 600., cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_narrowest_page_in_russian(cx: &mut TestAppContext) {
    bar_fits("ru", 800., 600., cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_narrowest_page_in_brazilian_portuguese(cx: &mut TestAppContext) {
    bar_fits("pt-BR", 800., 600., cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_default_page_in_german(cx: &mut TestAppContext) {
    bar_fits("de", 1200., 900., cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_default_page_in_russian(cx: &mut TestAppContext) {
    bar_fits("ru", 1200., 900., cx);
}

// The GameBanana fallback's lines exist in every language, keep their placeholders, and
// serveme.tf's old dead-end line is gone.
#[test]
fn the_gamebanana_fallback_speaks_every_language() {
    let text = |lang: &str, key: &str| {
        tf2tools_app::_rust_i18n_try_translate(lang, key).map(|t| t.into_owned())
    };
    let langs = ["en", "de", "es", "fr", "pt-BR", "ru", "zh-CN"];
    for key in [
        "demo_viewer.searching",
        "demo_viewer.searching_candidate",
        "demo_viewer.error.not_on_gamebanana",
        "demo_viewer.error.gb_not_synced",
    ] {
        let en = text("en", key).unwrap_or_else(|| panic!("en lacks {key}"));
        for lang in langs {
            let theirs = text(lang, key).unwrap_or_else(|| panic!("{lang} lacks {key}"));
            assert!(theirs.contains("%{map}"), "{lang} {key}: {theirs}");
            if lang != "en" {
                assert_ne!(theirs, en, "{lang} {key} is untranslated");
            }
        }
    }
    for lang in langs {
        let candidate = text(lang, "demo_viewer.searching_candidate").unwrap();
        assert!(
            candidate.contains("%{n}") && candidate.contains("%{of}"),
            "{lang}: {candidate}"
        );
        assert_eq!(
            text(lang, "demo_viewer.error.not_on_serveme"),
            None,
            "{lang} keeps not_on_serveme"
        );
    }
}

// The playback words exist in every language, keep their placeholders, and are translated.
#[test]
fn the_playback_words_speak_every_language() {
    let text = |lang: &str, key: &str| {
        tf2tools_app::_rust_i18n_try_translate(lang, key).map(|t| t.into_owned())
    };
    let langs = ["en", "de", "es", "fr", "pt-BR", "ru", "zh-CN"];
    let keys = [
        ("play", None),
        ("pause", None),
        ("step_back", None),
        ("step_forward", None),
        ("speed", None),
        ("timeline", None),
        ("round", Some("%{n}")),
        ("reading", Some("%{percent}")),
        ("reading_no_percent", None),
        ("cut_file", Some("%{time}")),
        ("cut_samples", Some("%{time}")),
        ("cut_ended", Some("%{time}")),
        ("tracks_failed", None),
    ];
    for (key, placeholder) in keys {
        let key = format!("demo_viewer.playback.{key}");
        let en = text("en", &key).unwrap_or_else(|| panic!("en lacks {key}"));
        for lang in langs {
            let theirs = text(lang, &key).unwrap_or_else(|| panic!("{lang} lacks {key}"));
            if let Some(p) = placeholder {
                assert!(theirs.contains(p), "{lang} {key}: {theirs}");
            }
            if lang != "en" {
                assert_ne!(theirs, en, "{lang} {key} is untranslated");
            }
        }
    }
}

// The camera chip's words exist in every language, keep their placeholders, and are translated.
#[test]
fn the_camera_words_speak_every_language() {
    let text = |lang: &str, key: &str| {
        tf2tools_app::_rust_i18n_try_translate(lang, key).map(|t| t.into_owned())
    };
    let langs = ["en", "de", "es", "fr", "pt-BR", "ru", "zh-CN"];
    let keys = [
        ("free", None),
        ("first_person", None),
        ("chase", None),
        ("mode", Some("%{mode}")),
        ("prev", None),
        ("next", None),
        ("choose", None),
        ("none", None),
        ("unnamed", Some("%{entity}")),
        ("dead", None),
        ("target_dead", Some("%{name}")),
        ("players", None),
    ];
    for (key, placeholder) in keys {
        let key = format!("demo_viewer.camera.{key}");
        let en = text("en", &key).unwrap_or_else(|| panic!("en lacks {key}"));
        for lang in langs {
            let theirs = text(lang, &key).unwrap_or_else(|| panic!("{lang} lacks {key}"));
            if let Some(p) = placeholder {
                assert!(theirs.contains(p), "{lang} {key}: {theirs}");
            }
            if lang != "en" {
                assert_ne!(theirs, en, "{lang} {key} is untranslated");
            }
        }
    }
    // The new hint names Space, and every language translates it.
    let en_hint = text("en", "demo_viewer.hint").unwrap();
    assert!(en_hint.contains("Space"), "{en_hint}");
    for lang in langs.iter().filter(|l| **l != "en") {
        let hint = text(lang, "demo_viewer.hint").unwrap();
        assert_ne!(hint, en_hint, "{lang} hint is untranslated");
    }
}
