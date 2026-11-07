//! HUD animation scripts: the `hudanimations` event files the game's manifest lists, read with
//! the rules `AnimationController::ParseScriptFile` follows (first event of a name wins, a bad
//! command keeps what came before it, platform conditionals) and bounded by fixed caps.

use std::collections::{HashMap, HashSet};

use super::{
    AnimationAppend, AnimationChange, Change, ChangeTarget, Edited, HudFiles, KvDocument,
    KvEntryValue,
};
use crate::error::HudFilesError;
use crate::fonts::Platform;
use crate::fonts::keyvalues::{LexKind, condition_holds, lex};

/// Longest token read, in bytes: `AnimationController::ParseScriptFile`'s buffer.
pub const MAX_TOKEN: usize = 512;
/// Most commands kept in one event.
pub const MAX_COMMANDS: usize = 256;
/// Most events kept from one file.
pub const MAX_EVENTS: usize = 4096;
/// Most files read from one manifest.
pub const MAX_ANIM_FILES: usize = 64;
/// The events the game's code fires on its own.
pub const GAME_TRIGGERS: [&str; 9] = [
    "DamagedPlayer",
    "HudHealthBonusPulse",
    "HudHealthBonusPulseStop",
    "HudHealthDyingPulse",
    "HudHealthDyingPulseStop",
    "HudLowAmmoPulse",
    "HudLowAmmoPulseStop",
    "HudMedicCharged",
    "HudMedicChargedStop",
];

/// How an Animate moves from its start value to its target.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Interp {
    /// Straight progress.
    Linear,
    /// Starts slow, ends fast.
    Accel,
    /// Starts fast, ends slow.
    Deaccel,
    /// Slow at both ends.
    Spline,
    /// Swings back and forth this many times.
    Pulse(f32),
    /// Jumps between start and end at random, this often.
    Flicker(f32),
    /// Steepened in the middle by this amount.
    Gain(f32),
    /// Leaned towards one end by this amount.
    Bias(f32),
    /// Overshoots and settles.
    Bounce,
}

impl Interp {
    /// The curve named `name` (any case), and whether it reads a parameter; unknown is Linear.
    fn named(name: &str) -> (Interp, bool) {
        match name.to_ascii_lowercase().as_str() {
            "accel" => (Interp::Accel, false),
            "deaccel" => (Interp::Deaccel, false),
            "spline" => (Interp::Spline, false),
            "pulse" => (Interp::Pulse(0.0), true),
            "flicker" => (Interp::Flicker(0.0), true),
            "gain" => (Interp::Gain(0.0), true),
            "bias" => (Interp::Bias(0.0), true),
            "bounce" => (Interp::Bounce, false),
            _ => (Interp::Linear, false),
        }
    }

    fn with_param(self, p: f32) -> Interp {
        match self {
            Interp::Pulse(_) => Interp::Pulse(p),
            Interp::Flicker(_) => Interp::Flicker(p),
            Interp::Gain(_) => Interp::Gain(p),
            Interp::Bias(_) => Interp::Bias(p),
            other @ Interp::Linear
            | other @ Interp::Accel
            | other @ Interp::Deaccel
            | other @ Interp::Spline
            | other @ Interp::Bounce => other,
        }
    }

    fn words(self) -> String {
        match self {
            Interp::Linear => "Linear".into(),
            Interp::Accel => "Accel".into(),
            Interp::Deaccel => "Deaccel".into(),
            Interp::Spline => "Spline".into(),
            Interp::Bounce => "Bounce".into(),
            Interp::Pulse(p) => format!("Pulse {p}"),
            Interp::Flicker(p) => format!("Flicker {p}"),
            Interp::Gain(p) => format!("Gain {p}"),
            Interp::Bias(p) => format!("Bias {p}"),
        }
    }
}

