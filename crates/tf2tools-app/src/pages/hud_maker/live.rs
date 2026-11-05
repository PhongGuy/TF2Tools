//! The live link: a Save of the installed HUD also updates its copy in `tf/custom`, and the
//! toolbar says how to see the change in a running TF2.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, ClipboardItem, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use tf2tools_core::{HudId, LogLevel, NewLogEntry, Reinstall, ReinstallError, Tf2Install};

use super::HudMakerPage;
use crate::activity;
use crate::ids;
use crate::services::Services;
use crate::shell::toast::{ToastKind, toast};
use crate::widgets::Named as _;

/// The console command that reloads a HUD's layouts and colours in a running TF2.
pub const RELOAD_COMMAND: &str = "hud_reloadscheme";
/// The toolbar note after a Save updated the installed HUD.
pub const LIVE_NOTE: &str = "Your installed HUD is updated. Type hud_reloadscheme in TF2's \
     console to see it. Font, image and animation changes show after you restart TF2.";
const NOT_UPDATED_TITLE: &str = "Installed copy not updated";
const IN_USE: &str = "TF2 is using this HUD's files. Close TF2, then Save again to update it.";
const GAME_MOVED: &str = "Saved, but TF2 wasn't found where it used to be. Check the \
     TF2 folder in Settings.";
const NOT_UPDATED: &str =
    "Saved, but couldn't update the installed copy. Close TF2 and install it again from HUDs.";

/// What a Save did to the installed copy of the HUD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Live {
    /// The saved HUD is not the installed one, or TF2 is not found.
    NotInstalled,
    /// The installed copy now matches the save.
    Updated,
    /// The installed copy is unchanged; the line says why.
    Failed(&'static str),
}

/// The installed copy no longer matches what TF2Tools put there, so it was left alone.
const CHANGED: &str = "Saved, but your installed HUD has changes of its own, so it wasn't updated. Install this HUD again to choose what to do.";

/// The player's line for a failed live update: something holding the HUD's files means close
/// TF2, an installed copy with its own changes asks the player to choose, anything else means
/// install it again.
pub fn live_failure(err: &ReinstallError) -> &'static str {
    match err {
        ReinstallError::InUse { .. } | ReinstallError::SetAside { .. } => IN_USE,
        ReinstallError::Changed { .. } => CHANGED,
        ReinstallError::GameMissing { .. } => GAME_MOVED,
        _ => NOT_UPDATED,
    }
}

/// Reinstalls library HUD `id` when it is the installed one, writing an activity row when it
/// updated; a failure goes to the developer log only.
pub(crate) fn live_job(s: &Services, id: &HudId, name: &str, tf2: &Tf2Install) -> Live {
    match s.library.reinstall_hud(id, tf2) {
        Ok(Reinstall::Updated { folder }) => {
            let target = tf2.custom_dir().join(&folder);
            let entry = NewLogEntry::new(
                LogLevel::Info,
                "hud",
                "install",
                format!("Updated installed HUD: {name}"),
            )
            .with_details(format!("Installed to: {}", target.display()));
            activity::write(s, &entry);
            Live::Updated
        }
        Ok(_) => Live::NotInstalled,
        Err(err) => {
            tracing::warn!(error = %err, "the installed HUD was not updated");
            Live::Failed(live_failure(&err))
        }
    }
}

impl HudMakerPage {
    /// Whether the toolbar shows the note after a live update.
    pub fn live_note_shown(&self) -> bool {
        self.live_note
    }

    /// Updated: shows the note. Failed: hides it and toasts a warning with the line.
    /// NotInstalled: hides it, since the save left the installed copy alone.
    pub(super) fn land_live(&mut self, live: &Live, window: &mut Window, cx: &mut Context<Self>) {
        match live {
            Live::NotInstalled => self.live_note = false,
            Live::Updated => self.live_note = true,
            Live::Failed(line) => {
                self.live_note = false;
                toast(window, cx, ToastKind::Warning, NOT_UPDATED_TITLE, *line);
            }
        }
        cx.notify();
    }

    /// The note row with Copy and close buttons; None while hidden. It fills the toolbar's
    /// width, so it wraps onto its own line.
    pub(super) fn live_note_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.live_note {
            return None;
        }
        let copy = Button::new(ids::HUD_MAKER_LIVE_COPY)
            .outline()
            .small()
            .label("Copy")
            .flex_none()
            .on_click(|_, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(RELOAD_COMMAND.to_owned()));
                toast(
                    window,
                    cx,
                    ToastKind::Info,
                    "Copied",
                    "Paste it into TF2's console.",
                );
            });
        let close = Button::new(ids::HUD_MAKER_LIVE_CLOSE)
            .ghost()
            .small()
            .icon(IconName::Close)
            .named("Close")
            .flex_none()
            .on_click(cx.listener(|page, _, _, cx| {
                page.live_note = false;
                cx.notify();
            }));
        // Without `min_w_0` the sentence keeps its full width and pushes Copy off the toolbar.
        let note = div()
            .id(ids::HUD_MAKER_LIVE_NOTE)
            .aria_label(LIVE_NOTE)
            .test_support()
            .flex_1()
            .min_w_0()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(LIVE_NOTE);
        Some(
            h_flex()
                .w_full()
                .gap_2()
                .child(note)
                .child(copy)
                .child(close)
                .into_any_element(),
        )
    }
}
