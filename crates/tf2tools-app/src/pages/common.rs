//! Building blocks the pages share: v5's card frame, badges, empty state and skeletons.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, EventEmitter, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, Pixels, Role, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt as _, Window, div, px, rgb,
    transparent_black,
};

use crate::ids;
use crate::shell::route::{NavigateTo, Page};
use crate::widgets::labelled;

/// v5's size-sm button with an icon (h-8 px-2.5): kit Medium height with this padding.
pub(super) const SM_ICON_PX: Pixels = px(10.);

/// Tailwind orange-500, v5's HUD conflict box and melee slot colour.
pub(crate) const ORANGE_500: u32 = 0xf97316;

/// Tailwind yellow-500, v5's sound and crosshair conflicts colour.
pub(super) const YELLOW_500: u32 = 0xeab308;

// v5's shadcn Card frame: a bordered, rounded panel on the group-box background.
pub(crate) fn card_frame(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.group_box)
        .shadow_sm()
}

// shadcn's Badge, as the header draws it, labelled with its text for the tests and the pilot.
pub(crate) fn badge_of(
    id: impl Into<ElementId>,
    tag: Tag,
    border: Hsla,
    text: impl Into<SharedString>,
) -> impl IntoElement {
    let text = text.into();
    status(id, text.clone()).child(pill(tag, border, text, false))
}

/// `badge_of` for a store card's tag row: capped at the row's width, an overlong tag ends in an
/// ellipsis instead of running out. Status badges elsewhere keep their full text.
pub(crate) fn tag_badge(
    id: impl Into<ElementId>,
    tag: Tag,
    border: Hsla,
    text: impl Into<SharedString>,
) -> impl IntoElement {
    let text = text.into();
    status(id, text.clone())
        .min_w_0()
        .max_w_full()
        .overflow_hidden()
        .child(pill(tag, border, text, true))
}

// The badge's labelled wrapper, a status the tests and the pilot find by its text.
fn status(
    id: impl Into<ElementId>,
    text: SharedString,
) -> impl IntoElement + ParentElement + Styled {
    div()
        .id(id)
        .role(Role::Status)
        .aria_label(text)
        .test_support()
}

/// `badge_of`'s pill height: text-xs at 1.25 leading, py-0.5 and the 1 px border.
pub(crate) const BADGE_HEIGHT: Pixels = px(21.);

/// v5's `flex flex-wrap` badge row, `gap` apart both ways, capped at `rows` rows: a badge that
/// would start a row past the cap is clipped away whole. Test-supported under `id`.
pub(crate) fn badge_rows(
    id: impl Into<ElementId>,
    rows: usize,
    gap: Pixels,
) -> impl IntoElement + ParentElement + Styled {
    // Rows sit BADGE_HEIGHT + gap apart, so one past the cap starts below max_h and shows nothing.
    div()
        .id(id)
        .test_support()
        .flex()
        .flex_wrap()
        .gap(gap)
        .min_w_0()
        .max_h(BADGE_HEIGHT * rows as f32 + gap * rows.saturating_sub(1) as f32)
        .overflow_hidden()
}

/// v5's FilterBadge: `badge_of`'s 21 px pill, filled primary with a transparent border while
/// `active`, outline (foreground text, border colour) otherwise; a button whose a11y `selected`
/// is `active`, on the tab order with Enter and Space as a click. The caller adds the click.
pub(crate) fn filter_badge<I: Into<ElementId>, T: Into<SharedString>>(
    id: I,
    active: bool,
    text: T,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement + StatefulInteractiveElement + Styled + use<I, T> {
    let id = id.into();
    let text = text.into();
    let focus = window
        .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle().tab_stop(true))
        .read(cx)
        .clone();
    let theme = cx.theme();
    let (tag, border) = if active {
        (Tag::primary(), transparent_black())
    } else {
        (
            Tag::secondary().outline().text_color(theme.foreground),
            theme.border,
        )
    };
    // v5's focus-visible ring tints the border; a click does not take focus, as on the kit Button.
    let border = if focus.is_focused(window) {
        theme.ring
    } else {
        border
    };
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(text.clone())
        .aria_selected(active)
        .test_support()
        .track_focus(&focus)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .child(pill(tag, border, text, false))
}

