//! The Demo Viewer's kill feed: its rows at any tick, their words and icons, where it sits
//! beside the bar, and clicks passing through it to the map.

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::{
    ElementId, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    TestAppContext, px, size,
};
use tf2tools_app::ids;
use tf2tools_app::kill_icons::KillIconImages;
use tf2tools_app::pages::demo_viewer::{
    BarPlace, NAME_CAP, NAME_MIN, feed_key, feed_layout, feed_on_top, feed_rows, feed_words,
    timeline_fraction,
};
use tf2tools_core::{CameraMode, PlayerId};
use tf2tools_core::{Team, TfClass, WeaponNames};

use crate::support;
use crate::support::demo_viewer::{
    DemoViewerShell, LONG_KILLER, LONG_VICTIM, LockCall, assert_feed_fits, inside, open_with,
    open_with_icons, open_with_locked, open_with_tracks, seek_to as seek, send, walking_tracks,
};
use crate::support::demo_viewer::{camera_tracks, killfeed_kills, killfeed_tracks};

#[test]
fn the_feed_holds_the_last_five_kills_with_both_sides() {
    let tracks = killfeed_tracks();
    let rows = feed_rows(&tracks, 4100.);
    assert_eq!(rows.len(), 5);
    let alice = rows[0].killer.as_ref().expect("Alice");
    assert_eq!(
        (alice.name.as_ref(), alice.team),
        ("Alice", Some(Team::Blue))
    );
    assert_eq!(rows[0].victim.name.as_ref(), "Bob");
    assert_eq!(rows[0].weapon.as_deref(), Some("scattergun"));
    assert!(rows[4].killer.is_none(), "Bob's fall has no killer");
    let later = feed_rows(&tracks, 4160.);
    assert_eq!(later.len(), 5);
    let pyro = later[3].killer.as_ref().expect("the pyro");
    assert_eq!(
        pyro.name.as_ref(),
        "Player 3",
        "an unnamed killer falls back"
    );
    assert_eq!(later[3].killer_class, Some(TfClass::Pyro));
    assert_eq!(
        later[3].assister.as_ref().map(|a| a.name.as_ref()),
        Some("Bob")
    );
    assert_eq!(feed_rows(&tracks, 4045.).len(), 3);
    assert_eq!(
        feed_rows(&tracks, 4100.),
        rows,
        "the same tick gives the same feed"
    );
}

#[test]
fn the_feed_key_changes_exactly_when_the_rows_do() {
    let tracks = killfeed_tracks();
    let ticks = [
        4000., 4019., 4020., 4025., 4030., 4069.5, 4070., 4100., 4150., 4155., 4160., 4200., 4420.,
        4470., 4600.,
    ];
    for pair in ticks.windows(2) {
        let same_key = feed_key(&tracks, pair[0]) == feed_key(&tracks, pair[1]);
        let same_rows = feed_rows(&tracks, pair[0]) == feed_rows(&tracks, pair[1]);
        assert_eq!(same_key, same_rows, "between {} and {}", pair[0], pair[1]);
    }
    assert_eq!(feed_key(&tracks, 4100.), (5, Some(4070)));
    assert_eq!(feed_key(&tracks, 4000.), (0, None));
    // The 6 s hold is about 396 ticks: by 4470 only the two kills after 4074 remain.
    assert_eq!(feed_key(&tracks, 4420.), (5, Some(4160)));
    assert_eq!(feed_key(&tracks, 4470.), (2, Some(4160)));
    assert_eq!(feed_rows(&tracks, 4470.).len(), 2);
    assert_eq!(feed_key(&tracks, 4600.), (0, None));
}

#[test]
fn each_row_reads_as_one_sentence() {
    let tracks = killfeed_tracks();
    let names = WeaponNames::default();
    let words = |tick: f64, n: usize| feed_words(&feed_rows(&tracks, tick)[n], &names).to_string();
    assert_eq!(words(4100., 0), "Alice killed Bob with scattergun");
    assert_eq!(words(4100., 4), "Bob died");
    assert_eq!(
        words(4160., 3),
        "Player 3 and Bob killed Alice with tf_projectile_rocket"
    );
}

