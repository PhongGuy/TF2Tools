//! What a settings row draws of its pictures: the thumbnail of its current value, the
//! Show/Hide pictures button and, while the row is open, its pictures panel: the preset's
//! picture and yours behind a wipe, the zoom toggle, and the strip that picks a value by
//! its picture.

use std::path::Path;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div, hsla, px, relative, transparent_black,
};

use super::GraphicsPage;
use super::model::preset_label;
use super::pictures::{Compare, RowPictures};
use crate::i18n::tr;
use crate::ids;
use crate::pages::huds::rounded_picture;
use crate::remote_images::{ImageKey, RemoteImage, RemoteImages};
use crate::widgets::{Named as _, labelled};

/// The thumbnail's size, the pictures' 16:9.
const THUMB_WIDTH: f32 = 96.;
const THUMB_HEIGHT: f32 = 54.;
/// A strip value card's picture, also 16:9.
const STRIP_WIDTH: f32 = 112.;
/// The compare view's widest size; it shrinks with a narrow page.
const COMPARE_MAX_WIDTH: f32 = 720.;
/// The compare view's corner radius.
const COMPARE_RADIUS: f32 = 8.;
/// The wipe line, centred on the clip's left edge.
const WIPE_LINE_WIDTH: f32 = 2.;
/// From this wipe on, the player's side is too thin to draw.
const WIPE_ALL_PRESET: f32 = 0.995;

/// A bundled picture with its box and image rounded by `radius`, or a muted box while it
/// decodes. A picture that failed to decode leaves an unfilled box, so its slot keeps its shape.
fn rounded(
    area: ElementId,
    image: ElementId,
    key: Option<&ImageKey>,
    radius: Option<Pixels>,
    cx: &App,
) -> AnyElement {
    let failed = key
        .zip(RemoteImages::global(cx))
        .is_some_and(|(key, images)| matches!(images.read(cx).image(key), RemoteImage::Failed));
    if failed {
        div()
            .id(area)
            .w_full()
            .aspect_ratio(16. / 9.)
            .border_1()
            .border_color(cx.theme().border)
            .when_some(radius, |area, r| area.rounded(r))
            .into_any_element()
    } else {
        rounded_picture(area, image, key, radius, cx).into_any_element()
    }
}

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
    let picture = rounded(
        ids::graphics_thumb_area(module),
        ids::graphics_thumb_image(module),
        key.as_ref(),
        Some(px(7.)),
        cx,
    );
    let target = module.to_owned();
    // A 1 px frame with an 8 px radius, in the accent while the row's pictures are open;
    // the picture inside is rounded 7 px itself so its corners stay within the frame's arc.
    let frame = if page.open_row() == Some(module) {
        cx.theme().primary
    } else {
        cx.theme().border
    };
    Button::new(ids::graphics_thumb(module))
        .ghost()
        .p_0()
        .flex_none()
        .h(px(THUMB_HEIGHT + 2.))
        .border_1()
        .border_color(frame)
        .rounded(px(8.))
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
        .ghost()
        .xsmall()
        .h(px(24.))
        .px_2()
        .font_weight(FontWeight::MEDIUM)
        .label(label)
        .on_click(cx.listener(move |page, _, window, cx| page.toggle_pictures(&target, window, cx)))
        .into_any_element()
}

