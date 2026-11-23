//! The Demo Viewer bar's stored place: its setting text and how it stays inside the page.

use gpui_kit::{Pixels, Point, Size, point, px, size};
use tf2tools_app::pages::demo_viewer::BarPlace;

fn area(w: f32, h: f32) -> Size<Pixels> {
    size(px(w), px(h))
}

fn at(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

#[test]
fn an_open_place_round_trips_through_its_setting() {
    let place = BarPlace {
        x: 0.25,
        y: 0.75,
        collapsed: false,
    };
    assert_eq!(place.to_setting(), "0.250 0.750 open");
    assert_eq!(BarPlace::from_setting(&place.to_setting()), Some(place));
}

#[test]
fn a_collapsed_place_round_trips_through_its_setting() {
    let place = BarPlace {
        x: 0.5,
        y: 0.94,
        collapsed: true,
    };
    assert_eq!(place.to_setting(), "0.500 0.940 collapsed");
    assert_eq!(BarPlace::from_setting(&place.to_setting()), Some(place));
}

#[test]
fn the_page_edges_are_valid_places() {
    let place = BarPlace::from_setting("0 1 open").unwrap();
    assert_eq!((place.x, place.y, place.collapsed), (0.0, 1.0, false));
    assert_eq!(BarPlace::from_setting(&place.to_setting()), Some(place));
}

#[test]
fn a_malformed_setting_is_rejected() {
    for text in [
        "",
        "0.5 0.5",
        "0.5 0.5 open extra",
        "nan 0.5 open",
        "inf 0.5 open",
        "0.5 0.5 shut",
        "1.5 0.5 open",
        "-0.1 0.5 open",
        "x 0.5 open",
    ] {
        assert_eq!(BarPlace::from_setting(text), None, "{text:?}");
    }
}

#[test]
fn a_shrunken_area_keeps_the_bar_inside() {
    let place = BarPlace {
        x: 1.0,
        y: 1.0,
        collapsed: false,
    };
    // Centre at the far corner of 400x300 pushes the 200x50 bar out; it pins to the corner.
    assert_eq!(
        place.clamp(area(400., 300.), area(200., 50.)),
        at(200., 250.)
    );
}

#[test]
fn a_bar_bigger_than_the_area_sits_at_the_origin() {
    let place = BarPlace {
        x: 0.5,
        y: 0.5,
        collapsed: false,
    };
    assert_eq!(place.clamp(area(100., 40.), area(300., 60.)), at(0., 0.));
}

#[test]
fn a_zero_area_puts_the_bar_at_the_origin() {
    assert_eq!(
        BarPlace::DEFAULT.clamp(area(0., 0.), area(200., 50.)),
        at(0., 0.)
    );
}

#[test]
fn the_default_place_is_bottom_centre_inside_the_page() {
    let bar = area(400., 48.);
    let origin = BarPlace::DEFAULT.clamp(area(1000., 800.), bar);
    assert!(BarPlace::DEFAULT.to_setting().ends_with(" open"));
    assert_eq!(origin.x, px(300.));
    assert!(origin.y > px(800. * 0.8), "{origin:?}");
    assert!(origin.y + bar.height <= px(800.), "{origin:?}");
}

#[test]
fn a_drag_moves_the_centre_by_the_pointer_as_fractions_of_the_area() {
    let moved = BarPlace::DEFAULT.dragged(at(500., 700.), at(-100., -300.), area(1000., 800.));
    assert_eq!((moved.x, moved.y), (0.4, 0.5));
    assert!(!moved.collapsed);
}

#[test]
fn a_drag_past_the_edge_stops_at_the_edge() {
    let moved = BarPlace::DEFAULT.dragged(at(500., 700.), at(-900., 900.), area(1000., 800.));
    assert_eq!((moved.x, moved.y), (0.0, 1.0));
}

#[test]
fn a_drag_over_a_zero_area_leaves_the_place() {
    for page in [area(0., 0.), area(0., 800.), area(1000., 0.)] {
        let moved = BarPlace::DEFAULT.dragged(at(10., 10.), at(5., 5.), page);
        assert_eq!(moved, BarPlace::DEFAULT, "{page:?}");
    }
}
