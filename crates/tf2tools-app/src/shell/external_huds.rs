//! The question before an action removes HUDs the library cannot put back: keep a copy in
//! the library, remove them anyway, or stop. Core refuses until it is told, so this is
//! where every HUD action learns what the player wants.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AsyncApp, FontWeight, InteractiveElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use tf2tools_core::{ExternalHud, LogLevel, NewLogEntry, Tf2Install};

use crate::activity;
use crate::ids;
use crate::local_messages::{describe_keep_hud, describe_library};
use crate::services::{OpError, Services, run_blocking};
use crate::shell::toast::toast_error;
use crate::widgets::dialog_frame::{DIALOG_WIDTH, DialogFrame};

// More names than this push the dialog's buttons off a small window.
const MOST_NAMES: usize = 10;

/// What is about to remove the HUDs, which words the question uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudAction {
    Install,
    Uninstall,
    ActivateProfile,
}

/// The dialog's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalAnswer {
    Keep,
    Discard,
    Cancel,
}

/// What the player agreed to for the HUDs in the way; the default is that none were.
#[derive(Debug, Clone, Default)]
pub struct ExternalPlan {
    huds: Vec<ExternalHud>,
    keep: bool,
}

impl ExternalPlan {
    /// The plan for `answer` about `huds`; `None` when the player stopped.
    pub fn from_answer(answer: ExternalAnswer, huds: Vec<ExternalHud>) -> Option<Self> {
        match answer {
            ExternalAnswer::Keep => Some(Self { huds, keep: true }),
            ExternalAnswer::Discard => Some(Self { huds, keep: false }),
            ExternalAnswer::Cancel => None,
        }
    }

    /// Blocking. Adds the kept HUDs to the library and returns the HUDs the action may now
    /// remove. A failed copy is an error titled `action`, and the action must not run.
    pub fn prepare(
        &self,
        s: &Services,
        tf2: &Tf2Install,
        action: &'static str,
    ) -> Result<&[ExternalHud], OpError> {
        if self.keep {
            let kept = s
                .library
                .keep_external_huds(&self.huds, tf2)
                .map_err(|e| describe_keep_hud(&e).into_op(action))?;
            for hud in kept {
                let message = format!("Kept HUD in library: {}", hud.folder_name());
                let entry = NewLogEntry::new(LogLevel::Info, "hud", "import", message);
                activity::write(s, &entry);
            }
        }
        Ok(&self.huds)
    }

    /// Writes the activity row for HUDs the finished action removed without a copy.
    pub fn log_discarded(&self, s: &Services) {
        if !self.keep && !self.huds.is_empty() {
            let message = format!("Removed without keeping: {}", names_text(&self.huds));
            let entry = NewLogEntry::new(LogLevel::Warning, "hud", "remove", message);
            activity::write(s, &entry);
        }
    }

    /// The sentence a success toast adds about these HUDs, if any.
    pub fn note(&self) -> Option<String> {
        if self.huds.is_empty() {
            return None;
        }
        let names = names_text(&self.huds);
        Some(if self.keep {
            format!("A copy of {names} is in your library.")
        } else {
            format!("Removed without a copy: {names}.")
        })
    }
}

/// `huds`' names as a sentence would list them, cut short after `MOST_NAMES`.
pub fn names_text(huds: &[ExternalHud]) -> String {
    let names: Vec<&str> = huds.iter().map(ExternalHud::folder_name).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [start @ .., last] if names.len() <= MOST_NAMES => {
            format!("{} and {last}", start.join(", "))
        }
        all => format!(
            "{} and {} more",
            all[..MOST_NAMES].join(", "),
            all.len() - MOST_NAMES
        ),
    }
}

/// The dialog's texts for `action`: title, description and destructive button's label.
pub fn dialog_text(action: HudAction, huds: &[ExternalHud]) -> (String, String, &'static str) {
    let names = names_text(huds);
    let changes = huds.iter().any(ExternalHud::has_changes);
    let title = match huds {
        [one] if changes => format!("Keep your changes to {}?", one.folder_name()),
        [one] => format!("Keep {}?", one.folder_name()),
        _ if changes => "Keep your changes?".to_owned(),
        _ => "Keep these HUDs?".to_owned(),
    };
    let doing = match action {
        HudAction::Install => "Installing this HUD",
        HudAction::Uninstall => "Uninstalling",
        HudAction::ActivateProfile => "Activating this profile",
    };
    let label = match action {
        HudAction::Uninstall => "Remove without keeping",
        HudAction::Install | HudAction::ActivateProfile => "Replace without keeping",
    };
    let description = match huds {
        _ if changes => format!(
            "{doing} removes {names} from TF2. Your changes can't be brought back unless you \
             keep a copy."
        ),
        [_] => format!(
            "{doing} removes {names} from TF2. TF2Tools has no copy of it, so it can't come \
             back unless you keep one."
        ),
        _ => format!(
            "{doing} removes {names} from TF2. TF2Tools has no copy of them, so they can't \
             come back unless you keep one."
        ),
    };
    (title, description, label)
}