/// Wraps `child` in a same-bounds div whose hover lifts opacity by 1 / 0.9, cancelling the kit
/// Tag's own 0.9 hover dim (nested opacities multiply), since v5's span badges never dim.
pub(crate) fn steady(child: impl IntoElement) -> Div {
    div().hover(|style| style.opacity(1. / 0.9)).child(child)
}

// The shadcn Badge body both badges share: a rounded tag, px-2 py-0.5, medium text, steady;
// `truncate` lets it shrink below its text and end in an ellipsis.
fn pill(tag: Tag, border: Hsla, text: SharedString, truncate: bool) -> Div {
    let tag = tag
        .rounded_full()
        .border_color(border)
        .px_2()
        .py_0p5()
        .font_weight(FontWeight::MEDIUM);
    steady(if truncate {
        tag.min_w_0().child(div().truncate().child(text))
    } else {
        tag.child(text)
    })
}

/// The hover reason on an action that writes into the game while no TF2 folder is known.
pub(crate) const NEEDS_TF2: &str = "Set the TF2 folder in Settings first";

/// Shuts `button`, an action that writes into the game, while `has_tf2` is false and says why
/// on hover. Call it after the button's own `disabled`, which it would otherwise lose to.
pub(crate) fn needs_tf2(button: Button, has_tf2: bool) -> Button {
    if has_tf2 {
        button
    } else {
        button.disabled(true).tooltip(NEEDS_TF2)
    }
}

/// v5's orange "TF2 path not set" alert with `body` below the title; the caller decides when.
pub(crate) fn no_tf2_warning(id: &'static str, body: &'static str, cx: &App) -> AnyElement {
    tf2_alert(id, body, None, cx)
}

/// `no_tf2_warning` with an "Open Settings" button (`settings_id`) that asks the shell for
/// the Settings page, for pages whose actions wait on the TF2 folder.
pub(crate) fn no_tf2_notice<T: EventEmitter<NavigateTo>>(
    id: &'static str,
    settings_id: &'static str,
    body: &'static str,
    cx: &mut Context<T>,
) -> AnyElement {
    let settings = Button::new(settings_id)
        .outline()
        .small()
        .label("Open Settings")
        .on_click(cx.listener(|_, _, _, cx| cx.emit(NavigateTo(Page::Settings))));
    tf2_alert(id, body, Some(settings.into_any_element()), cx)
}

fn tf2_alert(
    id: &'static str,
    body: &'static str,
    action: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let orange: Hsla = rgb(ORANGE_500).into();
    let muted = cx.theme().muted_foreground;
    h_flex()
        .id(id)
        .role(Role::Alert)
        .aria_label("TF2 path not set")
        .test_support()
        .items_start()
        .gap_3()
        .p_4()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(orange.opacity(0.5))
        .bg(orange.opacity(0.1))
        .child(
            Icon::new(IconName::TriangleAlert)
                .with_size(Size::Size(px(20.)))
                .text_color(orange)
                .mt_0p5()
                .flex_none(),
        )
        .child(
            v_flex()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(orange)
                        .child("TF2 path not set"),
                )
                .child(div().text_sm().text_color(muted).child(body))
                .children(action.map(|action| div().mt_3().child(action))),
        )
        .into_any_element()
}

/// v5's EmptyState in a Card (`STORE_EMPTY_CARD`): a 48 px `icon` (`STORE_EMPTY_ICON`), the
/// title labelled `title_id`, then the muted description, labelled `description_id` when given.
pub(crate) fn empty_state_card(
    title_id: &'static str,
    icon: IconName,
    title: impl Into<SharedString>,
    description_id: Option<&'static str>,
    description: &'static str,
    cx: &App,
) -> AnyElement {
    empty_state_with_action(title_id, icon, title, description_id, description, None, cx)
}