#[test]
fn the_killers_class_picks_the_icon() {
    let image = |w| tf2tools_app::previews::to_render_image(w, 1, vec![255; w as usize * 4]);
    let icons = KillIconImages::from_map(HashMap::from([
        ("deflect_rocket".to_owned(), image(7).unwrap()),
        ("tf_projectile_rocket".to_owned(), image(4).unwrap()),
    ]));
    let row = &feed_rows(&killfeed_tracks(), 4160.)[3];
    let names = WeaponNames::default();
    let icon = icons.for_weapon(row.weapon.as_deref().unwrap(), row.killer_class, &names);
    assert_eq!(
        icon.map(|i| i.size(0).width.0),
        Some(7),
        "a pyro's rocket is a reflect"
    );
}

#[test]
fn the_feed_moves_to_the_bottom_only_under_a_bar_at_the_top() {
    let area = size(px(1000.), px(800.));
    let bar = size(px(600.), px(80.));
    assert!(feed_on_top(BarPlace::DEFAULT, area, bar));
    let top = BarPlace {
        y: 0.,
        ..BarPlace::DEFAULT
    };
    assert!(!feed_on_top(top, area, bar));
    let middle_high = BarPlace {
        y: 0.4,
        ..BarPlace::DEFAULT
    };
    assert!(!feed_on_top(middle_high, area, bar));
    // A bar that fills the page has no room either side; the feed keeps its usual corner.
    assert!(feed_on_top(top, bar, bar));
}

fn label(cx: &mut TestAppContext, w: &DemoViewerShell, id: impl Into<ElementId>) -> Option<String> {
    support::label_of(cx, w.s.win, id)
}

fn camera(cx: &mut TestAppContext, w: &DemoViewerShell) -> (CameraMode, Option<u32>) {
    cx.update(|cx| {
        let page = w.s.shell.read(cx).demo_viewer_page(cx).expect("the viewer");
        let state = page
            .read(cx)
            .viewport()
            .expect("a viewport")
            .read(cx)
            .camera_state();
        (state.mode, state.target.map(PlayerId::entity))
    })
}

#[gpui_kit::test]
fn the_feed_names_its_rows_follows_seeks_and_reads_the_same_after_seeking_back(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    let w = open_with(cx, &env, killfeed_tracks());
    seek(cx, &w, 4100.);
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_KILLFEED).as_deref(),
        Some("Kill feed")
    );
    assert_eq!(
        label(cx, &w, ids::demo_viewer_kill(0)).as_deref(),
        Some("Alice killed Bob with scattergun")
    );
    assert_eq!(
        label(cx, &w, ids::demo_viewer_kill(4)).as_deref(),
        Some("Bob died")
    );
    assert!(!support::present(cx, w.s.win, ids::demo_viewer_kill(5)));
    let rows = |cx: &mut TestAppContext| -> Vec<Option<String>> {
        (0..5)
            .map(|n| label(cx, &w, ids::demo_viewer_kill(n)))
            .collect()
    };
    let before = rows(cx);
    seek(cx, &w, 4160.);
    assert_eq!(
        label(cx, &w, ids::demo_viewer_kill(3)).as_deref(),
        Some("Player 3 and Bob killed Alice with tf_projectile_rocket")
    );
    seek(cx, &w, 4045.);
    assert!(support::present(cx, w.s.win, ids::demo_viewer_kill(2)));
    assert!(!support::present(cx, w.s.win, ids::demo_viewer_kill(3)));
    seek(cx, &w, 4100.);
    assert_eq!(rows(cx), before);
}