/// One line of an event.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum AnimCommand {
    /// Moves a panel field to a value over time.
    Animate {
        /// The panel it acts on, found by name.
        panel: String,
        /// The panel field, such as `FgColor` or `xpos`.
        field: String,
        /// The value, as the script spells it.
        value: String,
        /// The curve.
        interp: Interp,
        /// Seconds from the event start until it begins.
        start: f32,
        /// Seconds it takes.
        duration: f32,
    },
    /// Starts another event.
    RunEvent {
        /// The event it names.
        event: String,
        /// Seconds from the event start.
        delay: f32,
    },
    /// Stops another event.
    StopEvent {
        /// The event it names.
        event: String,
        /// Seconds from the event start.
        delay: f32,
    },
    /// Starts an event within a child panel.
    RunEventChild {
        /// The panel it acts on, found by name.
        panel: String,
        /// The event it names.
        event: String,
        /// Seconds from the event start.
        delay: f32,
    },
    /// Shows or hides a panel.
    SetVisible {
        /// The panel it acts on, found by name.
        panel: String,
        /// Shown or hidden.
        visible: bool,
        /// Seconds from the event start.
        delay: f32,
    },
    /// Turns a panel's input on or off.
    SetInputEnabled {
        /// The panel it acts on, found by name.
        panel: String,
        /// Input on or off.
        enabled: bool,
        /// Seconds from the event start.
        delay: f32,
    },
    /// Runs a console command; never run, written or borrowed by the Maker.
    FireCommand {
        /// Seconds from the event start.
        delay: f32,
        /// The console command.
        command: String,
    },
    /// Plays a sound.
    PlaySound {
        /// Seconds from the event start.
        delay: f32,
        /// The sound file.
        sound: String,
    },
    /// Stops one field's animation.
    StopAnimation {
        /// The panel it acts on, found by name.
        panel: String,
        /// The panel field, such as `FgColor` or `xpos`.
        field: String,
        /// Seconds from the event start.
        delay: f32,
    },
    /// Stops every animation of a panel.
    StopPanelAnimations {
        /// The panel it acts on, found by name.
        panel: String,
        /// Seconds from the event start.
        delay: f32,
    },
    /// Sets a font field.
    SetFont {
        /// The panel it acts on, found by name.
        panel: String,
        /// The panel field, such as `FgColor` or `xpos`.
        field: String,
        /// The value, as the script spells it.
        value: String,
        /// Seconds from the event start.
        delay: f32,
    },
    /// Sets a texture field.
    SetTexture {
        /// The panel it acts on, found by name.
        panel: String,
        /// The panel field, such as `FgColor` or `xpos`.
        field: String,
        /// The value, as the script spells it.
        value: String,
        /// Seconds from the event start.
        delay: f32,
    },
    /// Sets a text field.
    SetString {
        /// The panel it acts on, found by name.
        panel: String,
        /// The panel field, such as `FgColor` or `xpos`.
        field: String,
        /// The value, as the script spells it.
        value: String,
        /// Seconds from the event start.
        delay: f32,
    },
}

impl AnimCommand {
    /// The command as one canonical script line, which parses back to the same command.
    pub fn to_line(&self) -> String {
        let flag = |b: bool| u8::from(b);
        match self {
            AnimCommand::Animate {
                panel,
                field,
                value,
                interp,
                start,
                duration,
            } => format!(
                "Animate {} {} \"{value}\" {} {start} {duration}",
                q(panel),
                q(field),
                interp.words()
            ),
            AnimCommand::RunEvent { event, delay } => format!("RunEvent {} {delay}", q(event)),
            AnimCommand::StopEvent { event, delay } => format!("StopEvent {} {delay}", q(event)),
            AnimCommand::RunEventChild {
                panel,
                event,
                delay,
            } => format!("RunEventChild {} {} {delay}", q(panel), q(event)),
            AnimCommand::SetVisible {
                panel,
                visible,
                delay,
            } => format!("SetVisible {} {} {delay}", q(panel), flag(*visible)),
            AnimCommand::SetInputEnabled {
                panel,
                enabled,
                delay,
            } => format!("SetInputEnabled {} {} {delay}", q(panel), flag(*enabled)),
            AnimCommand::FireCommand { delay, command } => {
                format!("FireCommand {delay} \"{command}\"")
            }
            AnimCommand::PlaySound { delay, sound } => format!("PlaySound {delay} \"{sound}\""),
            AnimCommand::StopAnimation {
                panel,
                field,
                delay,
            } => format!("StopAnimation {} {} {delay}", q(panel), q(field)),
            AnimCommand::StopPanelAnimations { panel, delay } => {
                format!("StopPanelAnimations {} {delay}", q(panel))
            }
            AnimCommand::SetFont {
                panel,
                field,
                value,
                delay,
            } => format!("SetFont {} {} {} {delay}", q(panel), q(field), q(value)),
            AnimCommand::SetTexture {
                panel,
                field,
                value,
                delay,
            } => format!("SetTexture {} {} {} {delay}", q(panel), q(field), q(value)),
            AnimCommand::SetString {
                panel,
                field,
                value,
                delay,
            } => format!("SetString {} {} \"{value}\" {delay}", q(panel), q(field)),
        }
    }