/// The open row's pictures panel, the card's full width on the page's darker fill: the
/// compare view with the zoom toggle on its corner when the setting has crops, then the
/// strip of value pictures and its caption.
pub(super) fn panel(
    page: &GraphicsPage,
    module: &str,
    pictures: &RowPictures,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
    let zoom = pictures.has_zoom.then(|| {
        let label = if page.zoom() {
            tr!("graphics.pictures.full")
        } else {
            tr!("graphics.pictures.zoom")
        };
        Button::new(ids::graphics_zoom(module))
            .ghost()
            .absolute()
            .right(px(10.))
            .bottom(px(10.))
            .h(px(26.))
            .px(px(10.))
            .rounded_full()
            .bg(hsla(0., 0., 0., 0.8))
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(0., 0., 1., 1.))
            .label(label)
            .on_click(cx.listener(|page, _, _, cx| page.toggle_zoom(cx)))
    });
    let theme = cx.theme();
    let (fill, border) = (theme.background, theme.border);
    // The bottom rule overlaps the next row's top rule, so the two never double.
    v_flex()
        .id(ids::graphics_pictures(module))
        .test_support()
        .w_full()
        .pt_1()
        .px_4()
        .pb_4()
        .gap_3()
        .mb(px(-1.))
        .bg(fill)
        .border_b_1()
        .border_color(border)
        .children(compare(page, module, &pictures.compare, zoom, cx))
        .when(!pictures.strip.is_empty(), |panel| {
            panel
                .child(strip(page, module, pictures, cx))
                .child(labelled(
                    div().text_xs().text_color(cx.theme().muted_foreground),
                    ids::graphics_strip_caption(module),
                    tr!("graphics.pictures.caption"),
                ))
        })
        .into_any_element()
}

// One card per value with a picture, in catalogue order: its picture over its label, the
// chosen one outlined in the primary colour and the preset's carrying the preset's name.
fn strip(
    page: &GraphicsPage,
    module: &str,
    pictures: &RowPictures,
    cx: &mut Context<GraphicsPage>,
) -> AnyElement {
    let preset = preset_label(page.shown_against().preset());
    let theme = cx.theme();
    let (ring, picked, border, muted) = (
        theme.primary,
        theme.secondary,
        theme.border,
        theme.muted_foreground,
    );
    let buttons = pictures.strip.iter().map(|entry| {
        let key = page
            .pictures
            .as_deref()
            .map(|index| ImageKey::card(&index.url(&entry.picture)));
        let image = rounded(
            ids::graphics_strip_area(module, &entry.value),
            ids::graphics_strip_image(module, &entry.value),
            key.as_ref(),
            Some(px(5.)),
            cx,
        );
        // A tag naming the card's own label would read "Medium Medium"; it says "Preset" then.
        let tag = entry.preset.then(|| {
            let text = if entry.label == preset {
                tr!("graphics.preset.heading")
            } else {
                preset.clone()
            };
            div()
                .id(ids::graphics_strip_tag(module))
                .aria_label(text.clone())
                .test_support()
                .text_color(muted)
                .child(text)
        });
        let (target, value) = (module.to_owned(), entry.value.clone());
        Button::new(ids::graphics_strip_picture(module, &entry.value))
            .ghost()
            .pt_1()
            .px_1()
            .pb(px(6.))
            .h_auto()
            .rounded(px(8.))
            .border_1()
            .map(|card| {
                if entry.chosen {
                    card.border_color(ring).bg(picked)
                } else {
                    card.border_color(border).bg(transparent_black())
                }
            })
            .accessibility_label(entry.label.clone())
            .disabled(page.busy)
            .on_click(cx.listener(move |page, _, _, cx| page.pick_value(&target, &value, cx)))
            .child(
                v_flex()
                    .w(px(STRIP_WIDTH))
                    .gap(px(6.))
                    .text_xs()
                    .child(image)
                    .child(
                        h_flex()
                            .px(px(2.))
                            .gap(px(6.))
                            .items_baseline()
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(entry.label.clone()),
                            )
                            .children(tag),
                    ),
            )
    });
    h_flex()
        .id(ids::graphics_strip(module))
        .test_support()
        .flex_wrap()
        .gap_2()
        .children(buttons)
        .into_any_element()
}