/// `empty_state_card` with v5's EmptyState `action` (a button) 16 px under the description.
pub(crate) fn empty_state_with_action(
    title_id: &'static str,
    icon: IconName,
    title: impl Into<SharedString>,
    description_id: Option<&'static str>,
    description: &'static str,
    action: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let text = div()
        .mt_1()
        .max_w(px(384.))
        .text_sm()
        .text_center()
        .text_color(muted);
    let description = match description_id {
        Some(id) => labelled(text, id, description).into_any_element(),
        None => text.child(description).into_any_element(),
    };
    // v5 pads three layers: the Card by py-6, its CardContent and the EmptyState by py-12 each.
    card_frame(cx)
        .id(ids::STORE_EMPTY_CARD)
        .test_support()
        .w_full()
        .py_6()
        .child(
            v_flex().px_6().py_12().items_center().child(
                v_flex()
                    .py_12()
                    .items_center()
                    .child(
                        div().id(ids::STORE_EMPTY_ICON).test_support().mb_4().child(
                            Icon::new(icon)
                                .with_size(Size::Size(px(48.)))
                                .text_color(muted),
                        ),
                    )
                    .child(labelled(
                        div().text_lg().font_weight(FontWeight::SEMIBOLD),
                        title_id,
                        title,
                    ))
                    .child(description)
                    .children(action.map(|action| div().mt_4().child(action))),
            ),
        )
        .into_any_element()
}

/// What tops a skeleton card, as v5's store cards are topped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkeletonTop {
    /// Nothing: the addon cards.
    None,
    /// A 16:9 image block: the HUD and crosshair cards.
    Image,
    /// A muted py-6 band around a 40 px square: the sound cards' icon header.
    Icon,
}

/// `count` pulsing placeholder cards (`ids::skeleton_card(i)`) in a `columns`-wide grid labelled
/// `label` under `id`, each topped by `top`.
pub(crate) fn skeleton_cards(
    id: &'static str,
    label: impl Into<SharedString>,
    count: usize,
    columns: usize,
    top: SkeletonTop,
    cx: &App,
) -> AnyElement {
    let radius = cx.theme().radius;
    let (muted, muted_foreground) = (cx.theme().muted, cx.theme().muted_foreground);
    // v5's skeleton Card: an optional image block, a title bar, two text bars, a button bar.
    let card = |ix: usize| {
        card_frame(cx)
            .id(ids::skeleton_card(ix))
            .test_support()
            .overflow_hidden()
            .flex()
            .flex_col()
            .map(|card| match top {
                SkeletonTop::None => card,
                SkeletonTop::Image => {
                    card.child(Skeleton::new().w_full().h_auto().aspect_ratio(16. / 9.))
                }
                SkeletonTop::Icon => card.child(
                    h_flex().justify_center().py_6().bg(muted).child(
                        Skeleton::new()
                            .size_10()
                            .rounded(radius)
                            .bg(muted_foreground.opacity(0.1)),
                    ),
                ),
            })
            .child(
                v_flex()
                    .p_6()
                    .gap_6()
                    .child(Skeleton::new().h_5().w(px(128.)).rounded(radius))
                    .child(
                        v_flex()
                            .gap_2()
                            .child(Skeleton::new().rounded(radius))
                            .child(Skeleton::new().w_2_3().rounded(radius)),
                    )
                    .child(Skeleton::new().h_8().rounded(radius)),
            )
    };
    div()
        .id(id)
        .role(Role::Status)
        .aria_label(label.into())
        .test_support()
        .grid()
        .grid_cols(columns as u16)
        .gap_4()
        .children((0..count).map(card))
        .into_any_element()
}