    /// The panel the command acts on, when it names one.
    pub fn panel(&self) -> Option<&str> {
        match self {
            AnimCommand::Animate { panel, .. }
            | AnimCommand::RunEventChild { panel, .. }
            | AnimCommand::SetVisible { panel, .. }
            | AnimCommand::SetInputEnabled { panel, .. }
            | AnimCommand::StopAnimation { panel, .. }
            | AnimCommand::StopPanelAnimations { panel, .. }
            | AnimCommand::SetFont { panel, .. }
            | AnimCommand::SetTexture { panel, .. }
            | AnimCommand::SetString { panel, .. } => Some(panel),
            AnimCommand::RunEvent { .. }
            | AnimCommand::StopEvent { .. }
            | AnimCommand::FireCommand { .. }
            | AnimCommand::PlaySound { .. } => None,
        }
    }
}

/// One named event: its commands, and where it was read from.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct AnimEvent {
    name: String,
    commands: Vec<AnimCommand>,
    file: String,
    layer: Option<usize>,
    own_len: usize,
}

impl AnimEvent {
    /// The event's name as first spelt.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The commands in order.
    pub fn commands(&self) -> &[AnimCommand] {
        &self.commands
    }

    /// Every command's canonical line, joined by newlines.
    pub fn body(&self) -> String {
        lines_of(&self.commands)
    }

    /// The script file the event came from; empty for a parsed snippet.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// Whether the event runs a console command, which locks it against changes.
    pub fn runs_game_commands(&self) -> bool {
        (self.commands.iter()).any(|c| matches!(c, AnimCommand::FireCommand { .. }))
    }

    /// The HUD layer of that file; `None` for a snippet or a change.
    pub fn layer(&self) -> Option<usize> {
        self.layer
    }

    /// How many commands are the event's own, before any a feature appended.
    pub fn own_len(&self) -> usize {
        self.own_len
    }

    /// The own commands' lines, without appended ones.
    pub fn own_body(&self) -> String {
        lines_of(&self.commands[..self.own_len.min(self.commands.len())])
    }
}

/// A word as the script writes it: always quoted, so the game reads it as one token however
/// its tokenizer splits words. The parser refuses quotes and control characters inside a
/// token, so a word can never break out of its quotes.
fn q(word: &str) -> String {
    format!("\"{word}\"")
}

/// Whether a token is one the engine reads as written: no quote, line break or other
/// control character, which could otherwise smuggle a second line into a written script.
fn clean_token(token: &str) -> bool {
    !(token.chars()).any(|c| c.is_control() || c == '"')
}

