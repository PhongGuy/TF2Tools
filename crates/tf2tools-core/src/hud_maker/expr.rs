//! The engine's position and size expressions (`xpos "c-100"`, `wide "f0"`), ported from
//! `ComputePos`, `ComputeWide` and `ComputeTall` in the SDK's `Panel.cpp`. Evaluation follows
//! the engine exactly, garbage included; `is_well_formed` says whether the text fits the grammar.

/// How a position term combines with the terms before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TermOp {
    /// The first term: sets the position.
    Set,
    /// A term after `+`.
    Add,
    /// A term after `-`.
    Sub,
}

/// Which edge of the parent a position term measures from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Align {
    /// No prefix: from the left or top edge.
    Start,
    /// `c`: from the parent's centre.
    Centre,
    /// `r`: back from the right or bottom edge.
    End,
}

/// What a position term's number counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PosUnit {
    /// No prefix: proportional units, scaled by screen height.
    Units,
    /// `s`: a fraction of the panel's own size.
    OwnSize,
    /// `p`: a fraction of the parent's size.
    ParentSize,
}

/// What a size expression's number counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SizeForm {
    /// No prefix: proportional units.
    Units,
    /// `f`: the parent's size less the units.
    Full,
    /// `p`: a fraction of the parent's size less the scaled units.
    ParentFrac,
    /// `s`: a fraction of the panel's current size.
    OwnFrac,
    /// `o`: a multiple of the other axis.
    OtherAxis,
}

/// One term of a position expression as the engine scans it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PosTerm {
    op: TermOp,
    align: Align,
    unit: PosUnit,
    number: String,
    /// Byte offset in the expression where `atoi`/`atof` start reading this term.
    value_at: usize,
}

impl PosTerm {
    /// How the term combines with the ones before it.
    pub fn op(&self) -> TermOp {
        self.op
    }

    /// The edge the term measures from.
    pub fn align(&self) -> Align {
        self.align
    }

    /// What the term's number counts.
    pub fn unit(&self) -> PosUnit {
        self.unit
    }

    /// The signed number the engine's scanner stepped over, possibly empty.
    pub fn number(&self) -> &str {
        &self.number
    }
}

/// A position key's value (`xpos`, `ypos`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PosExpr {
    text: String,
    terms: Vec<PosTerm>,
    well_formed: bool,
}

impl PosExpr {
    /// Reads a position expression; never fails, since the engine accepts any text.
    pub fn parse(text: &str) -> PosExpr {
        PosExpr {
            text: text.to_owned(),
            terms: scan_pos(text),
            well_formed: pos_well_formed(text.as_bytes()),
        }
    }

    /// The text as written.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The terms in the order the engine applies them.
    pub fn terms(&self) -> &[PosTerm] {
        &self.terms
    }

    /// Whether the text fits the documented grammar.
    pub fn is_well_formed(&self) -> bool {
        self.well_formed
    }

    /// Pixels as `ComputePos` gives them for a panel `own` pixels long in a parent `parent` long.
    pub fn eval(&self, own: i32, parent: i32, scale: ScreenScale) -> i32 {
        let mut pos: i32 = 0;
        for term in &self.terms {
            let rest = &self.text.as_bytes()[term.value_at..];
            let fraction = atof(rest);
            let delta = match term.unit {
                PosUnit::OwnSize => (own as f32 * fraction) as i32,
                PosUnit::ParentSize => (parent as f32 * fraction) as i32,
                PosUnit::Units => scale.scale(atoi(rest)),
            };
            let value = match term.align {
                Align::End => parent.wrapping_sub(delta),
                Align::Centre => (parent / 2).wrapping_add(delta),
                Align::Start => delta,
            };
            pos = match term.op {
                TermOp::Set => value,
                TermOp::Add => pos.wrapping_add(value),
                TermOp::Sub => pos.wrapping_sub(value),
            };
        }
        pos
    }
}

/// A size key's value (`wide`, `tall`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SizeExpr {
    text: String,
    form: SizeForm,
    number: String,
    well_formed: bool,
}

