//! The Demos.tf header crumb while a demo shows: a "Demos.tf" link back to the list and the
//! demo's name.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, WeakEntity,
    Window, div,
};

use crate::i18n::tr;
use crate::ids;
use crate::pages::demo_viewer::DemoViewerPage;
use crate::shell::route::Page;

use super::DemosTfPage;

/// The header slot's view for the Demos.tf page in viewer mode. It holds the page and the
/// viewer weakly, so Back frees the viewer, and redraws as the viewer's title changes.
pub struct DemosTfCrumb {
    page: WeakEntity<DemosTfPage>,
    viewer: WeakEntity<DemoViewerPage>,
    _observe: Subscription,
}

impl DemosTfCrumb {
    /// A crumb for `viewer`, whose link sends `page` back to its list.
    pub fn new(
        page: WeakEntity<DemosTfPage>,
        viewer: &gpui_kit::Entity<DemoViewerPage>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            page,
            viewer: viewer.downgrade(),
            _observe: cx.observe(viewer, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for DemosTfCrumb {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(viewer) = self.viewer.upgrade() else {
            return h_flex().into_any_element();
        };
        let title = viewer.read(cx).title();
        let muted = cx.theme().muted_foreground;
        let page = self.page.clone();
        let back = Button::new(ids::DEMOS_TF_CRUMB)
            .ghost()
            .small()
            .label(Page::DemosTf.label())
            .accessibility_label(tr!("demos_tf.back"))
            .on_click(move |_, _, cx| {
                let _ = page.update(cx, |page, cx| page.back(cx));
            });
        h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2()
            .text_lg()
            .child(div().flex_none().child(back))
            .child(div().flex_none().text_color(muted).child("\u{203a}"))
            .child(
                div()
                    .id(ids::DEMOS_TF_CRUMB_NAME)
                    .aria_label(title.clone())
                    .test_support()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .into_any_element()
    }
}
