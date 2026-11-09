//! The card a GameBanana add-on tab draws for one item.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    TestSupportExt as _, Window, div, img, px, transparent_black,
};
use tf2tools_core::AddonCard;

use crate::counts::count_stat;
use crate::ids;
use crate::pages::common::{badge_of, card_frame};
use crate::remote_images::RemoteImage;
use crate::widgets::labelled;

/// The card's text area under its 16:9 thumbnail: p-6 around the name, author and stats rows.
pub const CARD_TEXT_HEIGHT: Pixels = px(116.);

/// What a click on the card does.
pub type OpenAddon = Rc<dyn Fn(&mut Window, &mut App)>;

/// A GameBanana add-on card in the `gamebanana_card` layout: the 16:9 thumbnail (else a muted
/// area), the name with an "Outdated" tag when obsolete, "by {author}" and the like count.
/// A click anywhere on it calls `open`. With `reveal`, a "Mature" cover lies over the thumbnail
/// and a click on it calls `reveal` instead.
pub fn gb_addon_card(
    section: &str,
    card: &AddonCard,
    image: Option<RemoteImage>,
    open: OpenAddon,
    reveal: Option<OpenAddon>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let hover = crate::theme::card_hover(theme);
    let shot = div()
        .id(ids::addon_gb_card_image(section, card.id))
        .test_support()
        .flex_none()
        .w_full()
        .aspect_ratio(16. / 9.)
        .overflow_hidden()
        .bg(theme.muted)
        .relative()
        .flex()
        .items_center()
        .justify_center();
    let shot = match image {
        Some(RemoteImage::Ready(image)) => {
            shot.child(img(image).size_full().object_fit(ObjectFit::Cover))
        }
        _ => shot.child(
            Icon::new(IconName::Puzzle)
                .large()
                .text_color(theme.muted_foreground),
        ),
    };
    // The cover swallows its click, so revealing a thumbnail never opens the detail.
    let cover = reveal.map(|reveal| {
        h_flex()
            .id(ids::addon_gb_card_reveal(section, card.id))
            .test_support()
            .absolute()
            .inset_0()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(theme.background.opacity(0.4))
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.foreground)
            .cursor_pointer()
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                reveal(window, cx);
            })
            .child(Icon::new(IconName::EyeOff))
            .child("Mature")
    });
    let shot = shot.children(cover);
    let outdated = card.obsolete.then(|| {
        div().flex_none().child(badge_of(
            ids::addon_gb_card_outdated(section, card.id),
            Tag::secondary(),
            transparent_black(),
            "Outdated",
        ))
    });
    card_frame(cx)
        .id(ids::addon_gb_card(section, card.id))
        .accessibility_id(ids::addon_gb_card(section, card.id).to_string())
        .aria_label(card.name.clone())
        .test_support()
        .cursor_pointer()
        .hover(move |frame| frame.bg(hover))
        .on_click(move |_, window, cx| open(window, cx))
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(shot)
        .child(
            v_flex()
                .h(CARD_TEXT_HEIGHT)
                .flex_none()
                .min_w_0()
                .p_6()
                .child(
                    h_flex()
                        .min_w_0()
                        .gap_2()
                        .items_center()
                        .child(labelled(
                            div()
                                .min_w_0()
                                .truncate()
                                .line_height(px(24.))
                                .font_weight(FontWeight::SEMIBOLD),
                            ids::addon_gb_card_name(section, card.id),
                            card.name.clone(),
                        ))
                        .children(outdated),
                )
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .line_height(px(20.))
                        .text_color(theme.muted_foreground)
                        .child(format!("by {}", card.author)),
                )
                .child(
                    h_flex()
                        .mt_2()
                        .text_xs()
                        .line_height(px(16.))
                        .text_color(theme.muted_foreground)
                        .child(count_stat(
                            ids::addon_gb_card_likes(section, card.id),
                            IconName::ThumbsUp,
                            card.like_count,
                        )),
                ),
        )
        .into_any_element()
}
