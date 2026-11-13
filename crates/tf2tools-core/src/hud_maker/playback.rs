//! Playing animation events to one moment: a pure simulation from 0 to `t`, following
//! `AnimationController`'s rules for delays, replacing animations, RunEvent and StopEvent.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use super::animations::atof;
use super::expr::atoi;
use super::{
    AnimCommand, HudAnimations, HudLayout, HudModel, Interp, PartKey, PosExpr, Rect, Rgba,
    ScreenScale,
};

/// Most commands one simulation executes, so a zero-delay loop still returns.
const MAX_EXECUTED: usize = 4096;
/// The length shown for events that loop.
const LOOP_LENGTH: f32 = 2.0;
/// The engine's colour for a name the scheme lacks.
const MAGENTA: Rgba = Rgba::new(255, 0, 255, 255);

/// One event the game starts, within a part (`None`: the whole HUD).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fired {
    event: String,
    within: Option<PartKey>,
}

impl Fired {
    /// Event `event` started within `within`, or over the whole HUD.
    pub fn new(event: &str, within: Option<PartKey>) -> Fired {
        Fired {
            event: event.to_owned(),
            within,
        }
    }

    /// The event's name.
    pub fn event(&self) -> &str {
        &self.event
    }

    /// The part whose children the event's panel names are found under.
    pub fn within(&self) -> Option<&PartKey> {
        self.within.as_ref()
    }
}

impl Interp {
    /// Progress 0..=1 shaped as GetInterpolatedValue does; Flicker uses `seed`.
    pub fn at(self, progress: f32, seed: u32) -> f32 {
        let p = if progress.is_nan() {
            0.0
        } else {
            progress.clamp(0.0, 1.0)
        };
        let out = match self {
            Interp::Linear => p,
            Interp::Accel => p * p,
            Interp::Deaccel => p.sqrt(),
            Interp::Spline => p * p * (3.0 - 2.0 * p),
            Interp::Pulse(freq) => 0.5 + 0.5 * (p * std::f32::consts::TAU * freq).cos(),
            Interp::Flicker(chance) => {
                // A fixed hash of the seed and moment stands in for the game's random draw.
                let roll = mix(seed ^ p.to_bits()) as f32 / u32::MAX as f32;
                if roll < chance { 1.0 } else { 0.0 }
            }
            Interp::Gain(g) => gain(p, g),
            Interp::Bias(b) => bias(p, b),
            Interp::Bounce => bounce(p),
        };
        if out.is_finite() { out } else { p }
    }
}

fn bias(x: f32, b: f32) -> f32 {
    x.powf(b.ln() / 0.5f32.ln())
}

fn gain(x: f32, g: f32) -> f32 {
    if x < 0.5 {
        bias(2.0 * x, 1.0 - g) / 2.0
    } else {
        1.0 - bias(2.0 - 2.0 * x, 1.0 - g) / 2.0
    }
}

/// Source's Interpolator_Bounce: starts at the end, falls to the start and back by 0.33,
/// then dips to 0.5 and 0.8 before settling at the end.
fn bounce(p: f32) -> f32 {
    use std::f32::consts::PI;
    let (h1, h2) = (0.33, 0.67);
    if p < h1 {
        1.0 - (PI * p / h1).sin()
    } else if p < h2 {
        0.5 + 0.5 * (1.0 - (PI * (p - h1) / (h2 - h1)).sin())
    } else {
        0.8 + 0.2 * (1.0 - (PI * (p - h2) / (1.0 - h2)).sin())
    }
}

/// A small integer hash, enough to spread flicker draws.
fn mix(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// What playback changed on one part.
#[non_exhaustive]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Animated {
    fg: Option<Rgba>,
    bg: Option<Rgba>,
    alpha: Option<u8>,
    rect: Option<Rect>,
    visible: Option<bool>,
}

impl Animated {
    /// The text colour, when played.
    pub fn fg(&self) -> Option<Rgba> {
        self.fg
    }

    /// The background colour, when played.
    pub fn bg(&self) -> Option<Rgba> {
        self.bg
    }

    /// The part's alpha, when played.
    pub fn alpha(&self) -> Option<u8> {
        self.alpha
    }

    /// The box in screen pixels, when moved or sized.
    pub fn rect(&self) -> Option<Rect> {
        self.rect
    }

    /// Shown or hidden by SetVisible, when it ran.
    pub fn visible(&self) -> Option<bool> {
        self.visible
    }
}

