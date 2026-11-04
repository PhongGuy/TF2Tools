//! The Sound Store's detail body: v5's sound detail under the shared file list.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{App, IntoElement, ParentElement as _, Styled as _, WeakEntity, transparent_black};
use tf2tools_core::{StoreItem, StoreItemId};

use super::model::category_label;
use crate::audio::player::Player;
use crate::counts::count_stat;
use crate::ids;
use crate::pages::common::badge_of;
use crate::store_detail::DetailBody;
use crate::store_preview::{play_preview, preview_key};

/// v5's sound detail: category badge, likes and views, a "Play Preview" / "Stop Preview" button
/// (`ids::STORE_PREVIEW`, only with a preview URL) toggling `player`, and the tags.
pub fn sound_body(player: WeakEntity<Player>) -> DetailBody {
    DetailBody::new(move |item: &StoreItem, _, cx: &mut App| {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        // An empty category reads as "Sound", as on the card, never as an empty badge.
        let summary = h_flex()
            .gap_3()
            .items_center()
            .text_sm()
            .text_color(muted)
            .child(badge_of(
                ids::STORE_DETAIL_CATEGORY,
                Tag::secondary(),
                transparent_black(),
                category_label(item.category()),
            ))
            .child(count_stat(
                ids::STORE_DETAIL_LIKES,
                IconName::ThumbsUp,
                item.like_count(),
            ))
            .child(count_stat(
                ids::STORE_DETAIL_VIEWS,
                IconName::Eye,
                item.view_count(),
            ));
        v_flex()
            .gap_3()
            .child(summary)
            // The column stretches its children; the row keeps the button as wide as its label.
            .children(preview_button(&player, item, cx).map(|b| h_flex().child(b)))
            .child(
                h_flex()
                    .gap_1p5()
                    .flex_wrap()
                    .children(item.tags().iter().enumerate().map(|(ix, t)| {
                        badge_of(
                            ids::store_detail_tag("tags", ix),
                            Tag::secondary().outline().text_color(theme.foreground),
                            theme.border,
                            t.clone(),
                        )
                    })),
            )
            .into_any_element()
    })
}

// Toggles the page's player on this sound's preview; it reads "Stop" while that preview plays.
fn preview_button(player: &WeakEntity<Player>, item: &StoreItem, cx: &App) -> Option<Button> {
    let StoreItemId::GameBanana(id) = *item.id() else {
        return None;
    };
    item.preview_url().filter(|url| !url.is_empty())?;
    let playing = player.upgrade().is_some_and(|p| {
        p.read(cx)
            .now_playing()
            .is_some_and(|now| now.key == preview_key(id))
    });
    let (player, name, author) = (
        player.clone(),
        item.name().to_owned(),
        item.author().to_owned(),
    );
    let button = Button::new(ids::STORE_PREVIEW)
        .outline()
        .icon(if playing {
            IconName::Square
        } else {
            IconName::Play
        })
        .label(if playing {
            "Stop Preview"
        } else {
            "Play Preview"
        })
        .on_click(move |_, _, cx| {
            if let Some(player) = player.upgrade() {
                play_preview(&player, id, &name, &author, cx);
            }
        });
    Some(button)
}
