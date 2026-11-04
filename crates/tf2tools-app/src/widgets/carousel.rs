//! A screenshot carousel (one image, looping Previous / Next) and the lightbox dialog around it.
//! Hand-rolled because the kit's carousel fixes its control ids and runs scroll timers.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, FontWeight, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, Render, SharedString, Styled as _, StyledImage as _, Subscription,
    TestSupportExt as _, Window, div, img, px,
};

use crate::ids;
use crate::remote_images::{ImageKey, RemoteImage, RemoteImages};
use crate::widgets::dialog_frame::DialogFrame;
use crate::widgets::labelled;

/// Shows one of `urls` at a time through `RemoteImages`, wanting the shown and the next image.
pub struct Carousel {
    urls: Vec<String>,
    index: usize,
    viewer: String,
    _subscriptions: Vec<Subscription>,
}

impl Carousel {
    /// Starts on the first URL. Without a `RemoteImages` global it draws only placeholders.
    pub fn new(urls: Vec<String>, cx: &mut Context<Self>) -> Self {
        let viewer = format!("carousel-{}", cx.entity_id());
        let mut subscriptions = Vec::new();
        // Images redraw the carousel as they land; its wants go when it does.
        if let Some(images) = RemoteImages::global(cx) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let gone = viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&gone, Vec::new(), cx));
            }));
        }
        let carousel = Self {
            urls,
            index: 0,
            viewer,
            _subscriptions: subscriptions,
        };
        carousel.want(cx);
        carousel
    }

    /// Replaces the images and goes back to the first.
    pub fn set_urls(&mut self, urls: Vec<String>, cx: &mut Context<Self>) {
        self.urls = urls;
        self.index = 0;
        self.changed(cx);
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn len(&self) -> usize {
        self.urls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.urls.is_empty()
    }

    /// Steps forward, from the last back to the first.
    pub fn next(&mut self, cx: &mut Context<Self>) {
        if !self.is_empty() {
            self.index = (self.index + 1) % self.len();
            self.changed(cx);
        }
    }

    /// Steps back, from the first round to the last.
    pub fn prev(&mut self, cx: &mut Context<Self>) {
        if !self.is_empty() {
            self.index = (self.index + self.len() - 1) % self.len();
            self.changed(cx);
        }
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.want(cx);
        cx.notify();
    }

    // Asks for the shown image and the next one, so a step forward rarely waits.
    fn want(&self, cx: &mut Context<Self>) {
        let Some(images) = RemoteImages::global(cx) else {
            return;
        };
        let mut keys = Vec::new();
        if let Some(url) = self.urls.get(self.index) {
            keys.push(ImageKey::carousel(url.clone()));
        }
        if self.len() > 1 {
            let next = &self.urls[(self.index + 1) % self.len()];
            keys.push(ImageKey::carousel(next.clone()));
        }
        let viewer = self.viewer.clone();
        images.update(cx, |images, cx| images.want(&viewer, keys, cx));
    }
}

impl Render for Carousel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (muted, muted_foreground) = (cx.theme().muted, cx.theme().muted_foreground);
        let Some(url) = self.urls.get(self.index) else {
            return labelled(
                div()
                    .w_full()
                    .py_8()
                    .flex()
                    .justify_center()
                    .text_sm()
                    .text_color(muted_foreground),
                ids::CAROUSEL_EMPTY,
                "No screenshots",
            )
            .into_any_element();
        };
        let image = RemoteImages::global(cx)
            .map(|images| images.read(cx).image(&ImageKey::carousel(url.clone())));
        let mut area = div()
            .id(ids::CAROUSEL_IMAGE)
            .test_support()
            .relative()
            .w_full()
            .aspect_ratio(16. / 9.)
            .overflow_hidden()
            .rounded_md()
            .bg(muted);
        area = match image {
            Some(RemoteImage::Ready(image)) => {
                area.child(img(image).size_full().object_fit(ObjectFit::Contain))
            }
            Some(RemoteImage::Failed) => area
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(muted_foreground)
                .child("Screenshot unavailable"),
            _ => area,
        };
        // The controls float over the image's edges, and only when there is somewhere to go.
        if self.len() > 1 {
            area = area.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .child(
                        Button::new(ids::CAROUSEL_PREV)
                            .icon(IconName::ChevronLeft)
                            .outline()
                            .on_click(cx.listener(|this, _, _, cx| this.prev(cx))),
                    )
                    .child(
                        Button::new(ids::CAROUSEL_NEXT)
                            .icon(IconName::ChevronRight)
                            .outline()
                            .on_click(cx.listener(|this, _, _, cx| this.next(cx))),
                    ),
            );
        }
        area.into_any_element()
    }
}

/// Opens a wide dialog (`ids::LIGHTBOX`) titled `title` with a carousel over `urls`.
pub fn open_lightbox(title: SharedString, urls: Vec<String>, window: &mut Window, cx: &mut App) {
    // Deferred, as confirm does, so the click that opened it cannot pull focus back out.
    window.defer(cx, move |window, cx| {
        let carousel = cx.new(|cx| Carousel::new(urls, cx));
        let frame = DialogFrame::new(px(960.));
        // The title lives in the measured content, so the centring counts its height.
        window.open_dialog(cx, move |dialog, window, cx| {
            frame.apply(dialog, &*window, &*cx).child(
                frame.measure(
                    v_flex()
                        .id(ids::LIGHTBOX)
                        .test_support()
                        .w_full()
                        .gap_4()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title.clone()),
                        )
                        .child(carousel.clone()),
                ),
            )
        });
    });
}
