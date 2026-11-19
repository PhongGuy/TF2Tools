//! The updater's views: v5's banner under the header and the Settings "App Updates" card body.

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Entity, FontFeatures, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, Styled as _, TestSupportExt as _, Window, div, px,
};

use super::{UpdateState, Updater, reopen, updater};
use crate::i18n::{format, tr};
use crate::ids;
use crate::shell::leave;
use crate::widgets::Named as _;
use crate::widgets::labelled;

/// v5's banner for the installed updater, or None when it should not show.
pub fn banner(cx: &App) -> Option<AnyElement> {
    let u = updater(cx)?;
    let state = {
        let u = u.read(cx);
        if !u.banner_visible() {
            return None;
        }
        u.state().clone()
    };
    let theme = cx.theme();
    let (icon, text, action) = match &state {
        UpdateState::Available(offer) => {
            let action = if offer.installable {
                action_button(&u, tr!("updater.banner.update"), |u, _, cx| {
                    u.update(cx, |u, cx| u.download(cx));
                })
            } else {
                action_button(&u, tr!("updater.button.release_page"), |u, _, cx| {
                    u.read(cx).open_release_page(cx)
                })
            };
            let text = tr!("updater.banner.available", version = offer.version);
            (IconName::Download, text, Some(action))
        }
        UpdateState::Downloading { .. } => {
            (IconName::Download, tr!("updater.banner.downloading"), None)
        }
        UpdateState::Ready(_) => {
            let action = action_button(&u, tr!("updater.banner.restart"), ask_then_restart);
            let text = tr!("updater.status.ready");
            (IconName::RotateCw, text, Some(action))
        }
        UpdateState::Installing(_) => (IconName::RotateCw, tr!("updater.status.installing"), None),
        // The relaunch never quit the app, so the player reopens it by hand; nothing to click.
        UpdateState::Updated(_) => (IconName::RotateCw, reopen(), None),
        UpdateState::Unavailable
        | UpdateState::Idle
        | UpdateState::Checking
        | UpdateState::UpToDate
        | UpdateState::Failed(_) => return None,
    };
    // While downloading, v5 sizes the text to its words and lets a short bar take the rest.
    let progress = match &state {
        UpdateState::Downloading { percent, .. } => Some((
            Progress::new("update-banner-bar")
                .value(f32::from(*percent))
                .flex_1()
                .h(px(8.))
                .max_w(px(192.)),
            labelled(
                div()
                    .flex_none()
                    .font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)]))),
                ids::UPDATE_BANNER_PERCENT,
                format::percent(percent),
            ),
        )),
        UpdateState::Unavailable
        | UpdateState::Idle
        | UpdateState::Checking
        | UpdateState::UpToDate
        | UpdateState::Available(_)
        | UpdateState::Ready(_)
        | UpdateState::Installing(_)
        | UpdateState::Updated(_)
        | UpdateState::Failed(_) => None,
    };
    // A line too long for the banner is cut short on one line, so the buttons stay in view.
    let line = div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis();
    let text = match progress {
        Some(_) => labelled(line.flex_none(), ids::UPDATE_BANNER_TEXT, text).into_any_element(),
        None => labelled(line.flex_1(), ids::UPDATE_BANNER_TEXT, text).into_any_element(),
    };
    // A download or install in flight cannot be hidden, as in v5.
    let dismissable = !matches!(
        state,
        UpdateState::Downloading { .. } | UpdateState::Installing(_)
    );
    let dismiss = dismissable.then(|| {
        let u = u.clone();
        Button::new(ids::UPDATE_BANNER_DISMISS)
            .ghost()
            .icon(IconName::X)
            .named(tr!("updater.banner.close"))
            // Small draws v5's 14 px X in a 24 px button.
            .small()
            .flex_none()
            .on_click(move |_, _, cx| u.update(cx, |u, cx| u.dismiss(cx)))
    });
    let (bar, percent) = progress.unzip();
    Some(
        h_flex()
            .id(ids::UPDATE_BANNER)
            .test_support()
            .flex_none()
            .w_full()
            .min_h(BANNER_HEIGHT)
            .px(px(24.))
            .gap_3()
            .text_sm()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.primary.opacity(0.05))
            .child(
                Icon::new(icon)
                    .small()
                    .flex_none()
                    .text_color(theme.primary),
            )
            .child(text)
            .children(bar)
            .children(percent)
            .children(action)
            .children(dismiss)
            .into_any_element(),
    )
}

// Restart drops a drawing with unsaved changes, so it asks first; the check runs here rather
// than in the restart hook, which only fires once the update is already applied.
fn ask_then_restart(u: &Entity<Updater>, window: &mut Window, cx: &mut App) {
    let u = u.clone();
    leave::leave_then(window, cx, move |_, cx| {
        u.update(cx, |u, cx| u.restart(cx));
    });
}

// One height in every state, so the page never shifts: the small action button, 8 px above and
// below it, and the bottom border. A minimum, so a larger UI font grows the banner instead of clipping it.
pub(crate) const BANNER_HEIGHT: Pixels = px(41.);

