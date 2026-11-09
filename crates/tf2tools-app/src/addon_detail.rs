//! A GameBanana add-on's detail dialog: it reads the item's profile page on open and shows
//! its ratings, images, downloads, description, credits and files, with "Open on GameBanana".

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, TestSupportExt as _, Window, div, px,
    transparent_black,
};
use tf2tools_core::{AddonCard, AddonFile, AddonProfile};

use crate::ids;
use crate::links::gamebanana_page;
use crate::pages::addon_store::model::downloads_label;
use crate::pages::common::badge_of;
use crate::services::Services;
use crate::store_download::format_bytes;
use crate::store_messages::{addon_stopped, describe_browse};
use crate::tokio_bridge;
use crate::widgets::carousel::Carousel;
use crate::widgets::dialog_frame::{BOX_CHROME, DETAIL_WIDTH, DialogFrame};
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_beside;
use crate::widgets::store_error::error_line;

/// Where the item's profile page stands.
pub enum ProfileState {
    Loading,
    Ready(AddonProfile),
    /// The failure in player words.
    Failed(String),
}

/// The dialog's view: the card that opened it, and its profile once read.
pub struct AddonDetail {
    card: AddonCard,
    profile: ProfileState,
    carousel: Entity<Carousel>,
    scroll: ScrollHandle,
    load: Option<Task<()>>,
}

impl AddonDetail {
    /// Needs the `Services` global; starts reading `card`'s profile at once.
    pub fn new(card: AddonCard, cx: &mut Context<Self>) -> Self {
        let carousel = cx.new(|cx| Carousel::new(Vec::new(), cx));
        let mut detail = Self {
            card,
            profile: ProfileState::Loading,
            carousel,
            scroll: ScrollHandle::new(),
            load: None,
        };
        detail.load(cx);
        detail
    }

    pub fn profile(&self) -> &ProfileState {
        &self.profile
    }

    /// Reads the profile again after a failure.
    pub fn retry(&mut self, cx: &mut Context<Self>) {
        self.load(cx);
    }

    // The profile comes over the network, so it is read on the tokio bridge; the raw error
    // reaches only the log, the player the plain message.
    fn load(&mut self, cx: &mut Context<Self>) {
        self.profile = ProfileState::Loading;
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let catalog = cx.global::<Services>().store.addon_catalog();
        let id = self.card.id;
        let job = tokio_bridge::spawn(cx, async move { catalog.profile(id).await });
        self.load = Some(cx.spawn(async move |this, cx| {
            let result = match job.await {
                Ok(Ok(profile)) => Ok(profile),
                Ok(Err(err)) => Err(describe_browse(&err)),
                Err(_) => Err(addon_stopped(
                    "the add-on profile task stopped unexpectedly".to_owned(),
                )),
            }
            .map_err(|failure| {
                failure.log("add-on profile");
                failure.message
            });
            let _ = this.update(cx, |d, cx| d.loaded(result, cx));
        }));
        cx.notify();
    }

    fn loaded(&mut self, result: Result<AddonProfile, String>, cx: &mut Context<Self>) {
        self.load = None;
        self.profile = match result {
            Ok(profile) => {
                let urls = profile.images().to_vec();
                self.carousel.update(cx, |c, cx| c.set_urls(urls, cx));
                ProfileState::Ready(profile)
            }
            Err(message) => ProfileState::Failed(message),
        };
        cx.notify();
    }

    // The ratings, images, downloads, description, credits and files, in that order.
    fn ready_body(&self, profile: &AddonProfile, cx: &App) -> Vec<AnyElement> {
        let muted = cx.theme().muted_foreground;
        let heading = |text: &'static str| {
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(text)
                .into_any_element()
        };
        let mut body = Vec::new();
        if !profile.ratings().is_empty() {
            let text = format!("Mature content: {}", profile.ratings().join(", "));
            body.push(
                labelled(
                    div().text_sm().text_color(cx.theme().warning),
                    ids::ADDON_DETAIL_RATINGS,
                    text,
                )
                .into_any_element(),
            );
        }
        // A 16:9 box has no content height to defend, so a short window would squash it.
        if !profile.images().is_empty() {
            body.push(
                div()
                    .flex_shrink_0()
                    .child(self.carousel.clone())
                    .into_any_element(),
            );
        }
        body.push(
            labelled(
                div().text_sm().text_color(muted),
                ids::ADDON_DETAIL_DOWNLOADS,
                downloads_label(profile.download_count()),
            )
            .into_any_element(),
        );
        if !profile.description().is_empty() {
            body.push(
                labelled(
                    div().text_sm().whitespace_normal(),
                    ids::ADDON_DETAIL_DESCRIPTION,
                    profile.description().to_owned(),
                )
                .into_any_element(),
            );
        }
        if !profile.credits().is_empty() {
            let mut credits = v_flex().gap_2().child(heading("Credits"));
            for (g, group) in profile.credits().iter().enumerate() {
                let rows = group.credits().iter().enumerate().map(|(i, credit)| {
                    let text = if credit.role().is_empty() {
                        credit.name().to_owned()
                    } else {
                        format!("{} ({})", credit.name(), credit.role())
                    };
                    labelled(div().text_sm(), ids::addon_detail_credit(g, i), text)
                });
                credits = credits.child(
                    v_flex()
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(group.name().to_owned()),
                        )
                        .children(rows),
                );
            }
            body.push(credits.into_any_element());
        }
        if !profile.files().is_empty() {
            let rows = profile.files().iter().enumerate().map(|(i, file)| {
                labelled(div().text_sm(), ids::addon_detail_file(i), file_text(file))
            });
            body.push(
                v_flex()
                    .gap_1()
                    .child(heading("Files"))
                    .children(rows)
                    .into_any_element(),
            );
        }
        body
    }
}

