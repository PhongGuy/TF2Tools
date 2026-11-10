//! The Addons page's GameBanana section: the recorded add-ons by Addon Store section, each card
//! acting through `GbAddonInstalls`, which owns every operation on them.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, StatefulInteractiveElement as _, Styled, TestSupportExt as _, Window, div, px,
};
use tf2tools_core::{AddonSection, GbAddonEntry, GbId};

use super::{AddonsPage, section};
use crate::gb_addon_installs::{GbAddonInstalls, GbAddonStage};
use crate::ids;
use crate::links::gamebanana_page;
use crate::pages::common::{badge_of, card_frame};
use crate::services::tf2_state;
use crate::shell::confirm::{Confirm, confirm};
use crate::widgets::Named as _;
use crate::widgets::card_grid::stretch_rows;

/// The tag a card shows, if any: gone wins over an update, which wins over not installed.
fn tag_text(entry: &GbAddonEntry) -> Option<&'static str> {
    if entry.is_gone() {
        Some("No longer on GameBanana")
    } else if entry.has_update() {
        Some("Update available")
    } else if !entry.is_installed() {
        Some("Not installed")
    } else {
        None
    }
}

/// The card's main action, if any: Update for a newer file, else Install again when the files
/// are missing. An item gone from GameBanana offers neither.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Main {
    Update,
    Reinstall,
}

fn main_action(entry: &GbAddonEntry) -> Option<Main> {
    if entry.is_gone() {
        None
    } else if entry.has_update() {
        Some(Main::Update)
    } else if !entry.is_installed() {
        Some(Main::Reinstall)
    } else {
        None
    }
}

/// The entries the page draws: a row naming a section this build does not know has no group,
/// so it is left out of the count as well.
pub(super) fn shown(entries: &[GbAddonEntry]) -> impl Iterator<Item = &GbAddonEntry> {
    entries
        .iter()
        .filter(|e| AddonSection::by_key(e.section()).is_some())
}

/// "GameBanana add-ons" with one titled group per section that has an entry, in tab order.
pub(super) fn gb_section(
    installs: &Entity<GbAddonInstalls>,
    columns: usize,
    rest: bool,
    cx: &mut Context<AddonsPage>,
) -> AnyElement {
    let has_tf2 = tf2_state(cx).read(cx).install().is_some();
    // Copied out with their stages, as drawing a card needs the page's context mutably.
    let staged: Vec<(GbAddonEntry, Option<GbAddonStage>)> = {
        let installs = installs.read(cx);
        installs
            .entries()
            .iter()
            .map(|e| (e.clone(), installs.stage(e.gb_id())))
            .collect()
    };
    let mut groups = Vec::new();
    for kind in AddonSection::ALL {
        let cards: Vec<AnyElement> = staged
            .iter()
            .filter(|(e, _)| e.section() == kind.key())
            .map(|(entry, stage)| gb_card(entry, *stage, rest, has_tf2, cx))
            .collect();
        if cards.is_empty() {
            continue;
        }
        groups.push(
            v_flex()
                .gap_3()
                .child(labelled_group(kind, cx))
                .child(stretch_rows(cards, columns)),
        );
    }
    section(
        ids::ADDONS_GB,
        "GameBanana add-ons",
        v_flex().gap_6().children(groups).into_any_element(),
    )
    .into_any_element()
}

// A group's heading: muted and smaller than the section title, labelled for the pilot.
fn labelled_group(kind: AddonSection, cx: &App) -> impl IntoElement {
    crate::widgets::labelled(
        div()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().muted_foreground),
        ids::addons_gb_group(kind.key()),
        kind.title(),
    )
}

// Asks first, as Remove also deletes the library copy; a yes hands the work to the installs.
fn remove(id: GbId, name: String, window: &mut Window, cx: &mut Context<AddonsPage>) {
    let answer = confirm(Confirm::remove_gb_addon(&name), window, cx);
    let win = window.window_handle();
    cx.spawn(async move |_, cx| {
        if answer.recv().await.unwrap_or(false) {
            cx.update(|cx| {
                let installs = GbAddonInstalls::global(cx);
                installs.update(cx, |i, cx| i.remove(id, win, cx));
            });
        }
    })
    .detach();
}

