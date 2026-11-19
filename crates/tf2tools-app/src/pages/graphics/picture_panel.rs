//! What a settings row draws of its pictures: the thumbnail of its current value, the
//! Show/Hide pictures button and, while the row is open, its pictures panel.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    TestSupportExt as _, div, px,
};

use super::GraphicsPage;
use super::pictures::RowPictures;
use crate::i18n::tr;
use crate::ids;
use crate::pages::huds::hud_picture;
use crate::remote_images::{ImageKey, RemoteImage, RemoteImages};
use crate::widgets::Named as _;

/// The thumbnail's size, the pictures' 16:9.
const THUMB_WIDTH: f32 = 96.;
const THUMB_HEIGHT: f32 = 54.;

/// The current value's picture as a button that opens or closes the row's pictures. With no
/// picture for the value the slot stays empty; a picture that failed to decode leaves an
/// unfilled box, so the row keeps its shape either way.
pub(super) fn thumbnail(
    page: &GraphicsPage,
    module: &str,
    label: &str,
    pictures: &RowPictures,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
    let key = page
        .pictures
        .as_deref()
        .zip(pictures.thumb.as_deref())
        .map(|(index, thumb)| ImageKey::card(&index.url(thumb)));
    let failed = key
        .as_ref()
        .zip(RemoteImages::global(cx))
        .is_some_and(|(key, images)| matches!(images.read(cx).image(key), RemoteImage::Failed));
    let picture = if failed {
        div()
            .w_full()
            .aspect_ratio(16. / 9.)
            .border_1()
            .border_color(cx.theme().border)
            .into_any_element()
    } else {
        hud_picture(
            ids::graphics_thumb_area(module),
            ids::graphics_thumb_image(module),
            key.as_ref(),
            cx,
        )
        .into_any_element()
    };
    let target = module.to_owned();
    Button::new(ids::graphics_thumb(module))
        .ghost()
        .p_0()
        .flex_none()
        .h(px(THUMB_HEIGHT))
        .named(tr!("graphics.pictures.thumb", name = label))
        .on_click(cx.listener(move |page, _, window, cx| page.toggle_pictures(&target, window, cx)))
        .child(div().w(px(THUMB_WIDTH)).child(picture))
        .into_any_element()
}

/// "Show pictures", or "Hide pictures" while the row is open.
pub(super) fn toggle(
    page: &GraphicsPage,
    module: &str,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
    let open = page.open_row() == Some(module);
    let label = if open {
        tr!("graphics.pictures.hide")
    } else {
        tr!("graphics.pictures.show")
    };
    let target = module.to_owned();
    Button::new(ids::graphics_pictures_toggle(module))
        .outline()
        .xsmall()
        .label(label)
        .on_click(cx.listener(move |page, _, window, cx| page.toggle_pictures(&target, window, cx)))
        .into_any_element()
}

/// The open row's pictures panel; the compare view and strip fill it.
pub(super) fn panel(module: &str, _pictures: &RowPictures) -> AnyElement {
    v_flex()
        .id(ids::graphics_pictures(module))
        .test_support()
        .w_full()
        .into_any_element()
}
