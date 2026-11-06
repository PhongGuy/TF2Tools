//! Canvas fitting, without GPUI windows.

use gpui_kit::{Bounds, Pixels, Size, point, px, size};
use tf2tools_app::pages::hud_maker::view::list_font_px;
use tf2tools_app::pages::hud_maker::view::{
    CanvasFit, Floating, HeightCap, Panel, PanelMode, PanelPlace, Places, Settled, Zoom, avoid,
    least_covering, settle, settle_folding,
};
use tf2tools_core::Screen;
use tf2tools_core::{HudFiles, Platform, Scheme, font_tall_px};

const HD: Screen = Screen::new(1920, 1080);

fn area(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds {
        origin: point(px(x), px(y)),
        size: size(px(w), px(h)),
    }
}

#[test]
fn a_wide_area_letterboxes_left_and_right() {
    let fit = CanvasFit::new(area(100., 50., 1200., 540.), HD);
    assert_eq!(fit.scale(), 0.5);
    assert_eq!(fit.screen_bounds(), area(220., 50., 960., 540.));
}

#[test]
fn a_tall_area_letterboxes_top_and_bottom() {
    let fit = CanvasFit::new(area(0., 0., 960., 1000.), HD);
    assert_eq!(fit.scale(), 0.5);
    assert_eq!(fit.screen_bounds(), area(0., 230., 960., 540.));
}

#[test]
fn rects_and_points_convert_both_ways() {
    let fit = CanvasFit::new(area(100., 50., 960., 540.), HD);
    assert_eq!(fit.rect(171, 927, 99, 40), area(185.5, 513.5, 49.5, 20.));
    assert_eq!(fit.to_screen(point(px(186.5), px(514.5))), Some((173, 929)));
    assert_eq!(fit.to_screen(point(px(99.), px(60.))), None);
    assert_eq!(
        fit.to_screen(point(px(1060.), px(60.))),
        None,
        "the right edge is outside"
    );
}

#[test]
fn an_empty_area_does_not_panic() {
    let fit = CanvasFit::new(area(0., 0., 0., 0.), HD);
    assert_eq!(fit.scale(), 0.);
    assert_eq!(fit.to_screen(point(px(0.), px(0.))), None);
}

#[test]
fn the_panel_place_reads_back_what_it_writes() {
    let p = PanelPlace {
        mode: PanelMode::DockRight,
        x: 40.,
        y: 24.,
        collapsed: true,
    };
    assert_eq!(p.to_setting(), "right 40 24 collapsed");
    assert_eq!(PanelPlace::from_setting("right 40 24 collapsed"), Some(p));
    assert_eq!(PanelPlace::DEFAULT.to_setting(), "floating 16 16 open");
    assert_eq!(
        PanelPlace::from_setting("floating 16 16 open"),
        Some(PanelPlace::DEFAULT)
    );
}

#[test]
fn a_damaged_setting_is_ignored() {
    for text in [
        "",
        "floating",
        "sideways 1 2 open",
        "floating x 2 open",
        "floating 1 2 shut",
        "floating 1 2 open more",
        "floating NaN 2 open",
        "floating 1 inf open",
    ] {
        assert_eq!(PanelPlace::from_setting(text), None, "{text:?}");
    }
}

#[test]
fn clamp_keeps_a_floating_panel_inside() {
    let p = PanelPlace {
        x: 2000.,
        y: -50.,
        ..PanelPlace::DEFAULT
    };
    let c = p.clamp(size(px(1200.), px(800.)), size(px(280.), px(400.)));
    assert_eq!((c.x, c.y), (920., 0.));
}

#[test]
fn avoid_leaves_a_panel_that_covers_nothing() {
    let panel = area(500., 100., 280., 400.);
    assert_eq!(
        avoid(panel, area(100., 100., 50., 20.), size(px(1200.), px(800.))),
        panel.origin
    );
}

#[test]
fn avoid_takes_the_shortest_move_that_fits() {
    // Left would start at -138; right moves 108, below 128.
    let panel = area(100., 100., 280., 400.);
    assert_eq!(
        avoid(panel, area(150., 200., 50., 20.), size(px(1200.), px(800.))),
        point(px(208.), px(100.))
    );
}

