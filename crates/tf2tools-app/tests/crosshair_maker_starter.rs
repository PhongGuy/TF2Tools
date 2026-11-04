use tf2tools_app::pages::crosshair_maker::model::CanvasSize;
use tf2tools_app::pages::crosshair_maker::starter::{
    Param, Starter, StarterParams, starter_cells, starter_drawing,
};

const SIZES: [CanvasSize; 2] = [CanvasSize::Small, CanvasSize::Large];

fn params(length: u32, thickness: u32, gap: u32, size: u32) -> StarterParams {
    StarterParams {
        length,
        thickness,
        gap,
        size,
    }
}

/// A cell's distance from the true centre along one axis, counted from 1.
fn from_centre(side: u32, i: u32) -> u32 {
    let h = side / 2;
    if i >= h { i - h + 1 } else { h - i }
}

/// The painted cells as a side-by-side grid of flags; panics on a cell off the grid.
fn grid_of(side: u32, cells: &[(u32, u32)]) -> Vec<bool> {
    let mut g = vec![false; (side * side) as usize];
    for &(x, y) in cells {
        assert!(x < side && y < side, "({x}, {y}) is off the {side} grid");
        g[(y * side + x) as usize] = true;
    }
    g
}

/// Checks the shape is not empty, lists each cell once in row order and is its own mirror
/// image both ways, which on an even grid means centred on the true centre. Returns the
/// grid and how far the shape reaches from the centre across and down.
fn check(shape: Starter, p: StarterParams, size: CanvasSize) -> (Vec<bool>, (u32, u32)) {
    let n = size.side();
    let cells = starter_cells(shape, p, size);
    assert!(!cells.is_empty(), "{shape:?} {p:?} {n}");
    let mut sorted = cells.clone();
    sorted.sort_by_key(|&(x, y)| (y, x));
    sorted.dedup();
    assert_eq!(
        sorted, cells,
        "{shape:?} {p:?} {n}: row order, each cell once"
    );
    let g = grid_of(n, &cells);
    let at = |x: u32, y: u32| g[(y * n + x) as usize];
    for y in 0..n {
        for x in 0..n {
            assert_eq!(at(x, y), at(n - 1 - x, y), "{shape:?} {p:?} {n} ({x}, {y})");
            assert_eq!(at(x, y), at(x, n - 1 - y), "{shape:?} {p:?} {n} ({x}, {y})");
        }
    }
    let reach = cells.iter().fold((0, 0), |(a, d), &(x, y)| {
        (a.max(from_centre(n, x)), d.max(from_centre(n, y)))
    });
    (g, reach)
}

/// Every value a slider for `param` can take at `size`.
fn values(param: Param, size: CanvasSize) -> Vec<u32> {
    let (min, max, step) = param.limits(size);
    (min..=max).step_by(step as usize).collect()
}

