//! A GameBanana add-on's detail dialog: it reads the item's profile page on open and shows
//! its ratings, images, downloads, description, credits and files, with the install control
//! and "Open on GameBanana".

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    div, transparent_black,
};
use tf2tools_core::{AddonCard, AddonFile, AddonProfile, AddonSection, BrowseError, DownloadPhase};

use crate::gb_addon_installs::{
    GbAddonInstalls, GbAddonOffer as Offer, GbAddonStage, install_blocker, map_install_blocker,
};
use crate::i18n::{Phrase, tr};
use crate::ids;
use crate::links::gamebanana_page;
use crate::pages::addon_store::model::downloads_label;
use crate::pages::common::{badge_of, needs_tf2};
use crate::services::{Services, tf2_state};
use crate::shell::confirm::{Confirm, confirm};
use crate::store_download::{Progress, format_bytes, progress_label, progress_percent};
use crate::store_messages::{addon_stopped, addon_stopped_phrase, browse_phrase, describe_browse};
use crate::tokio_bridge;
use crate::widgets::carousel::Carousel;
use crate::widgets::detail_header::detail_header;
use crate::widgets::dialog_frame::{DETAIL_WIDTH, DialogFrame, content_max_height};
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_beside;
use crate::widgets::store_error::error_line;

/// Where the item's profile page stands.
pub enum ProfileState {
    /// The profile is being read, on open or after a retry.
    Loading,
    /// Boxed so the failure variants do not carry the profile's size.
    Ready(Box<AddonProfile>),
    /// The failure in player words, worded when drawn.
    Failed(Phrase),
    /// The item left GameBanana, so a retry cannot help; the message is worded when drawn.
    Gone(Phrase),
}

/// The dialog's view: the section and card that opened it, and its profile once read.
pub struct AddonDetail {
    section: AddonSection,
    card: AddonCard,
    installs: Entity<GbAddonInstalls>,
    _installs_changed: Subscription,
    profile: ProfileState,
    carousel: Entity<Carousel>,
    scroll: ScrollHandle,
    load: Option<Task<()>>,
}

impl AddonDetail {
    /// Needs the `Services` global and the TF2 state; starts reading `card`'s profile at once.
    /// Installs go into `section`.
    pub fn new(section: AddonSection, card: AddonCard, cx: &mut Context<Self>) -> Self {
        let carousel = cx.new(|cx| Carousel::new(Vec::new(), cx));
        let installs = GbAddonInstalls::global(cx);
        let installs_changed = cx.observe(&installs, |_, _, cx| cx.notify());
        let mut detail = Self {
            section,
            card,
            installs,
            _installs_changed: installs_changed,
            profile: ProfileState::Loading,
            carousel,
            scroll: ScrollHandle::new(),
            load: None,
        };
        detail.load(cx);
        detail
    }

    /// Where the item's profile read stands now.
    pub fn profile(&self) -> &ProfileState {
        &self.profile
    }

