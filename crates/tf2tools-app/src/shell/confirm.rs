//! v5's destructive confirm dialog, drawn in-app because GPUI's platform prompt is
//! invisible to AT-SPI and so to the pilot.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};

use crate::i18n::{plural, tr};
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
            title: tr!("confirm.discard_changes.title"),
            description: tr!("confirm.discard_changes.body"),
            confirm_label: tr!("confirm.discard_changes.ok"),
            cancel_label: tr!("confirm.discard_changes.keep"),
        }
    }

    /// The prompt before leaving a HUD with unsaved changes; `can_save` adds that "Save and
    /// leave" is offered, and without it the player is told a name lets them save.
    pub fn leave_unsaved(can_save: bool) -> Self {
        let description = match can_save {
            true => tr!("confirm.leave_unsaved.can_save"),
            false => tr!("confirm.leave_unsaved.needs_name"),
        };
        Self {
            description,
            ..Self::discard_changes()
        }
    }

    /// The prompt before removing TF2Tools 4, whose uninstaller closes this app too.
    pub fn remove_old_v4() -> Self {
        Self {
            title: tr!("confirm.remove_old_v4.title"),
            description: tr!("confirm.remove_old_v4.body"),
            confirm_label: tr!("confirm.remove_old_v4.ok"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// The prompt before shrinking a crosshair drawing to 32x32 would cut paint off its edges.
    pub fn shrink_canvas() -> Self {
        Self {
            title: tr!("confirm.shrink_canvas.title"),
            description: tr!("confirm.shrink_canvas.body"),
            confirm_label: tr!("confirm.shrink_canvas.ok"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// v5's HudCard delete prompt for the HUD called `name`.
    pub fn delete_hud(name: &str) -> Self {
        Self {
            title: tr!("confirm.delete_hud.title"),
            description: format!(
                "{} {}",
                tr!("confirm.delete_hud.body", name = name),
                tr!("confirm.cannot_undo")
            )
            .into(),
            confirm_label: tr!("confirm.delete"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// v5's sounds page delete prompt.
    pub fn delete_sound() -> Self {
        Self {
            title: tr!("confirm.delete_sound.title"),
            description: format!(
                "{} {}",
                tr!("confirm.delete_sound.body"),
                tr!("confirm.cannot_undo")
            )
            .into(),
            confirm_label: tr!("confirm.delete"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// The prompt before deleting crosshair `name`, which `weapons` weapons use. `installed`
    /// adds that it also leaves the game, for a crosshair whose files are in TF2.
    pub fn delete_crosshair(name: &str, weapons: usize, installed: bool) -> Self {
        let used = match weapons {
            0 => String::new(),
            n => format!(
                " {}",
                plural(
                    "confirm.crosshair_used",
                    u64::try_from(n).unwrap_or(u64::MAX)
                )
            ),
        };
        let game = if installed {
            format!(" {}", tr!("confirm.delete_crosshair.in_tf2"))
        } else {
            String::new()
        };
        Self {
            title: tr!("confirm.delete_crosshair.title"),
            description: format!(
                "{}{used}{game} {}",
                tr!("confirm.delete_crosshair.body", name = name),
                tr!("confirm.cannot_undo")
            )
            .into(),
            confirm_label: tr!("confirm.delete"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// The prompt before a HUD Maker history snapshot is deleted.
    pub fn delete_snapshot(name: &str) -> Self {
        Self {
            title: tr!("confirm.delete_snapshot.title"),
            description: tr!("confirm.delete_snapshot.body", name = name),
            confirm_label: tr!("confirm.delete"),
            cancel_label: tr!("confirm.delete_snapshot.keep"),
        }
    }

    /// The prompt before the HUD Maker resets every option of the open HUD.
    pub fn reset_all_options() -> Self {
        Self {
            title: tr!("confirm.reset_all_options.title"),
            description: tr!("confirm.reset_all_options.body"),
            confirm_label: tr!("confirm.reset_all_options.ok"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// v5's Activity Log "Clear All" prompt.
    pub fn clear_logs() -> Self {
        Self {
            title: tr!("confirm.clear_logs.title"),
            description: tr!("confirm.clear_logs.body"),
            confirm_label: tr!("activity_log.clear_all"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// The Storage card's prompt before deleting the downloaded demos, which take `size` on disk.
    pub fn clear_demo_cache(size: &str) -> Self {
        Self {
            title: tr!("confirm.clear_demo_cache.title"),
            description: tr!("confirm.clear_demo_cache.body", size = size),
            confirm_label: tr!("confirm.clear_demo_cache.ok"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// v5's confirm before removing an addon from the library, and from tf/custom if installed.
    pub fn delete_addon() -> Self {
        Self {
            title: tr!("confirm.delete_addon.title"),
            description: tr!("confirm.delete_addon.body"),
            confirm_label: tr!("confirm.delete"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// The prompt before removing the GameBanana map called `name` from TF2.
    pub fn remove_gb_map(name: &str) -> Self {
        Self {
            title: tr!("confirm.remove_gb_map.title"),
            description: tr!("confirm.remove_gb_map.body", name = name),
            confirm_label: tr!("confirm.remove"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// The prompt before deleting the downloaded serveme.tf map `name` from TF2.
    pub fn remove_map(name: &str) -> Self {
        Self {
            title: tr!("confirm.remove_map.title"),
            description: tr!("confirm.remove_map.body", name = name),
            confirm_label: tr!("confirm.remove"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// v5's ProfileCard delete prompt for the profile called `name`; `active` adds that the
    /// game stays as it is.
    pub fn delete_profile(name: &str, active: bool) -> Self {
        let mut description = format!(
            "{} {}",
            tr!("confirm.delete_profile.body", name = name),
            tr!("confirm.cannot_undo")
        );
        if active {
            description = format!("{description} {}", tr!("confirm.delete_profile.active"));
        }
        Self {
            title: tr!("confirm.delete_profile.title"),
            description: description.into(),
            confirm_label: tr!("confirm.delete"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// The prompt before removing every crosshair from a profile.
    pub fn clear_profile_crosshairs() -> Self {
        Self {
            title: tr!("confirm.clear_profile_crosshairs.title"),
            description: tr!("confirm.clear_profile_crosshairs.body"),
            confirm_label: tr!("confirm.clear_profile_crosshairs.ok"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// The prompt before removing mastercomfig from TF2.
    pub fn remove_mastercomfig() -> Self {
        Self {
            title: "Remove mastercomfig".into(),
            description:
                "This removes mastercomfig and the preset you picked from TF2. Your own config files stay."
                    .into(),
            confirm_label: "Remove".into(),
            cancel_label: "Cancel".into(),
        }
    }

    /// Before replacing a modules.cfg TF2Tools did not write.
    pub fn replace_modules_cfg() -> Self {
        Self {
            title: "Replace your modules.cfg?".into(),
            description: "TF2 already has a modules.cfg that TF2Tools didn't write. Your file is kept as a backup next to it.".into(),
            confirm_label: "Replace it".into(),
            cancel_label: "Cancel".into(),
        }
    }

    /// The prompt before installing an addon into a TF2 without mastercomfig's base.
    pub fn addon_needs_mastercomfig() -> Self {
        Self {
            title: tr!("confirm.addon_needs_mastercomfig.title"),
            description: tr!("confirm.addon_needs_mastercomfig.body"),
            confirm_label: tr!("confirm.addon_needs_mastercomfig.ok"),
            cancel_label: tr!("confirm.cancel"),
        }
    }

    /// The prompt before activating a profile while TF2 runs.
    pub fn tf2_running() -> Self {
        Self {
            title: tr!("confirm.tf2_running.title"),
            description: tr!("confirm.tf2_running.body"),
            confirm_label: tr!("confirm.tf2_running.ok"),
            cancel_label: tr!("confirm.cancel"),
        }
    }
}

/// Opens an in-app dialog: ids confirm-dialog, confirm-title, confirm-description,
/// confirm-cancel (`cancel_label`) and confirm-ok (danger, `confirm_label`). Cancel, Escape, Enter
/// and OK close it; only OK answers true. The receiver yields the answer once; it errs if the dialog goes away unanswered.
pub fn confirm(spec: Confirm, window: &mut Window, cx: &mut App) -> async_channel::Receiver<bool> {
    ask(spec, None, false, true, window, cx)
}

/// What the player chose in a dialog that offers a third way out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Cancel, Escape, Enter or the dialog going away.
    Stay,
    /// The danger button (`confirm_label`).
    Confirm,
    /// The primary button added beside it, under `ids::CONFIRM_SAVE`.
    Other,
}

/// As `confirm`, answered as a `Choice`, with a primary `other` button after the danger one
/// when it is given.
pub fn confirm_or(
    spec: Confirm,
    other: Option<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> async_channel::Receiver<Choice> {
    let other = other.map(|label| (label, Choice::Other));
    ask(spec, other, Choice::Stay, Choice::Confirm, window, cx)
}

// The one dialog behind both: `no` for every way of backing out, `yes` for the danger button
// and `other`'s answer for its own button.
fn ask<T: Copy + Send + 'static>(
    spec: Confirm,
    other: Option<(SharedString, T)>,
    no: T,
    yes: T,
    window: &mut Window,
    cx: &mut App,
) -> async_channel::Receiver<T> {
    // A one-slot channel and try_send: the first answer wins, any later one drops.
    let (tx, rx) = async_channel::bounded(1);
    // Deferred so a menu restoring focus after its click handler cannot pull focus out.
    window.defer(cx, move |window, cx| {
        let frame = DialogFrame::new(DIALOG_WIDTH);
        window.open_dialog(cx, move |dialog, window, cx| {
            // The builder runs every frame, so each handler clones what it captures.
            let answer = |said: T| {
                let tx = tx.clone();
                move |_: &_, _: &mut Window, _: &mut App| {
                    let _ = tx.try_send(said);
                    true
                }
            };
            let button = |id: &'static str, said: T| {
                let tx = tx.clone();
                Button::new(id).on_click(move |_, window, cx| {
                    let _ = tx.try_send(said);
                    window.close_dialog(cx);
                })
            };
            let other = other
                .clone()
                .map(|(label, said)| button(ids::CONFIRM_SAVE, said).primary().label(label));
            let muted = cx.theme().muted_foreground;
            frame
                .apply(dialog, &*window, &*cx)
                .close_button(false)
                // The kit binds Enter to Confirm; v5 focuses Cancel on open, so Enter cancels
                // here too and only a click on confirm-ok answers yes.
                .on_ok(answer(no))
                .on_cancel(answer(no))
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
                                        button(ids::CONFIRM_CANCEL, no)
                                            .outline()
                                            .label(spec.cancel_label.clone()),
                                    )
                                    .child(
                                        button(ids::CONFIRM_OK, yes)
                                            .danger()
                                            .label(spec.confirm_label.clone()),
                                    )
                                    .children(other),
                            ),
                    ),
                )
        });
    });
    rx
}
