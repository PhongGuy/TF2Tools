//! The demo playback clock and the frame-time rule it shares with the free-fly camera.

/// The longest frame that counts; a stall or a resumed window does not jump ahead.
const LONGEST_FRAME: f32 = 0.25;
const MIN_SPEED: f32 = 0.25;
const MAX_SPEED: f32 = 4.0;
const DEFAULT_SECONDS_PER_TICK: f64 = 1.0 / 66.0;

/// A frame time that is safe to accumulate: NaN or not positive reads as no time, and a
/// frame never counts for more than a quarter second.
pub(crate) fn sane_dt(dt: f32) -> f32 {
    if dt.is_nan() || dt <= 0.0 {
        0.0
    } else {
        dt.min(LONGEST_FRAME)
    }
}

/// Where a demo's playback is, in fractional ticks, and whether and how fast it runs.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PlaybackClock {
    first: u32,
    last: u32,
    seconds_per_tick: f64,
    tick: f64,
    speed: f32,
    playing: bool,
}

impl PlaybackClock {
    /// A paused clock at `first_tick` at normal speed. A `last_tick` below the first gives an
    /// empty range, and a non-finite or non-positive tick interval reads as 66 tick.
    pub fn new(first_tick: u32, last_tick: u32, seconds_per_tick: f32) -> Self {
        let spt = f64::from(seconds_per_tick);
        Self {
            first: first_tick,
            last: last_tick.max(first_tick),
            seconds_per_tick: if spt.is_finite() && spt > 0.0 {
                spt
            } else {
                DEFAULT_SECONDS_PER_TICK
            },
            tick: f64::from(first_tick),
            speed: 1.0,
            playing: false,
        }
    }

    /// The current tick, possibly between two whole ticks.
    pub fn tick(&self) -> f64 {
        self.tick
    }

    /// Whether the clock is running.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// The playback speed, 0.25 to 4.
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Seconds played since the first tick.
    pub fn elapsed_seconds(&self) -> f64 {
        (self.tick - f64::from(self.first)) * self.seconds_per_tick
    }

    /// Seconds from the first tick to the last.
    pub fn total_seconds(&self) -> f64 {
        f64::from(self.last - self.first) * self.seconds_per_tick
    }

    /// Moves a playing clock on by one frame and says whether the tick changed. Reaching the
    /// last tick stops there and pauses.
    pub fn advance(&mut self, dt_seconds: f32) -> bool {
        if !self.playing {
            return false;
        }
        let before = self.tick;
        let ticks = f64::from(sane_dt(dt_seconds) * self.speed) / self.seconds_per_tick;
        let last = f64::from(self.last);
        self.tick = (self.tick + ticks).min(last);
        if self.tick >= last {
            self.playing = false;
        }
        self.tick != before
    }

    /// Jumps to `tick`, clamped into range, keeping the play state; NaN does not move.
    pub fn seek(&mut self, tick: f64) {
        if !tick.is_nan() {
            self.tick = tick.clamp(f64::from(self.first), f64::from(self.last));
        }
    }

    /// Pauses and moves `delta` whole ticks from the whole tick at or below the current one.
    pub fn step(&mut self, delta: i32) {
        self.playing = false;
        self.seek(self.tick.floor() + f64::from(delta));
    }

    /// Sets the speed, clamped to 0.25..=4; NaN is ignored.
    pub fn set_speed(&mut self, speed: f32) {
        if !speed.is_nan() {
            self.speed = speed.clamp(MIN_SPEED, MAX_SPEED);
        }
    }

    /// Starts playing; from the last tick it starts over at the first.
    pub fn play(&mut self) {
        if self.tick >= f64::from(self.last) {
            self.tick = f64::from(self.first);
        }
        self.playing = true;
    }

    /// Stops playing where it is.
    pub fn pause(&mut self) {
        self.playing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_time_is_never_negative_nan_or_past_a_quarter_second() {
        assert_eq!(sane_dt(-1.), 0.);
        assert_eq!(sane_dt(f32::NAN), 0.);
        assert_eq!(sane_dt(f32::INFINITY), 0.25);
        assert_eq!(sane_dt(0.016), 0.016);
        assert_eq!(sane_dt(3.), 0.25);
    }
}
