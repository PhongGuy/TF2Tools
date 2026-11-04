//! Store sound previews: the row is read off the UI thread, the preview fetched on the tokio
//! runtime, and the bytes played through the floating `Player`.

use gpui_kit::{App, Entity, SharedString, Task};
use tf2tools_core::{GbId, StoreItemId, StoreKind};

use crate::audio::player::{NowPlaying, Player};
use crate::services::{OpError, Services, run_blocking};
use crate::store_messages::{StoreFailure, describe_preview, preview_gone, preview_unavailable};
use crate::tokio_bridge;

const FAILED: &str = "Could not play preview";
const KEY_PREFIX: &str = "store-sound:";

/// The player key of a store sound's preview: "store-sound:{id}".
pub fn preview_key(id: GbId) -> SharedString {
    format!("{KEY_PREFIX}{}", id.get()).into()
}

/// The GameBanana id in a `preview_key`, or None for any other key.
pub fn parse_preview_key(key: &str) -> Option<GbId> {
    key.strip_prefix(KEY_PREFIX)?.parse().ok().map(GbId::new)
}

/// Reads the store row with `run_blocking`, then fetches `preview_audio` through `tokio_bridge::spawn`.
/// Every failure (row gone, no preview, untrusted URL, HTTP error, join error) is
/// `OpError::core("Could not play preview", ..)` with a plain message; the raw error is logged.
pub fn load_preview(id: GbId, cx: &App) -> Task<Result<Vec<u8>, OpError>> {
    let read = run_blocking(cx, move |s| {
        s.store
            .store_item(StoreKind::Sounds, &StoreItemId::GameBanana(id))
            .map_err(|e| failed(preview_unavailable(e.to_string())))?
            .ok_or_else(|| failed(preview_gone(id)))
    });
    let store = cx.global::<Services>().store.clone();
    cx.spawn(async move |cx| {
        let item = read.await?;
        // The core read stays on gpui's pool; only the HTTP fetch needs the tokio runtime.
        let fetch = cx
            .update(|cx| tokio_bridge::spawn(cx, async move { store.preview_audio(&item).await }));
        match fetch.await {
            Ok(bytes) => bytes.map_err(|e| failed(describe_preview(&e))),
            Err(join) => Err(failed(preview_unavailable(join.to_string()))),
        }
    })
}

// The player reads only the plain message; the raw error goes to the developer log.
fn failed(failure: StoreFailure) -> OpError {
    failure.log("store preview");
    OpError::core(FAILED, failure.message)
}

/// Toggles `player` on this sound's preview, showing `name` and "by {author}"; detaches the play.
pub fn play_preview(player: &Entity<Player>, id: GbId, name: &str, author: &str, cx: &mut App) {
    let now = NowPlaying {
        key: preview_key(id),
        name: name.to_owned().into(),
        subtitle: format!("by {author}").into(),
    };
    player.update(cx, |player, cx| {
        player
            .toggle_with(now, move |cx| load_preview(id, cx), cx)
            .detach()
    });
}
