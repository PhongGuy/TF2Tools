//! v5's destructive confirm dialog, drawn in-app because GPUI's platform prompt is
//! invisible to AT-SPI and so to the pilot.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};

use crate::ids;
use crate::widgets::dialog_frame::{DIALOG_WIDTH, DialogFrame};

/// The texts of one confirm dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub title: SharedString,
    pub description: SharedString,
    pub confirm_label: SharedString,
    pub cancel_label: SharedString,
}

impl Confirm {
    /// The prompt before dropping a drawing with unsaved changes.
    pub fn discard_changes() -> Self {
        Self {
            title: "Unsaved changes".into(),
            description: "You have unsaved changes. Discard them?".into(),
            confirm_label: "Discard".into(),
            cancel_label: "Keep editing".into(),
        }
    }

    /// v5's HudCard delete prompt for the HUD called `name`.
    pub fn delete_hud(name: &str) -> Self {
        Self {
            title: "Delete HUD".into(),
            description: format!(
                "Are you sure you want to delete {name} from your library? This cannot be undone."
            )
            .into(),
            confirm_label: "Delete".into(),
            cancel_label: "Cancel".into(),
        }
    }

    /// v5's sounds page delete prompt.
    pub fn delete_sound() -> Self {
        Self {
            title: "Delete Sound".into(),
            description:
                "Are you sure you want to delete this sound from your library? This cannot be undone."
                    .into(),
            confirm_label: "Delete".into(),
            cancel_label: "Cancel".into(),
        }
    }

    /// The prompt before deleting crosshair `name`, which `weapons` weapons use. `installed`
    /// adds that it also leaves the game, for a crosshair whose files are in TF2.
    pub fn delete_crosshair(name: &str, weapons: usize, installed: bool) -> Self {
        let used = match weapons {
            0 => String::new(),
            1 => " It is used by 1 weapon. It goes back to the default crosshair.".to_owned(),
            n => format!(" It is used by {n} weapons. They go back to the default crosshair."),
        };
        let game = if installed {
            " It is also removed from TF2."
        } else {
            ""
        };
        Self {
            title: "Delete Crosshair".into(),
            description: format!(
                "Are you sure you want to delete {name} from your library?{used}{game} This cannot be undone."
            )
            .into(),
            confirm_label: "Delete".into(),
            cancel_label: "Cancel".into(),
        }
    }

    /// The prompt before a HUD Maker history snapshot is deleted.
    pub fn delete_snapshot(name: &str) -> Self {
        Self {
            title: "Delete this snapshot?".into(),
            description: format!("{name} will be gone for good. Your HUD stays as it is.").into(),
            confirm_label: "Delete".into(),
            cancel_label: "Keep".into(),
        }
    }

    /// v5's Activity Log "Clear All" prompt.
    pub fn clear_logs() -> Self {
        Self {
            title: "Clear All Logs".into(),
            description: "Are you sure you want to clear all activity logs? This cannot be undone."
                .into(),
            confirm_label: "Clear All".into(),
            cancel_label: "Cancel".into(),
        }
    }

    /// v5's confirm before removing an addon from the library, and from tf/custom if installed.
    pub fn delete_addon() -> Self {
        Self {
            title: "Delete Addon".into(),
            description: "Are you sure you want to remove this addon from your library? If installed, it will also be removed from TF2."
                .into(),
            confirm_label: "Delete".into(),
            cancel_label: "Cancel".into(),
        }
    }

    /// v5's ProfileCard delete prompt for the profile called `name`.
    pub fn delete_profile(name: &str) -> Self {
        Self {
            title: "Delete Profile".into(),
            description: format!(
                "Are you sure you want to delete \"{name}\"? This cannot be undone."
            )
            .into(),
            confirm_label: "Delete".into(),
            cancel_label: "Cancel".into(),
        }
    }

    /// v5's prompt before activating a profile with no HUD, sounds or crosshairs.
    pub fn activate_empty() -> Self {
        Self {
            title: "Activate Empty Profile".into(),
            description: "This profile has no customizations. Activating it will remove all installed HUDs, sounds, and crosshairs."
                .into(),
            confirm_label: "Activate Anyway".into(),
            cancel_label: "Cancel".into(),
        }
    }

    /// The prompt before activating a profile while TF2 runs.
    pub fn tf2_running() -> Self {
        Self {
            title: "TF2 is running".into(),
            description: "TF2 is running; changes take effect after a restart.".into(),
            confirm_label: "Continue".into(),
            cancel_label: "Cancel".into(),
        }
    }
}

/// Opens an in-app dialog: ids confirm-dialog, confirm-title, confirm-description,
/// confirm-cancel (`cancel_label`) and confirm-ok (danger, `confirm_label`). Cancel, Escape, Enter
/// and OK close it; only OK answers true. The receiver yields the answer once; it errs if the dialog goes away unanswered.
pub fn confirm(spec: Confirm, window: &mut Window, cx: &mut App) -> async_channel::Receiver<bool> {
    // A one-slot channel and try_send: the first answer wins, any later one drops.
    let (tx, rx) = async_channel::bounded(1);
    // Deferred so a menu restoring focus after its click handler cannot pull focus out.
    window.defer(cx, move |window, cx| {
        let frame = DialogFrame::new(DIALOG_WIDTH);
        window.open_dialog(cx, move |dialog, window, cx| {
            // The builder runs every frame, so each handler clones what it captures.
            let answer = |yes: bool| {
                let tx = tx.clone();
                move |_: &_, _: &mut Window, _: &mut App| {
                    let _ = tx.try_send(yes);
                    true
                }
            };
            let button = |id: &'static str, yes: bool| {
                let tx = tx.clone();
                Button::new(id).on_click(move |_, window, cx| {
                    let _ = tx.try_send(yes);
                    window.close_dialog(cx);
                })
            };
            let muted = cx.theme().muted_foreground;
            frame
                .apply(dialog, &*window, &*cx)
                .close_button(false)
                // The kit binds Enter to Confirm; v5 focuses Cancel on open, so Enter cancels
                // here too and only a click on confirm-ok answers yes.
                .on_ok(answer(false))
                .on_cancel(answer(false))
                .child(
                    frame.measure(
                        v_flex()
                            .id(ids::CONFIRM_DIALOG)
                            .test_support()
                            .gap_4()
                            // v5's dialog header: title and description 8 px apart.
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(ids::CONFIRM_TITLE)
                                            .aria_label(spec.title.clone())
                                            .test_support()
                                            .text_lg()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(spec.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .id(ids::CONFIRM_DESCRIPTION)
                                            .aria_label(spec.description.clone())
                                            .test_support()
                                            .text_sm()
                                            .text_color(muted)
                                            .child(spec.description.clone()),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        button(ids::CONFIRM_CANCEL, false)
                                            .outline()
                                            .label(spec.cancel_label.clone()),
                                    )
                                    .child(
                                        button(ids::CONFIRM_OK, true)
                                            .danger()
                                            .label(spec.confirm_label.clone()),
                                    ),
                            ),
                    ),
                )
        });
    });
    rx
}
