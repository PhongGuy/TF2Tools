//! Monotone cubic Hermite interpolation over a short window of knots.

/// The Fritsch-Carlson monotone cubic through the knots `(xs, ys)` at `x`, clamped to the ends.
/// `xs` strictly increases and both slices share one non-zero length.
pub(crate) fn monotone(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let n = xs.len().min(ys.len());
    if n < 2 || x <= xs[0] {
        return ys[0];
    }
    if x >= xs[n - 1] {
        return ys[n - 1];
    }
    let i = (xs[..n].partition_point(|&k| k <= x) - 1).min(n - 2);
    let h = xs[i + 1] - xs[i];
    let t = (x - xs[i]) / h;
    let (m0, m1) = (tangent(xs, ys, n, i), tangent(xs, ys, n, i + 1));
    let (t2, t3) = (t * t, t * t * t);
    (2. * t3 - 3. * t2 + 1.) * ys[i]
        + (t3 - 2. * t2 + t) * h * m0
        + (3. * t2 - 2. * t3) * ys[i + 1]
        + (t3 - t2) * h * m1
}

// The slope at knot `k`: the adjacent secant at an end, flat at a turn or a stop, and otherwise
// Brodlie's weighted harmonic mean of the two secants, which keeps every segment monotone.
fn tangent(xs: &[f64], ys: &[f64], n: usize, k: usize) -> f64 {
    let secant = |j: usize| (ys[j + 1] - ys[j]) / (xs[j + 1] - xs[j]);
    if k == 0 {
        return secant(0);
    }
    if k == n - 1 {
        return secant(n - 2);
    }
    let (d0, d1) = (secant(k - 1), secant(k));
    if d0 == 0. || d1 == 0. || d0.signum() != d1.signum() {
        return 0.;
    }
    let (h0, h1) = (xs[k] - xs[k - 1], xs[k + 1] - xs[k]);
    3. * (h0 + h1) / ((2. * h1 + h0) / d0 + (h1 + 2. * h0) / d1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(xs: &[f64], ys: &[f64], from: f64, to: f64) -> Vec<f64> {
        (0..=1000)
            .map(|i| monotone(xs, ys, from + (to - from) * i as f64 / 1000.))
            .collect()
    }

    #[test]
    fn it_passes_through_every_knot() {
        let (xs, ys) = ([0., 1., 3., 4.], [0., 2., 1., 5.]);
        for (x, y) in xs.iter().zip(ys) {
            assert!((monotone(&xs, &ys, *x) - y).abs() < 1e-12, "{x}");
        }
    }

    #[test]
    fn two_points_are_a_line_and_one_point_is_flat() {
        assert!((monotone(&[0., 4.], &[0., 8.], 1.) - 2.).abs() < 1e-12);
        assert_eq!(monotone(&[3.], &[7.], 10.), 7.);
    }

    #[test]
    fn evenly_spaced_linear_data_stays_linear() {
        let (xs, ys) = ([0., 2., 4., 6.], [0., 20., 40., 60.]);
        for x in [0.5, 1., 2.5, 5.9] {
            assert!((monotone(&xs, &ys, x) - x * 10.).abs() < 1e-9, "{x}");
        }
    }

    #[test]
    fn a_stop_does_not_overshoot() {
        let (xs, ys) = ([0., 1., 2., 3., 4.], [0., 10., 20., 20., 20.]);
        let run = samples(&xs, &ys, 0., 2.);
        assert!(
            run.windows(2).all(|w| w[1] >= w[0]),
            "rising part is monotone"
        );
        assert!(
            run.iter().all(|&y| (0. ..=20.).contains(&y)),
            "stays in range"
        );
        assert!(
            samples(&xs, &ys, 2., 4.)
                .iter()
                .all(|&y| (y - 20.).abs() < 1e-9),
            "held flat after the stop"
        );
    }

    #[test]
    fn a_landing_does_not_dip_below_the_floor() {
        let (xs, ys) = ([0., 1., 2., 3., 4., 5.], [100., 90., 60., 0., 0., 0.]);
        let run = samples(&xs, &ys, 0., 5.);
        assert!(run.iter().all(|&y| y >= 0.), "never below the floor");
        assert!(run.windows(2).all(|w| w[1] <= w[0]), "never rises");
    }

    #[test]
    fn velocity_is_continuous_through_each_knot_on_uneven_spacing() {
        let (xs, ys) = ([0., 1., 3., 4., 7.], [0., 2., 5., 6., 9.]);
        let e = 1e-6;
        for &k in &xs[1..4] {
            let left = (monotone(&xs, &ys, k) - monotone(&xs, &ys, k - e)) / e;
            let right = (monotone(&xs, &ys, k + e) - monotone(&xs, &ys, k)) / e;
            assert!((left - right).abs() < 1e-3, "knot {k}: {left} vs {right}");
        }
    }

    #[test]
    fn outside_the_knots_it_holds_the_ends() {
        let (xs, ys) = ([0., 1., 2.], [5., 6., 9.]);
        assert_eq!(monotone(&xs, &ys, -3.), 5.);
        assert_eq!(monotone(&xs, &ys, 9.), 9.);
    }
}
