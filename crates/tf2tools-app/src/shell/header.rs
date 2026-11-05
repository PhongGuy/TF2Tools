//! v5's Header: the sidebar toggle, the page title and a right-aligned status slot.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    App, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
    transparent_black,
};

use tf2tools_core::StoreKind;

use super::route::Page;
use super::{HEADER_HEIGHT, Shell};
use crate::ids;
use crate::services::{Tf2State, tf2_state};
use crate::sync::{store_name, store_slug, sync_hub};
use crate::widgets::Named as _;
use crate::widgets::sync_ring::{RingColors, SyncRing};

/// Element `header`, `HEADER_HEIGHT` tall, titled for `page`.
pub(super) fn render(page: Page, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let border = cx.theme().border;
    let title = page.title();
    // No wrapper while nothing syncs, so an empty row adds no gap before the badge.
    let rings = sync_rings(cx);
    let rings = (!rings.is_empty()).then(|| h_flex().gap_1p5().children(rings));
    h_flex()
        .id(ids::HEADER)
        .test_support()
        .flex_none()
        .h(HEADER_HEIGHT)
        .px(px(24.))
        .gap_4()
        .border_b_1()
        .border_color(border)
        .child(
            Button::new(ids::SIDEBAR_TOGGLE)
                .ghost()
                .icon(IconName::PanelLeft)
                // v5's `size-7` trigger around a 16 px icon: medium icon, 28 px box.
                .size(px(28.))
                .named("Toggle Sidebar")
                .on_click(cx.listener(|shell, _, window, cx| shell.toggle_sidebar(window, cx))),
        )
        .child(
            div()
                .id(ids::HEADER_TITLE)
                .aria_label(title)
                .test_support()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        // v5's status slot: the rings in their own `gap-1.5` row, then the TF2 badge.
        .child(
            h_flex()
                .ml_auto()
                .gap_2()
                .children(rings)
                .children(tf2_badge(cx)),
        )
}

// v5's rendered "TF2 Connected" width; GPUI's Inter shapes it 4 px narrower than WebKit.
const TF2_BADGE_MIN_WIDTH: Pixels = px(111.);

// v5's badge: "TF2 Connected" or "TF2 Not Found"; none while detection runs, so it never flickers.
fn tf2_badge(cx: &App) -> Option<impl IntoElement + use<>> {
    let (tag, text) = match tf2_state(cx).read(cx) {
        Tf2State::Detecting => return None,
        Tf2State::Found(_) => (
            Tag::secondary().min_w(TF2_BADGE_MIN_WIDTH).justify_center(),
            "TF2 Connected",
        ),
        Tf2State::Missing { .. } => (Tag::danger(), "TF2 Not Found"),
    };
    Some(
        div()
            .id(ids::TF2_BADGE)
            .role(Role::Status)
            .aria_label(text)
            .test_support()
            // shadcn's Badge: a borderless pill, `px-2 py-0.5 text-xs font-medium`.
            .child(
                tag.rounded_full()
                    .border_color(transparent_black())
                    .px_2()
                    .py_0p5()
                    .font_weight(FontWeight::MEDIUM)
                    .child(text),
            ),
    )
}

// v5's SyncIndicator, in its S, C, H, M order: a 24 px arc with the store's letter centred,
// spinning for Curated HUDs, which has no total. Addons sync without a ring.
fn sync_rings(cx: &App) -> Vec<impl IntoElement + use<>> {
    const RINGS: [(StoreKind, &str); 4] = [
        (StoreKind::Sounds, "S"),
        (StoreKind::Crosshairs, "C"),
        (StoreKind::GameBananaHuds, "H"),
        (StoreKind::CuratedHuds, "M"),
    ];
    let Some(hub) = sync_hub(cx) else {
        return Vec::new();
    };
    let hub = hub.read(cx);
    let (colors, muted) = (RingColors::of(cx.theme()), cx.theme().muted_foreground);
    RINGS
        .into_iter()
        .filter_map(|(kind, letter)| {
            let tick = hub.progress(kind)?;
            let curated = kind == StoreKind::CuratedHuds;
            let label = if curated {
                "Syncing Curated HUDs...".to_owned()
            } else {
                format!(
                    "Syncing {}: {} / {}",
                    store_name(kind),
                    tick.fetched,
                    tick.total
                )
            };
            let value = if tick.total == 0 {
                0.
            } else {
                tick.fetched as f32 / tick.total as f32 * 100.
            };
            let tip = label.clone();
            Some(
                div()
                    .id(ids::sync_ring(kind))
                    .aria_label(label)
                    .test_support()
                    .flex_none()
                    .size(px(24.))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(
                        SyncRing::new(ids::keyed("sync-ring-arc", store_slug(kind)), colors)
                            .value(value)
                            .spinning(curated)
                            .child(
                                div()
                                    .text_size(px(7.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(muted)
                                    .child(letter),
                            ),
                    ),
            )
        })
        .collect()
}