// The words on a running operation's footer.
fn busy_label(stage: GbAddonStage) -> &'static str {
    match stage {
        GbAddonStage::Downloading { .. } => "Downloading...",
        GbAddonStage::Choosing | GbAddonStage::Installing => "Installing...",
        GbAddonStage::Removing => "Removing...",
    }
}

// The mastercomfig card's frame for one add-on: its name, "by {author}" and tag, then the
// main action, Remove and the GameBanana link, or one loading button while it works.
fn gb_card(
    entry: &GbAddonEntry,
    stage: Option<GbAddonStage>,
    rest: bool,
    has_tf2: bool,
    cx: &mut Context<AddonsPage>,
) -> AnyElement {
    let (danger, muted, theme_fg, border) = {
        let theme = cx.theme();
        (
            theme.danger,
            theme.muted_foreground,
            theme.foreground,
            theme.border,
        )
    };
    let id = entry.gb_id();
    let name = entry.name().to_owned();
    // Everything but the link writes to tf/custom, so it waits for TF2 and for any operation.
    let disabled = rest || !has_tf2;
    let main = main_action(entry);
    let buttons: Vec<AnyElement> = match stage {
        Some(stage) => vec![
            Button::new(ids::addons_gb_busy(id))
                .outline()
                .flex_1()
                .loading(true)
                .disabled(true)
                .label(busy_label(stage))
                .into_any_element(),
        ],
        None => {
            let main_button = main.map(|main| {
                let (button, label, icon) = match main {
                    Main::Update => (
                        Button::new(ids::addons_gb_update(id)),
                        "Update",
                        IconName::RefreshCw,
                    ),
                    Main::Reinstall => (
                        Button::new(ids::addons_gb_reinstall(id)),
                        "Install again",
                        IconName::Download,
                    ),
                };
                button
                    .primary()
                    .flex_1()
                    .icon(icon)
                    .label(label)
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        let win = window.window_handle();
                        let installs = GbAddonInstalls::global(cx);
                        installs.update(cx, |i, cx| match main {
                            Main::Update => i.update(id, win, cx),
                            Main::Reinstall => i.reinstall(id, win, cx),
                        });
                    })
                    .into_any_element()
            });
            // Remove takes the wide labelled place unless a main action already holds it.
            let remove_button = {
                let name = name.clone();
                let button = Button::new(ids::addons_gb_remove(id))
                    .outline()
                    .icon(Icon::new(IconName::Trash).text_color(danger))
                    .disabled(disabled)
                    .on_click(
                        cx.listener(move |_, _, window, cx| remove(id, name.clone(), window, cx)),
                    );
                if main.is_some() {
                    button.named("Remove")
                } else {
                    button.flex_1().label("Remove")
                }
            };
            let url = gamebanana_page("", id);
            let open = Button::new(ids::addons_gb_open(id))
                .ghost()
                .icon(IconName::ExternalLink)
                .named("Open on GameBanana")
                .on_click(move |_, _, cx| cx.open_url(&url));
            main_button
                .into_iter()
                .chain([remove_button.into_any_element(), open.into_any_element()])
                .collect()
        }
    };
    let tag = tag_text(entry).map(|text| {
        // An update waiting is the one state worth the accent colour, as on the store cards.
        let tag = if main == Some(Main::Update) {
            Tag::primary()
        } else {
            Tag::secondary().outline().text_color(theme_fg)
        };
        h_flex().child(badge_of(ids::addons_gb_tag(id), tag, border, text))
    });
    card_frame(cx)
        .id(ids::addons_gb_card(id))
        .accessibility_id(ids::addons_gb_card(id).to_string())
        .aria_label(name.clone())
        .test_support()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .px_6()
        .py_6()
        .gap_6()
        // The tag gets its own line, since "No longer on GameBanana" beside the name would
        // squeeze it to nothing on a narrow card.
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1p5()
                .pb_2()
                .child(
                    div()
                        .truncate()
                        .line_height(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name),
                )
                .children((!entry.author().is_empty()).then(|| {
                    div()
                        .text_sm()
                        .text_color(muted)
                        .truncate()
                        .child(format!("by {}", entry.author()))
                }))
                .children(tag),
        )
        .child(h_flex().w_full().gap_2().children(buttons))
        .into_any_element()
}
