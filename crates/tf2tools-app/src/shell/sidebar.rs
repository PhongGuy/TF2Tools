//! v5's AppSidebar: the "TF2Tools" title, the tools, stores and makers groups and the footer,
//! split by separators and collapsing to 32 px icon buttons with tooltips.

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::separator::Separator;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, Size, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};

use super::route::Page;
use super::{SIDEBAR_COLLAPSED_WIDTH, SIDEBAR_WIDTH, Shell};
use crate::ids;

const TOOLS: [Page; 6] = [
    Page::Huds,
    Page::Sounds,
    Page::Crosshairs,
    Page::Profiles,
    Page::Addons,
    Page::Graphics,
];
const STORES: [Page; 4] = [
    Page::HudStore,
    Page::SoundStore,
    Page::CrosshairStore,
    Page::AddonStore,
];
const MAKERS: [Page; 2] = [Page::CrosshairMaker, Page::HudMaker];
// Maps gets its own group under the makers: it is the least-used item.
const MAPS: [Page; 1] = [Page::Maps];
const FOOTER: [Page; 2] = [Page::Settings, Page::Logs];
// v5's nav item height, and the shorter one a short window uses so every item fits.
const NAV_HEIGHT: f32 = 32.;
const NAV_TIGHT_HEIGHT: f32 = 26.;
// The window height below which v5's spacing pushes the open rail's last group under its
// footer; shorter windows tighten the spacing and the items so every item shows whole.
const ROOMY_HEIGHT: Pixels = px(660.);

/// Element `sidebar`, `SIDEBAR_WIDTH` wide, or `SIDEBAR_COLLAPSED_WIDTH` when `collapsed`, in a
/// window `window_height` tall.
pub(super) fn render(
    active: Page,
    collapsed: bool,
    window_height: Pixels,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let (bg, border, fg, title) = (
        theme.sidebar,
        theme.sidebar_border,
        theme.sidebar_foreground,
        theme.primary,
    );
    // The shut rail's title row takes no height, so only the open one needs the tighter spacing.
    let tight = !collapsed && window_height < ROOMY_HEIGHT;
    let group = |pages: &[Page], cx: &mut Context<Shell>| {
        v_flex().gap_1().children(
            pages
                .iter()
                .map(|&p| nav_item(p, p == active, collapsed, tight, cx))
                .collect::<Vec<_>>(),
        )
    };
    // v5's SidebarSeparator spans its container: the full rail between groups, inset by the
    // footer's padding at the bottom.
    let separator = || Separator::horizontal().color(border);
    let padded = |group| {
        div()
            .px_2()
            .when(tight, |d| d.py_1())
            .when(!tight, |d| d.py_2())
            .child(group)
    };
    let (tools, stores, makers, maps, footer) = (
        group(&TOOLS, cx),
        group(&STORES, cx),
        group(&MAKERS, cx),
        group(&MAPS, cx),
        group(&FOOTER, cx),
    );
    v_flex()
        .id(ids::SIDEBAR)
        .test_support()
        .flex_none()
        .h_full()
        .w(if collapsed {
            SIDEBAR_COLLAPSED_WIDTH
        } else {
            SIDEBAR_WIDTH
        })
        .overflow_hidden()
        .bg(bg)
        .border_r_1()
        .border_color(border)
        .text_color(fg)
        // As in v5, the title row's text height goes away when collapsed, lifting the icons.
        .child(
            div()
                .p_2()
                .child(div().my_3().px_2().when(!collapsed, |row| {
                    row.h(px(28.)).child(
                        div()
                            .id(ids::SIDEBAR_TITLE)
                            .aria_label("TF2Tools")
                            .test_support()
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .text_color(title)
                            .child("TF2Tools"),
                    )
                })),
        )
        // As v5's SidebarContent, only the groups scroll when the window is short, so the title
        // and the Settings/Logs footer stay put and the footer's divider never crosses an item.
        // The scrollable sits inside the id'd rail because it re-ids the element it wraps; its
        // thin bar shows only while the groups scroll.
        .child(
            div()
                .id(ids::SIDEBAR_GROUPS)
                .test_support()
                .flex_1()
                .min_h_0()
                .child(
                    v_flex()
                        .when(tight, |groups| groups.gap_1())
                        .when(!tight, |groups| groups.gap_2())
                        .child(padded(tools))
                        .child(separator())
                        .child(padded(stores))
                        .child(separator())
                        .child(padded(makers))
                        .child(separator())
                        .child(padded(maps))
                        .overflow_y_scrollbar(),
                ),
        )
        .child(v_flex().p_2().gap_2().child(separator()).child(footer))
}

// One nav button: icon and label, or the icon alone with a tooltip when collapsed. The
// accessible name is the label either way, so the pilot finds it in both states.
fn nav_item(
    page: Page,
    active: bool,
    collapsed: bool,
    tight: bool,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let (accent, accent_fg) = (theme.sidebar_accent, theme.sidebar_accent_foreground);
    let label = page.label();
    div()
        .id(ids::nav(page))
        .accessibility_id(ids::nav(page).to_string())
        .role(Role::Button)
        .aria_label(label)
        .test_support()
        .flex()
        .items_center()
        .gap_2()
        .h(px(if tight { NAV_TIGHT_HEIGHT } else { NAV_HEIGHT }))
        .rounded(px(8.))
        .text_sm()
        .cursor_pointer()
        .when(collapsed, |item| item.w(px(32.)).justify_center())
        .when(!collapsed, |item| item.w_full().px_2())
        .when(active, |item| {
            item.bg(accent)
                .text_color(accent_fg)
                .font_weight(FontWeight::MEDIUM)
        })
        .hover(move |item| item.bg(accent).text_color(accent_fg))
        .child(Icon::new(icon(page)).with_size(Size::Size(px(16.))))
        .when(!collapsed, |item| item.child(label))
        .when(collapsed, |item| {
            item.tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
        })
        .on_click(cx.listener(move |shell, _, window, cx| shell.request_navigate(page, window, cx)))
}

/// The page's sidebar icon: one lucide icon per kind of thing, shared by its library, store
/// and maker pages.
pub fn icon(page: Page) -> IconName {
    match page {
        Page::Huds | Page::HudStore | Page::HudMaker => IconName::Paintbrush,
        Page::Sounds | Page::SoundStore => IconName::Volume2,
        Page::Crosshairs | Page::CrosshairStore | Page::CrosshairMaker => IconName::Crosshair,
        Page::Profiles => IconName::Layers,
        Page::Addons | Page::AddonStore => IconName::Puzzle,
        Page::Settings => IconName::Settings,
        Page::Logs => IconName::ScrollText,
        Page::Maps => IconName::Map,
        Page::Graphics => IconName::Gauge,
    }
}
