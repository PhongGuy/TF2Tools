//! The app's keyboard actions and their bindings.

use gpui_kit::{Action, App, KeyBinding, KeyBindingContextPredicate};

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
        NudgeDownFar,
        /// Zooms the canvas in around its middle.
        ZoomInHud,
        /// Zooms the canvas out around its middle.
        ZoomOutHud,
        /// Fits the screen to the canvas again.
        ZoomFitHud,
        /// Hides the selected part, or shows it again when the player hid it.
        ToggleHudPartHidden,
        /// Hides the selected part; never shows it.
        HideHudPart,
        /// Opens the keyboard shortcuts sheet.
        ShowHudShortcuts
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
    let keys = [
        secondary(TOGGLE_SIDEBAR_KEYS, ToggleSidebar, Some("Shell"), cx),
        KeyBinding::new(Tool::Pencil.key(), UsePencil, maker),
        KeyBinding::new(Tool::Eraser.key(), UseEraser, maker),
        KeyBinding::new(Tool::Fill.key(), UseFill, maker),
        KeyBinding::new(Tool::Line.key(), UseLine, maker),
        KeyBinding::new(Tool::Rectangle.key(), UseRectangle, maker),
        KeyBinding::new(Tool::Circle.key(), UseCircle, maker),
        secondary("secondary-z", UndoDrawing, maker, cx),
        secondary("secondary-y", RedoDrawing, maker, cx),
        secondary("secondary-shift-z", RedoDrawing, maker, cx),
        // Plus arrives as `=`, `+` or Shift+`=` depending on the layout.
        secondary("secondary-=", ZoomInGrid, maker, cx),
        secondary("secondary-+", ZoomInGrid, maker, cx),
        secondary("secondary-shift-=", ZoomInGrid, maker, cx),
        secondary("secondary--", ZoomOutGrid, maker, cx),
        secondary("secondary-0", FitGrid, maker, cx),
        secondary("secondary-z", UndoHudChange, hud, cx),
        secondary("secondary-y", RedoHudChange, hud, cx),
        secondary("secondary-shift-z", RedoHudChange, hud, cx),
        KeyBinding::new("escape", CancelHudDrag, hud),
        KeyBinding::new("left", NudgeLeft, hud),
        KeyBinding::new("right", NudgeRight, hud),
        KeyBinding::new("up", NudgeUp, hud),
        KeyBinding::new("down", NudgeDown, hud),
        KeyBinding::new("shift-left", NudgeLeftFar, hud),
        KeyBinding::new("shift-right", NudgeRightFar, hud),
        KeyBinding::new("shift-up", NudgeUpFar, hud),
        KeyBinding::new("shift-down", NudgeDownFar, hud),
        secondary("secondary-=", ZoomInHud, hud, cx),
        secondary("secondary-+", ZoomInHud, hud, cx),
        secondary("secondary-shift-=", ZoomInHud, hud, cx),
        secondary("secondary--", ZoomOutHud, hud, cx),
        secondary("secondary-0", ZoomFitHud, hud, cx),
        KeyBinding::new("h", ToggleHudPartHidden, hud),
        KeyBinding::new("delete", HideHudPart, hud),
        // "?" arrives as Shift+/ on most layouts and as the bare character on others.
        KeyBinding::new("shift-/", ShowHudShortcuts, hud),
        KeyBinding::new("?", ShowHudShortcuts, hud),
    ];
    cx.bind_keys(keys);
}

/// A binding whose `secondary-` means Ctrl, or Cmd on macOS. `KeyBinding::new` takes only
/// explicit modifiers, so the alias resolves against the host OS through `KeyBinding::load`;
/// like `new`, it panics on a malformed literal.
#[allow(clippy::expect_used)]
fn secondary<A: Action>(keys: &str, action: A, context: Option<&str>, cx: &App) -> KeyBinding {
    let context = context.map(|c| {
        KeyBindingContextPredicate::parse(c)
            .expect("a static key context")
            .into()
    });
    KeyBinding::load(keys, Box::new(action), context, false, None, cx).expect("a static keystroke")
}