    fn ready(&self) -> Option<&AddonProfile> {
        match &self.profile {
            ProfileState::Ready(profile) => Some(profile.as_ref()),
            ProfileState::Loading | ProfileState::Failed(_) | ProfileState::Gone(_) => None,
        }
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
                Ok(Err(err)) => {
                    describe_browse(&err).log("add-on profile");
                    let phrase = browse_phrase(&err);
                    Err(if matches!(err, BrowseError::Gone { .. }) {
                        ProfileState::Gone(phrase)
                    } else {
                        ProfileState::Failed(phrase)
                    })
                }
                Err(_) => {
                    addon_stopped("the add-on profile task stopped unexpectedly".to_owned())
                        .log("add-on profile");
                    Err(ProfileState::Failed(addon_stopped_phrase()))
                }
            };
            let _ = this.update(cx, |d, cx| d.loaded(result, cx));
        }));
        cx.notify();
    }

    fn is_map(&self) -> bool {
        self.section == AddonSection::MAPS
    }

    // A map puts a missing folder before an update; add-ons keep their own order.
    fn offer(&self, cx: &App) -> Offer {
        let entry = self.installs.read(cx).entry(self.card.id);
        if self.is_map() {
            Offer::of_map(entry)
        } else {
            Offer::of(entry)
        }
    }

    // Starts what the control offers; GbAddonInstalls refuses it while another operation runs.
    // Only a first install needs the read profile; the rest work from the library entry.
    fn act(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ready = self.ready().cloned();
        let (section, id) = (self.section, self.card.id);
        let offer = self.offer(cx);
        let win = window.window_handle();
        self.installs.update(cx, |installs, cx| {
            match offer {
                Offer::Install => {
                    ready.is_some_and(|profile| installs.install(section, profile, win, cx))
                }
                Offer::Update => installs.update(id, win, cx),
                Offer::InstallAgain => installs.reinstall(id, win, cx),
                Offer::Installed => false,
            };
        });
    }

    // The header's control from the library entry, so an unreadable profile still offers
    // what the library holds; or, once the profile is read, why a download can't happen.
    // A first install waits for the profile, which it downloads from.
    fn install_control(&self, cx: &mut Context<Self>) -> (Option<Button>, Option<AnyElement>) {
        let ready = self.ready();
        let offer = self.offer(cx);
        let installs = self.installs.read(cx);
        let stage = installs.stage(self.card.id);
        let busy = installs.is_busy();
        let store = &cx.global::<Services>().store;
        if stage.is_none()
            && offer.downloads()
            && let Some(reason) = ready.and_then(|profile| {
                if self.is_map() {
                    map_install_blocker(store, profile)
                } else {
                    install_blocker(store, profile)
                }
            })
        {
            let line = labelled(
                div().text_sm().text_color(cx.theme().muted_foreground),
                ids::ADDON_DETAIL_BLOCKED,
                reason,
            );
            return (None, Some(line.into_any_element()));
        }
        let label = match stage {
            Some(GbAddonStage::Removing) => tr!("addon_detail.button.removing"),
            Some(_) => tr!("addon_store.button.installing"),
            None => offer.label(),
        };
        let button = Button::new(ids::ADDON_DETAIL_INSTALL)
            .label(label)
            .loading(stage.is_some())
            .disabled(
                busy || offer == Offer::Installed || (offer == Offer::Install && ready.is_none()),
            )
            .on_click(cx.listener(|this, _, window, cx| this.act(window, cx)));
        let button = if offer == Offer::Installed {
            button.secondary()
        } else {
            button.primary()
        };
        let has_tf2 = tf2_state(cx).read(cx).install().is_some();
        (Some(needs_tf2(button, has_tf2)), None)
    }

    // A map's Remove, once the library records it; core refuses it while TF2 runs.
    fn remove_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let installs = self.installs.read(cx);
        if !self.is_map() || installs.entry(self.card.id).is_none() {
            return None;
        }
        let button = Button::new(ids::ADDON_DETAIL_REMOVE)
            .outline()
            .icon(IconName::Trash)
            .label(tr!("addon_detail.button.remove"))
            .disabled(installs.is_busy())
            .on_click(cx.listener(|this, _, window, cx| this.remove(window, cx)));
        let has_tf2 = tf2_state(cx).read(cx).install().is_some();
        Some(needs_tf2(button, has_tf2).into_any_element())
    }

    // Asks first, then hands the removal to the installs, which toast the outcome.
    fn remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.card.id;
        let Some(name) = self
            .installs
            .read(cx)
            .entry(id)
            .map(|e| e.name().to_owned())
        else {
            return;
        };
        let installs = self.installs.clone();
        let win = window.window_handle();
        let answer = confirm(Confirm::remove_gb_map(&name), window, cx);
        cx.spawn(async move |_, cx| {
            if answer.recv().await.unwrap_or(false) {
                cx.update(|cx| installs.update(cx, |i, cx| i.remove(id, win, cx)));
            }
        })
        .detach();
    }

    // The maps an installed map item put into TF2, one row each.
    fn installed_maps(&self, cx: &App) -> Option<AnyElement> {
        let entry = self.installs.read(cx).entry(self.card.id)?;
        if !self.is_map() || !entry.is_installed() {
            return None;
        }
        let rows =
            entry.payloads().iter().enumerate().map(|(i, name)| {
                labelled(div().text_sm(), ids::addon_detail_map(i), map_line(name))
            });
        Some(
            v_flex()
                .gap_1()
                .child(labelled(
                    div().text_sm().font_weight(FontWeight::SEMIBOLD),
                    ids::ADDON_DETAIL_MAPS,
                    tr!("addon_detail.maps"),
                ))
                .children(rows)
                .into_any_element(),
        )
    }

    // The download's text and bar while this item downloads.
    fn progress(&self, cx: &App) -> Option<AnyElement> {
        let GbAddonStage::Downloading { downloaded, total } =
            self.installs.read(cx).stage(self.card.id)?
        else {
            return None;
        };
        let progress = Some(Progress {
            phase: DownloadPhase::Downloading,
            downloaded,
            total,
        });
        // Until the first byte the download may still be queued behind other GameBanana
        // requests, so a byte count of 0 would read as a stalled download.
        let label = if downloaded == 0 {
            tr!("addon_detail.waiting")
        } else {
            SharedString::from(progress_label(progress))
        };
        Some(
            v_flex()
                .id(ids::ADDON_DETAIL_PROGRESS)
                .aria_label(label.clone())
                .test_support()
                .gap_1()
                .child(div().text_sm().child(label))
                .child(ProgressBar::new(ids::ADDON_DETAIL_BAR).value(progress_percent(progress)))
                .into_any_element(),
        )
    }

    fn loaded(&mut self, result: Result<AddonProfile, ProfileState>, cx: &mut Context<Self>) {
        self.load = None;
        self.profile = match result {
            Ok(profile) => {
                let urls = profile.images().to_vec();
                self.carousel.update(cx, |c, cx| c.set_urls(urls, cx));
                ProfileState::Ready(Box::new(profile))
            }
            Err(failed) => failed,
        };
        cx.notify();
    }

    // The ratings, images, downloads, description, credits and files, in that order.
    fn ready_body(&self, profile: &AddonProfile, cx: &App) -> Vec<AnyElement> {
        let muted = cx.theme().muted_foreground;
        let heading = |text: SharedString| {
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(text)
                .into_any_element()
        };
        let mut body = Vec::new();
        if !profile.ratings().is_empty() {
            let text = tr!(
                "addon_detail.mature",
                reasons = crate::i18n::separated(profile.ratings())
            );
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
            let mut credits = v_flex().gap_2().child(heading(tr!("addon_detail.credits")));
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
                    .child(heading(tr!("addon_detail.files")))
                    .children(rows)
                    .into_any_element(),
            );
        }
        body
    }
}

