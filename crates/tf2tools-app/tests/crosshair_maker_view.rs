use gpui_kit::{Pixels, ScrollDelta, point, px, size};
use tf2tools_app::pages::crosshair_maker::model::{
    FIT_TIP, Viewport, ZOOM_IN_TIP, ZOOM_OUT_TIP, ZOOMS, wheel_scroll, wheel_zoom_steps,
};

/// A 512x448 view of a 64x64 grid: fitted cells are 7 px and the grid sits 32 px in.
fn view_64() -> Viewport {
    Viewport::new(size(px(512.), px(448.)), 64)
}

/// The 512x448 view `steps` levels in from the fit, scrolled to `scroll` (clamped).
fn zoomed_to(side: u32, steps: i32, scroll: (f32, f32)) -> Viewport {
    let mut v = Viewport::new(size(px(512.), px(448.)), side);
    v.zoom_centre(steps);
    v.scroll_by(point(px(-1e6), px(-1e6)));
    v.scroll_by(point(px(scroll.0), px(scroll.1)));
    v
}

#[test]
fn a_fitted_grid_is_centred_and_its_margins_hold_no_cells() {
    let v = view_64();
    assert_eq!((v.level(), v.cell(), v.length()), (0, px(7.), px(448.)));
    assert_eq!(v.origin(), point(px(32.), px(0.)));
    assert_eq!(v.label(), "100%");
    assert!(v.can_zoom_in() && !v.can_zoom_out());
    assert_eq!(v.cell_at(point(px(31.75), px(10.))), None);
    assert_eq!(v.cell_at(point(px(32.), px(0.))), Some((0, 0)));
    assert_eq!(v.cell_at(point(px(479.75), px(447.75))), Some((63, 63)));
    assert_eq!(v.cell_at(point(px(480.), px(10.))), None);
    assert_eq!(v.cell_at(point(px(100.), px(448.))), None);
    assert_eq!(v.cell_at(point(px(100.), px(-0.25))), None);
}

#[test]
fn zoom_steps_through_whole_pixel_cells_and_stops_at_both_ends() {
    let mut v = view_64();
    let mut cells = vec![v.cell()];
    for _ in 0..10 {
        v.zoom_centre(1);
        cells.push(v.cell());
    }
    let want = [7., 14., 21., 28., 42., 56., 84., 112., 112., 112., 112.].map(px);
    assert_eq!(cells, want);
    assert_eq!(v.label(), "1600%");
    assert!(!v.can_zoom_in() && v.can_zoom_out());
    v.zoom_centre(-3);
    assert_eq!((v.cell(), v.label()), (px(42.), "600%".to_owned()));
    v.fit();
    assert_eq!((v.level(), v.scroll()), (0, point(px(0.), px(0.))));
    assert_eq!(v.origin(), point(px(32.), px(0.)));
    v.zoom_centre(-1);
    assert_eq!(v.level(), 0);
    assert_eq!(ZOOMS.len(), 8);
}

