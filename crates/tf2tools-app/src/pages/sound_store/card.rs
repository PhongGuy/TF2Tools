//! The Sound Store's card: v5's `SoundStoreCard` as a fixed-height text card.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, Window, div, px,
    transparent_black,
};
use tf2tools_core::{StoreCard, StoreItemId, StoreKind};

use super::body::sound_body;
use super::model::category_label;
use crate::audio::player::Player;
use crate::counts::count_stat;
use crate::ids;
use crate::pages::common::{badge_rows, card_frame, tag_badge};
use crate::store_detail::{DetailItem, open_detail};
use crate::store_download::download_button;
use crate::store_preview::play_preview;
use crate::widgets::labelled;

/// v5 shows at most this many tags beside the category badge.
const CARD_TAGS: usize = 3;
/// The badges wrap onto at most this many rows; a badge past them is hidden whole.
const BADGE_ROWS: usize = 2;
/// The muted header band above the card's text.
const HEADER_HEIGHT: f32 = 88.;

/// What the page knows about a card beyond its store row.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct CardState {
    /// The player holds this card's preview.
    pub playing: bool,
    /// The library holds this sound, as stored or downloaded this session.
    pub in_library: bool,
}

/// One sound card's content (everything inside the shared `store_card` cell).
pub(super) fn sound_card(
    card: &StoreCard,
    state: CardState,
    player: &WeakEntity<Player>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let play = play_button(card, state, player);
    let item = DetailItem {
        kind: StoreKind::Sounds,
        id: card.id.clone(),
        name: card.name.clone(),
        in_library: state.in_library,
    };
    let button = {
        let (item, player) = (item.clone(), player.clone());
        let label = if state.in_library {
            "In Library"
        } else {
            "Download"
        };
        download_button(
            StoreKind::Sounds,
            &card.id,
            label.into(),
            state.in_library,
            false,
            move |window, cx| {
                // Kit buttons let clicks bubble, and the card behind opens the dialog too.
                cx.stop_propagation();
                open_detail(item.clone(), sound_body(player.clone()), window, cx);
            },
            cx,
        )
    };
    let theme = cx.theme();
    let hover = theme.accent.opacity(0.5);
    // The muted header is the card's image area, so tests and the pilot find it by that id.
    let header = div()
        .id(ids::store_card_image(StoreKind::Sounds, &card.id))
        .test_support()
        .flex_none()
        .w_full()
        .h(px(HEADER_HEIGHT))
        .bg(theme.muted)
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::new(IconName::Volume2)
                .large()
                .text_color(theme.muted_foreground.opacity(0.3)),
        );
    let badge = tag_badge(
        ids::store_card_category(StoreKind::Sounds, &card.id),
        Tag::secondary(),
        transparent_black(),
        category_label(card.category.as_deref()),
    );
    // The grid already wraps this card in its `store_card` id, so the click target gets its own.
    let open_player = player.clone();
    card_frame(cx)
        .id(ids::SOUND_STORE_CARD_TARGET)
        .cursor_pointer()
        // v5's hover:bg-accent/50 over the card's group-box background.
        .hover(move |card| card.bg(hover))
        .on_click(move |_, window, cx| {
            open_detail(item.clone(), sound_body(open_player.clone()), window, cx);
        })
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        // v5's card py-6: a 24 px band above the header, then the body's own p-6 below it.
        .pt_6()
        .child(header)
        .child(
            v_flex()
                .flex_1()
                .min_h_0()
                .p_6()
                .gap_6()
                // v5's row: the name-and-author column takes the width, the play button sits
                // right with its top on the name's (v5's items-start; the kit's h_flex centres).
                // Below it, v5's CardHeader pb-2 and the grid's 6 px row gap.
                .child(
                    h_flex()
                        .items_start()
                        .gap_2()
                        .pb(px(14.))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    h_flex()
                                        .min_w_0()
                                        .gap_1()
                                        .child(labelled(
                                            div()
                                                .truncate()
                                                .line_height(px(24.))
                                                .font_weight(FontWeight::SEMIBOLD),
                                            ids::store_card_name(StoreKind::Sounds, &card.id),
                                            card.name.clone(),
                                        ))
                                        .children(state.in_library.then(|| {
                                            Icon::new(IconName::CircleCheck)
                                                .small()
                                                .text_color(theme.primary)
                                        })),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_sm()
                                        .line_height(px(20.))
                                        .text_color(theme.muted_foreground)
                                        .child(format!("by {}", card.author)),
                                ),
                        )
                        .children(play),
                )
                // v5's CardContent: the category badge and tags wrapped gap-1.5 apart, then mt-2
                // the text-xs stats, over pb-2.
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .pb_2()
                        .child(
                            badge_rows(
                                ids::store_card_tags(StoreKind::Sounds, &card.id),
                                BADGE_ROWS,
                                px(6.),
                            )
                            .child(badge)
                            .children(
                                card.tags.iter().take(CARD_TAGS).enumerate().map(|(ix, t)| {
                                    tag_badge(
                                        ids::store_card_tag(StoreKind::Sounds, &card.id, ix),
                                        Tag::secondary().outline().text_color(theme.foreground),
                                        theme.border,
                                        t.clone(),
                                    )
                                }),
                            ),
                        )
                        .child(
                            h_flex()
                                .flex_none()
                                .mt_2()
                                .gap_3()
                                .text_xs()
                                .line_height(px(16.))
                                .text_color(theme.muted_foreground)
                                .child(count_stat(
                                    ids::store_likes(StoreKind::Sounds, &card.id),
                                    IconName::ThumbsUp,
                                    card.like_count,
                                ))
                                .child(count_stat(
                                    ids::store_views(StoreKind::Sounds, &card.id),
                                    IconName::Eye,
                                    card.view_count,
                                )),
                        ),
                )
                // v5's footer: the button alone at the card's bottom, full width.
                .child(div().mt_auto().w_full().child(button)),
        )
        .into_any_element()
}

// v5 shows the play button only for a row with a preview; it toggles the page's player.
fn play_button(card: &StoreCard, state: CardState, player: &WeakEntity<Player>) -> Option<Button> {
    let StoreItemId::GameBanana(id) = card.id else {
        return None;
    };
    if !card.has_preview {
        return None;
    }
    let (player, name, author) = (player.clone(), card.name.clone(), card.author.clone());
    // The icon-only button names its action, so a screen reader and a test hear Play or Stop.
    let (icon, action) = if state.playing {
        (IconName::Square, "Stop")
    } else {
        (IconName::Play, "Play")
    };
    let button = Button::new(ids::store_play(StoreKind::Sounds, &card.id))
        .ghost()
        .flex_none()
        .icon(icon)
        .size(px(36.))
        .tooltip(action)
        .accessibility_label(action)
        .on_click(move |_, _, cx| {
            // The card itself opens the detail, so the click stops here.
            cx.stop_propagation();
            if let Some(player) = player.upgrade() {
                play_preview(&player, id, &name, &author, cx);
            }
        });
    Some(button)
}
