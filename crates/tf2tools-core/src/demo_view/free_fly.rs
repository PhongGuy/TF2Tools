//! TF2's spectator free-fly: a camera that accelerates toward the keys held and slides to a
//! stop, stepped at a fixed rate so the path does not depend on the frame rate.

/// TF2's defaults: specspeed 3, maxspeed 320, specaccelerate 5, friction 4, move speed 450.
const SPEC_SPEED: f32 = 3.0;
const MAX_SPEED: f32 = 320.0 * SPEC_SPEED;
const ACCELERATE: f32 = 5.0;
const FRICTION: f32 = 4.0;
const MOVE_SPEED: f32 = 450.0;
/// The fixed physics step, and the longest frame that is simulated.
const STEP: f32 = 1.0 / 240.0;
const LONGEST_FRAME: f32 = 0.25;
const MAX_PITCH: f32 = 89.0;

/// The movement keys held for a frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub struct FlyInput {
    /// -1 back to 1 forward.
    pub forward: f32,
    /// -1 left to 1 right.
    pub side: f32,
    /// Shift: half the wish speed.
    pub slow: bool,
}

impl FlyInput {
    /// An input with each axis in -1..=1.
    pub fn new(forward: f32, side: f32, slow: bool) -> Self {
        Self {
            forward,
            side,
            slow,
        }
    }
}

/// A free-flying camera in Source units: x forward at yaw 0, y left, z up.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FreeFly {
    origin: [f32; 3],
    velocity: [f32; 3],
    yaw: f32,
    pitch: f32,
    accumulator: f32,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

impl FreeFly {
    /// A camera at rest; `pitch_deg` is clamped to +-89.
    pub fn new(origin: [f32; 3], yaw_deg: f32, pitch_deg: f32) -> Self {
        Self {
            origin,
            velocity: [0.0; 3],
            yaw: yaw_deg,
            pitch: pitch_deg.clamp(-MAX_PITCH, MAX_PITCH),
            accumulator: 0.0,
        }
    }

    /// Turns the view by the given degrees; pitch stays within +-89.
    pub fn look(&mut self, d_yaw_deg: f32, d_pitch_deg: f32) {
        self.yaw += d_yaw_deg;
        self.pitch = (self.pitch + d_pitch_deg).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Moves the camera by `dt_seconds` (at most a quarter second counts) in whole fixed
    /// steps, carrying the remainder to the next call.
    pub fn advance(&mut self, dt_seconds: f32, input: FlyInput) {
        self.accumulator += dt_seconds.min(LONGEST_FRAME);
        while self.accumulator >= STEP {
            self.accumulator -= STEP;
            self.step(input);
        }
    }

    /// One fixed step: accelerate toward the wish velocity, apply friction, move.
    fn step(&mut self, input: FlyInput) {
        let factor = if input.slow {
            SPEC_SPEED / 2.0
        } else {
            SPEC_SPEED
        };
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        let ahead = [cp * cy, cp * sy, -sp];
        let right = [sy, -cy, 0.0];
        let (f, s) = (
            MOVE_SPEED * input.forward * factor,
            MOVE_SPEED * input.side * factor,
        );
        let wish = [0, 1, 2].map(|i| ahead[i] * f + right[i] * s);
        let raw = length(wish);
        // With no keys held the wish vector is zero and is never normalised.
        if raw > 0.0 {
            let wish_speed = raw.min(MAX_SPEED);
            let dir = wish.map(|c| c / raw);
            let add = wish_speed - dot(self.velocity, dir);
            if add > 0.0 {
                let gain = (ACCELERATE * STEP * wish_speed).min(add);
                for (v, d) in self.velocity.iter_mut().zip(dir) {
                    *v += d * gain;
                }
            }
        }
        let speed = length(self.velocity);
        if speed < 1.0 {
            self.velocity = [0.0; 3];
        } else {
            let control = speed.max(MAX_SPEED / 4.0);
            let keep = (speed - control * FRICTION * STEP).max(0.0) / speed;
            self.velocity = self.velocity.map(|c| c * keep);
        }
        for (o, v) in self.origin.iter_mut().zip(self.velocity) {
            *o += v * STEP;
        }
    }

    /// Where the camera is.
    pub fn origin(&self) -> [f32; 3] {
        self.origin
    }

    /// The view's yaw in degrees, counter-clockwise from +x.
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// The view's pitch in degrees, positive looking down.
    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    /// The current speed in units a second.
    pub fn speed(&self) -> f32 {
        length(self.velocity)
    }

    /// Whether the camera moves at one unit a second or more.
    pub fn is_moving(&self) -> bool {
        self.speed() >= 1.0
    }
}