/// Every part playback changed at one moment, and the fields it saw but does not play.
#[non_exhaustive]
#[derive(Debug, Clone, Default)]
pub struct AnimFrame {
    parts: HashMap<PartKey, Animated>,
    in_game_only: Vec<String>,
}

impl AnimFrame {
    /// What playback changed on `key`.
    pub fn get(&self, key: &PartKey) -> Option<&Animated> {
        self.parts.get(key)
    }

    /// Field names seen but not played, first spelling of each.
    pub fn in_game_only(&self) -> &[String] {
        &self.in_game_only
    }

    /// `layout` with moved boxes at their played place and descendants shifted with them.
    pub fn laid_out(&self, layout: &HudLayout) -> HudLayout {
        let rects = (self.parts.iter())
            .filter_map(|(k, a)| a.rect.map(|r| (k.clone(), r)))
            .collect();
        layout.moved(&rects)
    }
}

/// A field playback animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Field {
    Fg,
    Bg,
    Alpha,
    X,
    Y,
    Wide,
    Tall,
}

impl Field {
    /// The fields an Animate field name drives; empty when it is not played.
    fn named(name: &str) -> &'static [Field] {
        match name.to_ascii_lowercase().as_str() {
            "fgcolor" => &[Field::Fg],
            "bgcolor" => &[Field::Bg],
            "alpha" => &[Field::Alpha],
            "xpos" => &[Field::X],
            "ypos" => &[Field::Y],
            "wide" => &[Field::Wide],
            "tall" => &[Field::Tall],
            "position" => &[Field::X, Field::Y],
            "size" => &[Field::Wide, Field::Tall],
            _ => &[],
        }
    }
}

/// A field's value: colours per channel, everything else one number.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Value {
    Colour([f32; 4]),
    Num(f32),
}

impl Value {
    fn lerp(self, to: Value, k: f32) -> Value {
        match (self, to) {
            (Value::Colour(a), Value::Colour(b)) => {
                Value::Colour(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k))
            }
            (Value::Num(a), Value::Num(b)) => Value::Num(a + (b - a) * k),
            (_, to) => to,
        }
    }

    fn colour(self) -> Option<Rgba> {
        match self {
            Value::Colour(c) => {
                let [r, g, b, a] = c.map(channel);
                Some(Rgba::new(r, g, b, a))
            }
            Value::Num(_) => None,
        }
    }

    fn num(self) -> f32 {
        match self {
            Value::Num(n) => n,
            Value::Colour(_) => 0.0,
        }
    }
}

fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