impl SizeExpr {
    /// Reads a size expression; never fails, since the engine accepts any text.
    pub fn parse(text: &str) -> SizeExpr {
        let bytes = text.as_bytes();
        let (form, start) = match bytes.first() {
            Some(b'f' | b'F') => (SizeForm::Full, 1),
            Some(b'o' | b'O') => (SizeForm::OtherAxis, 1),
            Some(b'p' | b'P') => (SizeForm::ParentFrac, 1),
            Some(b's' | b'S') => (SizeForm::OwnFrac, 1),
            _ => (SizeForm::Units, 0),
        };
        let end = start + scanned_number_len(&bytes[start..]);
        let rest = &bytes[start..];
        let well_formed = match signed_number_len(rest) {
            Some(n) => n == rest.len(),
            None => start == 1 && rest.is_empty(),
        };
        SizeExpr {
            text: text.to_owned(),
            form,
            number: text[start..end].to_owned(),
            well_formed,
        }
    }

    /// The text as written.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// What the number counts.
    pub fn form(&self) -> SizeForm {
        self.form
    }

    /// The signed number the engine's scanner would step over, possibly empty.
    pub fn number(&self) -> &str {
        &self.number
    }

    /// Whether the text fits the documented grammar.
    pub fn is_well_formed(&self) -> bool {
        self.well_formed
    }

    /// Pixels as `ComputeWide`/`ComputeTall` give them; `other` is the other axis's key, if set.
    pub fn eval(
        &self,
        this: AxisSize,
        other: Option<&SizeExpr>,
        other_axis: AxisSize,
        scale: ScreenScale,
    ) -> i32 {
        self.compute(this, other, other_axis, scale, false)
    }

    /// The shared body of `ComputeWide` and `ComputeTall`; `computing_other` is set while an
    /// `o` size evaluates the other axis, so two `o` keys give 0 rather than recursing.
    fn compute(
        &self,
        this: AxisSize,
        other: Option<&SizeExpr>,
        other_axis: AxisSize,
        scale: ScreenScale,
        computing_other: bool,
    ) -> i32 {
        let start = usize::from(self.form != SizeForm::Units);
        let rest = &self.text.as_bytes()[start..];
        let factor = atof(rest);
        match self.form {
            SizeForm::OtherAxis => {
                if computing_other {
                    return 0;
                }
                // The other axis in pixels, back to units and up again, which loses the
                // remainder the engine loses.
                let other_px = match other {
                    Some(expr) => expr.compute(other_axis, None, this, scale, true),
                    None => other_axis.current,
                };
                let units = scale.normalise(other_px);
                (scale.scale(units) as f32 * factor) as i32
            }
            SizeForm::ParentFrac => {
                let px = this.parent.wrapping_sub(scale.scale(atoi(rest)));
                (px as f32 * factor) as i32
            }
            SizeForm::OwnFrac => (this.current as f32 * factor) as i32,
            SizeForm::Full => this.parent.wrapping_sub(scale.scale(atoi(rest))),
            SizeForm::Units => scale.scale(atoi(rest)),
        }
    }
}

/// The engine's proportional scaling for one screen height (HUD panels are proportional).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScreenScale {
    screen_tall: u32,
}

impl ScreenScale {
    /// The scale for a screen `screen_tall` pixels high; 0 is treated as 1.
    pub fn new(screen_tall: u32) -> ScreenScale {
        ScreenScale {
            screen_tall: screen_tall.max(1),
        }
    }

    /// Proportional units (a 480-high screen) to pixels, truncating toward zero.
    pub fn scale(self, units: i32) -> i32 {
        clamp_i32(i64::from(units) * i64::from(self.screen_tall) / 480)
    }

    /// Pixels back to proportional units, truncating toward zero.
    pub fn normalise(self, px: i32) -> i32 {
        clamp_i32(i64::from(px) * 480 / i64::from(self.screen_tall))
    }
}

/// One axis of a panel: its current size and its parent's size, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct AxisSize {
    current: i32,
    parent: i32,
}

impl AxisSize {
    /// An axis from the panel's current size and its parent's size.
    pub fn new(current: i32, parent: i32) -> AxisSize {
        AxisSize { current, parent }
    }
}

