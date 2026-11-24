//! The Demos.tf page: a list of demos that opens the chosen one in the 3D viewer.

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, TestSupportExt as _, Window, div,
};

use crate::i18n::tr;
use crate::ids;
use crate::pages::demo_viewer::DemoViewerPage;
use crate::picker::PickKind;
use crate::services::Services;
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::widgets::labelled;

/// What the page shows: the list, or one demo in the viewer.
enum Mode {
    List,
    Viewer(Entity<DemoViewerPage>),
}

/// The Demos.tf page: starts on the list and swaps to the viewer once a demo is opened.
pub struct DemosTfPage {
    mode: Mode,
}

impl DemosTfPage {
    /// A page on its list.
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { mode: Mode::List }
    }

    /// The viewer, while a demo shows.
    pub fn viewer(&self) -> Option<&Entity<DemoViewerPage>> {
        match &self.mode {
            Mode::Viewer(viewer) => Some(viewer),
            Mode::List => None,
        }
    }

    /// Hiding drops the viewer, freeing its renderer and pointer lock, so a return starts on
    /// the list.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if hidden && self.viewer().is_some() {
            self.mode = Mode::List;
            cx.notify();
        }
    }

    // Asks for a demo from the computer; a cancelled pick stays on the list.
    fn open_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picking = cx.global::<Services>().picker.pick(PickKind::OpenDemo, cx);
        cx.spawn_in(window, async move |page, cx| {
            let Some(path) = picking.await else { return };
            let _ = page.update_in(cx, |page, window, cx| {
                let viewer = cx.new(|cx| DemoViewerPage::open_path(path, window, cx));
                page.mode = Mode::Viewer(viewer);
                cx.notify();
            });
        })
        .detach();
    }

    fn list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let open = Button::new(ids::DEMOS_TF_OPEN)
            .outline()
            .small()
            .label(tr!("demos_tf.open"))
            .on_click(cx.listener(|page, _, window, cx| page.open_demo(window, cx)));
        v_flex()
            .id(ids::DEMOS_TF_LIST)
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(labelled(
                        div().min_w_0().text_sm().text_color(muted),
                        ids::DEMOS_TF_COMING,
                        tr!("demos_tf.coming"),
                    ))
                    .child(div().flex_none().child(open)),
            )
    }
}

impl Render for DemosTfPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.mode {
            Mode::List => self.list(cx).into_any_element(),
            Mode::Viewer(viewer) => viewer.clone().into_any_element(),
        };
        div()
            .id(ids::page(Page::DemosTf))
            .test_support()
            .size_full()
            .child(body)
    }
}