#[test]
fn avoid_skips_places_outside_the_area() {
    // Right would end at 1438; left moves 188, below 128.
    let panel = area(900., 100., 280., 400.);
    assert_eq!(
        avoid(
            panel,
            area(1000., 200., 150., 20.),
            size(px(1200.), px(800.))
        ),
        point(px(900.), px(228.))
    );
}

#[test]
fn avoid_stays_put_when_nothing_fits() {
    let panel = area(0., 0., 280., 400.);
    assert_eq!(
        avoid(panel, area(0., 0., 600., 600.), size(px(620.), px(620.))),
        panel.origin
    );
}

#[test]
fn the_places_read_back_what_they_write() {
    let places = Places {
        options: PanelPlace {
            mode: PanelMode::DockRight,
            x: 40.,
            y: 24.,
            collapsed: true,
        },
        parts: PanelPlace {
            mode: PanelMode::Floating,
            x: 300.,
            y: 16.,
            collapsed: false,
        },
        toolbar_collapsed: true,
    };
    let text = places.to_setting();
    assert_eq!(
        text,
        "right 40 24 collapsed; floating 300 16 open; toolbar collapsed"
    );
    assert_eq!(Places::from_setting(&text), Some(places));
    assert_eq!(
        Places::from_setting(&Places::DEFAULT.to_setting()),
        Some(Places::DEFAULT)
    );
}

#[test]
fn a_one_panel_setting_from_before_still_loads() {
    let places = Places::from_setting("right 40 24 collapsed").unwrap();
    assert_eq!(
        places.options,
        PanelPlace {
            mode: PanelMode::DockRight,
            x: 40.,
            y: 24.,
            collapsed: true
        }
    );
    assert_eq!(places.parts, PanelPlace::PARTS_DEFAULT);
    assert!(!places.toolbar_collapsed);
}

#[test]
fn a_damaged_segment_falls_back_on_its_own() {
    let places =
        Places::from_setting("floating 1 2 open; sideways 1 2 open; toolbar shut").unwrap();
    assert_eq!(places.options.to_setting(), "floating 1 2 open");
    assert_eq!(places.parts, PanelPlace::PARTS_DEFAULT);
    assert!(!places.toolbar_collapsed);
    assert_eq!(
        Places::from_setting("sideways 1 2 open; floating 1 2 open; toolbar open"),
        None
    );
    assert_eq!(Places::from_setting(""), None);
}

#[test]
fn the_parts_panel_starts_collapsed_and_narrower() {
    assert_eq!(Places::DEFAULT.options, PanelPlace::DEFAULT);
    assert_eq!(Places::DEFAULT.get(Panel::Parts), PanelPlace::PARTS_DEFAULT);
    const { assert!(PanelPlace::PARTS_DEFAULT.collapsed) };
    assert_eq!(PanelPlace::PARTS_DEFAULT.mode, PanelMode::Floating);
    const { assert!(!Places::DEFAULT.toolbar_collapsed) };
    assert_eq!(Panel::Options.width(), px(280.));
    assert_eq!(Panel::Parts.width(), px(240.));
    let mut places = Places::DEFAULT;
    places.set(Panel::Parts, PanelPlace::DEFAULT);
    assert_eq!(places.parts, PanelPlace::DEFAULT);
}

#[test]
fn settle_leaves_everything_home_without_a_selection() {
    let homes = [area(100., 100., 280., 400.), area(400., 100., 280., 200.)];
    assert_eq!(
        settle(&homes, &[], None, size(px(1200.), px(800.))),
        vec![homes[0].origin, homes[1].origin]
    );
}

#[test]
fn settle_moves_one_panel_as_avoid_does() {
    let room = size(px(1200.), px(800.));
    for (panel, part) in [
        (area(100., 100., 280., 400.), area(150., 200., 50., 20.)),
        (area(900., 100., 280., 400.), area(1000., 200., 150., 20.)),
        (area(500., 100., 280., 400.), area(100., 100., 50., 20.)),
        (area(0., 0., 280., 400.), area(0., 0., 1200., 800.)),
    ] {
        assert_eq!(
            settle(&[panel], &[], Some(part), room),
            vec![avoid(panel, part, room)],
            "{panel:?}"
        );
    }
}