/// Scans terms as `ComputePos` does: one alignment and one unit letter, then the value; the
/// scanner steps over one sign and any digits and dots, and continues only on `+` or `-`.
fn scan_pos(text: &str) -> Vec<PosTerm> {
    let bytes = text.as_bytes();
    let mut terms = Vec::new();
    let mut i = 0;
    let mut op = TermOp::Set;
    loop {
        let align = match bytes.get(i) {
            Some(b'r' | b'R') => Align::End,
            Some(b'c' | b'C') => Align::Centre,
            _ => Align::Start,
        };
        if align != Align::Start {
            i += 1;
        }
        let unit = match bytes.get(i) {
            Some(b's' | b'S') => PosUnit::OwnSize,
            Some(b'p' | b'P') => PosUnit::ParentSize,
            _ => PosUnit::Units,
        };
        if unit != PosUnit::Units {
            i += 1;
        }
        let end = i + scanned_number_len(&bytes[i..]);
        terms.push(PosTerm {
            op,
            align,
            unit,
            number: text[i..end].to_owned(),
            value_at: i,
        });
        op = match bytes.get(end) {
            Some(b'+') => TermOp::Add,
            Some(b'-') => TermOp::Sub,
            _ => return terms,
        };
        i = end + 1;
    }
}

/// How far the engine's scanner steps past a value: one sign, then digits and dots.
fn scanned_number_len(bytes: &[u8]) -> usize {
    let sign = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    sign + bytes[sign..]
        .iter()
        .take_while(|b| b.is_ascii_digit() || **b == b'.')
        .count()
}

/// Grammar: `term (("+"|"-") term)*`, a term `[rRcC]? [sSpP]? [+-]? number`, where the number
/// may be empty only after a prefix letter and no sign.
fn pos_well_formed(bytes: &[u8]) -> bool {
    let mut i = 0;
    loop {
        let mut prefixed = false;
        if matches!(bytes.get(i), Some(b'r' | b'R' | b'c' | b'C')) {
            i += 1;
            prefixed = true;
        }
        if matches!(bytes.get(i), Some(b's' | b'S' | b'p' | b'P')) {
            i += 1;
            prefixed = true;
        }
        match signed_number_len(&bytes[i..]) {
            Some(n) => i += n,
            None if prefixed && !matches!(bytes.get(i), Some(b'+' | b'-')) => {}
            None => return false,
        }
        match bytes.get(i) {
            None => return true,
            Some(b'+' | b'-') => i += 1,
            Some(_) => return false,
        }
    }
}

/// The length of `[+-]? (digits ("." digits*)? | "." digits)` at the start, if present.
fn signed_number_len(bytes: &[u8]) -> Option<usize> {
    let sign = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let rest = &bytes[sign..];
    let int = digit_run(rest);
    let len = if rest.get(int) == Some(&b'.') {
        let frac = digit_run(&rest[int + 1..]);
        if int == 0 && frac == 0 {
            return None;
        }
        int + 1 + frac
    } else if int == 0 {
        return None;
    } else {
        int
    };
    Some(sign + len)
}

fn digit_run(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|b| b.is_ascii_digit()).count()
}

/// C `isspace`: space, tab, newline, vertical tab, form feed, carriage return.
fn skip_c_space(bytes: &[u8]) -> &[u8] {
    let n = bytes
        .iter()
        .take_while(|b| matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .count();
    &bytes[n..]
}

/// C `atoi`: leading space, an optional sign and digits; anything else reads as 0. Overflow,
/// undefined in C, clamps to the `i32` range.
pub(super) fn atoi(bytes: &[u8]) -> i32 {
    let bytes = skip_c_space(bytes);
    let (negative, digits) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    let mut value: i64 = 0;
    for &b in digits.iter().take_while(|b| b.is_ascii_digit()) {
        value = (value * 10 + i64::from(b - b'0')).min(i64::from(i32::MAX) + 1);
    }
    clamp_i32(if negative { -value } else { value })
}

/// C `atof` stored into a `float`: leading space, sign, digits, a fraction and an exponent,
/// read as a double and rounded to `f32`; nothing read is 0.
pub(super) fn atof(bytes: &[u8]) -> f32 {
    let bytes = skip_c_space(bytes);
    let sign = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let int = digit_run(&bytes[sign..]);
    let mut end = sign + int;
    let mut mantissa_digits = int;
    if bytes.get(end) == Some(&b'.') {
        let frac = digit_run(&bytes[end + 1..]);
        mantissa_digits += frac;
        end += 1 + frac;
    }
    if mantissa_digits == 0 {
        return 0.0;
    }
    // An exponent counts only when at least one digit follows its optional sign.
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let exp_sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exp_digits = digit_run(&bytes[end + 1 + exp_sign..]);
        if exp_digits > 0 {
            end += 1 + exp_sign + exp_digits;
        }
    }
    // The prefix is ASCII sign, digits, dot and exponent, which Rust's float parser accepts.
    std::str::from_utf8(&bytes[..end])
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .map_or(0.0, |v| v as f32)
}

fn clamp_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}
