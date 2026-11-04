//! The GameBanana store card the HUD and crosshair stores share.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    TestSupportExt as _, Window, div, img, px,
};
use tf2tools_core::{StoreCard, StoreKind};

use crate::counts::count_stat;
use crate::ids;
use crate::pages::common::{badge_rows, card_frame, tag_badge};
use crate::remote_images::RemoteImage;
use crate::store_download::download_button;
use crate::widgets::labelled;

/// v5 shows at most this many tags on a GameBanana card.
const CARD_TAGS: usize = 3;

/// The tag rows each store's fixed card height holds, matching v5's first screen at the pilot's
/// 272 px column: GameBanana HUDs 470 px (three rows), Crosshairs 418 px (one row).
fn tag_rows(kind: StoreKind) -> usize {
    match kind {
        StoreKind::Crosshairs => 1,
        _ => 3,
    }
}

/// What a click on the card or its button does.
pub type OpenCard = Rc<dyn Fn(&mut Window, &mut App)>;

/// A GameBanana store card as v5's HUD and crosshair stores draw it: the 16:9 thumbnail (else a
/// muted area with `placeholder`), the name with a check when `in_library`, "by {author}", up to
/// three tags, likes and views, and `download_button(kind, id, "In Library" | "Download", ..)`.
/// The button and a click on the card call `open`.
pub fn gamebanana_card(
    kind: StoreKind,
    card: &StoreCard,
    image: Option<RemoteImage>,
    in_library: bool,
    placeholder: IconName,
    open: OpenCard,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let hover = theme.accent.opacity(0.5);
    let shot = div()
        .id(ids::store_card_image(kind, &card.id))
        .test_support()
        .flex_none()
        .w_full()
        .aspect_ratio(16. / 9.)
        .overflow_hidden()
        .bg(theme.muted)
        .flex()
        .items_center()
        .justify_center();
    let shot = match image {
        Some(RemoteImage::Ready(image)) => {
            shot.child(img(image).size_full().object_fit(ObjectFit::Cover))
        }
        _ => shot.child(
            Icon::new(placeholder)
                .large()
                .text_color(theme.muted_foreground),
        ),
    };
    let label = if in_library { "In Library" } else { "Download" };
    let button = {
        let open = open.clone();
        download_button(
            kind,
            &card.id,
            label.into(),
            in_library,
            false,
            move |window, cx| {
                // Kit buttons let clicks bubble, and the card behind opens the dialog too.
                cx.stop_propagation();
                open(window, cx);
            },
            cx,
        )
    };
    // The grid already wraps this card in its `store_card` id, so the click target gets its own.
    card_frame(cx)
        .id(ids::GAMEBANANA_CARD)
        .cursor_pointer()
        // v5's hover:bg-accent/50 over the card's group-box background.
        .hover(move |card| card.bg(hover))
        .on_click(move |_, window, cx| open(window, cx))
        .size_full()
        .flex()
        .flex_col()
        // v5's Card is py-6, so the image sits 24 px below the card's top edge.
        .pt_6()
        .overflow_hidden()
        .child(shot)
        .child(
            v_flex()
                .flex_1()
                .min_h_0()
                // v5's px-6 body under the Card's gap-6 and over its py-6, its sections gap-6 apart.
                .p_6()
                .gap_6()
                // v5's CardHeader: 24 px name, 20 px author, its 6 px row gap and pb-2 below.
                .child(
                    v_flex()
                        .min_w_0()
                        .pb(px(14.))
                        .child(
                            h_flex()
                                .gap_1()
                                .child(labelled(
                                    div()
                                        .truncate()
                                        .line_height(px(24.))
                                        .font_weight(FontWeight::SEMIBOLD),
                                    ids::store_card_name(kind, &card.id),
                                    card.name.clone(),
                                ))
                                .children(in_library.then(|| {
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
                // v5's CardContent: the wrapped tags (left out when there are none), then mt-2
                // the text-xs stats.
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .pb_2()
                        .when(!card.tags.is_empty(), |content| {
                            content.child(
                                badge_rows(
                                    ids::store_card_tags(kind, &card.id),
                                    tag_rows(kind),
                                    px(4.),
                                )
                                .children(
                                    card.tags.iter().take(CARD_TAGS).enumerate().map(|(ix, t)| {
                                        tag_badge(
                                            ids::store_card_tag(kind, &card.id, ix),
                                            Tag::secondary().outline().text_color(theme.foreground),
                                            theme.border,
                                            t.clone(),
                                        )
                                    }),
                                ),
                            )
                        })
                        .child(
                            h_flex()
                                .flex_none()
                                .mt_2()
                                .gap_3()
                                .text_xs()
                                .line_height(px(16.))
                                .text_color(theme.muted_foreground)
                                .child(count_stat(
                                    ids::store_likes(kind, &card.id),
                                    IconName::ThumbsUp,
                                    card.like_count,
                                ))
                                .child(count_stat(
                                    ids::store_views(kind, &card.id),
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
