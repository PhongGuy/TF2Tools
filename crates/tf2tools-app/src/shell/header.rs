//! v5's Header: the sidebar toggle, the page title and a right-aligned status slot, drawn as
//! the window's title bar with the window controls at its right.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, TitleBar, h_flex,
};
use gpui_kit::{
    App, Context, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, Role, Stateful, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div, px, transparent_black,
};

use tf2tools_core::StoreKind;

use super::leave::should_close;
use super::route::Page;
use super::sidebar::icon as nav_icon;
use super::{HEADER_HEIGHT, Shell};
use crate::ids;
use crate::logs_sync::{SyncProgress, try_logs_sync};
use crate::pages::logs_tf::text::ring_label;
use crate::services::{Tf2State, tf2_state};
use crate::sync::{store_name, store_slug, sync_hub};
use crate::widgets::Named as _;
use crate::widgets::sync_ring::{RingColors, SyncRing};

/// Steam's link that starts TF2 (app 440).
pub const PLAY_TF2_URL: &str = "steam://run/440";

/// Element `header`, `HEADER_HEIGHT` tall, titled for `page`, inside the kit's title bar.
/// `tf2_running` is the shell's remembered answer, so drawing never scans processes.
pub(super) fn render(
    page: Page,
    tf2_running: bool,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let (border, bg) = (cx.theme().border, cx.theme().background);
    // The kit bar moves the window from empty space, maximises on double-click and puts the
    // window controls at the right, restyled flat to keep v5's look. The `header` id wraps the
    // whole bar so it still measures v5's 56 px, border included.
    div().id(ids::HEADER).test_support().flex_none().child(
        TitleBar::new()
            .h(HEADER_HEIGHT)
            .pl_0()
            .bg(bg)
            .border_b_1()
            .border_color(border)
            // Linux's close button otherwise removes the window past the unsaved-work guard.
            .on_close_window(|_, window, cx| {
                if should_close(window, cx) {
                    window.remove_window();
                }
            })
            .child(content(page, tf2_running, cx)),
    )
}

// v5's header row: the sidebar toggle, the page title and the status slot.
fn content(page: Page, tf2_running: bool, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let title = page.title();
    // No wrapper while nothing syncs, so an empty row adds no gap before the badge.
    let mut rings: Vec<_> = sync_rings(cx)
        .into_iter()
        .map(|r| r.into_any_element())
        .collect();
    rings.extend(logs_ring(cx).map(|r| r.into_any_element()));
    let rings = (!rings.is_empty()).then(|| h_flex().gap_1p5().children(rings));
    h_flex()
        .id(ids::HEADER_ROW)
        .test_support()
        .flex_1()
        .h_full()
        .px(px(24.))
        .gap_4()
        // The kit button leaves its press unhandled, so the bar would start a window move from it.
        .child(
            div()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    Button::new(ids::SIDEBAR_TOGGLE)
                        .ghost()
                        .icon(IconName::PanelLeft)
                        // v5's `size-7` trigger around a 16 px icon: medium icon, 28 px box.
                        .size(px(28.))
                        .named("Toggle Sidebar")
                        .on_click(
                            cx.listener(|shell, _, window, cx| shell.toggle_sidebar(window, cx)),
                        ),
                ),
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
        .children((page == Page::HudMaker).then(|| shortcuts_button(cx)))
        // v5's status slot: the rings in their own `gap-1.5` row, then the TF2 badge.
        .child(
            h_flex()
                .ml_auto()
                .gap_2()
                .children(rings)
                .children(play_button(tf2_running, cx))
                .children(tf2_badge(cx)),
        )
}

// The HUD Maker's "?" beside its title, which opens the keyboard shortcuts sheet. It sits up
// here because the maker's own toolbar has no room left at small window sizes.
fn shortcuts_button(cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    div()
        .flex_none()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            Button::new(ids::HUD_MAKER_SHORTCUTS)
                .ghost()
                .small()
                .icon(IconName::CircleQuestionMark)
                .tooltip("Keyboard shortcuts (?)")
                .accessibility_label("Keyboard shortcuts")
                .on_click(cx.listener(|shell, _, window, cx| {
                    if let Some(page) = shell.hud_maker_page() {
                        page.update(cx, |page, cx| page.show_shortcuts(window, cx));
                    }
                })),
        )
}