#[test]
fn a_moving_panel_keeps_off_the_others() {
    // Right of the part would land on the second panel; below the part is the nearest clear place.
    let room = size(px(1200.), px(800.));
    let a = area(100., 100., 280., 400.);
    let b = area(400., 100., 280., 200.);
    assert_eq!(
        settle(&[a, b], &[], Some(area(150., 200., 50., 20.)), room),
        vec![point(px(100.), px(228.)), b.origin]
    );
}

#[test]
fn later_panels_keep_off_where_earlier_ones_went() {
    // Both cover the part. The first goes under the second's home; the second then goes
    // above the part rather than below it, where the first now is.
    let room = size(px(1200.), px(800.));
    let part = area(150., 200., 50., 20.);
    let a = area(100., 100., 280., 400.);
    let b = area(120., 150., 280., 100.);
    assert_eq!(
        settle(&[a, b], &[], Some(part), room),
        vec![point(px(100.), px(258.)), point(px(120.), px(92.))]
    );
}

#[test]
fn with_no_place_clear_of_the_others_a_panel_still_leaves_the_part() {
    let room = size(px(600.), px(500.));
    let part = area(0., 0., 100., 50.);
    let a = area(0., 0., 280., 400.);
    let b = area(0., 60., 600., 440.);
    assert_eq!(avoid(a, part, room), point(px(0.), px(58.)));
    assert_eq!(
        settle(&[a, b], &[], Some(part), room),
        vec![point(px(0.), px(58.)), b.origin]
    );
}

#[test]
fn a_moving_panel_takes_a_corner_rather_than_land_on_the_toolbar() {
    // Every side of the part or the toolbar is blocked, but up and right of the part is clear.
    let room = size(px(1716.), px(451.));
    let part = area(164., 30., 489., 213.);
    let toolbar = area(258., 395., 1200., 40.);
    let panel = area(33., 161., 280., 280.);
    let placed = settle(&[toolbar, panel], &[], Some(part), room);
    let moved = Bounds {
        origin: placed[1],
        size: panel.size,
    };
    assert_eq!(placed[0], toolbar.origin);
    assert!(!moved.intersects(&toolbar), "{moved:?}");
    assert!(!moved.intersects(&part), "{moved:?}");
    assert!(
        moved.origin.x >= px(0.)
            && moved.origin.y >= px(0.)
            && moved.right() <= room.width
            && moved.bottom() <= room.height,
        "{moved:?}"
    );
}

/// A small xorshift generator, so the random cases are the same on every run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u32) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % u64::from(n.max(1))) as f32
    }
}

/// Whether a box of `home`'s size fits inside `room` at least 8 px from every obstacle, found
/// by brute force on a 16 px grid plus the room's far edges, independent of settle's candidates.
fn clear_spot_exists(
    home: Bounds<Pixels>,
    obstacles: &[Bounds<Pixels>],
    room: Size<Pixels>,
) -> bool {
    let gap = px(8.);
    let (w, h) = (home.size.width, home.size.height);
    let steps = |span: Pixels| {
        let last = f32::from(span);
        let grid = (0..).map(|i| i as f32 * 16.).take_while(move |&v| v < last);
        grid.chain(std::iter::once(last)).filter(|&v| v >= 0.)
    };
    steps(room.width - w).any(|x| {
        steps(room.height - h).any(|y| {
            let padded = Bounds {
                origin: point(px(x) - gap, px(y) - gap),
                size: size(w + gap * 2., h + gap * 2.),
            };
            obstacles.iter().all(|o| !padded.intersects(o))
        })
    })
}

