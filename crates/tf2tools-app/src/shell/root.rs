//! The window's root view: v5's loading screen, then the shell or the startup error view.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, TestSupportExt as _, Window, div,
};

use super::{Shell, error_view};
use crate::ids;
use crate::services::StartupError;

/// What the main window shows while and after the app starts.
pub enum AppRoot {
    /// Element `startup-loading`: "TF2Tools" over "Loading...", as v5's layout shows it.
    Loading,
    Ready(Entity<Shell>),
    /// The startup error view, in player words.
    Failed {
        error: StartupError,
    },
}

impl Render for AppRoot {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self {
            Self::Loading => loading(cx).into_any_element(),
            Self::Ready(shell) => shell.clone().into_any_element(),
            Self::Failed { error } => error_view::render(error, cx).into_any_element(),
        }
    }
}

fn loading(cx: &mut Context<AppRoot>) -> AnyElement {
    let theme = cx.theme();
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.background)
        .child(
            v_flex()
                .id(ids::STARTUP_LOADING)
                .test_support()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_2xl()
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.foreground)
                        .child("TF2Tools"),
                )
                .child(div().text_color(theme.muted_foreground).child("Loading...")),
        )
        .into_any_element()
}
