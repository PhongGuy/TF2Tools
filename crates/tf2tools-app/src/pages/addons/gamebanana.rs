//! The Addons page's GameBanana section: the recorded add-ons by Addon Store section, each card
//! acting through `GbAddonInstalls`, which owns every operation on them.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement as _, Styled,
    TestSupportExt as _, Window, div, px,
};
use tf2tools_core::{AddonSection, GbAddonEntry, GbId};

use super::model::{AddonAction, actions};
use super::{AddonsPage, section};
use crate::gb_addon_installs::{GbAddonInstalls, GbAddonOffer, GbAddonStage};
use crate::i18n::names::addon_section_name;
use crate::i18n::tr;
use crate::ids;
use crate::links::gamebanana_page;
use crate::pages::common::{badge_of, card_frame};
use crate::pages::graphics::jobs::refuse_while_running;
use crate::pages::profiles::probe::Tf2Probe;
use crate::services::{run_blocking, tf2_state};
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::toast::toast_error;
use crate::widgets::Named as _;
use crate::widgets::card_grid::stretch_rows;

/// The tag a card shows, if any: gone wins over an update, which wins over not installed.
fn tag_text(entry: &GbAddonEntry) -> Option<SharedString> {
    if entry.is_gone() {
        Some(tr!("addons.badge.gone"))
    } else if entry.has_update() {
        Some(tr!("addons.badge.update_available"))
    } else if !entry.is_installed() {
        Some(tr!("addons.badge.not_installed"))
    } else {
        None
    }
}

/// The card's footer, as the mastercomfig card picks it: Update shows for a newer file still
/// on GameBanana, the same offer the store detail makes.
fn footer(entry: &GbAddonEntry) -> &'static [AddonAction] {
    let update = GbAddonOffer::of(Some(entry)) == GbAddonOffer::Update;
    actions(entry.is_installed(), update)
}

/// The entries the page draws: only Addon Store sections have a group, so a map or a section
/// this build does not know is left out of the count as well.
pub(super) fn shown(entries: &[GbAddonEntry]) -> impl Iterator<Item = &GbAddonEntry> {
    entries
        .iter()
        .filter(|e| AddonSection::ALL.iter().any(|s| s.key() == e.section()))
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
        tr!("addons.heading.gamebanana"),
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
        addon_section_name(kind),
    )
}

// Uninstall and Delete both take files out of TF2, so a VPK add-on refuses first while TF2
// runs; Delete then asks, as it also drops the library copy. The installs probe again, as
// TF2 may start while the dialog is open.
fn take_out(id: GbId, delete: bool, window: &mut Window, cx: &mut Context<AddonsPage>) {
    let installs = GbAddonInstalls::global(cx);
    let has_vpk = installs.read(cx).entry(id).is_some_and(has_vpk);
    let probe = Tf2Probe::get(cx);
    let refusing = run_blocking(cx, move |_| {
        if has_vpk {
            refuse_while_running(&*probe)
        } else {
            Ok(())
        }
    });
    let win = window.window_handle();
    cx.spawn(async move |_, cx| {
        if let Err(err) = refusing.await {
            let _ = win.update(cx, |_, window, cx| toast_error(window, cx, &err));
            return;
        }
        if !delete {
            cx.update(|cx| installs.update(cx, |i, cx| i.uninstall(id, win, cx)));
            return;
        }
        let Ok(answer) = win.update(cx, |_, window, cx| {
            confirm(Confirm::delete_addon(), window, cx)
        }) else {
            return;
        };
        if answer.recv().await.unwrap_or(false) {
            cx.update(|cx| installs.update(cx, |i, cx| i.remove(id, win, cx)));
        }
    })
    .detach();
}

/// Whether any of the add-on's recorded files is a VPK, which TF2 holds open while it runs.
fn has_vpk(entry: &GbAddonEntry) -> bool {
    entry.files().iter().any(|f| {
        std::path::Path::new(f)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("vpk"))
    })
}