#[test]
fn settled_things_never_overlap_when_a_clear_spot_exists() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut checked = 0;
    for _ in 0..4000 {
        let room = size(px(400. + rng.below(1400)), px(300. + rng.below(700)));
        let (rw, rh) = (f32::from(room.width) as u32, f32::from(room.height) as u32);
        let pw = 20. + rng.below(rw / 2);
        let ph = 20. + rng.below(rh / 2);
        let part = area(rng.below(rw - pw as u32), rng.below(rh - ph as u32), pw, ph);
        // Homes start inside the room and clear of one another, as the page keeps them.
        let mut homes: Vec<Bounds<Pixels>> = Vec::new();
        let count = 2 + rng.below(2) as usize;
        for _ in 0..50 {
            if homes.len() == count {
                break;
            }
            let w = 40. + rng.below(rw / 2);
            let h = 30. + rng.below(rh / 2);
            let home = area(rng.below(rw - w as u32), rng.below(rh - h as u32), w, h);
            if homes.iter().all(|o| !o.intersects(&home)) {
                homes.push(home);
            }
        }
        let placed: Vec<Bounds<Pixels>> = settle(&homes, &[], Some(part), room)
            .into_iter()
            .zip(&homes)
            .map(|(origin, home)| Bounds {
                origin,
                size: home.size,
            })
            .collect();
        for b in &placed {
            assert!(
                b.origin.x >= px(0.)
                    && b.origin.y >= px(0.)
                    && b.right() <= room.width
                    && b.bottom() <= room.height,
                "{b:?} outside {room:?}"
            );
        }
        // Replay the order: each thing that covered the part saw earlier ones where they went
        // and later ones at home. Only when every one of them had a clear spot must all be clear.
        let all_had_room = homes.iter().enumerate().all(|(i, home)| {
            if !home.intersects(&part) {
                return true;
            }
            let obstacles: Vec<Bounds<Pixels>> = std::iter::once(part)
                .chain(placed[..i].iter().copied())
                .chain(homes[i + 1..].iter().copied())
                .collect();
            clear_spot_exists(*home, &obstacles, room)
        });
        if !all_had_room {
            continue;
        }
        checked += 1;
        for (i, a) in placed.iter().enumerate() {
            assert!(
                !a.intersects(&part),
                "{a:?} covers {part:?} in {room:?}, homes {homes:?}"
            );
            for b in &placed[i + 1..] {
                assert!(
                    !a.intersects(b),
                    "{a:?} on {b:?} in {room:?}, homes {homes:?}, part {part:?}"
                );
            }
        }
    }
    assert!(checked > 1000, "only {checked} cases had room");
}

#[test]
fn a_moving_panel_never_lands_on_a_docked_panel() {
    // Left of the tall part is the nearest clear spot but holds a docked column, so the panel
    // goes below the part instead.
    let room = size(px(700.), px(1000.));
    let column = area(0., 0., 296., 1000.);
    let part = area(320., 100., 100., 600.);
    let panel = area(300., 80., 280., 200.);
    assert_eq!(
        settle(&[panel], &[], Some(part), room),
        vec![point(px(32.), px(80.))]
    );
    assert_eq!(
        settle(&[panel], &[column], Some(part), room),
        vec![point(px(300.), px(708.))]
    );
}

fn disjoint(
    placed: &[gpui_kit::Point<Pixels>],
    homes: &[Bounds<Pixels>],
    fixed: &[Bounds<Pixels>],
) {
    let boxes: Vec<Bounds<Pixels>> = placed
        .iter()
        .zip(homes)
        .map(|(&origin, home)| Bounds {
            origin,
            size: home.size,
        })
        .collect();
    for (i, a) in boxes.iter().enumerate() {
        for b in boxes[i + 1..].iter().chain(fixed) {
            assert!(!a.intersects(b), "{a:?} on {b:?}");
        }
    }
}

#[test]
fn without_a_selection_a_home_on_an_earlier_thing_moves_off_it() {
    let room = size(px(1000.), px(800.));
    let toolbar = area(100., 400., 600., 100.);
    let panel = area(500., 300., 240., 300.);
    let placed = settle(&[toolbar, panel], &[], None, room);
    assert_eq!(placed[0], toolbar.origin);
    disjoint(&placed, &[toolbar, panel], &[]);
}

#[test]
fn without_a_selection_a_home_on_a_docked_panel_moves_off_it() {
    let room = size(px(1000.), px(800.));
    let column = area(0., 0., 280., 400.);
    let panel = area(16., 16., 280., 200.);
    let placed = settle(&[panel], &[column], None, room);
    assert_eq!(placed, vec![point(px(288.), px(16.))]);
}

