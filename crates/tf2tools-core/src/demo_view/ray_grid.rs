//! A sparse uniform grid over a map's triangles, walked cell by cell to find a ray's nearest hit.

/// Edge length of one grid cell in map units.
pub(crate) const CELL: f32 = 128.0;
/// Most `(cell, triangle)` entries a grid may hold before it is refused.
pub(crate) const MAX_GRID_ENTRIES: usize = 16_000_000;

/// Triangle ids filed by cell: sorted cell keys, each owning a slice of `tris`, plus the padded
/// bounds of every filed triangle.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RayGrid {
    cell: f32,
    min: [f32; 3],
    max: [f32; 3],
    keys: Vec<u64>,
    starts: Vec<u32>,
    tris: Vec<u32>,
}

// Keys pack three 21-bit biased cell coordinates; triangles past half that range are not filed,
// so a padded bound or a clamped walk never leaves the key space.
const BIAS: i64 = 1 << 20;
const REACH: f32 = (1 << 19) as f32;

fn key(c: [i64; 3]) -> u64 {
    let [x, y, z] = c.map(|v| (v + BIAS) as u64);
    (x << 42) | (y << 21) | z
}

// The cell box `(lo, hi)` a triangle covers, or None when it is non-finite or out of reach.
fn cell_box(tri: &[[f32; 3]], cell: f32) -> Option<([i64; 3], [i64; 3])> {
    // `f32::min` and `max` skip a NaN, so one bad corner is refused here before the fold.
    if !tri.iter().flatten().all(|v| v.is_finite()) {
        return None;
    }
    let mut lo = [0; 3];
    let mut hi = [0; 3];
    for a in 0..3 {
        let low = (tri.iter().map(|v| v[a]).fold(f32::INFINITY, f32::min) / cell).floor();
        let high = (tri.iter().map(|v| v[a]).fold(f32::NEG_INFINITY, f32::max) / cell).floor();
        if !(low.is_finite() && high.is_finite() && low >= -REACH && high <= REACH) {
            return None;
        }
        (lo[a], hi[a]) = (low as i64, high as i64);
    }
    Some((lo, hi))
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// Two-sided Moller-Trumbore: the distance along `d` to the triangle, if it is hit ahead.
fn triangle_hit(o: [f32; 3], d: [f32; 3], t: &[[f32; 3]]) -> Option<f32> {
    let (e1, e2) = (sub(t[1], t[0]), sub(t[2], t[0]));
    let pv = cross(d, e2);
    let det = dot(e1, pv);
    if det.abs() < 1e-8 {
        return None;
    }
    let tv = sub(o, t[0]);
    let u = dot(tv, pv) / det;
    if !(0. ..=1.).contains(&u) {
        return None;
    }
    let q = cross(tv, e1);
    let v = dot(d, q) / det;
    if v < 0. || u + v > 1. {
        return None;
    }
    let dist = dot(e2, q) / det;
    (dist >= 0.).then_some(dist)
}

impl RayGrid {
    /// None when filing would pass `max_entries` (counted before allocating) or nothing is filed.
    pub fn build(positions: &[[f32; 3]], cell: f32, max_entries: usize) -> Option<RayGrid> {
        if !(cell.is_finite() && cell > 0.) {
            return None;
        }
        // First pass: count every box's entries and gather the bounds, so a huge triangle is
        // refused before anything is allocated for it.
        let cap = u64::try_from(max_entries).unwrap_or(u64::MAX);
        let (mut total, mut min, mut max) = (0u64, [f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for tri in positions.as_chunks::<3>().0 {
            let Some((lo, hi)) = cell_box(tri, cell) else {
                continue;
            };
            let count = (0..3).fold(1u64, |n, a| n.saturating_mul((hi[a] - lo[a] + 1) as u64));
            total = total.saturating_add(count);
            if total > cap {
                return None;
            }
            for v in tri {
                for a in 0..3 {
                    min[a] = min[a].min(v[a]);
                    max[a] = max[a].max(v[a]);
                }
            }
        }
        if total == 0 {
            return None;
        }

        let mut pairs: Vec<(u64, u32)> = Vec::with_capacity(usize::try_from(total).ok()?);
        for (t, tri) in positions.as_chunks::<3>().0.iter().enumerate() {
            let Some((lo, hi)) = cell_box(tri, cell) else {
                continue;
            };
            let id = u32::try_from(t).ok()?;
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        pairs.push((key([x, y, z]), id));
                    }
                }
            }
        }
        pairs.sort_unstable();
        let (mut keys, mut starts, mut tris) =
            (Vec::new(), Vec::new(), Vec::with_capacity(pairs.len()));
        for (k, t) in pairs {
            if keys.last() != Some(&k) {
                keys.push(k);
                starts.push(u32::try_from(tris.len()).ok()?);
            }
            tris.push(t);
        }
        starts.push(u32::try_from(tris.len()).ok()?);
        Some(RayGrid {
            cell,
            min: min.map(|v| v - 1.),
            max: max.map(|v| v + 1.),
            keys,
            starts,
            tris,
        })
    }

    /// Nearest two-sided hit along unit `dir` within `max_distance`.
    pub fn hit(
        &self,
        positions: &[[f32; 3]],
        origin: [f32; 3],
        dir: [f32; 3],
        max_distance: f32,
    ) -> Option<f32> {
        // Clip the segment to the filed bounds first, so far origins and huge distances only
        // walk the cells the map actually occupies.
        let (mut enter, mut leave) = (0f32, max_distance);
        for a in 0..3 {
            if dir[a] == 0. {
                if origin[a] < self.min[a] || origin[a] > self.max[a] {
                    return None;
                }
                continue;
            }
            let (t0, t1) = (
                (self.min[a] - origin[a]) / dir[a],
                (self.max[a] - origin[a]) / dir[a],
            );
            enter = enter.max(t0.min(t1));
            leave = leave.min(t0.max(t1));
        }
        if enter.is_nan() || leave.is_nan() || enter > leave {
            return None;
        }

        // Amanatides-Woo from the entry point, clamped to the bounds' cells against rounding;
        // distances stay measured from `origin`.
        let entry = [0, 1, 2].map(|a| origin[a] + dir[a] * enter);
        let lo = self.min.map(|v| (v / self.cell).floor() as i64);
        let hi = self.max.map(|v| (v / self.cell).floor() as i64);
        let mut c = [0, 1, 2].map(|a| ((entry[a] / self.cell).floor() as i64).clamp(lo[a], hi[a]));
        let step = dir.map(|d| if d >= 0. { 1i64 } else { -1 });
        let delta = dir.map(|d| {
            if d == 0. {
                f32::INFINITY
            } else {
                (self.cell / d).abs()
            }
        });
        let mut next = [0, 1, 2].map(|a| {
            if dir[a] == 0. {
                return f32::INFINITY;
            }
            let edge = (c[a] + i64::from(dir[a] > 0.)) as f32 * self.cell;
            enter + (edge - entry[a]) / dir[a]
        });
        let mut best: Option<f32> = None;
        loop {
            if let Ok(k) = self.keys.binary_search(&key(c)) {
                let ids = &self.tris[self.starts[k] as usize..self.starts[k + 1] as usize];
                for &t in ids {
                    let t = t as usize;
                    if let Some(h) = positions
                        .get(t * 3..t * 3 + 3)
                        .and_then(|tri| triangle_hit(origin, dir, tri))
                        && h <= max_distance
                        && best.is_none_or(|b| h < b)
                    {
                        best = Some(h);
                    }
                }
            }
            let a = if next[0] <= next[1] && next[0] <= next[2] {
                0
            } else if next[1] <= next[2] {
                1
            } else {
                2
            };
            let exit = next[a];
            if best.is_some_and(|b| b <= exit) || exit > leave {
                return best;
            }
            c[a] += step[a];
            if c[a] < lo[a] || c[a] > hi[a] {
                return best;
            }
            next[a] += delta[a];
        }
    }

    /// Heap bytes held by the grid's tables.
    pub fn heap_bytes(&self) -> usize {
        self.keys.capacity() * size_of::<u64>()
            + (self.starts.capacity() + self.tris.capacity()) * size_of::<u32>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(out: &mut Vec<[f32; 3]>, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) {
        out.extend([a, b, c, a, c, d]);
    }

    fn floor(z: f32) -> Vec<[f32; 3]> {
        let mut p = Vec::new();
        quad(
            &mut p,
            [-1000., -1000., z],
            [1000., -1000., z],
            [1000., 1000., z],
            [-1000., 1000., z],
        );
        p
    }

    fn near(a: Option<f32>, b: f32) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-3)
    }

    #[test]
    fn a_ray_hits_a_floor_from_either_side() {
        let p = floor(0.);
        let g = RayGrid::build(&p, CELL, MAX_GRID_ENTRIES).expect("grid");
        assert!(near(g.hit(&p, [10., 20., 50.], [0., 0., -1.], 96.), 50.));
        assert!(near(g.hit(&p, [10., 20., -30.], [0., 0., 1.], 96.), 30.));
        assert_eq!(g.hit(&p, [10., 20., 50.], [0., 0., -1.], 40.), None);
        assert_eq!(g.hit(&p, [10., 20., 50.], [1., 0., 0.], 96.), None);
        assert_eq!(g.hit(&p, [10., 20., 50.], [0., 0., 1.], 96.), None);
    }

    #[test]
    fn the_nearest_of_two_surfaces_wins() {
        let mut p = floor(0.);
        p.extend(floor(20.));
        let g = RayGrid::build(&p, CELL, MAX_GRID_ENTRIES).expect("grid");
        assert!(near(g.hit(&p, [5., 5., 50.], [0., 0., -1.], 96.), 30.));
    }

    #[test]
    fn the_grid_matches_one_big_cell() {
        let mut p = floor(0.);
        quad(
            &mut p,
            [300., -1000., -1000.],
            [300., 1000., -1000.],
            [300., 1000., 1000.],
            [300., -1000., 1000.],
        );
        quad(
            &mut p,
            [-200., 0., 0.],
            [-150., 40., 0.],
            [-150., 40., 300.],
            [-200., 0., 300.],
        );
        let small = RayGrid::build(&p, CELL, MAX_GRID_ENTRIES).expect("grid");
        let whole = RayGrid::build(&p, 1.0e6, MAX_GRID_ENTRIES).expect("grid");
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 40) as f32 / (1u64 << 24) as f32
        };
        let mut hits = 0;
        for _ in 0..2000 {
            let o = [rnd() * 800. - 400., rnd() * 800. - 400., rnd() * 300. - 50.];
            let d = [rnd() * 2. - 1., rnd() * 2. - 1., rnd() * 2. - 1.];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-3);
            let d = d.map(|c| c / len);
            let (a, b) = (small.hit(&p, o, d, 300.), whole.hit(&p, o, d, 300.));
            hits += usize::from(b.is_some());
            match (a, b) {
                (Some(a), Some(b)) => assert!((a - b).abs() < 1e-2, "{o:?} {d:?}: {a} vs {b}"),
                (a, b) => assert_eq!(a, b, "{o:?} {d:?}"),
            }
        }
        assert!(hits > 200, "{hits}");
    }

    #[test]
    fn a_grid_past_its_cap_is_not_built() {
        let p = floor(0.);
        assert!(RayGrid::build(&p, 1.0, 10).is_none());
        assert!(RayGrid::build(&[], CELL, MAX_GRID_ENTRIES).is_none());
        assert!(
            RayGrid::build(&p, CELL, MAX_GRID_ENTRIES)
                .expect("grid")
                .heap_bytes()
                > 0
        );
    }

    #[test]
    fn a_triangle_with_a_nan_corner_is_not_filed() {
        let mut p = floor(0.);
        p.extend([[f32::NAN, 0., 0.], [1000., 0., 0.], [0., 1000., 0.]]);
        let with_nan = RayGrid::build(&p, CELL, MAX_GRID_ENTRIES).expect("grid");
        let clean = RayGrid::build(&floor(0.), CELL, MAX_GRID_ENTRIES).expect("grid");
        assert_eq!(with_nan.heap_bytes(), clean.heap_bytes());
    }

    #[test]
    fn far_rays_end_without_walking_empty_space() {
        let p = floor(0.);
        let g = RayGrid::build(&p, CELL, MAX_GRID_ENTRIES).expect("grid");
        assert_eq!(g.hit(&p, [1e30, 0., 50.], [0., 0., -1.], 96.), None);
        let far = g
            .hit(&p, [0., 0., 1e6], [0., 0., -1.], 1e30)
            .expect("hit from far above");
        assert!((far - 1e6).abs() < 1., "{far}");
        assert!(near(g.hit(&p, [10., 20., 50.], [0., 0., -1.], 1e30), 50.));
        assert_eq!(g.hit(&p, [10., 20., 50.], [0., 0., 1.], 1e30), None);
    }
}
