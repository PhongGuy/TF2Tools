//! What the canvas paints for the rank panel's name, track and fill, resolved from the pieces
//! core reports and the HUD's scheme, so the look can be checked without drawing.

use tf2tools_core::{FontVariant, RankPieces, Rgba, Scheme};

use super::canvas::alignment;

/// The rank pieces' text, font, colours and alignment before the colour-blind view shades them.
#[derive(Debug, Clone, PartialEq)]
pub struct RankPaint {
    text: String,
    font: Option<FontVariant>,
    colour: Option<Rgba>,
    alignment: (usize, usize),
    track: Rgba,
    fill: Rgba,
}

impl RankPaint {
    /// The name as drawn, in capitals when the file asks.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The name's font, `None` when the file sets none or the scheme lacks it.
    pub fn font(&self) -> Option<&FontVariant> {
        self.font.as_ref()
    }

    /// The name's colour, `None` when unset (the canvas then draws white).
    pub fn colour(&self) -> Option<Rgba> {
        self.colour
    }

    /// The name's (column, row) in its box; unset is west, centred.
    pub fn alignment(&self) -> (usize, usize) {
        self.alignment
    }

    /// The track's colour; unset is the stock `20 20 20 180`.
    pub fn track(&self) -> Rgba {
        self.track
    }

    /// The fill's colour; unset is the scheme's `CreditsGreen`.
    pub fn fill(&self) -> Rgba {
        self.fill
    }
}

/// Resolves `pieces` against `scheme` on a screen `screen_tall` pixels high.
pub fn rank_paint(pieces: &RankPieces, scheme: &Scheme, screen_tall: u32) -> RankPaint {
    let text = if pieces.name_all_caps() {
        RankPieces::SAMPLE_NAME.to_uppercase()
    } else {
        RankPieces::SAMPLE_NAME.to_owned()
    };
    RankPaint {
        text,
        font: (pieces.name_font())
            .and_then(|name| scheme.font(name, screen_tall))
            .cloned(),
        colour: pieces.name_colour().map(|c| scheme.colour(c)),
        alignment: alignment(pieces.name_alignment()),
        track: (pieces.track_colour()).map_or(Rgba::new(20, 20, 20, 180), |c| scheme.colour(c)),
        fill: scheme.colour(pieces.fill_colour().unwrap_or("CreditsGreen")),
    }
}
