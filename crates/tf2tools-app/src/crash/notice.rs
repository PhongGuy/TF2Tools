//! The banner under the header on the start after a crash, offering the report to copy.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, Entity, Global,
    InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, TestSupportExt as _,
    div, px,
};

use crate::ids;
use crate::shell::toast::{ToastKind, toast};
use crate::updater::view::BANNER_HEIGHT;
use crate::widgets::labelled;

pub const NOTICE: &str = "TF2Tools closed unexpectedly last time.";

/// The last run's crash report, held until the player dismisses the banner.
pub struct CrashNotice {
    report: Option<String>,
}

struct CrashNoticeGlobal(Entity<CrashNotice>);
impl Global for CrashNoticeGlobal {}

impl CrashNotice {
    /// Creates the notice over `report` and makes it the app's global.
    pub fn install(cx: &mut App, report: Option<String>) -> Entity<CrashNotice> {
        let notice = cx.new(|_| CrashNotice { report });
        cx.set_global(CrashNoticeGlobal(notice.clone()));
        notice
    }

    pub fn report(&self) -> Option<&str> {
        self.report.as_deref()
    }

    /// Hides the banner for this run; notifies.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.report = None;
        cx.notify();
    }
}

pub fn crash_notice(cx: &App) -> Option<Entity<CrashNotice>> {
    cx.try_global::<CrashNoticeGlobal>().map(|g| g.0.clone())
}

/// The banner while a report is held, else None.
pub fn banner(cx: &App) -> Option<AnyElement> {
    let notice = crash_notice(cx)?;
    let report = notice.read(cx).report()?.to_owned();
    let theme = cx.theme();
    let copy = Button::new(ids::CRASH_BANNER_COPY)
        .outline()
        .small()
        .label("Copy details")
        .flex_none()
        .on_click(move |_, window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(report.clone()));
            toast(
                window,
                cx,
                ToastKind::Success,
                "Crash details copied",
                "Paste them into your bug report.",
            );
        });
    let dismiss = Button::new(ids::CRASH_BANNER_DISMISS)
        .ghost()
        .icon(IconName::X)
        .small()
        .flex_none()
        .on_click(move |_, _, cx| notice.update(cx, |n, cx| n.dismiss(cx)));
    // The update banner's row, so the two stack as one style under the header.
    Some(
        h_flex()
            .id(ids::CRASH_BANNER)
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
                Icon::new(IconName::TriangleAlert)
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
                ids::CRASH_BANNER_TEXT,
                NOTICE,
            ))
            .child(copy)
            .child(dismiss)
            .into_any_element(),
    )
}
