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
use crate::gb_addon_installs::GbAddonOffer;
use crate::ids;
use crate::pages::common::{badge_of, card_frame};
use crate::remote_images::RemoteImage;
use crate::widgets::labelled;

/// The card's text area under its thumbnail, laid out like `gb_card`: p-6 around a 58 px header
/// (name, author, pb 14 px), gap-6, then the mt-2 stats row.
const CARD_TEXT_HEIGHT: Pixels = px(154.);

/// The card's height at `width`: `gb_card`'s pt-6, the 16:9 thumbnail and the text area.
pub fn card_height(width: Pixels) -> Pixels {
    px(24.) + width * (9. / 16.) + CARD_TEXT_HEIGHT
}

/// What a click on the card does.
pub type OpenAddon = Rc<dyn Fn(&mut Window, &mut App)>;

/// A GameBanana add-on card: thumbnail, name (tagged with `badge`, "Installed" or "Update",
/// when given, else "Outdated" when obsolete), author and likes. A click calls `open`;
/// with `reveal`, a "Mature" cover hides the thumbnail and a click on the cover calls
/// `reveal` instead.
pub fn gb_addon_card(
    section: &str,
    card: &AddonCard,
    image: Option<RemoteImage>,
    badge: Option<&'static str>,
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
    // One status badge at most, so the name keeps its room on a narrow card; the install
    // badge wins over "Outdated".
    let outdated = (card.obsolete && badge.is_none()).then(|| {
        div().flex_none().child(badge_of(
            ids::addon_gb_card_outdated(section, card.id),
            Tag::secondary(),
            transparent_black(),
            "Outdated",
        ))
    });
    // An update waiting is the one state worth the accent colour; installed stays quiet.
    let installed = badge.map(|text| {
        let tag = if text == GbAddonOffer::Update.label() {
            Tag::primary()
        } else {
            Tag::secondary()
        };
        div().flex_none().child(badge_of(
            ids::addon_gb_card_badge(section, card.id),
            tag,
            transparent_black(),
            text,
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
        .pt_6()
        .overflow_hidden()
        .child(shot)
        .child(
            v_flex()
                .h(CARD_TEXT_HEIGHT)
                .flex_none()
                .min_w_0()
                .p_6()
                .gap_6()
                .child(
                    v_flex()
                        .min_w_0()
                        .pb(px(14.))
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
                                .children(outdated)
                                .children(installed),
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
