//! The status row: mastercomfig's state, the actions it offers and the mastercomfig docs link.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Styled as _, TestSupportExt as _, div, rgb,
};

use super::model::{StatusCard, detail, headline, update_to};
use super::{GraphicsPage, Job};
use crate::i18n::tr;
use crate::ids;
use crate::links::MASTERCOMFIG_DOCS_URL;
use crate::pages::common::{ORANGE_500, card_frame};
use crate::widgets::labelled;

impl GraphicsPage {
    // Headline and detail on the left, the state's actions and the docs link wrapping on the
    // right; the credit sits under the text only while mastercomfig is not installed.
    pub(super) fn status_row(&self, card: &StatusCard, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let busy = self.busy;
        let installing = self.running == Some(Job::Install);
        let checking = self.running == Some(Job::Check);
        let mut buttons: Vec<AnyElement> = Vec::new();
        match card {
            StatusCard::NotInstalled => buttons.push(
                Button::new(ids::GRAPHICS_INSTALL)
                    .primary()
                    .icon(IconName::Download)
                    .label(tr!("graphics.status.install"))
                    .loading(installing)
                    .disabled(busy)
                    .on_click(cx.listener(|page, _, window, cx| page.install(false, window, cx)))
                    .into_any_element(),
            ),
            StatusCard::Installed { .. } => {
                if let Some(tag) = update_to(card) {
                    buttons.push(
                        Button::new(ids::GRAPHICS_UPDATE)
                            .primary()
                            .icon(IconName::RefreshCw)
                            .label(tr!("graphics.status.update_to", version = tag))
                            .loading(installing)
                            .disabled(busy)
                            .on_click(
                                cx.listener(|page, _, window, cx| page.install(true, window, cx)),
                            )
                            .into_any_element(),
                    );
                }
                buttons.push(
                    Button::new(ids::GRAPHICS_CHECK)
                        .outline()
                        .icon(IconName::RotateCw)
                        .label(tr!("graphics.status.check"))
                        .loading(checking)
                        .disabled(busy)
                        .on_click(cx.listener(|page, _, _, cx| page.check(cx)))
                        .into_any_element(),
                );
                buttons.push(
                    Button::new(ids::GRAPHICS_REMOVE)
                        .outline()
                        .icon(IconName::Trash)
                        .label(tr!("graphics.status.remove"))
                        .disabled(busy)
                        .on_click(cx.listener(|page, _, window, cx| page.remove(window, cx)))
                        .into_any_element(),
                );
            }
            StatusCard::Outside { .. } => buttons.push(
                Button::new(ids::GRAPHICS_TAKE_OVER)
                    .primary()
                    .label(tr!("graphics.status.take_over"))
                    .disabled(busy)
                    .on_click(cx.listener(|page, _, window, cx| page.take_over(window, cx)))
                    .into_any_element(),
            ),
        }
        buttons.push(
            Button::new(ids::GRAPHICS_DOCS)
                .link()
                .label(tr!("graphics.status.docs"))
                .on_click(|_, _, cx| cx.open_url(MASTERCOMFIG_DOCS_URL))
                .into_any_element(),
        );
        let credit = matches!(card, StatusCard::NotInstalled).then(|| {
            labelled(
                div().text_xs().text_color(muted),
                ids::GRAPHICS_CREDIT,
                tr!("graphics.status.credit"),
            )
        });
        let text = v_flex()
            .flex_1()
            .min_w_64()
            .gap_1()
            .child(labelled(
                div().text_base().font_weight(FontWeight::SEMIBOLD),
                ids::GRAPHICS_HEADLINE,
                headline(card),
            ))
            .child(labelled(
                div().text_sm().text_color(muted),
                ids::GRAPHICS_DETAIL,
                detail(card),
            ))
            .children(credit);
        card_frame(cx)
            .id(ids::GRAPHICS_STATUS)
            .test_support()
            .px_4()
            .py_3()
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x_4()
                    .gap_y_3()
                    .child(text)
                    .child(
                        h_flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .children(buttons),
                    ),
            )
            .into_any_element()
    }
}

// The amber line under the status row while TF2 runs. It only warns: edits stay enabled and
// take effect when TF2 next reads its config.
pub(super) fn running_banner(cx: &App) -> AnyElement {
    let orange: Hsla = rgb(ORANGE_500).into();
    labelled(
        div()
            .px_4()
            .py_2()
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(orange.opacity(0.5))
            .bg(orange.opacity(0.1))
            .text_sm()
            .text_color(orange),
        ids::GRAPHICS_RUNNING,
        tr!("graphics.running"),
    )
    .into_any_element()
}
