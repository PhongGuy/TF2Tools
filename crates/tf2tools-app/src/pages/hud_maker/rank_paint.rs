//! What the canvas paints for the rank panel's labels and track, resolved from the pieces core
//! reports and the HUD's scheme, so the look can be checked without drawing.

use tf2tools_core::{FontVariant, RankLabel, RankPieces, Rgba, Scheme};

use super::canvas::alignment;

/// One label's text, font, colour and alignment before the colour-blind view shades them.
#[derive(Debug, Clone, PartialEq)]
pub struct RankText {
    text: String,
    font: Option<FontVariant>,
    colour: Option<Rgba>,
    alignment: (usize, usize),
}

impl RankText {
    /// The text as drawn, in capitals when the file asks.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The font, `None` when the file sets none or the scheme lacks it.
    pub fn font(&self) -> Option<&FontVariant> {
        self.font.as_ref()
    }

    /// The colour, `None` when unset (the canvas then draws white).
    pub fn colour(&self) -> Option<Rgba> {
        self.colour
    }

    /// The (column, row) in the label's box; unset is west, centred.
    pub fn alignment(&self) -> (usize, usize) {
        self.alignment
    }
}

/// The rank name's paint and the track's and fill's colours.
#[derive(Debug, Clone, PartialEq)]
pub struct RankPaint {
    name: RankText,
    track: Rgba,
    fill: Rgba,
}

impl RankPaint {
    /// The name as drawn, in capitals when the file asks.
    pub fn text(&self) -> &str {
        self.name.text()
    }

    /// The name's font, `None` when the file sets none or the scheme lacks it.
    pub fn font(&self) -> Option<&FontVariant> {
        self.name.font()
    }

    /// The name's colour, `None` when unset (the canvas then draws white).
    pub fn colour(&self) -> Option<Rgba> {
        self.name.colour()
    }

    /// The name's (column, row) in its box; unset is west, centred.
    pub fn alignment(&self) -> (usize, usize) {
        self.name.alignment()
    }

    /// The name's paint as a whole.
    pub fn name(&self) -> &RankText {
        &self.name
    }

    /// The track segments' colour; unset is the stock `20 20 20 180`.
    pub fn track(&self) -> Rgba {
        self.track
    }

    /// The filled segments' colour; unset is the scheme's `CreditsGreen`.
    pub fn fill(&self) -> Rgba {
        self.fill
    }
}

/// Resolves a label's sample `text` and keys against `scheme` on a screen `screen_tall` high.
fn resolve(
    text: &str,
    caps: bool,
    font: Option<&str>,
    colour: Option<&str>,
    align: &str,
    scheme: &Scheme,
    screen_tall: u32,
) -> RankText {
    RankText {
        text: if caps {
            text.to_uppercase()
        } else {
            text.to_owned()
        },
        font: font
            .and_then(|name| scheme.font(name, screen_tall))
            .cloned(),
        colour: colour.map(|c| scheme.colour(c)),
        alignment: alignment(align),
    }
}

/// Resolves `pieces` against `scheme` on a screen `screen_tall` pixels high.
pub fn rank_paint(pieces: &RankPieces, scheme: &Scheme, screen_tall: u32) -> RankPaint {
    RankPaint {
        name: resolve(
            RankPieces::SAMPLE_NAME,
            pieces.name_all_caps(),
            pieces.name_font(),
            pieces.name_colour(),
            pieces.name_alignment(),
            scheme,
            screen_tall,
        ),
        track: (pieces.track_colour()).map_or(Rgba::new(20, 20, 20, 180), |c| scheme.colour(c)),
        fill: scheme.colour(pieces.fill_colour().unwrap_or("CreditsGreen")),
    }
}

/// Resolves one of the rank lines or xp labels against `scheme`.
pub fn rank_text(label: &RankLabel, scheme: &Scheme, screen_tall: u32) -> RankText {
    resolve(
        label.text(),
        label.all_caps(),
        label.font(),
        label.colour(),
        label.alignment(),
        scheme,
        screen_tall,
    )
}