// The compare view. The preset's picture fills the box; on a wipe, an overlay from the wipe
// line to the right edge shows the matching right part of yours, which keeps the box's full
// width so both pictures line up.
fn compare(
    page: &GraphicsPage,
    module: &str,
    compare: &Compare,
    zoom: Option<Button>,
    cx: &mut Context<GraphicsPage>,
) -> Option<AnyElement> {
    let index = page.pictures.as_deref()?;
    let key = |path: &Path| ImageKey::carousel(&index.url(path));
    // Each side rounds its own corners, since gpui clips children only to rectangles.
    let side = |name: &str, path: &Path, cx: &App| {
        rounded(
            ids::graphics_compare_area(module, name),
            ids::graphics_compare_image(module, name),
            Some(&key(path)),
            Some(px(COMPARE_RADIUS)),
            cx,
        )
    };
    let frame = div()
        .id(ids::graphics_compare(module))
        .test_support()
        .relative()
        .mt_3()
        .w_full()
        .max_w(px(COMPARE_MAX_WIDTH))
        .aspect_ratio(16. / 9.)
        .rounded(px(COMPARE_RADIUS))
        .overflow_hidden();
    let full = |child: AnyElement| div().absolute().inset_0().size_full().child(child);
    let view = match compare {
        Compare::None => return None,
        Compare::Same {
            picture,
            pill: text,
        } => frame
            .child(full(side("preset", picture, cx)))
            .child(pill(module, "same", text, false))
            .children(zoom),
        Compare::Single {
            picture,
            pill: text,
            yours,
        } => {
            let name = if *yours { "yours" } else { "preset" };
            frame
                .child(full(side(name, picture, cx)))
                .child(pill(module, name, text, *yours))
                .children(zoom)
        }
        Compare::Wipe {
            preset,
            yours,
            preset_pill,
            yours_pill,
        } => {
            let w = page.wipe(cx) / 100.;
            // The line sits apart from the clip: a border would narrow the clip's content box
            // and so the width yours takes from it.
            let line = (w < WIPE_ALL_PRESET).then(|| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(relative(w))
                    .ml(px(-WIPE_LINE_WIDTH / 2.))
                    .w(px(WIPE_LINE_WIDTH))
                    .bg(hsla(0., 0., 1., 1.))
            });
            let clip = (w < WIPE_ALL_PRESET).then(|| {
                div()
                    .id(ids::graphics_compare_clip(module))
                    .test_support()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(relative(1. - w))
                    .overflow_hidden()
                    .child(
                        div()
                            .id(ids::graphics_compare_yours(module))
                            .test_support()
                            .absolute()
                            .top_0()
                            .right_0()
                            .h_full()
                            .w(relative(1. / (1. - w)))
                            .child(side("yours", yours, cx)),
                    )
            });
            let slider = div()
                .id(ids::graphics_compare_slider(module))
                .aria_label(tr!("graphics.pictures.wipe"))
                .test_support()
                .w_full()
                .max_w(px(COMPARE_MAX_WIDTH))
                .child(
                    Slider::new(page.wipe_state())
                        .accessibility_label(tr!("graphics.pictures.wipe")),
                );
            let view = frame
                .child(full(side("preset", preset, cx)))
                .children(clip)
                .children(line)
                .child(pill(module, "preset", preset_pill, false))
                .child(pill(module, "yours", yours_pill, true))
                .children(zoom);
            return Some(
                v_flex()
                    .w_full()
                    .gap_3()
                    .child(view)
                    .child(slider)
                    .into_any_element(),
            );
        }
    };
    Some(view.into_any_element())
}

// A label over a compare picture's top corner: left for the preset's side, right for yours.
fn pill(module: &str, side: &str, text: &SharedString, right: bool) -> impl IntoElement {
    div()
        .id(ids::graphics_compare_pill(module, side))
        .aria_label(text.clone())
        .test_support()
        .absolute()
        .top_2()
        .map(|pill| if right { pill.right_2() } else { pill.left_2() })
        .px_2()
        .py_0p5()
        .rounded_md()
        .bg(hsla(0., 0., 0., 0.6))
        .text_xs()
        .text_color(hsla(0., 0., 1., 1.))
        .child(text.clone())
}
