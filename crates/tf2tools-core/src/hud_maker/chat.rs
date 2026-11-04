//! The sample chat: three lines placed in the chat history the way the game's `RichText`
//! lays out its lines, coloured and sized from the chat's own scheme.

use super::expr::atoi;
use super::{FontVariant, HudModel, PartKey, Rgba, SampleSet, Scene, Screen, Team, font_tall_px};

const HISTORY: &str = "HudChat/HudChatHistory";
/// The code's own team colours, used when the chat scheme names none (`CHudChat::GetClientColor`).
const RED: Rgba = Rgba::new(255, 63, 63, 255);
const BLUE: Rgba = Rgba::new(153, 204, 255, 255);

/// The sample chat the preview draws in the chat history while nobody types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChatSample {
    history: PartKey,
    x: i32,
    lines: Vec<ChatLine>,
    font: Option<FontVariant>,
    line_tall: i32,
}

impl ChatSample {
    /// The HudChatHistory part the lines draw in.
    pub fn history(&self) -> &PartKey {
        &self.history
    }

    /// Every line's left, from the history's left.
    pub fn x(&self) -> i32 {
        self.x
    }

    /// The sample lines, top first.
    pub fn lines(&self) -> &[ChatLine] {
        &self.lines
    }

    /// The chat scheme's ChatFont at the screen, if it has one.
    pub fn font(&self) -> Option<&FontVariant> {
        self.font.as_ref()
    }

    /// A line's height: the font's pixel height, else a 12-unit font's.
    pub fn line_tall(&self) -> i32 {
        self.line_tall
    }
}

/// One sample chat line: a player's name, then what they said.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChatLine {
    y: i32,
    name: &'static str,
    name_colour: Rgba,
    text: &'static str,
    text_colour: Rgba,
}

impl ChatLine {
    /// The line's top, from the history's top.
    pub fn y(&self) -> i32 {
        self.y
    }

    /// The sender, a sample casual player.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The sender's team colour.
    pub fn name_colour(&self) -> Rgba {
        self.name_colour
    }

    /// What follows the name: the `" :  "` separator and the message.
    pub fn text(&self) -> &'static str {
        self.text
    }

    /// The colour of everything after the name.
    pub fn text_colour(&self) -> Rgba {
        self.text_colour
    }
}

impl HudModel {
    /// The sample chat at `screen`; `None` outside the HUD scene, without a HudChat or
    /// HudChatHistory part, when the history is unreadable, or when its box is hidden.
    pub fn chat_sample(&self, screen: Screen) -> Option<ChatSample> {
        if self.scene() != Scene::Hud {
            return None;
        }
        let history = PartKey::new(HISTORY);
        self.part(&PartKey::new("HudChat"))?;
        if !self.part(&history)?.is_readable() {
            return None;
        }
        if self.layout(screen).get(&history)?.hidden().is_some() {
            return None;
        }

        // CHudChatHistory::ApplySchemeSettings draws in the chat scheme's ChatFont, and a
        // RichText line is as tall as its font (RichText::GetLineHeight).
        let scheme = self.chat_scheme();
        let font = scheme.font("ChatFont", screen.tall()).cloned();
        let line_tall = match &font {
            Some(font) => font.pixel_tall(screen.tall()),
            None => font_tall_px(12, screen.tall()),
        };

        // RichText::ApplySchemeSettings: a non-empty RichText.InsetX replaces the
        // constructor's proportional draw offsets, which scale with the screen's width.
        let inset = |name: &str| scheme.resource_string(name).map(|v| atoi(v.as_bytes()));
        let (x, offset_y) = match scheme.resource_string("RichText.InsetX") {
            Some(inset_x) if !inset_x.is_empty() => (
                atoi(inset_x.as_bytes()),
                inset("RichText.InsetY").unwrap_or(0),
            ),
            _ => {
                let unit = f64::from(screen.wide()) / 640.0;
                ((3.0 * unit) as i32, unit as i32)
            }
        };

        // CHudChat::GetClientColor and GetTextColorForClient; the game draws chat opaque.
        let opaque = |c: Rgba| {
            let [r, g, b, _] = c.to_array();
            Rgba::new(r, g, b, 255)
        };
        let named = |name: &str, or: Rgba| opaque(scheme.named_colour(name).unwrap_or(or));
        let (red, blue) = (
            named("TFColors.ChatTextRed", RED),
            named("TFColors.ChatTextBlue", BLUE),
        );
        let text_colour = scheme
            .named_colour("TFColors.ChatTextYellow")
            .or_else(|| scheme.named_colour("DullWhite"))
            .map_or(Rgba::WHITE, opaque);

        let casual = SampleSet::Casual;
        let name = |team: Team, i: usize| casual.players(team).get(i).map_or("", |p| p.name());
        let said = [
            (name(Team::Red, 1), red, " :  Nice shot!"),
            (name(Team::Blue, 5), blue, " :  Push the cart!"),
            (name(Team::Red, 0), red, " :  Thanks, Medic!"),
        ];
        // RichText::AddAnotherLine: each line starts one line height and one offset below
        // the last.
        let pitch = line_tall.saturating_add(offset_y);
        let lines = (said.into_iter().zip(0i32..))
            .map(|((name, name_colour, text), i)| ChatLine {
                y: offset_y.saturating_add(i.saturating_mul(pitch)),
                name,
                name_colour,
                text,
                text_colour,
            })
            .collect();
        Some(ChatSample {
            history,
            x,
            lines,
            font,
            line_tall,
        })
    }
}