fn action_button(
    u: &Entity<Updater>,
    label: SharedString,
    run: impl Fn(&Entity<Updater>, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let u = u.clone();
    Button::new(ids::UPDATE_BANNER_ACTION)
        .outline()
        .small()
        .label(label)
        .flex_none()
        .on_click(move |_, window, cx| run(&u, window, cx))
        .into_any_element()
}

/// The App Updates card's body: the status line and its buttons; None without an updater.
pub fn settings_content(cx: &App) -> Option<AnyElement> {
    let u = updater(cx)?;
    let state = u.read(cx).state().clone();
    let theme = cx.theme();
    // Each state's line, its colour, and which of check, install and restart it offers; the
    // install slot opens the release page whenever this copy cannot update itself.
    let (text, color, buttons): (SharedString, _, [bool; 3]) = match &state {
        UpdateState::Unavailable => (
            tr!("updater.status.unavailable"),
            None,
            [false, true, false],
        ),
        UpdateState::Idle | UpdateState::UpToDate => {
            (tr!("updater.status.up_to_date"), None, [true, false, false])
        }
        UpdateState::Checking => (
            tr!("updater.status.checking"),
            Some(theme.muted_foreground),
            [false; 3],
        ),
        UpdateState::Available(offer) if offer.installable => (
            tr!("updater.status.available", version = offer.version),
            None,
            [true, true, false],
        ),
        UpdateState::Available(offer) => (
            tr!("updater.status.available_page", version = offer.version),
            None,
            [true, true, false],
        ),
        UpdateState::Downloading { percent, .. } => (
            tr!(
                "updater.status.downloading",
                percent = format::percent(percent)
            ),
            None,
            [false; 3],
        ),
        UpdateState::Ready(_) => (tr!("updater.status.ready"), None, [false, false, true]),
        UpdateState::Installing(_) => (tr!("updater.status.installing"), None, [false; 3]),
        // The relaunch never quit the app, so the player reopens it by hand; nothing to click.
        UpdateState::Updated(_) => (tr!("updater.status.reopen"), None, [false; 3]),
        UpdateState::Failed(failure) => (
            failure.message.text(),
            Some(theme.danger),
            [true, false, false],
        ),
    };
    let installable = matches!(&state, UpdateState::Available(offer) if offer.installable);
    let [show_check, show_install, show_restart] = buttons;
    let check = show_check.then(|| {
        card_button(
            &u,
            ids::SETTINGS_UPDATE_CHECK,
            tr!("updater.button.check"),
            |u, _, cx| {
                u.update(cx, |u, cx| u.check(cx));
            },
        )
    });
    let install = show_install.then(|| {
        if installable {
            card_button(
                &u,
                ids::SETTINGS_UPDATE_INSTALL,
                tr!("updater.button.install"),
                |u, _, cx| {
                    u.update(cx, |u, cx| u.download(cx));
                },
            )
        } else {
            card_button(
                &u,
                ids::SETTINGS_UPDATE_INSTALL,
                tr!("updater.button.release_page"),
                |u, _, cx| u.read(cx).open_release_page(cx),
            )
        }
    });
    let restart = show_restart.then(|| {
        card_button(
            &u,
            ids::SETTINGS_UPDATE_RESTART,
            tr!("updater.button.restart"),
            ask_then_restart,
        )
    });
    let line = match color {
        Some(color) => div().text_color(color),
        None => div(),
    };
    // v5's row holds a short line beside its buttons; a longer one would squeeze into a narrow
    // column, so its buttons drop under it instead.
    let stacked = text.chars().count() > STACK_AFTER;
    let line = labelled(line.text_sm(), ids::SETTINGS_UPDATE_STATUS, text);
    let actions = h_flex()
        .flex_none()
        .gap_2()
        .children(check)
        .children(install)
        .children(restart);
    let body = if stacked {
        v_flex().gap_3().child(line).child(actions)
    } else {
        h_flex()
            .items_center()
            .justify_between()
            .gap_4()
            .child(div().flex_1().min_w_0().child(line))
            .child(actions)
    };
    Some(body.into_any_element())
}

// The longest status line, in characters, that sits beside the card's buttons without wrapping.
const STACK_AFTER: usize = 36;

// v5's outline small button on the card.
fn card_button(
    u: &Entity<Updater>,
    id: &'static str,
    label: SharedString,
    run: impl Fn(&Entity<Updater>, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let u = u.clone();
    Button::new(id)
        .outline()
        .small()
        .label(label)
        .on_click(move |_, window, cx| run(&u, window, cx))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    // The banner, the card and the activity log share these keys, so their English must not drift.
    #[test]
    fn the_card_says_what_the_banner_says() {
        let en = |key: &str| crate::_rust_i18n_try_translate("en", key).map(|t| t.into_owned());
        assert_eq!(
            en("updater.status.unavailable").as_deref(),
            Some(
                "Automatic updates aren't available in this version. Get new versions from the release page."
            )
        );
        assert_eq!(
            en("updater.status.reopen").as_deref(),
            Some("TF2Tools has been updated. Close and reopen it to finish.")
        );
    }
}