#[test]
fn things_keep_off_each_other_even_with_nowhere_clear_of_the_part() {
    // The part fills the room, so nothing can clear it; they still must not stack.
    let room = size(px(600.), px(400.));
    let part = area(0., 0., 600., 400.);
    let homes = [area(0., 0., 200., 100.), area(100., 50., 200., 100.)];
    let placed = settle(&homes, &[], Some(part), room);
    disjoint(&placed, &homes, &[]);
}

fn open_at(x: f32, y: f32) -> Settled {
    Settled {
        origin: point(px(x), px(y)),
        collapsed: false,
    }
}

fn folding(home: Bounds<Pixels>) -> Floating {
    Floating {
        home,
        cap: None,
        header: Some(size(home.size.width, px(40.))),
        title: px(40.),
        spill: false,
        fold_first: false,
        shown: None,
        avoid: [None, None],
    }
}

#[test]
fn with_no_spot_clear_of_both_a_panel_clears_the_part_and_may_overlap_another() {
    // Right of the part does not fit, so the second panel goes left of it, onto the first's
    // body but below its title bar.
    let room = size(px(590.), px(300.));
    let part = area(250., 0., 100., 300.);
    let first = area(0., 0., 240., 300.);
    let second = area(200., 48., 240., 250.);
    assert_eq!(
        settle_folding(&[folding(first), folding(second)], &[], Some(part), room),
        vec![open_at(0., 0.), open_at(2., 48.)]
    );
    // From higher up the same spot would cover the first's title bar, so the second folds.
    let higher = area(200., 0., 240., 250.);
    let placed = settle_folding(&[folding(first), folding(higher)], &[], Some(part), room);
    assert!(placed[1].collapsed, "{placed:?}");
}

#[test]
fn with_no_spot_clear_of_the_part_a_panel_collapses_to_a_header_off_it() {
    // The part leaves only a 50 px band above it: too low for the open panel, room for its header
    // where it stands.
    let room = size(px(590.), px(300.));
    let part = area(150., 50., 290., 250.);
    let panel = area(100., 0., 240., 280.);
    assert_eq!(
        settle_folding(&[folding(panel)], &[], Some(part), room),
        vec![Settled {
            origin: point(px(100.), px(0.)),
            collapsed: true,
        }]
    );
    // A thing that can't collapse keeps the old fallback.
    let fixed = Floating::fixed(panel);
    assert_eq!(
        settle_folding(&[fixed], &[], Some(part), room),
        vec![Settled {
            origin: avoid(panel, part, room),
            collapsed: false,
        }]
    );
}

#[test]
fn a_panel_that_spills_takes_the_spot_overlapping_the_part_least() {
    // Nothing clears the part, so the panel drops to the room's bottom, over it least.
    let room = size(px(600.), px(500.));
    let part = area(0., 0., 600., 300.);
    let panel = Floating {
        spill: true,
        ..Floating::fixed(area(100., 0., 240., 300.))
    };
    assert_eq!(
        settle_folding(&[panel], &[], Some(part), room),
        vec![open_at(100., 200.)]
    );
    // With every spot overlapping alike, it keeps its home.
    let whole = area(0., 0., 600., 500.);
    assert_eq!(
        settle_folding(&[panel], &[], Some(whole), room),
        vec![open_at(100., 0.)]
    );
}

