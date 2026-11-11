//! v5's sonner toasts, drawn above every dialog.
//!
//! The kit's own notification layer is an ordinary child of the window root, and dialogs paint
//! deferred, so they would cover it. Toasts therefore live in their own deferred root plugin.

use gpui_kit::base::{POPUP_PRIORITY, Root, RootPlugin};
use gpui_kit::component::notification::{Notification, NotificationList, NotificationType};
use gpui_kit::component::{ActiveTheme as _, Theme, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, deferred, div, px,
};

use crate::ids;
use crate::services::OpError;

/// The toasts' bottom margin while the sound player shows: above the open player's top.
pub const ABOVE_PLAYER: Pixels = px(120.);
/// The toasts' usual bottom margin, 16 px from the window corner.
const CORNER: Pixels = px(16.);

// Above the kit's dialogs, which paint at 10 plus their depth, and under an open menu or tooltip.
const TOAST_PRIORITY: usize = POPUP_PRIORITY - 1;

/// The window's toast stack, one per window, painted above every dialog and its dimming.
pub struct ToastLayer {
    list: Entity<NotificationList>,
}

impl ToastLayer {
    /// Registers the layer for every window opened after this call.
    pub fn register(cx: &mut App) {
        Root::register_plugin(cx, |window, cx| Self {
            list: cx.new(|cx| NotificationList::new(window, cx)),
        });
    }

    // This window's toast list, or `None` in a window opened before `register`.
    fn list(window: &Window, cx: &App) -> Option<Entity<NotificationList>> {
        let root = window.root::<Root>().flatten()?;
        let layer = root.read(cx).plugin::<Self>()?;
        Some(layer.read(cx).list.clone())
    }
}

impl RootPlugin for ToastLayer {}

impl Render for ToastLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        deferred(div().absolute().inset_0().child(self.list.clone())).with_priority(TOAST_PRIORITY)
    }
}

/// How many toasts this window holds, closing ones included.
pub fn toast_count(window: &Window, cx: &App) -> usize {
    ToastLayer::list(window, cx).map_or(0, |list| list.read(cx).notifications().len())
}

/// Closes every toast in this window.
pub fn clear_toasts(window: &mut Window, cx: &mut App) {
    if let Some(list) = ToastLayer::list(window, cx) {
        list.update(cx, |list, cx| list.clear(window, cx));
    }
}

/// Lifts the toast stack above the sound player, or puts it back 16 px from the corner.
pub fn lift_above_player(lifted: bool, cx: &mut App) {
    lift_toasts(lifted.then_some(ABOVE_PLAYER), cx);
}

/// Puts the toast stack's bottom `bottom` above the window's bottom edge, or with `None` back
/// 16 px from the corner.
pub fn lift_toasts(bottom: Option<Pixels>, cx: &mut App) {
    let bottom = bottom.unwrap_or(CORNER);
    Theme::update(cx, |t| t.notification.margins.bottom = bottom);
}

/// How a toast reads: the kit's info toast, or v5's success, warning or error toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

/// Pushes a kit notification whose content is element `toast` with `Role::Status`, labelled
/// "{title}. {description}", or the title alone when the description is empty.
pub fn toast(
    window: &mut Window,
    cx: &mut App,
    kind: ToastKind,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
) {
    let (title, description) = (title.into(), description.into());
    let label = label(&title, &description);
    let kind = match kind {
        ToastKind::Info => NotificationType::Info,
        ToastKind::Success => NotificationType::Success,
        ToastKind::Warning => NotificationType::Warning,
        ToastKind::Error => NotificationType::Error,
    };
    // No `id1` key, so two identical toasts both show as v5's sonner does; `message` stays
    // unset so the text draws once, inside the observable content.
    let toast = Notification::new()
        .with_type(kind)
        .content(move |_, _, cx| {
            let muted = cx.theme().muted_foreground;
            v_flex()
                .id(ids::TOAST)
                .role(Role::Status)
                .aria_label(label.clone())
                .test_support()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title.clone()))
                .when(!description.is_empty(), |el| {
                    el.child(div().text_sm().text_color(muted).child(description.clone()))
                })
                .into_any_element()
        });
    if let Some(list) = ToastLayer::list(window, cx) {
        list.update(cx, |list, cx| list.push(toast, window, cx));
    }
}

/// The label `toast_error` gives `err`'s toast, so a caller can tell a repeat failure.
pub fn error_label(err: &OpError) -> SharedString {
    label(err.title(), err.description())
}

// "{title}. {description}", or the title alone when the description is empty. A title
// that already ends a sentence gets no second full stop.
fn label(title: &str, description: &str) -> SharedString {
    if description.is_empty() {
        SharedString::from(title.to_owned())
    } else if title.ends_with(['.', '!', '?']) {
        SharedString::from(format!("{title} {description}"))
    } else {
        SharedString::from(format!("{title}. {description}"))
    }
}

/// The error toast for a failed page action: its title and description.
pub fn toast_error(window: &mut Window, cx: &mut App, err: &OpError) {
    toast(
        window,
        cx,
        ToastKind::Error,
        err.title().to_owned(),
        err.description().to_owned(),
    );
}