fn channel(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn rgba(c: Rgba) -> Value {
    Value::Colour(c.to_array().map(f32::from))
}

/// A running Animate.
#[derive(Debug, Clone)]
struct Running {
    key: PartKey,
    field: Field,
    from: Value,
    to: Value,
    interp: Interp,
    start: f32,
    duration: f32,
    event: String,
    seed: u32,
}

impl Running {
    fn at(&self, t: f32) -> Value {
        if self.duration <= 0.0 || t >= self.start + self.duration {
            return self.to;
        }
        let k = self.interp.at((t - self.start) / self.duration, self.seed);
        self.from.lerp(self.to, k)
    }
}

/// A command waiting for its moment, with the event it belongs to and where it looks.
#[derive(Debug, Clone)]
struct Queued {
    time: f32,
    order: u64,
    command: AnimCommand,
    event: String,
    within: Option<PartKey>,
}

impl PartialEq for Queued {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Queued {}

impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Queued {
    // Reversed so the heap pops the earliest time first, then the earliest queued.
    fn cmp(&self, other: &Self) -> Ordering {
        (other.time.total_cmp(&self.time)).then(other.order.cmp(&self.order))
    }
}

/// One simulation's state.
struct Sim<'a> {
    anims: &'a HudAnimations,
    model: &'a HudModel,
    layout: &'a HudLayout,
    queue: BinaryHeap<Queued>,
    order: u64,
    running: Vec<Running>,
    settled: HashMap<(PartKey, Field), Value>,
    visible: HashMap<PartKey, bool>,
    unplayed: Vec<String>,
}

impl HudAnimations {
    /// Each fired event started at 0 within its part (None: the whole HUD), simulated to `t`.
    pub fn frame(
        &self,
        model: &HudModel,
        layout: &HudLayout,
        fired: &[Fired],
        t: f32,
    ) -> AnimFrame {
        let t = if t.is_nan() { 0.0 } else { t };
        let mut sim = Sim {
            anims: self,
            model,
            layout,
            queue: BinaryHeap::new(),
            order: 0,
            running: Vec::new(),
            settled: HashMap::new(),
            visible: HashMap::new(),
            unplayed: Vec::new(),
        };
        for f in fired {
            sim.start(&f.event, 0.0, f.within.clone());
        }
        let mut executed = 0;
        while executed < MAX_EXECUTED {
            let Some(next) = sim.queue.peek() else { break };
            if next.time > t {
                break;
            }
            let Some(q) = sim.queue.pop() else { break };
            sim.run(q);
            executed += 1;
        }
        sim.finish(t)
    }

    /// The longest end time of the fired events and what they run; 2.0 when any loops.
    pub fn length(&self, fired: &[Fired]) -> f32 {
        let mut done = HashMap::new();
        let mut longest = 0.0f32;
        for f in fired {
            match self.end_of(&f.event, &mut done) {
                Some(end) => longest = longest.max(end),
                None => return LOOP_LENGTH,
            }
        }
        longest
    }

    /// When `event` and what it runs end; `None` when it runs itself again. A depth-first
    /// walk on an explicit stack, so a chain of any length cannot overflow; `done` keeps each
    /// finished event's end and `on_path` the events still open, where a return means a loop.
    fn end_of(&self, event: &str, done: &mut HashMap<String, f32>) -> Option<f32> {
        // Each frame: the event, the next command to read, the end so far, and the delay its
        // caller adds once it finishes.
        struct Frame<'a> {
            name: String,
            commands: &'a [AnimCommand],
            next: usize,
            end: f32,
            delay: f32,
        }
        let open = |name: String, delay: f32| {
            let commands = self.event(&name).map_or(&[][..], |e| e.commands());
            Frame {
                name,
                commands,
                next: 0,
                end: 0.0,
                delay,
            }
        };
        let root = event.to_ascii_lowercase();
        if let Some(&end) = done.get(&root) {
            return Some(end);
        }
        let mut on_path: HashSet<String> = HashSet::from([root.clone()]);
        let mut stack = vec![open(root, 0.0)];
        while let Some(top) = stack.last_mut() {
            let commands = top.commands;
            let Some(command) = commands.get(top.next) else {
                let Some(finished) = stack.pop() else { break };
                on_path.remove(&finished.name);
                done.insert(finished.name, finished.end);
                match stack.last_mut() {
                    Some(caller) => caller.end = caller.end.max(finished.delay + finished.end),
                    None => return Some(finished.end),
                }
                continue;
            };
            top.next += 1;
            match command {
                AnimCommand::Animate {
                    start, duration, ..
                } => top.end = top.end.max(start + duration),
                AnimCommand::RunEvent { event, delay }
                | AnimCommand::RunEventChild { event, delay, .. } => {
                    let name = event.to_ascii_lowercase();
                    if let Some(&end) = done.get(&name) {
                        top.end = top.end.max(delay + end);
                    } else if on_path.contains(&name) {
                        return None;
                    } else {
                        let delay = *delay;
                        on_path.insert(name.clone());
                        stack.push(open(name, delay));
                    }
                }
                other @ AnimCommand::StopEvent { .. }
                | other @ AnimCommand::SetVisible { .. }
                | other @ AnimCommand::SetInputEnabled { .. }
                | other @ AnimCommand::FireCommand { .. }
                | other @ AnimCommand::PlaySound { .. }
                | other @ AnimCommand::StopAnimation { .. }
                | other @ AnimCommand::StopPanelAnimations { .. }
                | other @ AnimCommand::SetFont { .. }
                | other @ AnimCommand::SetTexture { .. }
                | other @ AnimCommand::SetString { .. } => top.end = top.end.max(delay_of(other)),
            }
        }
        Some(0.0)
    }
}

/// When a command runs, from its event's start.
fn delay_of(command: &AnimCommand) -> f32 {
    match command {
        AnimCommand::Animate { start, .. } => *start,
        AnimCommand::RunEvent { delay, .. }
        | AnimCommand::StopEvent { delay, .. }
        | AnimCommand::RunEventChild { delay, .. }
        | AnimCommand::SetVisible { delay, .. }
        | AnimCommand::SetInputEnabled { delay, .. }
        | AnimCommand::FireCommand { delay, .. }
        | AnimCommand::PlaySound { delay, .. }
        | AnimCommand::StopAnimation { delay, .. }
        | AnimCommand::StopPanelAnimations { delay, .. }
        | AnimCommand::SetFont { delay, .. }
        | AnimCommand::SetTexture { delay, .. }
        | AnimCommand::SetString { delay, .. } => *delay,
    }
}