#[test]
fn settling_again_from_where_things_were_drawn_moves_nothing() {
    // A side panel whose height follows its top, over a part it can't clear, in the canvas
    // areas of 1280x800, 1024x768 and 800x600 windows; the second run starts from the first's
    // spots and the sizes drawn there.
    for (w, h) in [(1024., 711.), (768., 679.), (544., 511.)] {
        let room = size(px(w), px(h));
        let cap = HeightCap {
            share: 0.6,
            min: px(240.),
            max: px(h - 120.),
        };
        let side = Floating {
            cap: Some(cap),
            spill: true,
            ..Floating::fixed(area(16., 16., 280., 520.))
        };
        let list = Floating {
            header: Some(size(px(240.), px(42.))),
            ..Floating::fixed(area(w - 256., 16., 240., 42.))
        };
        for part in [area(0., 60., w, h - 120.), area(0., h / 2. - 40., w, 80.)] {
            let first = settle_folding(&[side, list], &[], Some(part), room);
            let again: Vec<Floating> = [side, list]
                .iter()
                .zip(&first)
                .map(|(t, s)| Floating {
                    shown: Some(s.origin),
                    ..*t
                })
                .collect();
            assert_eq!(
                settle_folding(&again, &[], Some(part), room),
                first,
                "{w}x{h} {part:?}"
            );
        }
    }
}

#[test]
fn a_panel_that_folds_first_folds_where_it_stands_rather_than_move() {
    // The part sits under the panel's body with room to its left: an ordinary panel moves
    // there, one that folds first shows its header at home.
    let room = size(px(600.), px(400.));
    let part = area(400., 200., 100., 100.);
    let panel = area(340., 16., 240., 300.);
    let moved = settle_folding(&[folding(panel)], &[], Some(part), room);
    assert_ne!(moved[0].origin, panel.origin, "setup: {moved:?}");
    let first = Floating {
        fold_first: true,
        ..folding(panel)
    };
    assert_eq!(
        settle_folding(&[first], &[], Some(part), room),
        vec![Settled {
            origin: panel.origin,
            collapsed: true,
        }]
    );
    // Without a selection, or with the part under its header, it settles as before.
    assert_eq!(
        settle_folding(&[first], &[], None, room),
        vec![open_at(340., 16.)]
    );
    let under_header = area(400., 20., 100., 20.);
    let placed = settle_folding(&[first], &[], Some(under_header), room);
    assert!(
        !Bounds::new(placed[0].origin, size(px(240.), px(40.))).intersects(&under_header),
        "{placed:?}"
    );
}

#[test]
fn a_panel_with_nowhere_clear_still_collapses() {
    let room = size(px(600.), px(400.));
    let part = area(0., 0., 600., 400.);
    let placed = settle_folding(&[folding(area(0., 0., 240., 300.))], &[], Some(part), room);
    assert!(placed[0].collapsed, "{placed:?}");
}

#[test]
fn a_panel_clear_of_the_part_stays_open_and_home() {
    let room = size(px(600.), px(400.));
    let panel = area(0., 0., 240., 300.);
    assert_eq!(
        settle_folding(&[folding(panel)], &[], Some(area(400., 0., 50., 50.)), room),
        vec![open_at(0., 0.)]
    );
    assert_eq!(
        settle_folding(&[folding(panel)], &[], None, room),
        vec![open_at(0., 0.)]
    );
}

#[test]
fn a_setting_without_the_parts_place_keeps_the_toolbar_collapsed() {
    let places = Places::from_setting("floating 1 2 open; toolbar collapsed").unwrap();
    assert_eq!(places.parts, PanelPlace::PARTS_DEFAULT);
    assert!(places.toolbar_collapsed);
}

fn scheme(text: &str) -> Scheme {
    let files = HudFiles::from_memory(vec![vec![(
        "resource/clientscheme.res".to_owned(),
        text.as_bytes().to_vec(),
    )]]);
    Scheme::load(&files, Platform::Linux).unwrap()
}

