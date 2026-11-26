//! Gaussian local linear smoothing of a run's angles and eye height.

use super::TrackSample;

/// The angle sigma in seconds.
pub(crate) const ANGLE_SIGMA: f32 = 0.015;

/// ANGLE_SIGMA in ticks for a tick length, never under one tick.
pub(crate) fn sigma_ticks(seconds_per_tick: f32) -> f64 {
    (f64::from(ANGLE_SIGMA) / f64::from(seconds_per_tick)).max(1.0)
}

/// The Gaussian local linear fit of `value` over `samples` (one run, ticks increasing,
/// non-empty) at `tick`.
pub(crate) fn fit(
    samples: &[TrackSample],
    tick: f64,
    sigma: f64,
    value: impl Fn(&TrackSample) -> f64,
) -> f64 {
    // Only samples within three sigma weigh anything; with none, the nearest one speaks.
    let reach = 3.0 * sigma;
    let lo = samples.partition_point(|s| f64::from(s.tick) < tick - reach);
    let hi = samples.partition_point(|s| f64::from(s.tick) <= tick + reach);
    if lo >= hi {
        return value(nearest(samples, tick));
    }

    // Weighted least squares of a line in x = sample tick - tick; its value at x = 0 is the fit.
    let (mut sw, mut swx, mut swxx, mut swy, mut swxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for s in &samples[lo..hi] {
        let x = f64::from(s.tick) - tick;
        let w = (-x * x / (2.0 * sigma * sigma)).exp();
        let y = value(s);
        sw += w;
        swx += w * x;
        swxx += w * x * x;
        swy += w * y;
        swxy += w * x * y;
    }
    let det = sw * swxx - swx * swx;
    if det <= 1e-9 * sw * sw {
        return swy / sw;
    }
    (swy * swxx - swx * swxy) / det
}

fn nearest(samples: &[TrackSample], tick: f64) -> &TrackSample {
    let at = samples.partition_point(|s| f64::from(s.tick) < tick);
    let before = at.checked_sub(1).map(|i| &samples[i]);
    match (before, samples.get(at)) {
        (Some(b), Some(a)) => {
            if tick - f64::from(b.tick) <= f64::from(a.tick) - tick {
                b
            } else {
                a
            }
        }
        (Some(only), None) | (None, Some(only)) => only,
        (None, None) => unreachable!("a run always holds a sample"),
    }
}

/// Smoothed yaw in 0..360, unwrapped around the sample nearest `tick`.
pub(crate) fn yaw(samples: &[TrackSample], tick: f64, sigma: f64) -> f32 {
    // Unwrapping around one reference keeps a turn across the seam continuous for the fit.
    let reference = f64::from(nearest(samples, tick).yaw) / 100.0;
    let fitted = fit(samples, tick, sigma, |s| {
        let raw = f64::from(s.yaw) / 100.0;
        reference + ((raw - reference + 180.0).rem_euclid(360.0) - 180.0)
    });
    let wrapped = fitted.rem_euclid(360.0) as f32;
    if wrapped >= 360.0 { 0.0 } else { wrapped }
}

/// Smoothed pitch, clamped to -89..=89.
pub(crate) fn pitch(samples: &[TrackSample], tick: f64, sigma: f64) -> f32 {
    fit(samples, tick, sigma, |s| f64::from(s.pitch) / 100.0).clamp(-89.0, 89.0) as f32
}

/// Smoothed eye height above the feet.
pub(crate) fn eye(samples: &[TrackSample], tick: f64, sigma: f64) -> f32 {
    fit(samples, tick, sigma, |s| f64::from(s.eye_z)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::tracks::pack_sample;

    fn run(points: &[(u32, f32, f32, f32)]) -> Vec<TrackSample> {
        points
            .iter()
            .map(|&(tick, yaw, pitch, eye)| pack_sample(tick, [1., 1., 1.], yaw, pitch, eye, false))
            .collect()
    }

    fn ramp(ticks: impl IntoIterator<Item = u32>) -> Vec<TrackSample> {
        let points: Vec<_> = ticks
            .into_iter()
            .map(|t| (t, 10. + 2. * t as f32, 0., 68.))
            .collect();
        run(&points)
    }

    #[test]
    fn sigma_is_a_tick_at_66_and_never_less() {
        assert!((sigma_ticks(1. / 66.) - 1.).abs() < 1e-6);
        assert!((sigma_ticks(1. / 33.) - 1.).abs() < 1e-6);
        assert!((sigma_ticks(1. / 132.) - 1.98).abs() < 1e-3);
    }

    #[test]
    fn a_steady_turn_has_no_lag_inside_or_at_the_ends() {
        let s = ramp(0..=20);
        for (t, want) in [
            (10.5, 31.),
            (0., 10.),
            (20., 50.),
            (0.3, 10.6),
            (19.7, 49.4),
        ] {
            let got = yaw(&s, t, 1.);
            assert!((got - want).abs() < 1e-3, "tick {t}: {got} vs {want}");
        }
    }

    #[test]
    fn uneven_spacing_keeps_a_steady_turn() {
        let s = ramp([0, 1, 3, 4, 7, 8, 9]);
        let got = yaw(&s, 2., 1.);
        assert!((got - 14.).abs() < 1e-3, "{got}");
    }

    #[test]
    fn a_quantised_staircase_comes_out_smoother() {
        let points: Vec<_> = (0..=60u32)
            .map(|t| (t, (0.1 * t as f32 / 0.35).round() * 0.35, 0., 68.))
            .collect();
        let s = run(&points);
        let raw: Vec<f64> = (10..=50).map(|t| f64::from(points[t].1)).collect();
        let smooth: Vec<f64> = (10..=50)
            .map(|t| f64::from(yaw(&s, t as f64, 1.)))
            .collect();
        let d2 = |v: &[f64]| {
            v.windows(3)
                .map(|w| (w[2] - 2. * w[1] + w[0]).abs())
                .sum::<f64>()
        };
        assert!(
            d2(&smooth) < 0.5 * d2(&raw),
            "{} vs {}",
            d2(&smooth),
            d2(&raw)
        );
    }

    #[test]
    fn yaw_turns_the_short_way_across_the_seam() {
        let points: Vec<_> = (0..=10u32).map(|t| (t, 175. + t as f32, 0., 68.)).collect();
        let s = run(&points);
        let at5 = yaw(&s, 5., 1.);
        assert!((at5 - 180.).abs() < 1e-2, "{at5}");
        let at7 = yaw(&s, 7., 1.);
        assert!((at7 - 182.).abs() < 1e-2, "{at7}");
        let wrap: Vec<_> = (0..=10u32).map(|t| (t, 355. + t as f32, 0., 68.)).collect();
        let s = run(&wrap);
        let at5 = yaw(&s, 5., 1.);
        assert!((0. ..360.).contains(&at5), "{at5}");
        assert!(at5.min(360. - at5) < 1e-2, "{at5}");
        assert!((yaw(&s, 4.5, 1.) - 359.5).abs() < 1e-2);
    }

    #[test]
    fn one_sample_returns_itself() {
        let s = run(&[(5, 42., 10., 68.)]);
        assert!((yaw(&s, 5.5, 1.) - 42.).abs() < 1e-3);
        assert!((pitch(&s, 5.5, 1.) - 10.).abs() < 1e-3);
        assert!((eye(&s, 5.5, 1.) - 68.).abs() < 1e-3);
    }

    #[test]
    fn pitch_is_clamped_after_the_fit() {
        let points: Vec<_> = (0..10u32).map(|t| (t, 0., 80. + t as f32, 68.)).collect();
        let s = run(&points);
        assert_eq!(pitch(&s, 9.9, 1.), 89.);
    }

    #[test]
    fn a_crouch_eases_the_eye() {
        let points: Vec<_> = (0..=20u32)
            .map(|t| (t, 0., 0., if t < 10 { 68. } else { 45. }))
            .collect();
        let s = run(&points);
        let at10 = eye(&s, 10., 1.);
        assert!(at10 > 48. && at10 < 56., "{at10}");
        assert!((eye(&s, 2., 1.) - 68.).abs() < 1e-3);
    }
}