// Starts TF2 through Steam; only while TF2 is found, and waiting while it already runs.
fn play_button(tf2_running: bool, cx: &App) -> Option<impl IntoElement + use<>> {
    tf2_state(cx).read(cx).install()?;
    let button = Button::new(ids::PLAY_TF2)
        .primary()
        .small()
        .icon(IconName::Play)
        .label("Play TF2")
        .accessibility_label("Play TF2")
        .disabled(tf2_running)
        .on_click(|_, _, cx| cx.open_url(PLAY_TF2_URL));
    let button = if tf2_running {
        button.tooltip("TF2 is already running")
    } else {
        button
    };
    // Like the sidebar toggle, a press on the button must not start a window move.
    Some(
        div()
            .flex_none()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(button),
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

/// The icon a store's sync ring shows: the sidebar icon of its store page, so Curated HUDs
/// shares the HUD Store's.
pub fn ring_icon(kind: StoreKind) -> IconName {
    nav_icon(match kind {
        StoreKind::Sounds => Page::SoundStore,
        StoreKind::Crosshairs => Page::CrosshairStore,
        StoreKind::Addons => Page::AddonStore,
        StoreKind::CuratedHuds | StoreKind::GameBananaHuds | _ => Page::HudStore,
    })
}

// Fits inside the ring's 3 px stroke with a little air around it.
const RING_ICON: Pixels = px(12.);

// v5's SyncIndicator, in its S, C, H, M order: a 24 px arc with the store's icon centred where
// v5 had its letter, spinning for Curated HUDs, which has no total. Addons sync without a ring.
fn sync_rings(cx: &App) -> Vec<impl IntoElement + use<>> {
    const RINGS: [StoreKind; 4] = [
        StoreKind::Sounds,
        StoreKind::Crosshairs,
        StoreKind::GameBananaHuds,
        StoreKind::CuratedHuds,
    ];
    let Some(hub) = sync_hub(cx) else {
        return Vec::new();
    };
    let hub = hub.read(cx);
    RINGS
        .into_iter()
        .filter_map(|kind| {
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
            Some(
                ring(
                    Ring {
                        id: ids::sync_ring(kind),
                        slug: store_slug(kind),
                        label,
                        value,
                        spinning: curated,
                        icon: ring_icon(kind),
                    },
                    cx,
                )
                .test_support(),
            )
        })
        .collect()
}

// The logs.tf sync's ring after the store rings: counts while logs download, spinning while
// the list is read. Unlike the store rings it opens its page, through the shell's leave guard.
fn logs_ring(cx: &mut Context<Shell>) -> Option<impl IntoElement + use<>> {
    let sync = try_logs_sync(cx)?;
    let sync = sync.read(cx);
    sync.running()?;
    let progress = sync.progress();
    let value = match progress {
        Some(SyncProgress::Downloading { done, total, .. }) if total > 0 => {
            done as f32 / total as f32 * 100.
        }
        _ => 0.,
    };
    let downloading = matches!(progress, Some(SyncProgress::Downloading { .. }));
    let ring = ring(
        Ring {
            id: ids::logstf_sync_ring(),
            slug: "logs-tf",
            label: ring_label(progress),
            value,
            spinning: !downloading,
            icon: nav_icon(Page::LogsTf),
        },
        cx,
    );
    // Like the sidebar toggle, a press on the ring must not start a window move.
    Some(
        ring.cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.request_navigate(Page::LogsTf, window, cx);
            }))
            .test_support(),
    )
}

// What one header ring shows; `slug` keys its arc and icon ids.
struct Ring {
    id: ElementId,
    slug: &'static str,
    label: String,
    value: f32,
    spinning: bool,
    icon: IconName,
}

// v5's 24 px ring with its tooltip and the page's icon centred in the arc. Callers add
// `test_support` last, since the observed wrapper takes no more listeners.
fn ring(ring: Ring, cx: &App) -> Stateful<Div> {
    let (colors, muted) = (RingColors::of(cx.theme()), cx.theme().muted_foreground);
    let tip = ring.label.clone();
    div()
        .id(ring.id)
        .aria_label(ring.label)
        .flex_none()
        .size(px(24.))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .child(
            SyncRing::new(ids::keyed("sync-ring-arc", ring.slug), colors)
                .value(ring.value)
                .spinning(ring.spinning)
                .child(
                    div()
                        .id(ids::keyed("sync-ring-icon", ring.slug))
                        .test_support()
                        .flex_none()
                        .child(
                            Icon::new(ring.icon)
                                .with_size(Size::Size(RING_ICON))
                                .text_color(muted),
                        ),
                ),
        )
}