/// The 12x12 block around the centre of a 32x32 shape as rows of '#' and '.'.
fn middle(cells: &[(u32, u32)]) -> String {
    let g = grid_of(32, cells);
    (10..22)
        .map(|y| {
            (10..22)
                .map(|x| if g[(y * 32 + x) as usize] { '#' } else { '.' })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_starters_are_five_shapes_with_plain_names() {
    assert_eq!(
        Starter::ALL.map(Starter::label),
        [
            "Dot",
            "Cross",
            "Cross with gap",
            "Circle",
            "Circle with dot"
        ]
    );
    assert_eq!(
        Starter::ALL.map(Starter::slug),
        ["dot", "cross", "cross-gap", "circle", "circle-dot"]
    );
    assert_eq!(
        Param::ALL.map(Param::label),
        ["Length", "Thickness", "Gap", "Size"]
    );
    assert_eq!(
        Param::ALL.map(Param::slug),
        ["length", "thickness", "gap", "size"]
    );
    assert_eq!(Starter::Dot.params(), [Param::Size]);
    assert_eq!(Starter::Cross.params(), [Param::Length, Param::Thickness]);
    assert_eq!(
        Starter::CrossWithGap.params(),
        [Param::Length, Param::Thickness, Param::Gap]
    );
    assert_eq!(Starter::Circle.params(), [Param::Thickness, Param::Size]);
    assert_eq!(
        Starter::CircleWithDot.params(),
        [Param::Thickness, Param::Size]
    );
    let mut p = params(1, 2, 3, 4);
    p.set(Param::Gap, 9);
    assert_eq!(Param::ALL.map(|q| p.get(q)), [1, 2, 9, 4]);
}

#[test]
fn slider_limits_scale_with_the_canvas() {
    let (small, large) = (CanvasSize::Small, CanvasSize::Large);
    assert_eq!(
        (Param::Length.limits(small), Param::Length.limits(large)),
        ((1, 16, 1), (1, 32, 1))
    );
    assert_eq!(
        (
            Param::Thickness.limits(small),
            Param::Thickness.limits(large)
        ),
        ((2, 8, 2), (2, 16, 2))
    );
    assert_eq!(
        (Param::Gap.limits(small), Param::Gap.limits(large)),
        ((1, 15, 1), (1, 31, 1))
    );
    assert_eq!(
        (Param::Size.limits(small), Param::Size.limits(large)),
        ((2, 32, 2), (2, 64, 2))
    );
}

#[test]
fn a_slider_value_snaps_to_its_step_inside_its_limits() {
    let (small, large) = (CanvasSize::Small, CanvasSize::Large);
    assert_eq!(
        [5., 4.9, 0., -3., 99.].map(|v| Param::Thickness.snap(large, v)),
        [6, 4, 2, 2, 16]
    );
    assert_eq!(
        [16.4, 40., 0.].map(|v| Param::Length.snap(small, v)),
        [16, 16, 1]
    );
    assert_eq!([31., 30.9].map(|v| Param::Size.snap(small, v)), [32, 30]);
    // Out-of-range values draw as their snapped ones.
    assert_eq!(
        starter_cells(Starter::Dot, params(0, 0, 0, 3), small),
        starter_cells(Starter::Dot, params(1, 2, 1, 4), small)
    );
    assert_eq!(
        starter_cells(Starter::CrossWithGap, params(999, 999, 999, 999), large),
        starter_cells(Starter::CrossWithGap, params(32, 16, 31, 64), large)
    );
}

#[test]
fn defaults_sit_inside_the_limits() {
    for size in SIZES {
        for shape in Starter::ALL {
            let d = shape.defaults(size);
            for param in Param::ALL {
                let (min, max, step) = param.limits(size);
                let v = d.get(param);
                assert!(
                    v >= min && v <= max && (v - min) % step == 0,
                    "{shape:?} {param:?} {v}"
                );
            }
        }
    }
    let (small, large) = (CanvasSize::Small, CanvasSize::Large);
    assert_eq!(Starter::Dot.defaults(large), params(8, 2, 4, 4));
    assert_eq!(Starter::Cross.defaults(large), params(8, 2, 4, 32));
    assert_eq!(Starter::Dot.defaults(small), params(4, 2, 2, 2));
    assert_eq!(Starter::Circle.defaults(small), params(4, 2, 2, 16));
}

#[test]
fn each_starter_draws_as_pictured_on_32() {
    let small = CanvasSize::Small;
    let picture = |rows: [&str; 12]| rows.join("\n");
    let blank = "............";
    assert_eq!(
        middle(&starter_cells(Starter::Dot, params(1, 2, 1, 4), small)),
        picture([
            blank,
            blank,
            blank,
            blank,
            "....####....",
            "....####....",
            "....####....",
            "....####....",
            blank,
            blank,
            blank,
            blank
        ])
    );
    assert_eq!(
        middle(&starter_cells(Starter::Cross, params(3, 2, 1, 2), small)),
        picture([
            blank,
            blank,
            blank,
            ".....##.....",
            ".....##.....",
            "...######...",
            "...######...",
            ".....##.....",
            ".....##.....",
            blank,
            blank,
            blank
        ])
    );
    assert_eq!(
        middle(&starter_cells(
            Starter::CrossWithGap,
            params(3, 2, 2, 2),
            small
        )),
        picture([
            blank,
            ".....##.....",
            ".....##.....",
            ".....##.....",
            blank,
            ".###....###.",
            ".###....###.",
            blank,
            ".....##.....",
            ".....##.....",
            ".....##.....",
            blank
        ])
    );
    assert_eq!(
        middle(&starter_cells(Starter::Circle, params(1, 2, 1, 8), small)),
        picture([
            blank,
            blank,
            "....####....",
            "...######...",
            "..###..###..",
            "..##....##..",
            "..##....##..",
            "..###..###..",
            "...######...",
            "....####....",
            blank,
            blank
        ])
    );
    assert_eq!(
        middle(&starter_cells(
            Starter::CircleWithDot,
            params(1, 2, 1, 8),
            small
        )),
        picture([
            blank,
            blank,
            "....####....",
            "...######...",
            "..###..###..",
            "..##.##.##..",
            "..##.##.##..",
            "..###..###..",
            "...######...",
            "....####....",
            blank,
            blank
        ])
    );
    // A ring as thick as the radius is a disc.
    assert_eq!(
        middle(&starter_cells(Starter::Circle, params(1, 4, 1, 6), small)),
        picture([
            blank,
            blank,
            blank,
            "....####....",
            "...######...",
            "...######...",
            "...######...",
            "...######...",
            "....####....",
            blank,
            blank,
            blank
        ])
    );
}

#[test]
fn every_slider_value_keeps_each_shape_centred_on_the_grid() {
    for size in SIZES {
        let (n, h) = (size.side(), size.side() / 2);
        let base = Starter::Cross.defaults(size);
        for s in values(Param::Size, size) {
            let (_, reach) = check(
                Starter::Dot,
                params(base.length, base.thickness, base.gap, s),
                size,
            );
            assert_eq!(reach, (s / 2, s / 2), "dot {s}");
        }
        for length in values(Param::Length, size) {
            for thickness in values(Param::Thickness, size) {
                let p = params(length, thickness, base.gap, base.size);
                let (_, reach) = check(Starter::Cross, p, size);
                let r = length.max(thickness / 2);
                assert_eq!(reach, (r, r), "cross {p:?}");
                for gap in values(Param::Gap, size) {
                    let p = params(length, thickness, gap, base.size);
                    let (g, reach) = check(Starter::CrossWithGap, p, size);
                    let r = (gap + length).min(h).max(thickness / 2);
                    assert_eq!(reach, (r, r), "cross with gap {p:?}");
                    // The middle stays empty out to the gap, and each arm starts right after it.
                    for y in h - gap..h + gap {
                        for x in h - gap..h + gap {
                            assert!(!g[(y * n + x) as usize], "{p:?} ({x}, {y})");
                        }
                    }
                    assert!(g[(h * n + h + gap) as usize], "{p:?}");
                }
            }
        }
        for s in values(Param::Size, size) {
            for thickness in values(Param::Thickness, size) {
                let p = params(base.length, thickness, base.gap, s);
                let (g, reach) = check(Starter::Circle, p, size);
                assert_eq!(reach, (s / 2, s / 2), "circle {p:?}");
                let across = (h..n).filter(|&x| g[(h * n + x) as usize]).count() as u32;
                assert_eq!(across, thickness.min(s / 2), "ring width {p:?}");
                let (g, reach) = check(Starter::CircleWithDot, p, size);
                let r = (s / 2).max(thickness / 2);
                assert_eq!(reach, (r, r), "circle with dot {p:?}");
                let half = thickness / 2;
                for y in h - half..h + half {
                    for x in h - half..h + half {
                        assert!(g[(y * n + x) as usize], "dot {p:?} ({x}, {y})");
                    }
                }
            }
        }
    }
}

#[test]
fn a_starter_drawing_paints_its_cells_and_leaves_the_rest_clear() {
    for size in SIZES {
        for shape in Starter::ALL {
            let p = shape.defaults(size);
            let d = starter_drawing(shape, p, size, [10, 20, 30, 128]);
            let n = size.side();
            assert_eq!((d.width(), d.height()), (n, n));
            let g = grid_of(n, &starter_cells(shape, p, size));
            for (i, texel) in d.pixels().chunks(4).enumerate() {
                let want = if g[i] {
                    [10, 20, 30, 128]
                } else {
                    [0, 0, 0, 0]
                };
                assert_eq!(texel, want, "{shape:?} {n} texel {i}");
            }
        }
    }
}
