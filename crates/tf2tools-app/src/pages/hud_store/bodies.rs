//! The HUD stores' detail bodies: what the dialog shows under the carousel.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, Size, h_flex, v_flex};
use gpui_kit::{
    App, Div, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    TestSupportExt as _, div, px, transparent_black,
};
use tf2tools_core::StoreItem;

use super::curated::CuratedHud;
use crate::counts::count_stat;
use crate::ids;
use crate::pages::common::badge_of;
use crate::store_detail::DetailBody;

/// A curated HUD: release date, flags, OS / gamemodes / resolutions, tags, socials, and its
/// Repository button beside the name.
pub fn curated_body() -> DetailBody {
    DetailBody::new(|item: &StoreItem, _, cx: &mut App| {
        let Some(hud) = CuratedHud::from_item(item) else {
            return div().into_any_element();
        };
        let socials: Vec<(String, String, String)> = hud
            .socials
            .iter()
            .map(|(key, url)| (key.clone(), capitalize(key), url.clone()))
            .collect();
        let traits = [
            ("OS", &hud.os),
            ("Gamemodes", &hud.gamemodes),
            ("Resolutions", &hud.resolutions),
        ];
        v_flex()
            .gap_3()
            .children(
                hud.release_date
                    .as_ref()
                    .map(|d| text_row(cx, format!("Released {d}"))),
            )
            .children(tag_row("flags", &hud.flags, false, cx))
            .children(
                traits
                    .into_iter()
                    .filter(|(_, values)| !values.is_empty())
                    .map(|(name, values)| text_row(cx, format!("{name}: {}", values.join(", ")))),
            )
            .children(tag_row("tags", &hud.tags, true, cx))
            .children((!socials.is_empty()).then(|| link_row(socials)))
            .into_any_element()
    })
    .with_action(|item, _, _| {
        let repo = CuratedHud::from_item(item)?.repo?;
        Some(link_button("repo", "Repository", repo).into_any_element())
    })
}

/// A GameBanana HUD: added date, likes and views, tags, and its GameBanana button beside the name.
pub fn gamebanana_body() -> DetailBody {
    DetailBody::new(|item: &StoreItem, _, cx: &mut App| {
        let added = chrono::DateTime::from_timestamp(item.date_added(), 0)
            .map(|d| format!("Added {}", d.format("%b %-d, %Y")));
        v_flex()
            .gap_3()
            .children(added.map(|a| text_row(cx, a)))
            // v5's Stats row: the like and view counts as icon stats.
            .child(
                h_flex()
                    .gap_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(count_stat(
                        ids::STORE_DETAIL_LIKES,
                        IconName::ThumbsUp,
                        item.like_count(),
                    ))
                    .child(count_stat(
                        ids::STORE_DETAIL_VIEWS,
                        IconName::Eye,
                        item.view_count(),
                    )),
            )
            .children(tag_row("tags", item.tags(), true, cx))
            .into_any_element()
    })
    .with_action(|item, _, _| {
        let url = item.profile_url();
        (!url.is_empty())
            .then(|| link_button("gamebanana", "GameBanana", url.to_owned()).into_any_element())
    })
}

fn text_row(cx: &App, text: String) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

// v5's header link: an outline button with the ExternalLink icon, opening `url` in the browser.
fn link_button(key: &str, label: &'static str, url: String) -> Button {
    Button::new(ids::store_detail_link(key))
        .outline()
        .icon(IconName::ExternalLink)
        .label(label)
        .on_click(move |_, _, cx| cx.open_url(&url))
}

/// v5's icon for a curated HUD social, by its key: Discord a message bubble, Support a heart,
/// any other an external-link arrow.
pub fn social_icon(key: &str) -> IconName {
    match key {
        "discord" => IconName::MessageCircle,
        "support" => IconName::Heart,
        _ => IconName::ExternalLink,
    }
}

// The HUD's socials as ghost buttons, each opening its URL; (key, label, url). The ids carry a
// `social:` prefix so a social named "repo" never shares the header button's id. v5 leads each
// with a 14 px icon 6 px before the label; the kit's own icon slot can carry neither an id nor that
// gap, so icon and label ride in one child row and the button keeps its name for screen readers.
fn link_row(links: Vec<(String, String, String)>) -> impl IntoElement {
    h_flex()
        .gap_2()
        .flex_wrap()
        .children(links.into_iter().map(|(key, label, url)| {
            let icon = div()
                .id(ids::store_detail_social_icon(&key))
                .test_support()
                .flex()
                .flex_none()
                .child(Icon::new(social_icon(&key)).with_size(Size::Size(px(14.))));
            let text = div()
                .id(ids::store_detail_social_label(&key))
                .test_support()
                .child(label.clone());
            Button::new(ids::store_detail_link(&format!("social:{key}")))
                .ghost()
                .accessibility_label(label)
                .child(h_flex().gap_1p5().child(icon).child(text))
                .on_click(move |_, _, cx| cx.open_url(&url))
        }))
}

// v5's flags are secondary badges and its tags outline ones, the cards' badges, keyed by `row`.
fn tag_row(
    row: &'static str,
    values: &[String],
    outline: bool,
    cx: &App,
) -> Option<impl IntoElement> {
    let (foreground, outline_border) = (cx.theme().foreground, cx.theme().border);
    (!values.is_empty()).then(|| {
        h_flex()
            .gap_1p5()
            .flex_wrap()
            .children(values.iter().enumerate().map(move |(ix, v)| {
                let (tag, border) = if outline {
                    (
                        Tag::secondary().outline().text_color(foreground),
                        outline_border,
                    )
                } else {
                    (Tag::secondary(), transparent_black())
                };
                badge_of(ids::store_detail_tag(row, ix), tag, border, v.clone())
            }))
    })
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