// At `width` x `height`, with the bar at the bottom and then the top, the feed fits and its
// long row still names the 32-character killer.
fn feed_fits(cx: &mut TestAppContext, width: f32, height: f32) {
    for top in [false, true] {
        let env = support::env();
        let w = open_with(cx, &env, killfeed_tracks());
        cx.simulate_window_resize(w.s.win, size(px(width), px(height)));
        support::settle(cx, w.s.win);
        assert_feed_fits(cx, &w, top, &format!("{width}x{height} top={top}"));
        let long = label(cx, &w, ids::demo_viewer_kill(4)).unwrap_or_default();
        assert!(long.contains(LONG_KILLER), "the long row reads {long:?}");
    }
}

#[gpui_kit::test]
fn the_feed_fits_at_800_beside_a_bar_at_the_bottom_or_the_top(cx: &mut TestAppContext) {
    feed_fits(cx, 800., 600.);
}

#[gpui_kit::test]
fn the_feed_fits_at_1280_beside_a_bar_at_the_bottom_or_the_top(cx: &mut TestAppContext) {
    feed_fits(cx, 1280., 800.);
}

#[test]
fn the_feed_grows_to_600_and_caps_each_name_on_its_own() {
    // 1023 is the page beside the sidebar in a 1280 window.
    let wide = feed_layout(px(1023.));
    assert_eq!((wide.width, wide.name), (px(600.), NAME_CAP));
    // An 800 window leaves a page of about 543: the caps shrink but three names still fit.
    let narrow = feed_layout(px(543.));
    assert_eq!(narrow.width, px(543.) - px(16.));
    assert!(
        narrow.name < NAME_CAP && narrow.name >= NAME_MIN,
        "{narrow:?}"
    );
    assert!(narrow.name * 3. + px(64.) < narrow.width, "{narrow:?}");
    // However narrow the page, a name keeps room for a few letters.
    assert_eq!(feed_layout(px(150.)).name, NAME_MIN);
    assert_eq!(feed_layout(px(0.)).width, px(0.));
}

const MASTER: &str = "TheMasterOfDisaster";
const FORGOTTEN: &str = ".forgotten";
const IMPACT: &str = "Тов. Impact";

// `killfeed_tracks` with one more kill at 4155: three typical names, killer, assister and
// victim, which at 4160 is the feed's fourth row (3) above the two 32-character names (4).
fn typical_names_tracks() -> Arc<tf2tools_core::DemoTracks> {
    let mut kills = killfeed_kills();
    let master = (6, MASTER, Team::Blue, TfClass::Soldier);
    let forgotten = (7, FORGOTTEN, Team::Blue, TfClass::Medic);
    let impact = (8, IMPACT, Team::Red, TfClass::Sniper);
    kills.push((
        4155,
        Some(master),
        Some(forgotten),
        impact,
        "rocketlauncher",
        false,
        false,
    ));
    let tracks = Arc::into_inner(camera_tracks()).expect("a fresh Arc");
    Arc::new(tracks.with_test_kills(&kills))
}

fn width_of(cx: &mut TestAppContext, w: &DemoViewerShell, id: ElementId) -> f32 {
    let bounds = support::bounds_of(cx, w.s.win, id.clone()).unwrap_or_else(|| panic!("no {id:?}"));
    f32::from(bounds.size.width)
}

