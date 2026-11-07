//! The Start from card over a new, unchanged HUD: a ready-made HUD (its starting points open in
//! the card and a pick previews like any recipe), one of the player's HUDs (the HUDs library,
//! where each HUD opens here) or Blank, which keeps TF2's own HUD.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use tf2tools_core::StartingPoint;

use super::HudMakerPage;
use crate::ids;
use crate::shell::route::{NavigateTo, Page};

const CARD_WIDTH: f32 = 380.;

/// Whether the player closed the card for this page, and whether it lists the starting points.
#[derive(Debug, Default)]
pub(super) struct StartCard {
    closed: bool,
    points: bool,
}

impl HudMakerPage {
    /// Whether the Start from card shows: a new HUD with no changes, no preview running, no part
    /// selected and the card not closed. The six-screen view has no room for it.
    pub fn shows_start_card(&self) -> bool {
        !self.start_card.closed
            && !self.every_screen
            && self.can_start_from()
            && self
                .editor()
                .is_some_and(|e| !e.previewing() && e.selected().is_none())
    }

    /// Closes the card for this page when `picked` is true.
    pub(super) fn close_start_card_on(&mut self, picked: bool) {
        self.start_card.closed |= picked;
    }

    // The card centred over the screen, in a wrapper that takes no presses, so the canvas
    // around the card still works.
    pub(super) fn start_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.shows_start_card() {
            return None;
        }
        let room = self.screen_area();
        let at = room.origin - self.area.get().origin;
        let theme = cx.theme();
        let card = v_flex()
            .id(ids::HUD_MAKER_START_CARD)
            .test_support()
            .occlude()
            .w(px(CARD_WIDTH))
            .max_w(room.size.width)
            .p_4()
            .gap_3()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .shadow_lg()
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Where do you want to start?"),
            )
            .child(self.start_choice(
                ids::HUD_MAKER_START_READY.into(),
                IconName::Sparkles,
                "A ready-made HUD",
                "A look to start from, which you change your way.",
                cx.listener(|page, _, _, cx| {
                    page.start_card.points = !page.start_card.points;
                    cx.notify();
                }),
                cx,
            ))
            .children(self.start_card.points.then(|| self.start_points(cx)))
            .child(self.start_choice(
                ids::HUD_MAKER_START_MINE.into(),
                IconName::Library,
                "One of my HUDs",
                "Pick a HUD in your library and press Edit in HUD Maker.",
                cx.listener(|_, _, _, cx| cx.emit(NavigateTo(Page::Huds))),
                cx,
            ))
            .child(self.start_choice(
                ids::HUD_MAKER_START_BLANK.into(),
                IconName::File,
                "Blank",
                "TF2's own HUD, nothing changed yet.",
                cx.listener(|page, _, _, cx| {
                    page.start_card.closed = true;
                    cx.notify();
                }),
                cx,
            ));
        Some(
            div()
                .absolute()
                .left(at.x)
                .top(at.y)
                .w(room.size.width)
                .h(room.size.height)
                .flex()
                .items_center()
                .justify_center()
                .child(card)
                .into_any_element(),
        )
    }

    // One choice: an icon, its name and a line saying what it does, the whole row one button.
    fn start_choice(
        &self,
        id: ElementId,
        icon: IconName,
        name: &'static str,
        about: &'static str,
        on_click: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (hover, muted, border) = (theme.accent, theme.muted_foreground, theme.border);
        h_flex()
            .id(id)
            .aria_label(name)
            .test_support()
            .gap_3()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .hover(move |el| el.bg(hover))
            .on_click(on_click)
            .child(Icon::new(icon))
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(name))
                    .child(div().text_xs().text_color(muted).child(about)),
            )
            .into_any_element()
    }

    // The four starting points under "A ready-made HUD"; a pick previews it, built on the
    // untouched HUD, and closes the card.
    fn start_points(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let points = StartingPoint::ALL.map(|which| {
            let pick = Button::new(ids::hud_maker_start_card_point(which.label()))
                .outline()
                .small()
                .label(which.label())
                .on_click(cx.listener(move |page, _, window, cx| {
                    let Some(recipe) = page.editor().map(|e| e.pristine().starting_point(which))
                    else {
                        return;
                    };
                    page.start_card.closed = true;
                    page.start_preview(recipe, 0, window, cx);
                }));
            v_flex()
                .gap_1()
                .items_start()
                .child(pick)
                .child(div().text_xs().text_color(muted).child(which.about()))
        });
        v_flex()
            .pl(px(36.))
            .gap_2()
            .children(points)
            .into_any_element()
    }
}