fn lines_of(commands: &[AnimCommand]) -> String {
    commands
        .iter()
        .map(AnimCommand::to_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// One parsed script: its events in file order, and what stopped or was skipped.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct AnimScript {
    events: Vec<AnimEvent>,
    stopped: Option<String>,
    skipped: usize,
}

impl AnimScript {
    /// Events in file order (first of a name kept), conditionals evaluated for `platform`.
    pub fn parse(text: &str, platform: Platform) -> AnimScript {
        let mut parser = Parser {
            tokens: tokens(text),
            at: 0,
            platform,
            script: AnimScript {
                events: Vec::new(),
                stopped: None,
                skipped: 0,
            },
        };
        parser.script();
        // Every line a script holds is its event's own; features append after them later.
        for event in &mut parser.script.events {
            event.own_len = event.commands.len();
        }
        parser.script
    }

    /// The kept events in file order.
    pub fn events(&self) -> &[AnimEvent] {
        &self.events
    }

    /// The token that stopped parsing, when one did.
    pub fn stopped(&self) -> Option<&str> {
        self.stopped.as_deref()
    }

    /// Lines, events and tokens skipped: over a cap, or a NaN, infinite or negative time.
    pub fn skipped(&self) -> usize {
        self.skipped
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Open,
    Close,
    Cond(String),
}

impl Tok {
    fn text(&self) -> String {
        match self {
            Tok::Word(w) => w.clone(),
            Tok::Open => "{".into(),
            Tok::Close => "}".into(),
            Tok::Cond(c) => format!("[{c}]"),
        }
    }
}

/// The KeyValues lexer's tokens: braces split from glued words, `//` comments dropped.
fn tokens(text: &str) -> Vec<Tok> {
    let bytes = text.as_bytes();
    let text_of = |r: std::ops::Range<usize>| String::from_utf8_lossy(&bytes[r]).into_owned();
    lex(bytes)
        .into_iter()
        .filter_map(|l| {
            let (s, e) = (l.span.start, l.span.end);
            match l.kind {
                LexKind::Bom | LexKind::Space | LexKind::Comment => None,
                LexKind::Open => Some(Tok::Open),
                LexKind::Close => Some(Tok::Close),
                LexKind::Bare => Some(Tok::Word(text_of(s..e))),
                LexKind::Quoted { closed } => {
                    Some(Tok::Word(text_of(s + 1..e - usize::from(closed))))
                }
                LexKind::Cond { closed } => {
                    Some(Tok::Cond(text_of(s + 1..e - usize::from(closed))))
                }
            }
        })
        .collect()
}

/// A number read as C `atof` does: the longest numeric prefix, else 0.
pub(super) fn atof(s: &str) -> f32 {
    let s = s.trim_start();
    let ends = s
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .collect::<Vec<_>>();
    ends.iter()
        .rev()
        .find_map(|&end| s[..end].parse::<f32>().ok())
        .unwrap_or(0.0)
}

/// A time or curve parameter the simulation can use: finite and not negative.
fn good_time(t: f32) -> bool {
    t.is_finite() && t >= 0.0
}

/// Why a command was not added.
enum Read {
    Kept(AnimCommand),
    Skipped,
    Dropped,
    Stop(String),
    End,
    Eof,
}

struct Parser {
    tokens: Vec<Tok>,
    at: usize,
    platform: Platform,
    script: AnimScript,
}

impl Parser {
    fn next(&mut self) -> Option<Tok> {
        let tok = self.tokens.get(self.at).cloned();
        self.at += usize::from(tok.is_some());
        tok
    }

    /// Consumes a conditional when one follows, and says whether it holds (true when none).
    fn condition(&mut self) -> bool {
        if let Some(Tok::Cond(expr)) = self.tokens.get(self.at) {
            let holds = condition_holds(expr, self.platform);
            self.at += 1;
            holds
        } else {
            true
        }
    }

    /// Skips to the brace closing a body whose `{` was just consumed.
    fn skip_body(&mut self) {
        let mut open = 1usize;
        while let Some(tok) = self.next() {
            match tok {
                Tok::Open => open += 1,
                Tok::Close => {
                    open -= 1;
                    if open == 0 {
                        return;
                    }
                }
                Tok::Word(_) | Tok::Cond(_) => {}
            }
        }
    }

    fn script(&mut self) {
        while let Some(tok) = self.next() {
            if !matches!(&tok, Tok::Word(w) if w.eq_ignore_ascii_case("event")) {
                self.script.stopped = Some(tok.text());
                return;
            }
            let Some(name) = self.next() else { return };
            let Tok::Word(name) = name else {
                self.script.stopped = Some(name.text());
                return;
            };
            let holds = self.condition();
            match self.next() {
                Some(Tok::Open) => {}
                Some(other) => {
                    self.script.stopped = Some(other.text());
                    return;
                }
                None => return,
            }
            let taken = self
                .script
                .events
                .iter()
                .any(|e| e.name.eq_ignore_ascii_case(&name));
            let over = name.len() > MAX_TOKEN
                || !clean_token(&name)
                || self.script.events.len() >= MAX_EVENTS;
            if !holds || taken || over {
                self.script.skipped += usize::from(holds && over && !taken);
                self.skip_body();
                continue;
            }
            // The event is kept before its commands are read, so a stop keeps what came first.
            self.script.events.push(AnimEvent {
                name,
                commands: Vec::new(),
                file: String::new(),
                layer: None,
                own_len: 0,
            });
            if !self.body() {
                return;
            }
        }
    }

    /// Reads commands up to the closing brace; false when parsing stops for the file.
    fn body(&mut self) -> bool {
        loop {
            match self.command() {
                Read::Kept(command) => match self.script.events.last_mut() {
                    Some(event) if event.commands.len() < MAX_COMMANDS => {
                        event.commands.push(command);
                    }
                    _ => self.script.skipped += 1,
                },
                Read::Skipped => self.script.skipped += 1,
                Read::Dropped => {}
                Read::Stop(token) => {
                    self.script.stopped = Some(token);
                    return false;
                }
                Read::End => return true,
                Read::Eof => return false,
            }
        }
    }

    /// Reads `n` argument tokens as text; `None` at the end of the file.
    fn args(&mut self, n: usize) -> Option<Vec<String>> {
        (0..n).map(|_| self.next().map(|t| t.text())).collect()
    }

    fn command(&mut self) -> Read {
        let name = match self.next() {
            None => return Read::Eof,
            Some(Tok::Close) => return Read::End,
            Some(Tok::Word(w)) => w,
            Some(other) => return Read::Stop(other.text()),
        };
        let lower = name.to_ascii_lowercase();
        let arity = match lower.as_str() {
            "animate" => 4,
            "runevent" | "stopevent" | "firecommand" | "playsound" | "stoppanelanimations" => 2,
            "runeventchild" | "setvisible" | "setinputenabled" | "stopanimation" => 3,
            "setfont" | "settexture" | "setstring" => 4,
            _ => return Read::Stop(name),
        };
        let Some(mut a) = self.args(arity) else {
            return Read::Eof;
        };
        let mut times = Vec::new();
        if lower == "animate" {
            let (interp, takes) = Interp::named(&a[3]);
            let extra = 2 + usize::from(takes);
            let Some(rest) = self.args(extra) else {
                return Read::Eof;
            };
            a.extend(rest);
            let interp = if takes {
                times.push(atof(&a[4]));
                interp.with_param(atof(&a[4]))
            } else {
                interp
            };
            let (start, duration) = (atof(&a[extra + 2]), atof(&a[extra + 3]));
            times.extend([start, duration]);
            let holds = self.condition();
            return self.finish(
                &a,
                &times,
                holds,
                AnimCommand::Animate {
                    panel: a[0].clone(),
                    field: a[1].clone(),
                    value: a[2].clone(),
                    interp,
                    start,
                    duration,
                },
            );
        }
        let holds = self.condition();
        let s = |i: usize| a[i].clone();
        let delay_at = |i: usize| atof(&a[i]);
        let flag = |i: usize| atof(&a[i]) != 0.0;
        let command = match lower.as_str() {
            "runevent" => AnimCommand::RunEvent {
                event: s(0),
                delay: delay_at(1),
            },
            "stopevent" => AnimCommand::StopEvent {
                event: s(0),
                delay: delay_at(1),
            },
            "firecommand" => AnimCommand::FireCommand {
                delay: delay_at(0),
                command: s(1),
            },
            "playsound" => AnimCommand::PlaySound {
                delay: delay_at(0),
                sound: s(1),
            },
            "stoppanelanimations" => AnimCommand::StopPanelAnimations {
                panel: s(0),
                delay: delay_at(1),
            },
            "runeventchild" => AnimCommand::RunEventChild {
                panel: s(0),
                event: s(1),
                delay: delay_at(2),
            },
            "setvisible" => AnimCommand::SetVisible {
                panel: s(0),
                visible: flag(1),
                delay: delay_at(2),
            },
            "setinputenabled" => AnimCommand::SetInputEnabled {
                panel: s(0),
                enabled: flag(1),
                delay: delay_at(2),
            },
            "stopanimation" => AnimCommand::StopAnimation {
                panel: s(0),
                field: s(1),
                delay: delay_at(2),
            },
            "setfont" => AnimCommand::SetFont {
                panel: s(0),
                field: s(1),
                value: s(2),
                delay: delay_at(3),
            },
            "settexture" => AnimCommand::SetTexture {
                panel: s(0),
                field: s(1),
                value: s(2),
                delay: delay_at(3),
            },
            _ => AnimCommand::SetString {
                panel: s(0),
                field: s(1),
                value: s(2),
                delay: delay_at(3),
            },
        };
        let delay_index = if matches!(lower.as_str(), "firecommand" | "playsound") {
            0
        } else {
            arity - 1
        };
        times.push(atof(&a[delay_index]));
        self.finish(&a, &times, holds, command)
    }

    /// Keeps a fully read command unless a token is over the cap, a time is bad, or its
    /// conditional is false.
    fn finish(&self, args: &[String], times: &[f32], holds: bool, command: AnimCommand) -> Read {
        let bad_token = |t: &String| t.len() > MAX_TOKEN || !clean_token(t);
        if args.iter().any(bad_token) || !times.iter().all(|&t| good_time(t)) {
            Read::Skipped
        } else if !holds {
            Read::Dropped
        } else {
            Read::Kept(command)
        }
    }
}

/// Whether `name` can name an event in a written script: 1-64 characters, none of them
/// whitespace, a quote or a brace, no `//`, and no leading `[`.
fn good_event_name(name: &str) -> bool {
    (1..=64).contains(&name.chars().count())
        && !name.starts_with('[')
        && !name.contains("//")
        && !(name.chars()).any(|c| c.is_whitespace() || matches!(c, '"' | '{' | '}'))
}

/// The commands of `text` as the body of event `name`, when the name is valid and the text
/// parses whole within the caps and holds no FireCommand.
pub(crate) fn checked_lines(
    name: &str,
    text: &str,
    platform: Platform,
) -> Option<Vec<AnimCommand>> {
    if !good_event_name(name) {
        return None;
    }
    let script = AnimScript::parse(&format!("event {name}\n{{\n{text}\n}}\n"), platform);
    let [event] = script.events.as_slice() else {
        return None;
    };
    let whole = script.stopped.is_none() && script.skipped == 0 && event.name == name;
    let safe = !(event.commands.iter()).any(|c| matches!(c, AnimCommand::FireCommand { .. }));
    (whole && safe).then(|| event.commands.clone())
}

/// The script a made HUD's animation changes are written to.
pub(crate) const OWN_SCRIPT: &str = "scripts/hudanimations_tf2tools.txt";

/// The manifest that lists a HUD's animation files.
pub(crate) const MANIFEST: &str = "scripts/hudanimations_manifest.txt";

/// A HUD's animation events: the manifest's files read in order, first event of a name kept.
#[non_exhaustive]
#[derive(Debug, Clone, Default)]
pub struct HudAnimations {
    events: Vec<AnimEvent>,
    index: HashMap<String, usize>,
    problems: Vec<String>,
    skipped: usize,
}

impl HudAnimations {
    /// The manifest's files in order across every layer; first event of a name wins.
    pub fn load(files: &HudFiles, platform: Platform) -> HudAnimations {
        let mut anims = HudAnimations::default();
        let Some(paths) = manifest_files(files, platform, &mut anims.problems) else {
            return anims;
        };
        anims.skipped += paths.len().saturating_sub(MAX_ANIM_FILES);
        for path in paths.iter().take(MAX_ANIM_FILES) {
            let file = match files.read(path) {
                Ok(Some(file)) => file,
                Ok(None) => {
                    anims.problems.push(format!("{path}: listed but missing"));
                    continue;
                }
                Err(e) => {
                    anims.problems.push(format!("{path}: {e}"));
                    continue;
                }
            };
            let script = AnimScript::parse(&String::from_utf8_lossy(file.bytes()), platform);
            anims.skipped += script.skipped;
            if let Some(token) = &script.stopped {
                anims
                    .problems
                    .push(format!("{path}: stopped at unknown \"{token}\""));
            }
            for mut event in script.events {
                event.file = path.clone();
                event.layer = Some(file.layer());
                anims.push(event);
            }
        }
        anims
    }

    /// Adds `event` unless one of its name (ignoring ASCII case) is already held.
    fn push(&mut self, event: AnimEvent) {
        let key = event.name.to_ascii_lowercase();
        if !self.index.contains_key(&key) {
            self.index.insert(key, self.events.len());
            self.events.push(event);
        }
    }

    /// Sets `name`'s whole body to `commands`, replacing any event of that name in place.
    pub(crate) fn with_event(&mut self, name: &str, commands: Vec<AnimCommand>) {
        let event = AnimEvent {
            name: name.to_owned(),
            own_len: commands.len(),
            commands,
            file: String::new(),
            layer: None,
        };
        match self.index.get(&name.to_ascii_lowercase()) {
            Some(&i) => self.events[i] = event,
            None => self.push(event),
        }
    }

    /// Adds `commands` after `name`'s body, creating an empty event first; a line already
    /// in the event is not added twice.
    pub(crate) fn with_appended(&mut self, name: &str, commands: Vec<AnimCommand>) {
        if self.event(name).is_none() {
            self.with_event(name, Vec::new());
        }
        let Some(&i) = self.index.get(&name.to_ascii_lowercase()) else {
            return;
        };
        let event = &mut self.events[i];
        for command in commands {
            let line = command.to_line();
            if event.commands.len() < MAX_COMMANDS
                && !event.commands.iter().any(|c| c.to_line() == line)
            {
                event.commands.push(command);
            }
        }
    }

    /// The events in load order.
    pub fn events(&self) -> &[AnimEvent] {
        &self.events
    }

    /// The event named `name`, ignoring ASCII case.
    pub fn event(&self, name: &str) -> Option<&AnimEvent> {
        (self.index.get(&name.to_ascii_lowercase())).map(|&i| &self.events[i])
    }

    /// The events with a command acting on the panel `panel`, ignoring ASCII case.
    pub fn naming(&self, panel: &str) -> Vec<&AnimEvent> {
        (self.events.iter())
            .filter(|e| {
                (e.commands.iter())
                    .any(|c| c.panel().is_some_and(|p| p.eq_ignore_ascii_case(panel)))
            })
            .collect()
    }

    /// What went wrong reading the files, one line per file.
    pub fn problems(&self) -> &[String] {
        &self.problems
    }

    /// Everything skipped: each script's `skipped()` plus manifest files past MAX_ANIM_FILES.
    pub fn skipped(&self) -> usize {
        self.skipped
    }
}

/// The `file` entries of the manifest's root block, in order; `None` without a manifest.
fn manifest_files(
    files: &HudFiles,
    platform: Platform,
    problems: &mut Vec<String>,
) -> Option<Vec<String>> {
    let bytes = match files.read(MANIFEST) {
        Ok(Some(file)) => file.into_bytes(),
        Ok(None) => return None,
        Err(e) => {
            problems.push(format!("{MANIFEST}: {e}"));
            return None;
        }
    };
    let doc = match KvDocument::parse(&bytes) {
        Ok(doc) => doc,
        Err(e) => {
            problems.push(format!("{MANIFEST}: {e}"));
            return None;
        }
    };
    let root = (doc.entries().iter()).find(|e| matches!(e.value(), KvEntryValue::Block(_)))?;
    Some(
        (root.children().iter())
            .filter(|e| e.key().eq_ignore_ascii_case("file") && e.holds(platform))
            .filter_map(|e| e.text().map(str::to_owned))
            .collect(),
    )
}

/// What saving writes for a made HUD's animation changes.
#[derive(Debug)]
pub(crate) struct AnimationFiles {
    /// The manifest's game path.
    pub(crate) manifest_path: String,
    /// The manifest with our script listed first.
    pub(crate) manifest: Vec<u8>,
    /// The manifest as the HUD ships it, for its hash.
    pub(crate) pristine: Vec<u8>,
    /// The script holding each changed event.
    pub(crate) script: String,
}

/// A HUD's events, with whole-event changes laid over them, for asking which events can
/// reach a FireCommand through RunEvent and RunEventChild.
pub(crate) struct ConsoleReach {
    events: HashMap<String, Vec<AnimCommand>>,
}

impl ConsoleReach {
    /// The events of `anims`, by lowercase name.
    pub(crate) fn over(anims: &HudAnimations) -> ConsoleReach {
        let events = (anims.events().iter())
            .map(|e| (e.name.to_ascii_lowercase(), e.commands.clone()))
            .collect();
        ConsoleReach { events }
    }

    /// Lays each valid whole-event change in `changes` over the events it names.
    pub(crate) fn lay_over(&mut self, changes: &[Change], platform: Platform) {
        for change in changes {
            if let ChangeTarget::Animation(a) = change.target()
                && let Some(commands) = a.check(platform)
            {
                self.events.insert(a.event().to_ascii_lowercase(), commands);
            }
        }
    }

    /// Whether running `event` can reach a FireCommand; the visited set ends a cycle.
    pub(crate) fn reaches_console(&self, event: &str) -> bool {
        let mut seen = HashSet::new();
        let mut todo = vec![event.to_ascii_lowercase()];
        while let Some(name) = todo.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            for command in self.events.get(&name).into_iter().flatten() {
                match command {
                    AnimCommand::FireCommand { .. } => return true,
                    AnimCommand::RunEvent { event, .. }
                    | AnimCommand::RunEventChild { event, .. } => {
                        todo.push(event.to_ascii_lowercase());
                    }
                    AnimCommand::Animate { .. }
                    | AnimCommand::StopEvent { .. }
                    | AnimCommand::SetVisible { .. }
                    | AnimCommand::SetInputEnabled { .. }
                    | AnimCommand::PlaySound { .. }
                    | AnimCommand::StopAnimation { .. }
                    | AnimCommand::StopPanelAnimations { .. }
                    | AnimCommand::SetFont { .. }
                    | AnimCommand::SetTexture { .. }
                    | AnimCommand::SetString { .. } => {}
                }
            }
        }
        false
    }

    /// `change` without the lines that run an event reaching a console command, and how
    /// many were cut. An invalid change passes unchanged, since the replay drops it.
    pub(crate) fn cut(&self, change: &Change, platform: Platform) -> (Change, usize) {
        let (event, commands) = match change.target() {
            ChangeTarget::Animation(a) => (a.event(), a.check(platform)),
            ChangeTarget::AnimationAppend(a) => (a.event(), a.check(platform)),
            ChangeTarget::Set(_)
            | ChangeTarget::Remove(_)
            | ChangeTarget::AddBlock(_)
            | ChangeTarget::Asset(_)
            | ChangeTarget::Option(_)
            | ChangeTarget::Credit(_) => return (change.clone(), 0),
        };
        let Some(commands) = commands else {
            return (change.clone(), 0);
        };
        let total = commands.len();
        let kept: Vec<String> = (commands.iter())
            .filter(|c| match c {
                AnimCommand::RunEvent { event, .. } | AnimCommand::RunEventChild { event, .. } => {
                    !self.reaches_console(event)
                }
                AnimCommand::Animate { .. }
                | AnimCommand::StopEvent { .. }
                | AnimCommand::SetVisible { .. }
                | AnimCommand::SetInputEnabled { .. }
                | AnimCommand::FireCommand { .. }
                | AnimCommand::PlaySound { .. }
                | AnimCommand::StopAnimation { .. }
                | AnimCommand::StopPanelAnimations { .. }
                | AnimCommand::SetFont { .. }
                | AnimCommand::SetTexture { .. }
                | AnimCommand::SetString { .. } => true,
            })
            .map(AnimCommand::to_line)
            .collect();
        let cut = total - kept.len();
        if cut == 0 {
            return (change.clone(), 0);
        }
        let text = kept.join("\n");
        let group = change.group();
        let change = match change.target() {
            ChangeTarget::Animation(_) => {
                Change::animation(group, AnimationChange::new(event, &text))
            }
            ChangeTarget::Set(_)
            | ChangeTarget::Remove(_)
            | ChangeTarget::AddBlock(_)
            | ChangeTarget::Asset(_)
            | ChangeTarget::Option(_)
            | ChangeTarget::Credit(_)
            | ChangeTarget::AnimationAppend(_) => {
                Change::animation_append(group, AnimationAppend::new(event, &text))
            }
        };
        (change, cut)
    }
}

/// What saving does with a made HUD's animation changes.
#[derive(Debug)]
pub(crate) enum AnimationSave {
    /// No applied change is an animation.
    Nothing,
    /// Write these files.
    Write(AnimationFiles),
    /// The changes cannot be written safely or listed, so they are left out and counted.
    Refused,
}

/// The bytes to write for the replayed model's animation changes. Event bodies come from
/// `edited.model().animations()`; which events to write, in the order of their first change
/// and under the name it spells, come from `edited.applied()`. Refused when the manifest
/// cannot take our line or the written script would not read back without a FireCommand.
pub(crate) fn animation_files(
    files: &HudFiles,
    edited: &Edited,
    platform: Platform,
) -> Result<AnimationSave, HudFilesError> {
    let mut names: Vec<&str> = Vec::new();
    for change in edited.applied() {
        let event = match change.target() {
            ChangeTarget::Animation(a) => a.event(),
            ChangeTarget::AnimationAppend(a) => a.event(),
            ChangeTarget::Set(_)
            | ChangeTarget::Remove(_)
            | ChangeTarget::AddBlock(_)
            | ChangeTarget::Asset(_)
            | ChangeTarget::Option(_)
            | ChangeTarget::Credit(_) => continue,
        };
        if !names.iter().any(|n| n.eq_ignore_ascii_case(event)) {
            names.push(event);
        }
    }
    if names.is_empty() {
        return Ok(AnimationSave::Nothing);
    }
    let Some(file) = files.read(MANIFEST)? else {
        return Ok(AnimationSave::Refused);
    };
    let pristine = file.into_bytes();
    let Ok(mut doc) = KvDocument::parse(&pristine) else {
        return Ok(AnimationSave::Refused);
    };
    let Some(root) = (doc.entries().iter()).find(|e| matches!(e.value(), KvEntryValue::Block(_)))
    else {
        return Ok(AnimationSave::Refused);
    };
    // A copy of a made HUD already lists our script, so it is listed once.
    let listed = (root.children().iter()).any(|e| {
        e.key().eq_ignore_ascii_case("file")
            && e.holds(platform)
            && e.text().is_some_and(|t| t.eq_ignore_ascii_case(OWN_SCRIPT))
    });
    let open = root.value_id();
    if !listed && !doc.insert_text_first(open, "file", OWN_SCRIPT) {
        return Ok(AnimationSave::Refused);
    }
    let anims = edited.model().animations();
    let script = (names.iter())
        .map(|name| {
            let lines: String = (anims.event(name).into_iter())
                .flat_map(|e| e.commands())
                .map(|c| format!("\t{}\n", c.to_line()))
                .collect();
            format!("event {name}\n{{\n{lines}}}\n")
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !script_is_safe(&script, names.len(), platform) {
        return Ok(AnimationSave::Refused);
    }
    Ok(AnimationSave::Write(AnimationFiles {
        manifest_path: MANIFEST.to_owned(),
        manifest: doc.as_bytes().to_vec(),
        pristine,
        script,
    }))
}

/// Whether `script`, read back as the game reads it, holds exactly `events` events, all of
/// it read, and no FireCommand.
fn script_is_safe(script: &str, events: usize, platform: Platform) -> bool {
    let back = AnimScript::parse(script, platform);
    back.stopped.is_none()
        && back.skipped == 0
        && back.events.len() == events
        && !(back.events.iter())
            .flat_map(|e| &e.commands)
            .any(|c| matches!(c, AnimCommand::FireCommand { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_holding_a_console_command_is_not_safe() {
        let ok = "event A\n{\n\tRunEvent B 0\n}\n";
        assert!(script_is_safe(ok, 1, Platform::Linux));
        let fire = "event A\n{\n\tFireCommand 0 \"quit\"\n}\n";
        assert!(!script_is_safe(fire, 1, Platform::Linux));
        let split = "event A\n{\n\tRunEvent B 0\n}\nevent C\n{\n}\n";
        assert!(
            !script_is_safe(split, 1, Platform::Linux),
            "an extra event is refused"
        );
    }
}