// At `width` x `height`, bar at the bottom then the top: the feed fits, and every name of the
// typical row and the long row keeps at least `NAME_MIN`. Returns the typical row's widths.
fn typical_names_fit(cx: &mut TestAppContext, width: f32, height: f32) -> [f32; 3] {
    let mut widths = [0.; 3];
    for top in [false, true] {
        let env = support::env();
        let w = open_with(cx, &env, typical_names_tracks());
        cx.simulate_window_resize(w.s.win, size(px(width), px(height)));
        support::settle(cx, w.s.win);
        let at = format!("{width}x{height} top={top}");
        assert_feed_fits(cx, &w, top, &at);
        widths = [
            width_of(cx, &w, ids::demo_viewer_kill_killer(3)),
            width_of(cx, &w, ids::demo_viewer_kill_assister(3)),
            width_of(cx, &w, ids::demo_viewer_kill_victim(3)),
        ];
        let long = [
            width_of(cx, &w, ids::demo_viewer_kill_killer(4)),
            width_of(cx, &w, ids::demo_viewer_kill_victim(4)),
        ];
        for name in widths.iter().chain(&long) {
            assert!(
                *name >= f32::from(NAME_MIN) - 0.5,
                "{at}: {widths:?} {long:?}"
            );
        }
        assert!(
            long.iter().all(|n| *n <= f32::from(NAME_CAP) + 0.5),
            "{at}: {LONG_KILLER} and {LONG_VICTIM} stay capped: {long:?}"
        );
    }
    widths
}

#[gpui_kit::test]
fn typical_names_show_in_full_at_1280(cx: &mut TestAppContext) {
    let [killer, assister, victim] = typical_names_fit(cx, 1280., 800.);
    // ".forgotten" is short enough to show whole anywhere; it gives one letter's width. Widths
    // round to whole pixels, so a whole name may read up to 2 px short; a cut one loses a letter.
    let letter = assister / FORGOTTEN.chars().count() as f32;
    let whole = |name: &str| name.chars().count() as f32 * letter - 2.;
    assert!(killer >= whole(MASTER), "{MASTER} is cut to {killer}");
    assert!(victim >= whole(IMPACT), "{IMPACT} is cut to {victim}");
}

#[gpui_kit::test]
fn typical_names_keep_room_at_800(cx: &mut TestAppContext) {
    typical_names_fit(cx, 800., 600.);
}

// The widest icon the game has (128 x 32) between two 32-character names, at 800x600: the row
// shows its icon and still fits the feed.
#[gpui_kit::test]
fn a_row_with_the_widest_icon_and_long_names_fits_the_feed(cx: &mut TestAppContext) {
    let image = || tf2tools_app::previews::to_render_image(128, 32, vec![255; 128 * 32 * 4]);
    let icons = KillIconImages::from_map(HashMap::from([
        ("knife".to_owned(), image().unwrap()),
        ("scattergun".to_owned(), image().unwrap()),
    ]));
    let env = support::env();
    let w = open_with_icons(cx, &env, killfeed_tracks(), icons);
    cx.simulate_window_resize(w.s.win, size(px(800.), px(600.)));
    support::settle(cx, w.s.win);
    assert_feed_fits(cx, &w, false, "icons at 800x600");
    let row = support::bounds_of(cx, w.s.win, ids::demo_viewer_kill(4)).expect("the long row");
    let icon = support::bounds_of(cx, w.s.win, ids::demo_viewer_kill_icon(4)).expect("its icon");
    assert!(
        inside(icon, row),
        "the icon {icon:?} leaves its row {row:?}"
    );
    assert!(
        icon.size.height > px(8.) && icon.size.width > px(8.),
        "{icon:?}"
    );
    assert!(
        !support::present(cx, w.s.win, ids::demo_viewer_kill_icon(2)),
        "Bob's fall has none"
    );
}

// A KWin click under the pointer lock: a buttonless move lands between the press and the
// release. Over a kill row it must still reach the map, which follows the first player.
#[gpui_kit::test]
fn a_click_on_the_feed_reaches_the_map(cx: &mut TestAppContext) {
    let env = support::env();
    let (w, lock) = open_with_locked(cx, &env, killfeed_tracks());
    seek(cx, &w, 4100.);
    let at = support::bounds_of(cx, w.s.win, ids::demo_viewer_kill(0))
        .expect("a kill row")
        .center();
    let modifiers = Modifiers::default();
    let moved = |pressed_button| MouseMoveEvent {
        position: at,
        pressed_button,
        modifiers,
    };
    let button = MouseButton::Left;
    send(cx, &w, moved(None));
    send(
        cx,
        &w,
        MouseDownEvent {
            button,
            position: at,
            modifiers,
            click_count: 1,
            first_mouse: false,
        },
    );
    send(cx, &w, moved(None));
    send(
        cx,
        &w,
        MouseUpEvent {
            button,
            position: at,
            modifiers,
            click_count: 1,
        },
    );
    assert_eq!(camera(cx, &w), (CameraMode::FirstPerson, Some(1)));
    assert!(lock.calls().contains(&LockCall::Lock), "{:?}", lock.calls());
}

