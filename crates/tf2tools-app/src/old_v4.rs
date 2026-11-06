//! The banner under the header offering to remove the old TF2Tools 4 on Windows. It asks once:
//! Remove starts v4's own uninstaller, Not now just hides it, and either answer is saved.

use std::io;
use std::path::Path;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Global, InteractiveElement as _,
    IntoElement, ParentElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};
use tf2tools_core::{OldV4, Settings};

use crate::ids;
use crate::local_messages::describe_settings;
use crate::services::run_blocking;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::updater::view::BANNER_HEIGHT;
use crate::widgets::labelled;

pub const NOTICE: &str = "The old version of TF2Tools is still installed.";

/// Starts an uninstaller with its arguments; the app passes one that spawns the process.
pub type UninstallRunner = Arc<dyn Fn(&Path, &[String]) -> io::Result<()> + Send + Sync>;

/// The runner the app uses: starts the program and returns without waiting.
pub fn system_runner() -> UninstallRunner {
    Arc::new(|exe: &Path, args: &[String]| {
        std::process::Command::new(exe).args(args).spawn().map(drop)
    })
}

/// The install to offer removing: `detect` runs only while the player has not answered yet.
pub fn pending(settings: &Settings, detect: impl FnOnce() -> Option<OldV4>) -> Option<OldV4> {
    if settings.old_v4_asked() {
        None
    } else {
        detect()
    }
}

/// The v4 install found at startup, held until the player answers.
pub struct OldV4Notice {
    found: Option<OldV4>,
    runner: UninstallRunner,
}

struct OldV4Global(Entity<OldV4Notice>);
impl Global for OldV4Global {}

impl OldV4Notice {
    /// Creates the notice over `found` and makes it the app's global.
    pub fn install(
        cx: &mut App,
        found: Option<OldV4>,
        runner: UninstallRunner,
    ) -> Entity<OldV4Notice> {
        let notice = cx.new(|_| OldV4Notice { found, runner });
        cx.set_global(OldV4Global(notice.clone()));
        notice
    }

    pub fn found(&self) -> Option<&OldV4> {
        self.found.as_ref()
    }

    // Either answer hides the banner and is saved, so it never asks again.
    fn answer(&mut self, remove: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(old) = self.found.take() else { return };
        cx.notify();
        if remove && (self.runner)(old.uninstaller(), old.arguments()).is_err() {
            toast(
                window,
                cx,
                ToastKind::Error,
                "Couldn't remove the old TF2Tools",
                "Remove it in Windows Settings > Apps instead.",
            );
        }
        let save = run_blocking(cx, |s| {
            s.db.set_old_v4_asked()
                .map_err(|e| describe_settings(&e).into_op("Save your answer"))
        });
        cx.spawn_in(window, async move |_, cx| {
            if let Err(err) = save.await {
                let _ = cx.update(|window, cx| toast_error(window, cx, &err));
            }
        })
        .detach();
    }
}

pub fn old_v4_notice(cx: &App) -> Option<Entity<OldV4Notice>> {
    cx.try_global::<OldV4Global>().map(|g| g.0.clone())
}

/// The banner while a v4 install waits for an answer, else None.
pub fn banner(cx: &App) -> Option<AnyElement> {
    let notice = old_v4_notice(cx)?;
    notice.read(cx).found()?;
    let theme = cx.theme();
    let button = |id: &'static str, label: &'static str, remove: bool| {
        let notice = notice.clone();
        let button = Button::new(id).small().label(label).flex_none();
        let button = if remove {
            button.outline()
        } else {
            button.ghost()
        };
        button.on_click(move |_, window, cx| {
            notice.update(cx, |n, cx| n.answer(remove, window, cx));
        })
    };
    // The crash banner's row, so the banners stack as one style under the header.
    Some(
        h_flex()
            .id(ids::OLD_V4_BANNER)
            .test_support()
            .flex_none()
            .w_full()
            .min_h(BANNER_HEIGHT)
            .px(px(24.))
            .gap_3()
            .text_sm()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.primary.opacity(0.05))
            .child(
                Icon::new(IconName::Info)
                    .small()
                    .flex_none()
                    .text_color(theme.primary),
            )
            .child(labelled(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis(),
                ids::OLD_V4_BANNER_TEXT,
                NOTICE,
            ))
            .child(button(ids::OLD_V4_BANNER_REMOVE, "Remove it", true))
            .child(button(ids::OLD_V4_BANNER_LATER, "Not now", false))
            .into_any_element(),
    )
}