// A file row's text: "{name} · {size} · {date}" with a numeric UTC date, leaving out an
// unknown size or date. GameBanana's missing date reads as 0, which is no upload date.
fn file_line(name: &str, size: Option<u64>, date_added: i64) -> String {
    let size = size.map(format_bytes);
    let date = (date_added > 0)
        .then(|| chrono::DateTime::from_timestamp(date_added, 0))
        .flatten()
        .map(|at| crate::i18n::format::numeric_date(at.date_naive()).to_string());
    std::iter::once(name.to_owned())
        .chain(size)
        .chain(date)
        .collect::<Vec<_>>()
        .join(" · ")
}

// An installed map's row; TF2's Create Server leaves out a map whose name has a dot.
fn map_line(name: &str) -> String {
    if name.contains('.') {
        tr!("addon_detail.map_unlisted", name = name).to_string()
    } else {
        name.to_owned()
    }
}

fn file_text(file: &AddonFile) -> String {
    file_line(file.name(), file.size(), file.date_added())
}

// v5's header link: an outline button with the ExternalLink icon, opening `url` in the browser.
fn link_button(key: &str, label: impl Into<SharedString>, url: String) -> Button {
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
            ProfileState::Loading | ProfileState::Failed(_) | ProfileState::Gone(_) => {
                (self.card.name.clone(), self.card.author.clone())
            }
        };
        let outdated = matches!(&self.profile, ProfileState::Ready(p) if p.obsolete()).then(|| {
            badge_of(
                ids::ADDON_DETAIL_OUTDATED,
                Tag::secondary(),
                transparent_black(),
                tr!("addon_store.card.outdated"),
            )
        });
        let body: Vec<AnyElement> = match &self.profile {
            ProfileState::Loading => vec![
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr!("addon_detail.loading"))
                    .into_any_element(),
            ],
            ProfileState::Failed(message) => vec![
                error_line(
                    ids::ADDON_DETAIL_ERROR,
                    ids::ADDON_DETAIL_RETRY,
                    message.text(),
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
            ProfileState::Gone(message) => vec![
                labelled(
                    div().text_sm().text_color(cx.theme().danger),
                    ids::ADDON_DETAIL_ERROR,
                    message.text(),
                )
                .into_any_element(),
            ],
            ProfileState::Ready(profile) => self.ready_body(profile, cx),
        };
        let (control, blocked) = self.install_control(cx);
        let remove = self.remove_control(cx);
        let scrolled = v_flex()
            .id(ids::ADDON_DETAIL_BODY)
            .test_support()
            .min_h_0()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .gap_4()
            .children(body);
        let link = link_button(
            "gamebanana",
            tr!("addon_detail.open"),
            gamebanana_page(&self.card.profile_url, self.card.id),
        );
        let actions = control
            .map(IntoElement::into_any_element)
            .into_iter()
            .chain(remove)
            .chain([link.into_any_element()]);
        let author = div()
            .text_sm()
            .text_color(muted)
            .child(tr!("addon_store.card.by", author = author))
            .into_any_element();
        v_flex()
            .id(ids::ADDON_DETAIL)
            .test_support()
            .w_full()
            .max_h(content_max_height(window, cx))
            .gap_4()
            .child(detail_header(
                ids::ADDON_DETAIL_NAME,
                name.into(),
                outdated.map(IntoElement::into_any_element),
                Some(author),
                actions,
                window,
                cx,
            ))
            .children(blocked)
            // Pinned under the header so a scrolled body never hides the download.
            .children(self.progress(cx))
            .children(self.installed_maps(cx))
            .child(scroll_beside(ids::ADDON_DETAIL_SCROLLBAR, &self.scroll, scrolled).min_h_0())
    }
}

/// Opens the dialog (`ids::ADDON_DETAIL`) for `card`, installing into `section`, and returns
/// its view.
pub fn open_addon_detail(
    section: AddonSection,
    card: &AddonCard,
    window: &mut Window,
    cx: &mut App,
) -> Entity<AddonDetail> {
    let card = card.clone();
    let detail = cx.new(|cx| AddonDetail::new(section, card, cx));
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

    #[test]
    fn a_dotted_map_name_says_create_server_does_not_list_it() {
        assert_eq!(super::map_line("cp_alpha"), "cp_alpha");
        assert_eq!(
            super::map_line("ctf_2fort.v2"),
            "ctf_2fort.v2 (not listed in Create Server; type map ctf_2fort.v2 in the console)"
        );
    }
}