#[test]
fn a_tick_maps_to_its_share_of_the_timeline() {
    assert_eq!(timeline_fraction(4000, 4000, 4200), 0.);
    assert_eq!(timeline_fraction(4100, 4000, 4200), 0.5);
    assert_eq!(timeline_fraction(4200, 4000, 4200), 1.);
    assert_eq!(timeline_fraction(3990, 4000, 4200), 0.);
    assert_eq!(timeline_fraction(4300, 4000, 4200), 1.);
    assert_eq!(timeline_fraction(4000, 4000, 4000), 0.);
}

#[gpui_kit::test]
fn the_kills_mark_the_timeline_in_one_named_strip(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with(cx, &env, killfeed_tracks());
    assert_eq!(
        label(cx, &w, ids::DEMO_VIEWER_KILL_MARKERS).as_deref(),
        Some("Kills on the timeline: 8")
    );
    let strip = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_KILL_MARKERS).expect("the strip");
    let timeline =
        support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_TIMELINE).expect("the timeline");
    assert!(inside(strip, timeline), "{strip:?} leaves {timeline:?}");
    assert!(strip.size.width > px(100.), "{strip:?}");
}

// `walking_tracks` starts rounds at 4050 and 4150; a kill on each round's tick puts the strip's
// marks right under the dots, which must still take the click and seek.
#[gpui_kit::test]
fn round_markers_still_seek_over_the_kill_strip(cx: &mut TestAppContext) {
    let alice = (1, "Alice", Team::Blue, TfClass::Scout);
    let bob = (2, "Bob", Team::Red, TfClass::Soldier);
    let tracks = Arc::into_inner(walking_tracks(None))
        .expect("a fresh Arc")
        .with_test_kills(&[
            (4050, Some(alice), None, bob, "scattergun", false, false),
            (
                4150,
                Some(bob),
                None,
                alice,
                "shotgun_soldier",
                false,
                false,
            ),
        ]);
    let env = support::env();
    let w = open_with(cx, &env, Arc::new(tracks));
    let strip = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_KILL_MARKERS).expect("the strip");
    // Each dot stands out past the strip's top and bottom, so the kill lines cannot hide it.
    for n in 0..2 {
        let dot = support::bounds_of(cx, w.s.win, ids::demo_viewer_round_marker(n)).expect("a dot");
        assert!(
            dot.top() < strip.top() && dot.bottom() > strip.bottom(),
            "round {n} at {dot:?} hides in the strip {strip:?}"
        );
        assert!(dot.size.width > strip.size.height, "{dot:?} {strip:?}");
    }
    support::click_id(cx, w.s.win, ids::demo_viewer_round_marker(1));
    assert_eq!(support::demo_viewer::playback(cx, &w).unwrap().tick, 4150.);
    support::click_id(cx, w.s.win, ids::demo_viewer_round_marker(0));
    assert_eq!(support::demo_viewer::playback(cx, &w).unwrap().tick, 4050.);
}

#[gpui_kit::test]
fn tracks_without_kills_draw_no_strip(cx: &mut TestAppContext) {
    let env = support::env();
    let w = open_with_tracks(cx, &env, None);
    assert!(!support::present(
        cx,
        w.s.win,
        ids::DEMO_VIEWER_KILL_MARKERS
    ));
}
