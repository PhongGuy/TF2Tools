//! The startup error view: v5 has none, so a centred card states why the app could not start.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};

use crate::ids;
use crate::local_messages::describe_startup;
use crate::services::StartupError;

/// Element `startup-error` (`Role::Alert`, labelled with the error's plain message).
pub(crate) fn render(error: &StartupError, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let message = describe_startup(error).message;
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.background)
        .text_color(theme.foreground)
        .child(
            v_flex()
                .id(ids::STARTUP_ERROR)
                .role(Role::Alert)
                .aria_label(message.clone())
                .test_support()
                .max_w(px(560.))
                .gap_3()
                .p_6()
                .border_1()
                .border_color(theme.border)
                .rounded(theme.radius_lg)
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::BOLD)
                        .child("TF2Tools could not start"),
                )
                .child(div().text_color(theme.muted_foreground).child(message)),
        )
}
