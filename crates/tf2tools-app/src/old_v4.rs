//! The banner under the header offering to remove the old TF2Tools 4 on Windows. It asks once:
//! Remove starts v4's own uninstaller and closes the app, Not now just hides it; either is saved.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Global, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, Styled as _, TestSupportExt as _, Window, div,
    px,
};
use tf2tools_core::{OldV4, Settings};

use crate::i18n::tr;
use crate::ids;
use crate::local_messages::describe_settings;
use crate::services::{Services, run_blocking};
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::leave;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::updater::view::BANNER_HEIGHT;
use crate::widgets::labelled;

/// The banner's sentence, in the player's language.
pub fn notice() -> SharedString {
    tr!("shell.old_v4.notice")
}

/// Starts an uninstaller with its arguments; the app passes one that spawns the process.
pub type UninstallRunner = Arc<dyn Fn(&Path, &[String]) -> io::Result<()> + Send + Sync>;

/// The runner the app uses: starts the program detached and returns without waiting.
pub fn system_runner() -> UninstallRunner {
    Arc::new(|exe: &Path, args: &[String]| {
        let mut command = std::process::Command::new(exe);
        command
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // Its own console-less process group, so it keeps running after this app closes.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
            let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
            let path = std::env::var_os("PATH").unwrap_or_default();
            command.env("PATH", uninstaller_path(&root, &path));
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
        }
        command.spawn().map(drop)
    })
}

/// The PATH v4's uninstaller runs with: Windows' own folders under `system_root`, then the
/// player's `path`. Its "is TF2Tools running" check pipes into `find`, and a Unix `find` from
/// Git, MSYS2 or Cygwin ahead of System32's makes it give up without removing anything.
pub fn uninstaller_path(system_root: &OsStr, path: &OsStr) -> OsString {
    let mut out = OsString::new();
    for dir in [
        r"\System32;",
        ";",
        r"\System32\Wbem;",
        r"\System32\WindowsPowerShell\v1.0\;",
    ] {
        out.push(system_root);
        out.push(dir);
    }
    out.push(path);
    out
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

    // Not now, and a failed Remove, hide the banner and save the answer so it never asks again.
    fn forget(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.found.take().is_none() {
            return;
        }
        cx.notify();
        let save = run_blocking(cx, |s| {
            s.db.set_old_v4_asked()
                .map_err(|e| describe_settings(&e).into_op(tr!("messages.action.save_answer")))
        });
        cx.spawn_in(window, async move |_, cx| {
            if let Err(err) = save.await {
                let _ = cx.update(|window, cx| toast_error(window, cx, &err));
            }
        })
        .detach();
    }
}

// v4's uninstaller looks for a running "TF2Tools.exe", which is this app too, so Remove warns
// first, passes the unsaved-work guard, starts the uninstaller silently (this was the one ask)
// and only then closes the window.
fn ask_then_remove(notice: &Entity<OldV4Notice>, window: &mut Window, cx: &mut App) {
    let answer = confirm(Confirm::remove_old_v4(), window, cx);
    let notice = notice.clone();
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        if !answer.recv().await.unwrap_or(false) {
            return;
        }
        cx.update(|cx| {
            let _ = handle.update(cx, |_, window, cx| {
                leave::leave_then(window, cx, move |window, cx| {
                    remove_and_close(&notice, window, cx)
                });
            });
        });
    })
    .detach();
}

fn remove_and_close(notice: &Entity<OldV4Notice>, window: &mut Window, cx: &mut App) {
    let (old, runner) = {
        let n = notice.read(cx);
        let Some(old) = n.found.clone() else { return };
        (old, Arc::clone(&n.runner))
    };
    if runner(old.uninstaller(), &old.silent_arguments()).is_err() {
        toast(
            window,
            cx,
            ToastKind::Error,
            tr!("shell.old_v4.failed"),
            tr!("shell.old_v4.failed_hint"),
        );
        notice.update(cx, |n, cx| n.forget(window, cx));
        return;
    }
    // Saved here rather than in the background, since the app is about to close.
    if let Err(e) = cx.global::<Services>().db.set_old_v4_asked() {
        tracing::warn!("saving the TF2Tools 4 answer failed: {e}");
    }
    notice.update(cx, |n, cx| {
        n.found = None;
        cx.notify();
    });
    window.remove_window();
}

pub fn old_v4_notice(cx: &App) -> Option<Entity<OldV4Notice>> {
    cx.try_global::<OldV4Global>().map(|g| g.0.clone())
}

/// The banner while a v4 install waits for an answer, else None.
pub fn banner(cx: &App) -> Option<AnyElement> {
    let notice = old_v4_notice(cx)?;
    notice.read(cx).found()?;
    let theme = cx.theme();
    let button = |id: &'static str, label: SharedString, remove: bool| {
        let notice = notice.clone();
        let button = Button::new(id).small().label(label).flex_none();
        let button = if remove {
            button.outline()
        } else {
            button.ghost()
        };
        button.on_click(move |_, window, cx| {
            if remove {
                ask_then_remove(&notice, window, cx);
            } else {
                notice.update(cx, |n, cx| n.forget(window, cx));
            }
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
                self::notice(),
            ))
            .child(button(
                ids::OLD_V4_BANNER_REMOVE,
                tr!("shell.old_v4.remove"),
                true,
            ))
            .child(button(
                ids::OLD_V4_BANNER_LATER,
                tr!("shell.old_v4.later"),
                false,
            ))
            .into_any_element(),
    )
}
