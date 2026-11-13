//! Every rule the in-game probe settles, one function each. Each body is a single table
//! holding the survey's code-derived default, so a probe correction changes one place.

use super::{HideTechnique, PartId, PinCorner, PosExpr, ScreenScale};

/// Pixel height of a proportional font of `tall` on a screen `screen_tall` px high.
pub fn font_tall_px(tall: i32, screen_tall: u32) -> i32 {
    let px = i64::from(tall) * i64::from(screen_tall) / 480;
    i32::try_from(px).unwrap_or(if px < 0 { i32::MIN } else { i32::MAX })
}

/// The sign applied to a pinned panel's offset per axis, by its own and its sibling's corner.
pub(crate) fn pin_offset_sign(own: PinCorner, sibling: PinCorner) -> (i32, i32) {
    // VPanel::Solve turns an axis around when the sibling's corner sits on its left or top
    // edge, so positive offsets push away from the sibling; the own corner plays no part.
    let _ = own;
    use PinCorner::*;
    let x = match sibling {
        TopLeft | BottomLeft | CenterLeft => -1,
        TopRight | BottomRight | CenterTop | CenterRight | CenterBottom => 1,
    };
    let y = match sibling {
        TopLeft | TopRight | CenterTop => -1,
        BottomLeft | BottomRight | CenterRight | CenterBottom | CenterLeft => 1,
    };
    (x, y)
}

/// A pinned panel's offset from its `xpos`/`ypos` text.
pub(crate) fn pinned_offset(expr: &PosExpr, own: i32, align: i32, scale: ScreenScale) -> i32 {
    expr.eval(own, align, scale)
}

/// The ways a hide can be written into a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HideHow {
    Visible,
    Enabled,
    Alpha,
    SizeZero,
    OffScreen,
}

/// Whether a hide written this way holds; `element` means a hudlayout element (visibility
/// reset each frame).
pub(crate) fn hide_holds(how: HideHow, element: bool) -> bool {
    match (how, element) {
        (HideHow::Visible, true) | (HideHow::Enabled, _) => false,
        (HideHow::Visible, false)
        | (HideHow::Alpha | HideHow::SizeZero | HideHow::OffScreen, _) => true,
    }
}

/// The hide the maker offers for a part: the survey's "Hide that works" column.
pub(crate) fn hide_technique(part: PartId) -> HideTechnique {
    match part {
        PartId::Health | PartId::ClassPortrait => HideTechnique::VisibleZero,
        PartId::DamageNumbers
        | PartId::MainMenu
        | PartId::FreezeCam
        | PartId::WinPanel
        | PartId::ClassSelect
        | PartId::Spectator => HideTechnique::NotOffered,
        PartId::Ammo
        | PartId::UberMeter
        | PartId::HealNumbers
        | PartId::TargetId
        | PartId::SpectatorTargetId
        | PartId::ItemMeter
        | PartId::RoundTimer
        | PartId::KothTimer => HideTechnique::OffScreenY,
        PartId::Scoreboard => HideTechnique::SizeZero,
        // ASSUMED, not probed in game: the unclassified hudlayout element's hide.
        PartId::Killfeed | PartId::Chat => HideTechnique::OffScreenX,
    }
}

/// The hide for a part the game's code shows or hides: `visible 0` does not stick there,
/// so every such part shrinks to nothing until the probe says otherwise.
pub(crate) fn code_toggled_hide(_panel: &str) -> HideTechnique {
    HideTechnique::SizeZero
}

/// The hide for a part no rule classifies: `OffScreenX` for a hudlayout element, else `SizeZero`.
pub(crate) fn unclassified_hide(element: bool) -> HideTechnique {
    match element {
        true => HideTechnique::OffScreenX,
        false => HideTechnique::SizeZero,
    }
}