// One footer button in the mastercomfig card's style: Update and Install fill the row, Install
// steps back to outline beside Update, and Uninstall shrinks to its icon there.
fn gb_button(
    id: GbId,
    action: AddonAction,
    beside_update: bool,
    disabled: bool,
    danger: Hsla,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    match action {
        AddonAction::Update => Button::new(ids::addons_gb_update(id))
            .primary()
            .flex_1()
            .icon(IconName::RefreshCw)
            .label(tr!("addons.button.update"))
            .disabled(disabled)
            .on_click(on_click)
            .into_any_element(),
        AddonAction::Install => {
            let button = Button::new(ids::addons_gb_install(id))
                .flex_1()
                .icon(IconName::Download)
                .label(tr!("addons.button.install"))
                .disabled(disabled)
                .on_click(on_click);
            if beside_update {
                button.outline().into_any_element()
            } else {
                button.primary().into_any_element()
            }
        }
        AddonAction::Uninstall => {
            let button = Button::new(ids::addons_gb_uninstall(id))
                .outline()
                .icon(IconName::Unplug)
                .disabled(disabled)
                .on_click(on_click);
            if beside_update {
                button
                    .named(tr!("addons.button.uninstall"))
                    .into_any_element()
            } else {
                button
                    .flex_1()
                    .label(tr!("addons.button.uninstall"))
                    .into_any_element()
            }
        }
        AddonAction::Delete => Button::new(ids::addons_gb_delete(id))
            .ghost()
            .icon(Icon::new(IconName::Trash).text_color(danger))
            .named(tr!("addons.button.delete"))
            .disabled(disabled)
            .on_click(on_click)
            .into_any_element(),
    }
}

// The words on a running operation's footer.
fn busy_label(stage: GbAddonStage) -> SharedString {
    match stage {
        GbAddonStage::Downloading { .. } => tr!("common.size.downloading"),
        GbAddonStage::Choosing | GbAddonStage::Installing => tr!("addons.button.installing"),
        GbAddonStage::Removing => tr!("addons.button.removing"),
    }
}

// The mastercomfig card's frame for one add-on: its name, "by {author}" and tag, then the
// mastercomfig footer and the GameBanana link, or one loading button while it works.
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
    // Everything but the link writes to tf/custom, Delete included as it clears the installed
    // files too, so it waits for TF2 and for any operation.
    let disabled = rest || !has_tf2;
    let footer = footer(entry);
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
            let beside_update = footer.contains(&AddonAction::Update);
            let on = |action: AddonAction| {
                cx.listener(move |_, _, window, cx| {
                    let win = window.window_handle();
                    let installs = GbAddonInstalls::global(cx);
                    match action {
                        AddonAction::Update => {
                            installs.update(cx, |i, cx| i.update(id, win, cx));
                        }
                        AddonAction::Install => {
                            installs.update(cx, |i, cx| i.reinstall(id, win, cx));
                        }
                        AddonAction::Uninstall => take_out(id, false, window, cx),
                        AddonAction::Delete => take_out(id, true, window, cx),
                    }
                })
            };
            let url = gamebanana_page("", id);
            let open = Button::new(ids::addons_gb_open(id))
                .ghost()
                .icon(IconName::ExternalLink)
                .named(tr!("addons.button.open_gamebanana"))
                .on_click(move |_, _, cx| cx.open_url(&url));
            footer
                .iter()
                .map(|&action| gb_button(id, action, beside_update, disabled, danger, on(action)))
                .chain([open.into_any_element()])
                .collect()
        }
    };
    let tag = tag_text(entry).map(|text| {
        // An update waiting is the one state worth the accent colour, as on the store cards.
        let tag = if footer.contains(&AddonAction::Update) {
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
                        .child(tr!("addons.card.by", author = entry.author()))
                }))
                .children(tag),
        )
        .child(h_flex().w_full().gap_2().children(buttons))
        .into_any_element()
}