impl Sim<'_> {
    /// Queues every command of `event` from `now`.
    fn start(&mut self, event: &str, now: f32, within: Option<PartKey>) {
        let Some(found) = self.anims.event(event) else {
            return;
        };
        let name = found.name().to_ascii_lowercase();
        for command in found.commands() {
            self.order += 1;
            self.queue.push(Queued {
                time: now + delay_of(command),
                order: self.order,
                command: command.clone(),
                event: name.clone(),
                within: within.clone(),
            });
        }
    }

    /// The first part named `panel` (ignoring case) depth first under `within`, or anywhere.
    fn find(&self, panel: &str, within: Option<&PartKey>) -> Option<PartKey> {
        let prefix = within.map(|w| format!("{}/", w.as_str()));
        (self.model.parts().iter())
            .filter(|p| {
                prefix
                    .as_ref()
                    .is_none_or(|pre| p.key().as_str().starts_with(pre))
            })
            .find(|p| p.name().eq_ignore_ascii_case(panel))
            .map(|p| p.key().clone())
    }

    fn run(&mut self, q: Queued) {
        let now = q.time;
        match &q.command {
            AnimCommand::Animate {
                panel,
                field,
                value,
                interp,
                start: _,
                duration,
            } => {
                let fields = Field::named(field);
                if fields.is_empty() {
                    if !self.unplayed.iter().any(|f| f.eq_ignore_ascii_case(field)) {
                        self.unplayed.push(field.clone());
                    }
                    return;
                }
                let Some(key) = self.find(panel, q.within.as_ref()) else {
                    return;
                };
                let targets = self.targets(&key, fields, value);
                for (&f, to) in fields.iter().zip(targets) {
                    let from = self.value_at(&key, f, now);
                    self.running.retain(|r| !(r.key == key && r.field == f));
                    let seed = q.order as u32;
                    self.running.push(Running {
                        key: key.clone(),
                        field: f,
                        from,
                        to,
                        interp: *interp,
                        start: now,
                        duration: *duration,
                        event: q.event.clone(),
                        seed,
                    });
                }
            }
            AnimCommand::RunEvent { event, .. } => self.start(event, now, q.within.clone()),
            AnimCommand::RunEventChild { panel, event, .. } => {
                if let Some(child) = self.find(panel, q.within.as_ref()) {
                    self.start(event, now, Some(child));
                }
            }
            AnimCommand::StopEvent { event, .. } => {
                let name = event.to_ascii_lowercase();
                let queue = std::mem::take(&mut self.queue);
                self.queue = queue.into_iter().filter(|w| w.event != name).collect();
                self.freeze(now, |r| r.event == name);
            }
            AnimCommand::StopAnimation { panel, field, .. } => {
                if let Some(key) = self.find(panel, q.within.as_ref()) {
                    let fields = Field::named(field);
                    self.freeze(now, |r| r.key == key && fields.contains(&r.field));
                }
            }
            AnimCommand::StopPanelAnimations { panel, .. } => {
                if let Some(key) = self.find(panel, q.within.as_ref()) {
                    self.freeze(now, |r| r.key == key);
                }
            }
            AnimCommand::SetVisible { panel, visible, .. } => {
                if let Some(key) = self.find(panel, q.within.as_ref()) {
                    self.visible.insert(key, *visible);
                }
            }
            AnimCommand::SetInputEnabled { .. }
            | AnimCommand::FireCommand { .. }
            | AnimCommand::PlaySound { .. }
            | AnimCommand::SetFont { .. }
            | AnimCommand::SetTexture { .. }
            | AnimCommand::SetString { .. } => {}
        }
    }

    /// Stops the running animations `which` picks, leaving each at the value it reached.
    fn freeze(&mut self, now: f32, which: impl Fn(&Running) -> bool) {
        let (stopped, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.running)
            .into_iter()
            .partition(|r| which(r));
        self.running = kept;
        for r in stopped {
            self.settled.insert((r.key.clone(), r.field), r.at(now));
        }
    }

    /// A field's value at `now`: a running animation, else the last one settled, else the
    /// part's own setting.
    fn value_at(&self, key: &PartKey, field: Field, now: f32) -> Value {
        if let Some(r) = self
            .running
            .iter()
            .find(|r| r.key == *key && r.field == field)
        {
            return r.at(now);
        }
        if let Some(v) = self.settled.get(&(key.clone(), field)) {
            return *v;
        }
        self.own_value(key, field)
    }

    /// The part's value before any animation, from the model and the layout.
    fn own_value(&self, key: &PartKey, field: Field) -> Value {
        let part = self.model.part(key);
        let rect = self.layout.get(key).map_or(Rect::default(), |b| b.rect());
        match field {
            Field::Fg => rgba(part.map_or(Rgba::WHITE, |p| self.model.text_colour(p))),
            Field::Bg => {
                let set = part.and_then(|p| p.value("bgcolor_override").or(p.value("fillcolor")));
                rgba(set.map_or(Rgba::new(0, 0, 0, 0), |l| {
                    self.model.scheme().colour(l.text())
                }))
            }
            Field::Alpha => Value::Num(
                part.and_then(|p| p.value("alpha"))
                    .map_or(255.0, |l| atoi(l.text().as_bytes()) as f32),
            ),
            Field::X => Value::Num(rect.x as f32),
            Field::Y => Value::Num(rect.y as f32),
            Field::Wide => Value::Num(rect.wide as f32),
            Field::Tall => Value::Num(rect.tall as f32),
        }
    }

    /// The target value of each field from an Animate's text, positions in screen pixels.
    fn targets(&self, key: &PartKey, fields: &[Field], text: &str) -> Vec<Value> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let scale = ScreenScale::new(self.layout.screen().tall());
        let own = self.layout.get(key);
        let parent = (self.model.part(key))
            .and_then(|p| p.parent())
            .and_then(|k| self.layout.get(k))
            .map(|b| (b.x(), b.y(), b.wide(), b.tall()));
        let screen = self.layout.screen();
        let (px, py, pw, ph) = parent.unwrap_or((
            0,
            0,
            i32::try_from(screen.wide()).unwrap_or(i32::MAX),
            i32::try_from(screen.tall()).unwrap_or(i32::MAX),
        ));
        let (ow, oh) = own.map_or((0, 0), |b| (b.wide(), b.tall()));
        fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let word = if fields.len() > 1 {
                    words.get(i).copied().unwrap_or("0")
                } else {
                    text.trim()
                };
                match f {
                    Field::Fg | Field::Bg => rgba(self.colour(text)),
                    Field::Alpha => Value::Num(finite(atof(word)).clamp(0.0, 255.0)),
                    Field::X => Value::Num(
                        px.saturating_add(PosExpr::parse(word).eval(ow, pw, scale)) as f32,
                    ),
                    Field::Y => Value::Num(
                        py.saturating_add(PosExpr::parse(word).eval(oh, ph, scale)) as f32,
                    ),
                    Field::Wide | Field::Tall => {
                        Value::Num(scale.scale(atoi(word.as_bytes())) as f32)
                    }
                }
            })
            .collect()
    }

    /// A colour value: numbers as written, else a scheme name, magenta when missing.
    fn colour(&self, text: &str) -> Rgba {
        let text = text.trim();
        let scheme = self.model.scheme();
        if text.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
            scheme.colour(text)
        } else {
            scheme.named_colour(text).unwrap_or(MAGENTA)
        }
    }

    /// Settles every running animation at `t` and gathers each part's played values.
    fn finish(mut self, t: f32) -> AnimFrame {
        for r in std::mem::take(&mut self.running) {
            self.settled.insert((r.key.clone(), r.field), r.at(t));
        }
        let mut parts: HashMap<PartKey, Animated> = HashMap::new();
        let mut moved: HashSet<PartKey> = HashSet::new();
        for ((key, field), v) in &self.settled {
            let a = parts.entry(key.clone()).or_default();
            match field {
                Field::Fg => a.fg = v.colour(),
                Field::Bg => a.bg = v.colour(),
                Field::Alpha => a.alpha = Some(channel(v.num())),
                Field::X | Field::Y | Field::Wide | Field::Tall => {
                    moved.insert(key.clone());
                }
            }
        }
        for key in moved {
            let pick = |f: Field| self.value_at(&key, f, t).num().round() as i32;
            let rect = Rect::new(
                pick(Field::X),
                pick(Field::Y),
                pick(Field::Wide),
                pick(Field::Tall),
            );
            parts.entry(key).or_default().rect = Some(rect);
        }
        for (key, shown) in self.visible {
            parts.entry(key).or_default().visible = Some(shown);
        }
        AnimFrame {
            parts,
            in_game_only: self.unplayed,
        }
    }
}
