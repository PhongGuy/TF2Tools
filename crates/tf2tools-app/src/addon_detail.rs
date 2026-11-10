//! A GameBanana add-on's detail dialog: it reads the item's profile page on open and shows
//! its ratings, images, downloads, description, credits and files, with the install control
//! and "Open on GameBanana".

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    div, px, transparent_black,
};
use tf2tools_core::{
    AddonCard, AddonFile, AddonProfile, AddonSection, BrowseError, DownloadPhase, GbAddonEntry,
};

use crate::gb_addon_installs::{GbAddonInstalls, GbAddonStage, install_blocker};
use crate::ids;
use crate::links::gamebanana_page;
use crate::pages::addon_store::model::downloads_label;
use crate::pages::common::{badge_of, needs_tf2};
use crate::services::{Services, tf2_state};
use crate::store_download::{Progress, format_bytes, progress_label, progress_percent};
use crate::store_messages::{addon_stopped, describe_browse};
use crate::tokio_bridge;
use crate::widgets::carousel::Carousel;
use crate::widgets::dialog_frame::{DETAIL_WIDTH, DialogFrame, content_max_height};
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_beside;
use crate::widgets::store_error::error_line;

/// Where the item's profile page stands.
pub enum ProfileState {
    /// The profile is being read, on open or after a retry.
    Loading,
    Ready(AddonProfile),
    /// The failure in player words.
    Failed(String),
    /// The item left GameBanana, so a retry cannot help; the message is in player words.
    Gone(String),
}

/// What the install control offers for an add-on, from its library entry.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Offer {
    Install,
    Update,
    InstallAgain,
    Installed,
}

impl Offer {
    fn of(entry: Option<&GbAddonEntry>) -> Self {
        match entry {
            None => Self::Install,
            Some(e) if e.has_update() => Self::Update,
            Some(e) if !e.is_installed() => Self::InstallAgain,
            Some(_) => Self::Installed,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Install => "Install",
            Self::Update => "Update",
            Self::InstallAgain => "Install again",
            Self::Installed => "Installed",
        }
    }

    // Install and Update download GameBanana's newest file; the rest use the library copy.
    fn downloads(self) -> bool {
        matches!(self, Self::Install | Self::Update)
    }
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
                Ok(Err(err)) => Err((
                    describe_browse(&err),
                    !matches!(err, BrowseError::Gone { .. }),
                )),
                Err(_) => Err((
                    addon_stopped("the add-on profile task stopped unexpectedly".to_owned()),
                    true,
                )),
            }
            .map_err(|(failure, retryable)| {
                failure.log("add-on profile");
                if retryable {
                    ProfileState::Failed(failure.message)
                } else {
                    ProfileState::Gone(failure.message)
                }
            });
            let _ = this.update(cx, |d, cx| d.loaded(result, cx));
        }));
        cx.notify();
    }

    // Starts what the control offers; GbAddonInstalls refuses it while another operation runs.
    fn act(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ProfileState::Ready(profile) = &self.profile else {
            return;
        };
        let (section, profile, id) = (self.section, profile.clone(), self.card.id);
        let win = window.window_handle();
        self.installs.update(cx, |installs, cx| {
            match Offer::of(installs.entry(id)) {
                Offer::Install => installs.install(section, profile, win, cx),
                Offer::Update => installs.update(id, win, cx),
                Offer::InstallAgain => installs.reinstall(id, win, cx),
                Offer::Installed => false,
            };
        });
    }

    // The header's control, or why there is none. It waits for the profile, which decides
    // the blocker; the blocker applies only to an offer that downloads, since a library copy
    // or an installed add-on needs no new file.
    fn install_control(&self, cx: &mut Context<Self>) -> (Option<Button>, Option<AnyElement>) {
        let ProfileState::Ready(profile) = &self.profile else {
            return (None, None);
        };
        let installs = self.installs.read(cx);
        let offer = Offer::of(installs.entry(self.card.id));
        let stage = installs.stage(self.card.id);
        let busy = installs.is_busy();
        if stage.is_none()
            && offer.downloads()
            && let Some(reason) = install_blocker(profile)
        {
            let line = labelled(
                div().text_sm().text_color(cx.theme().muted_foreground),
                ids::ADDON_DETAIL_BLOCKED,
                reason,
            );
            return (None, Some(line.into_any_element()));
        }
        let label = match stage {
            Some(GbAddonStage::Removing) => "Removing...",
            Some(_) => "Installing...",
            None => offer.label(),
        };
        let button = Button::new(ids::ADDON_DETAIL_INSTALL)
            .label(label)
            .loading(stage.is_some())
            .disabled(busy || offer == Offer::Installed)
            .on_click(cx.listener(|this, _, window, cx| this.act(window, cx)));
        let button = if offer == Offer::Installed {
            button.secondary()
        } else {
            button.primary()
        };
        let has_tf2 = tf2_state(cx).read(cx).install().is_some();
        (Some(needs_tf2(button, has_tf2)), None)
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
        let label = SharedString::from(progress_label(progress));
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
                ProfileState::Ready(profile)
            }
            Err(failed) => failed,
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
            ProfileState::Loading | ProfileState::Failed(_) | ProfileState::Gone(_) => {
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
            ProfileState::Gone(message) => vec![
                labelled(
                    div().text_sm().text_color(cx.theme().danger),
                    ids::ADDON_DETAIL_ERROR,
                    message.clone(),
                )
                .into_any_element(),
            ],
            ProfileState::Ready(profile) => self.ready_body(profile, cx),
        };
        let (control, blocked) = self.install_control(cx);
        let scrolled = v_flex()
            .id(ids::ADDON_DETAIL_BODY)
            .test_support()
            .min_h_0()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .gap_4()
            .children(self.progress(cx))
            .children(body);
        // The kit's close X floats into the top right corner, so the header keeps 22 px clear.
        v_flex()
            .id(ids::ADDON_DETAIL)
            .test_support()
            .w_full()
            .max_h(content_max_height(window, cx))
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
                                // The name may shrink and wrap, so a long one stays clear
                                // of the header button and the close X.
                                h_flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap_2()
                                    .child(labelled(
                                        div()
                                            .min_w_0()
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
                    .child(
                        h_flex()
                            .flex_none()
                            .gap_2()
                            .children(control)
                            .child(link_button(
                                "gamebanana",
                                "Open on GameBanana",
                                gamebanana_page(&self.card.profile_url, self.card.id),
                            )),
                    ),
            )
            .children(blocked)
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
}
