//! The app's keyboard actions and their bindings.

use gpui_kit::{App, KeyBinding};

use crate::pages::crosshair_maker::model::Tool;

gpui_kit::actions!(
    tf2tools,
    [
        /// Collapses or expands the sidebar.
        ToggleSidebar
    ]
);

gpui_kit::actions!(
    crosshair_maker,
    [
        /// Picks the Pencil.
        UsePencil,
        /// Picks the Eraser.
        UseEraser,
        /// Picks Fill.
        UseFill,
        /// Picks the Line tool.
        UseLine,
        /// Picks the Rectangle tool.
        UseRectangle,
        /// Picks the Circle tool.
        UseCircle,
        /// Undoes the last drawing step.
        UndoDrawing,
        /// Redoes the last undone drawing step.
        RedoDrawing,
        /// Zooms the grid in around its centre.
        ZoomInGrid,
        /// Zooms the grid out around its centre.
        ZoomOutGrid,
        /// Fits the grid to the window.
        FitGrid
    ]
);

gpui_kit::actions!(
    hud_maker,
    [
        /// Undoes the last HUD change, or the drag running now.
        UndoHudChange,
        /// Redoes the last undone HUD change.
        RedoHudChange,
        /// Drops the part drag running now.
        CancelHudDrag,
        /// Moves the selected part one HUD unit left.
        NudgeLeft,
        /// Moves the selected part one HUD unit right.
        NudgeRight,
        /// Moves the selected part one HUD unit up.
        NudgeUp,
        /// Moves the selected part one HUD unit down.
        NudgeDown,
        /// Moves the selected part `NUDGE_FAR` units left.
        NudgeLeftFar,
        /// Moves the selected part `NUDGE_FAR` units right.
        NudgeRightFar,
        /// Moves the selected part `NUDGE_FAR` units up.
        NudgeUpFar,
        /// Moves the selected part `NUDGE_FAR` units down.
        NudgeDownFar
    ]
);

/// The Shift+arrow nudge step in HUD units.
pub const NUDGE_FAR: i32 = 10;

/// The key context on the HUD Maker page root.
pub const HUD_MAKER_CONTEXT: &str = "HudMaker";

/// The key context on the Crosshair Maker page root.
pub const MAKER_CONTEXT: &str = "CrosshairMaker";

/// The keys for `ToggleSidebar`, v5's Ctrl+B (Cmd+B on macOS).
pub const TOGGLE_SIDEBAR_KEYS: &str = "secondary-b";

/// Binds the shell's actions to `Shell` and each maker's to its page while no text field has
/// focus; must run after `gpui_kit::init`.
pub fn bind(cx: &mut App) {
    // Any focused text field (context "Input") switches every maker key off, so letters type.
    let maker_keys = format!("{MAKER_CONTEXT} && !Input");
    let maker = Some(maker_keys.as_str());
    let hud_keys = format!("{HUD_MAKER_CONTEXT} && !Input");
    let hud = Some(hud_keys.as_str());
    cx.bind_keys([
        KeyBinding::new(TOGGLE_SIDEBAR_KEYS, ToggleSidebar, Some("Shell")),
        KeyBinding::new(Tool::Pencil.key(), UsePencil, maker),
        KeyBinding::new(Tool::Eraser.key(), UseEraser, maker),
        KeyBinding::new(Tool::Fill.key(), UseFill, maker),
        KeyBinding::new(Tool::Line.key(), UseLine, maker),
        KeyBinding::new(Tool::Rectangle.key(), UseRectangle, maker),
        KeyBinding::new(Tool::Circle.key(), UseCircle, maker),
        KeyBinding::new("secondary-z", UndoDrawing, maker),
        KeyBinding::new("secondary-y", RedoDrawing, maker),
        KeyBinding::new("secondary-shift-z", RedoDrawing, maker),
        // Plus arrives as `=`, `+` or Shift+`=` depending on the layout.
        KeyBinding::new("secondary-=", ZoomInGrid, maker),
        KeyBinding::new("secondary-+", ZoomInGrid, maker),
        KeyBinding::new("secondary-shift-=", ZoomInGrid, maker),
        KeyBinding::new("secondary--", ZoomOutGrid, maker),
        KeyBinding::new("secondary-0", FitGrid, maker),
        KeyBinding::new("secondary-z", UndoHudChange, hud),
        KeyBinding::new("secondary-y", RedoHudChange, hud),
        KeyBinding::new("secondary-shift-z", RedoHudChange, hud),
        KeyBinding::new("escape", CancelHudDrag, hud),
        KeyBinding::new("left", NudgeLeft, hud),
        KeyBinding::new("right", NudgeRight, hud),
        KeyBinding::new("up", NudgeUp, hud),
        KeyBinding::new("down", NudgeDown, hud),
        KeyBinding::new("shift-left", NudgeLeftFar, hud),
        KeyBinding::new("shift-right", NudgeRightFar, hud),
        KeyBinding::new("shift-up", NudgeUpFar, hud),
        KeyBinding::new("shift-down", NudgeDownFar, hud),
    ]);
}