/// Finds the HUDs `action` would lose and asks about them. `Some` carries on with what the
/// player chose, and with an empty plan when there is nothing to ask; `None` means stop,
/// after a toast when the scan failed.
pub async fn check_external(
    action: HudAction,
    tf2: Tf2Install,
    win: AnyWindowHandle,
    title: &'static str,
    cx: &AsyncApp,
) -> Option<ExternalPlan> {
    let scanning = cx.update(|cx| {
        run_blocking(cx, move |s| {
            let found = s.library.external_huds(&tf2);
            found.map_err(|e| describe_library(&e).into_op(title))
        })
    });
    match scanning.await {
        Ok(huds) if huds.is_empty() => Some(ExternalPlan::default()),
        Ok(huds) => ask_external(action, huds, win, cx).await,
        Err(err) => {
            let _ = cx.update(|cx| win.update(cx, |_, window, cx| toast_error(window, cx, &err)));
            None
        }
    }
}

/// Asks about `huds`; `None` means stop, and a dialog gone unanswered stops too.
pub async fn ask_external(
    action: HudAction,
    huds: Vec<ExternalHud>,
    win: AnyWindowHandle,
    cx: &AsyncApp,
) -> Option<ExternalPlan> {
    let asked = huds.clone();
    let answer = cx
        .update(|cx| {
            win.update(cx, move |_, window, cx| {
                external_huds_dialog(action, &asked, window, cx)
            })
        })
        .ok()?;
    ExternalPlan::from_answer(answer.recv().await.ok()?, huds)
}

// Opens the dialog over the window. Enter takes the default, the choice that loses nothing;
// Escape cancels. The first answer wins through the one-slot channel.
fn external_huds_dialog(
    action: HudAction,
    huds: &[ExternalHud],
    window: &mut Window,
    cx: &mut App,
) -> async_channel::Receiver<ExternalAnswer> {
    let (tx, rx) = async_channel::bounded(1);
    let (title, description, discard_label) = dialog_text(action, huds);
    let (title, description): (SharedString, SharedString) = (title.into(), description.into());
    // Deferred so a menu restoring focus after its click handler cannot pull focus out.
    window.defer(cx, move |window, cx| {
        let frame = DialogFrame::new(DIALOG_WIDTH);
        window.open_dialog(cx, move |dialog, window, cx| {
            // The builder runs every frame, so each handler clones what it captures.
            let answer = |reply: ExternalAnswer| {
                let tx = tx.clone();
                move |_: &_, _: &mut Window, _: &mut App| {
                    let _ = tx.try_send(reply);
                    true
                }
            };
            let button = |id: &'static str, reply: ExternalAnswer| {
                let tx = tx.clone();
                Button::new(id).on_click(move |_, window, cx| {
                    let _ = tx.try_send(reply);
                    window.close_dialog(cx);
                })
            };
            let muted = cx.theme().muted_foreground;
            frame
                .apply(dialog, &*window, &*cx)
                .close_button(false)
                .on_ok(answer(ExternalAnswer::Keep))
                .on_cancel(answer(ExternalAnswer::Cancel))
                .child(
                    frame.measure(
                        v_flex()
                            .id(ids::EXTERNAL_HUDS_DIALOG)
                            .test_support()
                            .gap_4()
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(ids::EXTERNAL_HUDS_TITLE)
                                            .aria_label(title.clone())
                                            .test_support()
                                            .text_lg()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(title.clone()),
                                    )
                                    .child(
                                        div()
                                            .id(ids::EXTERNAL_HUDS_DESCRIPTION)
                                            .aria_label(description.clone())
                                            .test_support()
                                            .text_sm()
                                            .text_color(muted)
                                            .child(description.clone()),
                                    ),
                            )
                            // Three long labels outgrow one row, so the row may wrap.
                            .child(
                                h_flex()
                                    .flex_wrap()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        button(ids::EXTERNAL_HUDS_CANCEL, ExternalAnswer::Cancel)
                                            .outline()
                                            .label("Cancel"),
                                    )
                                    .child(
                                        button(ids::EXTERNAL_HUDS_DISCARD, ExternalAnswer::Discard)
                                            .danger()
                                            .label(discard_label),
                                    )
                                    .child(
                                        button(ids::EXTERNAL_HUDS_KEEP, ExternalAnswer::Keep)
                                            .primary()
                                            .label("Keep a copy in my library"),
                                    ),
                            ),
                    ),
                )
        });
    });
    rx
}
