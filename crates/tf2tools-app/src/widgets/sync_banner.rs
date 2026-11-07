//! v5's muted sync banner for a GameBanana store: "Syncing {noun} from GameBanana...", the
//! "{fetched} / {total}" count and a bar, shown only while that store's sync runs.

use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, div};
use tf2tools_core::StoreKind;

use super::labelled;
use crate::ids;
use crate::sync::{SyncTick, sync_hub};

/// "Syncing {noun} from GameBanana..." plus " Not all items loaded yet." when the store has rows.
/// The noun is `sounds`, `crosshairs` or `HUDs`, and `items` for any other kind.
pub fn banner_text(kind: StoreKind, has_rows: bool) -> String {
    let noun = match kind {
        StoreKind::Sounds => "sounds",
        StoreKind::Crosshairs => "crosshairs",
        StoreKind::GameBananaHuds => "HUDs",
        StoreKind::CuratedHuds | StoreKind::Addons | _ => "items",
    };
    let mut text = format!("Syncing {noun} from GameBanana...");
    if has_rows {
        text.push_str(" Not all items loaded yet.");
    }
    text
}

/// The banner's right-hand count: "{fetched} / {total}".
pub fn banner_count(tick: SyncTick) -> String {
    format!("{} / {}", tick.fetched, tick.total)
}

/// The banner bar's fill in 0..=100; 0 while the total is still unknown.
pub fn banner_percent(tick: SyncTick) -> f32 {
    if tick.total == 0 {
        return 0.0;
    }
    (tick.fetched as f32 / tick.total as f32 * 100.0).min(100.0)
}

/// The banner while `kind` syncs; `None` without a hub or while that store is idle.
/// The page must observe the hub and its grid, or the banner only changes on unrelated redraws.
pub fn sync_banner(kind: StoreKind, has_rows: bool, cx: &App) -> Option<AnyElement> {
    let tick = sync_hub(cx)?.read(cx).progress(kind)?;
    let theme = cx.theme();
    let banner = v_flex()
        .gap_2()
        .p_4()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.3))
        .child(
            h_flex()
                .justify_between()
                .gap_4()
                .text_sm()
                .child(labelled(
                    div(),
                    ids::SYNC_BANNER,
                    banner_text(kind, has_rows),
                ))
                .child(labelled(
                    div().text_color(theme.muted_foreground),
                    ids::SYNC_BANNER_COUNT,
                    banner_count(tick),
                )),
        )
        .child(Progress::new("sync-banner-bar").value(banner_percent(tick)));
    Some(banner.into_any_element())
}