#[test]
fn every_cell_in_view_maps_back_to_itself_at_every_zoom_and_scroll() {
    let view = size(px(512.), px(448.));
    let mut checked = 0;
    for side in [32, 64] {
        for steps in 0..ZOOMS.len() as i32 {
            for scroll in [(0., 0.), (13., 250.), (1e6, 1e6)] {
                let v = zoomed_to(side, steps, scroll);
                let (o, c) = (v.origin(), v.cell());
                for y in 0..side {
                    for x in 0..side {
                        for d in [px(0.25), c / 2., c - px(0.25)] {
                            let at = point(o.x + c * x as f32 + d, o.y + c * y as f32 + d);
                            let shown = at.x >= px(0.)
                                && at.x < view.width
                                && at.y >= px(0.)
                                && at.y < view.height;
                            assert_eq!(
                                v.cell_at(at),
                                shown.then_some((x, y)),
                                "side {side} zoom {steps} scroll {scroll:?} cell ({x}, {y}) at {at:?}"
                            );
                            checked += usize::from(shown);
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 10_000, "{checked}");
}

#[test]
fn scrolling_clamps_to_the_grid_and_does_nothing_while_it_fits() {
    let mut v = view_64();
    v.scroll_by(point(px(100.), px(100.)));
    assert_eq!(v.scroll(), point(px(0.), px(0.)));
    v.zoom_centre(1);
    v.scroll_by(point(px(-1e4), px(-1e4)));
    assert_eq!(
        (v.scroll(), v.origin()),
        (point(px(0.), px(0.)), point(px(0.), px(0.)))
    );
    v.scroll_by(point(px(1e4), px(1e4)));
    assert_eq!(v.scroll(), point(px(384.), px(448.)));
    assert_eq!(v.origin(), point(px(-384.), px(-448.)));
    // The last cell ends on the view's far corner.
    assert_eq!(v.cell_at(point(px(511.75), px(447.75))), Some((63, 63)));
    // A wider view keeps the zoom and re-clamps; wider still, the grid fits across and centres.
    v.set_frame(size(px(800.), px(448.)), 64);
    assert_eq!((v.level(), v.scroll()), (1, point(px(96.), px(448.))));
    v.set_frame(size(px(1000.), px(448.)), 64);
    assert_eq!(v.scroll().x, px(0.));
    assert_eq!(v.origin().x, px(52.));
}

#[test]
fn zooming_keeps_the_cell_under_the_pointer_wherever_the_scroll_allows() {
    let mut checked = 0;
    for start in 0..ZOOMS.len() as i32 {
        for steps in [-1, 1, 2] {
            let mut base = zoomed_to(64, start, (0., 0.));
            let mid = point(
                (base.length() - px(512.)) / 2.,
                (base.length() - px(448.)) / 2.,
            );
            base.scroll_by(mid);
            let (o, c) = (base.origin(), base.cell());
            for y in 0..64 {
                for x in 0..64 {
                    let anchor = point(o.x + c * (x as f32 + 0.5), o.y + c * (y as f32 + 0.5));
                    if base.cell_at(anchor).is_none() {
                        continue;
                    }
                    let mut v = base;
                    v.zoom_at(steps, anchor);
                    let max = point(v.length() - px(512.), v.length() - px(448.));
                    let free = |s: Pixels, m: Pixels| s > px(0.) && s < m;
                    let got = v.cell_at(anchor);
                    if free(v.scroll().x, max.x) {
                        assert_eq!(got.map(|c| c.0), Some(x), "{start} {steps} ({x}, {y})");
                        checked += 1;
                    }
                    if free(v.scroll().y, max.y) {
                        assert_eq!(got.map(|c| c.1), Some(y), "{start} {steps} ({x}, {y})");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 1_000, "{checked}");
}

#[test]
fn a_drag_off_the_view_clamps_to_the_nearest_cell_that_shows() {
    let v = view_64();
    assert_eq!(v.nearest_cell(point(px(0.), px(-40.))), (0, 0));
    assert_eq!(v.nearest_cell(point(px(900.), px(900.))), (63, 63));
    assert_eq!(v.nearest_cell(point(px(46.), px(15.))), (2, 2));
    // 14 px cells scrolled by (140, 280): columns 10..=46 and rows 20..=51 show.
    let v = zoomed_to(64, 1, (140., 280.));
    assert_eq!(v.origin(), point(px(-140.), px(-280.)));
    assert_eq!(v.nearest_cell(point(px(-50.), px(10.))), (10, 20));
    assert_eq!(v.nearest_cell(point(px(900.), px(900.))), (46, 51));
    assert_eq!(v.nearest_cell(point(px(100.), px(100.))), (17, 27));
}

#[test]
fn a_wheel_notch_is_one_level_and_a_touchpad_needs_fifty_pixels() {
    let mut carry = px(0.);
    let lines = |y: f32| ScrollDelta::Lines(point(0., y));
    let pixels = |y: f32| ScrollDelta::Pixels(point(px(0.), px(y)));
    assert_eq!(wheel_zoom_steps(lines(1.), &mut carry), 1);
    assert_eq!(wheel_zoom_steps(lines(-3.), &mut carry), -1);
    assert_eq!(
        wheel_zoom_steps(ScrollDelta::Lines(point(2., 0.)), &mut carry),
        0
    );
    for _ in 0..4 {
        assert_eq!(wheel_zoom_steps(pixels(12.), &mut carry), 0);
    }
    assert_eq!(wheel_zoom_steps(pixels(12.), &mut carry), 1);
    assert_eq!(carry, px(0.));
    assert_eq!(wheel_zoom_steps(pixels(-400.), &mut carry), -1);
    assert_eq!(carry, px(0.));
}

#[test]
fn the_wheel_scrolls_against_its_delta_and_shift_turns_it_sideways() {
    let p = |x: f32, y: f32| point(px(x), px(y));
    assert_eq!(wheel_scroll(p(0., -20.), false), p(0., 20.));
    assert_eq!(wheel_scroll(p(0., -20.), true), p(20., 0.));
    assert_eq!(wheel_scroll(p(-8., 5.), false), p(8., -5.));
    assert_eq!(wheel_scroll(p(-8., 0.), true), p(8., 0.));
    assert_eq!(ZOOM_IN_TIP, "Zoom in (Ctrl+Plus)");
    assert_eq!(ZOOM_OUT_TIP, "Zoom out (Ctrl+Minus)");
    assert_eq!(FIT_TIP, "Fit to the window (Ctrl+0)");
}

#[test]
fn a_fractional_view_scrolled_to_the_end_shows_no_empty_strip() {
    // Fractions either side of a half pixel, so a rounded scroll can overshoot the end.
    for (w, h) in [(500.4, 448.6), (500.6, 448.4), (511.5, 447.5)] {
        for steps in 1..ZOOMS.len() as i32 {
            let mut v = Viewport::new(size(px(w), px(h)), 64);
            v.zoom_centre(steps);
            v.scroll_by(point(px(1e6), px(1e6)));
            let end = v.origin() + point(v.length(), v.length());
            assert!(
                end.x >= px(w) && end.y >= px(h),
                "view {w}x{h} zoom {steps}: grid ends at {end:?}"
            );
        }
    }
}