/// `if_*` sub-blocks active for a part in the default situation (RED, casual):
/// `["if_match"]` for the timer, `["if_wider"]` for the main menu.
pub(crate) fn active_conditions(part: PartId) -> &'static [&'static str] {
    match part {
        PartId::RoundTimer | PartId::KothTimer => &["if_match"],
        // The main menu previews a wide screen with no holiday running.
        PartId::MainMenu => &["if_wider"],
        PartId::Health
        | PartId::ClassPortrait
        | PartId::Ammo
        | PartId::UberMeter
        | PartId::HealNumbers
        | PartId::DamageNumbers
        | PartId::TargetId
        | PartId::ItemMeter
        | PartId::Killfeed
        | PartId::Chat
        | PartId::Scoreboard
        | PartId::FreezeCam
        | PartId::WinPanel
        | PartId::ClassSelect
        | PartId::Spectator
        | PartId::SpectatorTargetId => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud_maker::{HideTechnique, PartId, PinCorner, PosExpr, ScreenScale};

    #[test]
    fn pin_offset_signs_follow_the_sibling_corner() {
        use PinCorner::*;
        let table = [
            (TopLeft, (-1, -1)),
            (TopRight, (1, -1)),
            (BottomLeft, (-1, 1)),
            (BottomRight, (1, 1)),
            (CenterTop, (1, -1)),
            (CenterRight, (1, 1)),
            (CenterBottom, (1, 1)),
            (CenterLeft, (-1, 1)),
        ];
        for (sibling, want) in table {
            for own in PinCorner::ALL {
                assert_eq!(
                    pin_offset_sign(own, sibling),
                    want,
                    "{own:?} to {sibling:?}"
                );
            }
        }
    }

    #[test]
    fn pinned_offsets_keep_alignment_pending_probe() {
        let scale = ScreenScale::new(1080);
        for (text, want) in [("4", 9), ("-2", -4), ("r4", 1911), ("c-4", 951)] {
            assert_eq!(
                pinned_offset(&PosExpr::parse(text), 45, 1920, scale),
                want,
                "{text}"
            );
        }
    }

    #[test]
    fn hides_pending_probe() {
        let holds = [
            ((HideHow::Visible, true), false),
            ((HideHow::Visible, false), true),
            ((HideHow::Enabled, true), false),
            ((HideHow::Enabled, false), false),
            ((HideHow::Alpha, true), true),
            ((HideHow::SizeZero, false), true),
            ((HideHow::OffScreen, true), true),
        ];
        for ((how, element), want) in holds {
            assert_eq!(hide_holds(how, element), want, "{how:?} element={element}");
        }
        use HideTechnique::*;
        let techniques = [
            (PartId::Health, VisibleZero),
            (PartId::ClassPortrait, VisibleZero),
            (PartId::Ammo, OffScreenY),
            (PartId::UberMeter, OffScreenY),
            (PartId::HealNumbers, OffScreenY),
            (PartId::DamageNumbers, NotOffered),
            (PartId::TargetId, OffScreenY),
            (PartId::ItemMeter, OffScreenY),
            (PartId::RoundTimer, OffScreenY),
            (PartId::KothTimer, OffScreenY),
            (PartId::Scoreboard, SizeZero),
            (PartId::Killfeed, OffScreenX),
            (PartId::Chat, OffScreenX),
            (PartId::MainMenu, NotOffered),
        ];
        for (part, want) in techniques {
            assert_eq!(hide_technique(part), want, "{part:?}");
        }
        assert_eq!(
            (unclassified_hide(true), unclassified_hide(false)),
            (OffScreenX, SizeZero)
        );
    }

    #[test]
    fn active_conditions_pending_probe() {
        assert_eq!(active_conditions(PartId::RoundTimer), ["if_match"]);
        assert_eq!(active_conditions(PartId::KothTimer), ["if_match"]);
        for part in [
            PartId::Health,
            PartId::Ammo,
            PartId::TargetId,
            PartId::ItemMeter,
            PartId::Killfeed,
        ] {
            assert!(active_conditions(part).is_empty(), "{part:?}");
        }
        assert!(active_conditions(PartId::Scoreboard).is_empty());
        assert_eq!(active_conditions(PartId::MainMenu), ["if_wider"]);
        assert_eq!(code_toggled_hide("BlueTeamLabel"), HideTechnique::SizeZero);
    }
}