#[test]
fn the_list_font_is_the_schemes_default_or_twelve_units() {
    let with =
        scheme(r#""Scheme" { "Fonts" { "Default" { "1" { "name" "Verdana" "tall" "10" } } } }"#);
    assert_eq!(list_font_px(&with, HD), font_tall_px(10, 1080));
    let without = scheme(r#""Scheme" { "Colors" { } }"#);
    assert_eq!(list_font_px(&without, HD), font_tall_px(12, 1080));
}

// A 1000x600 area holding a 1000x400 screen with 100 px letterboxes above and below it, one
// drawn part near the screen's top right, and a 240x40 header over that part.
const COVER_AREA: f32 = 1000.;
fn cover_screen() -> Bounds<Pixels> {
    area(0., 100., 1000., 400.)
}
fn cover_drawn() -> Vec<Bounds<Pixels>> {
    vec![area(700., 120., 200., 60.)]
}
fn header(_: Pixels) -> Size<Pixels> {
    size(px(240.), px(40.))
}

#[test]
fn a_thing_over_a_drawn_part_moves_to_the_nearest_letterbox_spot() {
    let at = least_covering(
        point(px(744.), px(110.)),
        &header,
        None,
        &cover_drawn(),
        Some(cover_screen()),
        &[],
        size(px(COVER_AREA), px(600.)),
    );
    assert_eq!(at, point(px(744.), px(52.)));
}

#[test]
fn a_thing_over_nothing_drawn_stays_even_over_the_screen() {
    let home = point(px(100.), px(300.));
    let at = least_covering(
        home,
        &header,
        None,
        &cover_drawn(),
        Some(cover_screen()),
        &[],
        size(px(COVER_AREA), px(600.)),
    );
    assert_eq!(at, home);
}

#[test]
fn among_equally_near_spots_the_last_drawn_one_stays() {
    // A list too tall to pass above or below the part: left and right of it are equally near.
    let tall = |_: Pixels| size(px(240.), px(500.));
    for shown in [point(px(132.), px(50.)), point(px(668.), px(50.))] {
        let at = least_covering(
            point(px(400.), px(50.)),
            &tall,
            Some(shown),
            &[area(380., 120., 280., 60.)],
            None,
            &[],
            size(px(COVER_AREA), px(600.)),
        );
        assert_eq!(at, shown);
    }
}

#[test]
fn a_spot_on_something_kept_off_is_never_taken() {
    // The top letterbox's right half is taken, so the header goes to the bottom one.
    let at = least_covering(
        point(px(744.), px(110.)),
        &header,
        None,
        &cover_drawn(),
        Some(cover_screen()),
        &[area(600., 0., 400., 100.)],
        size(px(COVER_AREA), px(600.)),
    );
    assert_eq!(at, point(px(744.), px(508.)));
}

#[test]
fn with_no_spot_covering_less_the_thing_stays() {
    let home = point(px(744.), px(110.));
    let at = least_covering(
        home,
        &header,
        None,
        &[area(0., 0., 1000., 600.)],
        None,
        &[],
        size(px(COVER_AREA), px(600.)),
    );
    assert_eq!(at, home);
}

#[test]
fn a_capped_thing_is_sized_at_each_spot() {
    // Taller the higher it stands: at the top it would reach the drawn part, lower it is short.
    let tall = |y: Pixels| size(px(240.), (px(300.) - y).max(px(40.)));
    let at = least_covering(
        point(px(744.), px(110.)),
        &tall,
        None,
        &[area(700., 190., 200., 60.)],
        Some(cover_screen()),
        &[],
        size(px(COVER_AREA), px(600.)),
    );
    let b = Bounds::new(at, tall(at.y));
    assert!(!b.intersects(&area(700., 190., 200., 60.)), "{b:?}");
}

#[test]
fn a_folding_list_keeps_its_header_off_what_it_avoids() {
    // The part lies under the list's body and the box it avoids under its header: it folds,
    // and the header goes to the nearest spot clear of both.
    let kept_off = area(760., 20., 200., 30.);
    let part = area(700., 200., 100., 50.);
    let list = Floating {
        fold_first: true,
        avoid: [Some(kept_off), None],
        ..folding(area(700., 16., 240., 400.))
    };
    let at = settle_folding(&[list], &[], Some(part), size(px(1000.), px(600.)))[0];
    assert!(at.collapsed, "{at:?}");
    let header = Bounds::new(at.origin, size(px(240.), px(40.)));
    assert!(!header.intersects(&kept_off), "{at:?} on what it avoids");
    assert!(!header.intersects(&part), "{at:?} on the part");
}

#[test]
fn a_folding_list_keeps_off_both_boxes_it_avoids() {
    let (one, two) = (area(760., 20., 200., 30.), area(700., 80., 100., 30.));
    let part = area(700., 200., 100., 50.);
    let list = Floating {
        fold_first: true,
        avoid: [Some(one), Some(two)],
        ..folding(area(700., 16., 240., 400.))
    };
    let at = settle_folding(&[list], &[], Some(part), size(px(1000.), px(600.)))[0];
    let shown = match at.collapsed {
        true => Bounds::new(at.origin, size(px(240.), px(40.))),
        false => Bounds::new(at.origin, size(px(240.), px(400.))),
    };
    assert!(!shown.intersects(&one), "{at:?} on the first box");
    assert!(!shown.intersects(&two), "{at:?} on the second box");
    assert!(!shown.intersects(&part), "{at:?} on the part");
}

// The screen pixel under window point `at`, unrounded, so a zoom can be checked to keep it.
fn under(fit: &CanvasFit, at: gpui_kit::Point<Pixels>) -> (f32, f32) {
    let origin = fit.screen_bounds().origin;
    (
        f32::from(at.x - origin.x) / fit.scale(),
        f32::from(at.y - origin.y) / fit.scale(),
    )
}

#[test]
fn the_fit_zoom_is_the_plain_fit() {
    let room = area(100., 50., 1200., 540.);
    assert_eq!(
        CanvasFit::zoomed(room, HD, Zoom::FIT),
        CanvasFit::new(room, HD)
    );
    assert!(Zoom::FIT.is_fit() && !Zoom::FIT.can_zoom_out() && Zoom::FIT.can_zoom_in());
    assert_eq!(Zoom::FIT.label(), "100%");
}

#[test]
fn zooming_keeps_the_screen_pixel_under_the_anchor() {
    let room = area(100., 50., 960., 540.);
    let anchor = point(px(400.), px(300.));
    let before = under(&CanvasFit::new(room, HD), anchor);
    let zoom = Zoom::FIT.step(1, anchor, room, HD);
    let fit = CanvasFit::zoomed(room, HD, zoom);
    assert!(fit.scale() > 0.5, "{zoom:?}");
    let after = under(&fit, anchor);
    assert!(
        (before.0 - after.0).abs() < 0.01 && (before.1 - after.1).abs() < 0.01,
        "{before:?} became {after:?}"
    );
    assert_eq!(zoom.step(-1, anchor, room, HD), Zoom::FIT);
}

#[test]
fn zoom_stops_at_both_ends_and_labels_its_level() {
    let room = area(0., 0., 960., 540.);
    let mid = point(px(480.), px(270.));
    let mut zoom = Zoom::FIT;
    for _ in 0..20 {
        zoom = zoom.step(1, mid, room, HD);
    }
    assert!(!zoom.can_zoom_in() && zoom.can_zoom_out());
    assert_eq!(zoom.label(), "400%");
    assert_eq!(CanvasFit::zoomed(room, HD, zoom).scale(), 0.5 * 4.);
    assert_eq!(zoom.step(-20, mid, room, HD), Zoom::FIT);
}

#[test]
fn a_pan_moves_the_view_and_stops_at_the_screens_edges() {
    let room = area(0., 0., 960., 540.);
    let mid = point(px(480.), px(270.));
    let zoom = Zoom::FIT.step(3, mid, room, HD);
    let start = CanvasFit::zoomed(room, HD, zoom).screen_bounds();
    let panned = zoom.pan_by(point(px(-30.), px(20.)), room, HD);
    let moved = CanvasFit::zoomed(room, HD, panned).screen_bounds();
    assert_eq!(moved.origin - start.origin, point(px(-30.), px(20.)));
    // A pan far past the edge shows the screen's corner in the area's corner, never beyond.
    let far = zoom.pan_by(point(px(5000.), px(5000.)), room, HD);
    assert_eq!(
        CanvasFit::zoomed(room, HD, far).screen_bounds().origin,
        room.origin
    );
    let far = zoom.pan_by(point(px(-5000.), px(-5000.)), room, HD);
    let shown = CanvasFit::zoomed(room, HD, far).screen_bounds();
    assert_eq!(shown.bottom_right(), room.bottom_right());
    // At the fit nothing pans.
    assert_eq!(
        Zoom::FIT.pan_by(point(px(40.), px(40.)), room, HD),
        Zoom::FIT
    );
}