// A file row's text: "{name} · {size} · {YYYY-MM-DD}", leaving out an unknown size or date.
// GameBanana's missing date reads as 0, which is no upload date.
fn file_line(name: &str, size: Option<u64>, date_added: i64) -> String {
    let size = size.map(format_bytes);
    let date = (date_added > 0)
        .then(|| chrono::DateTime::from_timestamp(date_added, 0))
        .flatten()
        .map(|at| at.format("%Y-%m-%d").to_string());
    std::iter::once(name.to_owned())
        .chain(size)
        .chain(date)
        .collect::<Vec<_>>()
        .join(" · ")
}

fn file_text(file: &AddonFile) -> String {
    file_line(file.name(), file.size(), file.date_added())
}

// v5's header link: an outline button with the ExternalLink icon, opening `url` in the browser.
fn link_button(key: &str, label: &'static str, url: String) -> Button {
    Button::new(ids::store_detail_link(key))
        .outline()
        .icon(IconName::ExternalLink)
        .label(label)
        .on_click(move |_, _, cx| cx.open_url(&url))
}

impl Render for AddonDetail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        // The card names the item until the profile does, so the header never jumps empty.
        let (name, author) = match &self.profile {
            ProfileState::Ready(p) => (p.name().to_owned(), p.author().to_owned()),
            ProfileState::Loading | ProfileState::Failed(_) => {
                (self.card.name.clone(), self.card.author.clone())
            }
        };
        let outdated = matches!(&self.profile, ProfileState::Ready(p) if p.obsolete()).then(|| {
            badge_of(
                ids::ADDON_DETAIL_OUTDATED,
                Tag::secondary(),
                transparent_black(),
                "Outdated",
            )
        });
        let body: Vec<AnyElement> = match &self.profile {
            ProfileState::Loading => vec![
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Loading details...")
                    .into_any_element(),
            ],
            ProfileState::Failed(message) => vec![
                error_line(
                    ids::ADDON_DETAIL_ERROR,
                    ids::ADDON_DETAIL_RETRY,
                    SharedString::from(message.clone()),
                    {
                        let this = cx.entity().downgrade();
                        move |_, cx| {
                            let _ = this.update(cx, |d, cx| d.retry(cx));
                        }
                    },
                    cx,
                )
                .into_any_element(),
            ],
            ProfileState::Ready(profile) => self.ready_body(profile, cx),
        };
        let scrolled = v_flex()
            .id(ids::ADDON_DETAIL_BODY)
            .test_support()
            .min_h_0()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .gap_4()
            .children(body);
        // The kit's close X floats into the top right corner, so the header keeps 22 px clear.
        v_flex()
            .id(ids::ADDON_DETAIL)
            .test_support()
            .w_full()
            .max_h(window.viewport_size().height * 0.9 - BOX_CHROME)
            .gap_4()
            .child(
                h_flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .pr(px(22.))
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(labelled(
                                        div()
                                            .text_xl()
                                            .line_height(px(28.))
                                            .font_weight(FontWeight::SEMIBOLD),
                                        ids::ADDON_DETAIL_NAME,
                                        name,
                                    ))
                                    .children(outdated),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(format!("by {author}")),
                            ),
                    )
                    .child(link_button(
                        "gamebanana",
                        "Open on GameBanana",
                        gamebanana_page(&self.card.profile_url, self.card.id),
                    )),
            )
            .child(scroll_beside(ids::ADDON_DETAIL_SCROLLBAR, &self.scroll, scrolled).min_h_0())
    }
}

/// Opens the dialog (`ids::ADDON_DETAIL`) for `card` and returns its view.
pub fn open_addon_detail(
    card: &AddonCard,
    window: &mut Window,
    cx: &mut App,
) -> Entity<AddonDetail> {
    let card = card.clone();
    let detail = cx.new(|cx| AddonDetail::new(card, cx));
    let view = detail.clone();
    let frame = DialogFrame::new(DETAIL_WIDTH);
    // Deferred, as confirm does, so the click that opened it cannot pull focus back out.
    window.defer(cx, move |window, cx| {
        window.open_dialog(cx, move |dialog, window, cx| {
            frame
                .apply(dialog, &*window, &*cx)
                .child(frame.measure(view.clone()))
        });
    });
    detail
}

#[cfg(test)]
mod tests {
    use super::file_line;

    #[test]
    fn a_file_line_leaves_out_what_gamebanana_did_not_give() {
        let size = crate::store_download::format_bytes(2048);
        assert_eq!(
            file_line("a.zip", Some(2048), 86_400),
            format!("a.zip · {size} · 1970-01-02")
        );
        assert_eq!(file_line("a.zip", None, 86_400), "a.zip · 1970-01-02");
        assert_eq!(file_line("a.zip", Some(2048), 0), format!("a.zip · {size}"));
        assert_eq!(file_line("a.zip", None, 0), "a.zip");
    }
}
