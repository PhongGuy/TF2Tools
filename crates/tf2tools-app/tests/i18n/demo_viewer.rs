//! The Demo Viewer's bar, with a map loaded, in the languages with its longest hint, on the
//! narrowest page (an 800x600 window) and the default 944 px one.

use gpui_kit::{Bounds, Pixels, TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::shell::route::Page;

use crate::LocaleGuard;
use crate::support;
use support::demo_viewer::{
    assert_feed_fits, camera_tracks, inside, killfeed_tracks, open_with, open_with_tracks, overlap,
    seek_to,
};

const ENGLISH_HINT: &str =
    "Drag to look, WASD to move, Shift to slow down, Space to follow a player.";
const ENGLISH_FOLLOW_HINT: &str =
    "Space to change camera, click for the next player, right-click for the previous one.";

// Checks that every part of the bar lies inside it and the page and no two parts cover each
// other, first in free-fly, then following a player, or Bob through his death when `dead`.
fn bar_fits(lang: &str, width: f32, height: f32, dead: bool, cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let w = match dead {
        true => open_with(cx, &env, camera_tracks()),
        false => open_with_tracks(cx, &env, None),
    };
    cx.simulate_window_resize(w.s.win, size(px(width), px(height)));
    support::settle(cx, w.s.win);
    // The hint must be the translated one, or the widths below prove nothing about it.
    let hint = support::label_of(cx, w.s.win, ids::DEMO_VIEWER_HINT);
    assert!(
        hint.as_ref().is_some_and(|h| h != ENGLISH_HINT),
        "{lang}: the hint is not translated"
    );
    let mut parts = vec![
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
    ];
    let page = support::bounds_of(cx, w.s.win, ids::page(Page::DemosTf)).unwrap();
    let fits = |cx: &mut TestAppContext, parts: &[&'static str], phase: &str| {
        let bar = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_BAR).unwrap();
        assert!(
            inside(bar, page),
            "{lang} {width} {phase}: {bar:?} leaves {page:?}"
        );
        let mut seen: Vec<(&str, Bounds<Pixels>)> = Vec::new();
        for &id in parts {
            let part = support::bounds_of(cx, w.s.win, id)
                .unwrap_or_else(|| panic!("{lang} {width} {phase}: {id} is missing"));
            assert!(
                inside(part, bar) && inside(part, page),
                "{lang} {width} {phase}: {id} at {part:?} leaves the bar {bar:?} or the page {page:?}"
            );
            for (other, at) in &seen {
                assert!(
                    !overlap(part, *at),
                    "{lang} {width} {phase}: {id} at {part:?} covers {other} at {at:?}"
                );
            }
            seen.push((id, part));
        }
    };
    fits(cx, &parts, "free");
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET_NEXT);
    let follow = support::label_of(cx, w.s.win, ids::DEMO_VIEWER_HINT);
    assert!(
        follow
            .as_ref()
            .is_some_and(|h| h != ENGLISH_FOLLOW_HINT && h != hint.as_ref().unwrap()),
        "{lang}: the follow hint {follow:?} is not the translated one"
    );
    let width_of = |cx: &mut TestAppContext| {
        support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET)
            .unwrap()
            .size
            .width
    };
    // "Bob" is narrower than 48 px whole, so his dead pass proves the badge takes none of it.
    let alive_width = dead.then(|| {
        support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET_NEXT);
        width_of(cx)
    });
    if dead {
        let viewport = cx.update(|cx| {
            let page = w.s.shell.read(cx).demo_viewer_page(cx).expect("the viewer");
            page.read(cx).viewport().expect("a viewport").clone()
        });
        cx.update(|cx| viewport.update(cx, |v, cx| v.seek(4140., cx)));
        support::settle(cx, w.s.win);
        parts.push(ids::DEMO_VIEWER_CAMERA_DEAD);
    }
    fits(cx, &parts, "following");
    let shown = width_of(cx);
    match alive_width {
        Some(alive) => assert!(
            (shown - alive).abs() < px(0.5),
            "{lang} {width}: the dead badge squeezes the name from {alive:?} to {shown:?}"
        ),
        None => assert!(
            shown >= px(48.),
            "{lang} {width}: the followed name has only {shown:?}"
        ),
    }
    // The players list opens upward from the bar and must stay on the page.
    support::click_id(cx, w.s.win, ids::DEMO_VIEWER_CAMERA_TARGET);
    let list = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_PLAYERS)
        .unwrap_or_else(|| panic!("{lang} {width}: the players list did not open"));
    assert!(
        inside(list, page),
        "{lang} {width}: the players list at {list:?} leaves the page {page:?}"
    );
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_narrowest_page_in_german(cx: &mut TestAppContext) {
    bar_fits("de", 800., 600., false, cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_narrowest_page_in_russian(cx: &mut TestAppContext) {
    bar_fits("ru", 800., 600., false, cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_narrowest_page_in_brazilian_portuguese(cx: &mut TestAppContext) {
    bar_fits("pt-BR", 800., 600., false, cx);
}

#[gpui_kit::test]
fn the_bar_fits_a_dead_target_on_the_narrowest_page_in_german(cx: &mut TestAppContext) {
    bar_fits("de", 800., 600., true, cx);
}

#[gpui_kit::test]
fn the_bar_fits_a_dead_target_on_the_narrowest_page_in_russian(cx: &mut TestAppContext) {
    bar_fits("ru", 800., 600., true, cx);
}

#[gpui_kit::test]
fn the_bar_fits_a_dead_target_on_the_narrowest_page_in_brazilian_portuguese(
    cx: &mut TestAppContext,
) {
    bar_fits("pt-BR", 800., 600., true, cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_default_page_in_german(cx: &mut TestAppContext) {
    bar_fits("de", 1200., 900., false, cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_default_page_in_russian(cx: &mut TestAppContext) {
    bar_fits("ru", 1200., 900., false, cx);
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
    // Following a player has its own hint, which every language translates and tells apart.
    let en_follow = text("en", "demo_viewer.hint_follow").expect("en lacks hint_follow");
    assert!(en_follow.contains("Space"), "{en_follow}");
    for lang in langs {
        let follow = text(lang, "demo_viewer.hint_follow")
            .unwrap_or_else(|| panic!("{lang} lacks hint_follow"));
        let free = text(lang, "demo_viewer.hint").unwrap();
        assert_ne!(follow, free, "{lang} hint_follow repeats the free hint");
        if lang != "en" {
            assert_ne!(follow, en_follow, "{lang} hint_follow is untranslated");
        }
    }
}

// The feed's rows and their cached words hold translated text, so switching language while
// the feed stands still must still rename the unnamed pyro and reword the row.
#[gpui_kit::test]
fn the_kill_feed_renames_an_unnamed_player_when_the_language_changes(cx: &mut TestAppContext) {
    let guard = LocaleGuard::hold();
    let env = support::env();
    let w = open_with(cx, &env, killfeed_tracks());
    seek_to(cx, &w, 4160.);
    let row = |cx: &mut TestAppContext| {
        support::label_of(cx, w.s.win, ids::demo_viewer_kill(3)).unwrap_or_default()
    };
    assert!(row(cx).starts_with("Player 3 "), "{}", row(cx));
    tf2tools_app::i18n::apply("de");
    cx.update(|cx| cx.refresh_windows());
    support::settle(cx, w.s.win);
    let german = row(cx);
    assert!(
        german.starts_with("Spieler 3 ") && !german.contains(" killed "),
        "{german}"
    );
    drop(guard);
}

// The kill feed's words exist in every language, keep their placeholders, and are translated.
#[test]
fn the_killfeed_words_speak_every_language() {
    let text = |lang: &str, key: &str| {
        tf2tools_app::_rust_i18n_try_translate(lang, key).map(|t| t.into_owned())
    };
    let langs = ["en", "de", "es", "fr", "pt-BR", "ru", "zh-CN"];
    let keys: [(&str, &[&str]); 6] = [
        ("title", &[]),
        ("kill", &["%{killer}", "%{victim}", "%{weapon}"]),
        (
            "assisted",
            &["%{killer}", "%{assister}", "%{victim}", "%{weapon}"],
        ),
        ("died", &["%{victim}"]),
        ("unknown_weapon", &[]),
        ("markers", &["%{count}"]),
    ];
    for (key, placeholders) in keys {
        let key = format!("demo_viewer.killfeed.{key}");
        let en = text("en", &key).unwrap_or_else(|| panic!("en lacks {key}"));
        for lang in langs {
            let theirs = text(lang, &key).unwrap_or_else(|| panic!("{lang} lacks {key}"));
            for p in placeholders {
                assert!(theirs.contains(p), "{lang} {key} lacks {p}: {theirs}");
            }
            if lang != "en" {
                assert_ne!(theirs, en, "{lang} {key} is untranslated");
            }
        }
    }
}

// In `lang` on the narrowest page, with the bar at the bottom or the top: the feed fits, and
// its rows read translated.
fn feed_fits(lang: &str, top: bool, cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let w = open_with(cx, &env, killfeed_tracks());
    cx.simulate_window_resize(w.s.win, size(px(800.), px(600.)));
    support::settle(cx, w.s.win);
    assert_feed_fits(cx, &w, top, &format!("{lang} top={top}"));
    let row = support::label_of(cx, w.s.win, ids::demo_viewer_kill(3)).unwrap_or_default();
    assert!(
        row.contains("Alice") && !row.contains(" killed "),
        "{lang}: the row reads {row:?}"
    );
}

#[gpui_kit::test]
fn the_killfeed_fits_the_narrowest_page_in_german(cx: &mut TestAppContext) {
    feed_fits("de", false, cx);
}

#[gpui_kit::test]
fn the_killfeed_fits_under_a_top_bar_on_the_narrowest_page_in_german(cx: &mut TestAppContext) {
    feed_fits("de", true, cx);
}

#[gpui_kit::test]
fn the_killfeed_fits_the_narrowest_page_in_russian(cx: &mut TestAppContext) {
    feed_fits("ru", false, cx);
}
