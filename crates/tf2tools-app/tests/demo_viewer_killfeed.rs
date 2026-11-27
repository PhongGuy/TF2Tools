//! The Demo Viewer's kill feed: its rows at any tick, their words and icons, where it sits
//! beside the bar, and clicks passing through it to the map.

use std::collections::HashMap;

use gpui_kit::{px, size};
use tf2tools_app::kill_icons::KillIconImages;
use tf2tools_app::pages::demo_viewer::{BarPlace, feed_key, feed_on_top, feed_rows, feed_words};
use tf2tools_core::{Team, TfClass, WeaponNames};

use crate::support::demo_viewer::killfeed_tracks;

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
        4000., 4019., 4020., 4025., 4030., 4069.5, 4070., 4100., 4150., 4155., 4160., 4200.,
    ];
    for pair in ticks.windows(2) {
        let same_key = feed_key(&tracks, pair[0]) == feed_key(&tracks, pair[1]);
        let same_rows = feed_rows(&tracks, pair[0]) == feed_rows(&tracks, pair[1]);
        assert_eq!(same_key, same_rows, "between {} and {}", pair[0], pair[1]);
    }
    assert_eq!(feed_key(&tracks, 4100.), (5, Some(4070)));
    assert_eq!(feed_key(&tracks, 4000.), (0, None));
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
