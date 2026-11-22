//! What a settings row draws of its pictures: the thumbnail of its current value, which
//! opens and closes them, and, while the row is open, its pictures panel: the preset's
//! picture and yours behind a wipe, with the wipe slider under it and the zoom toggle in its
//! corner, and the strip that picks a value by its picture.

use std::path::Path;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div, hsla, px, relative, transparent_black,
};

use tf2tools_core::ModuleChoice;

use super::GraphicsChoicesEditor;
use super::model::preset_label;
use super::pictures::{Compare, RowPictures};
use crate::i18n::tr;
use crate::ids::{self, IdScope};
use crate::pages::huds::rounded_picture;
use crate::remote_images::{ImageKey, RemoteImage, RemoteImages};
use crate::widgets::{Named as _, labelled};

/// The thumbnail's size, the pictures' 16:9.
const THUMB_WIDTH: f32 = 96.;
const THUMB_HEIGHT: f32 = 54.;
/// A strip value card's picture, also 16:9.
const STRIP_WIDTH: f32 = 112.;
/// The compare view's widest size; it shrinks with a narrow editor.
const COMPARE_MAX_WIDTH: f32 = 720.;
/// The compare view's corner radius.
const COMPARE_RADIUS: f32 = 8.;
/// The row under the compare picture that holds the wipe slider.
const SLIDER_HEIGHT: f32 = 24.;
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
    radius: Pixels,
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
            .rounded(radius)
            .into_any_element()
    } else {
        rounded_picture(area, image, key, Some(radius), cx).into_any_element()
    }
}

/// The current value's picture as the button that opens or closes the row's pictures,
/// named "Show pictures" or "Hide pictures". With no picture for the value the slot stays
/// empty; a picture that failed to decode leaves an unfilled box, so the row keeps its shape.
pub(super) fn thumbnail(
    editor: &GraphicsChoicesEditor,
    module: &str,
    pictures: &RowPictures,
    cx: &mut Context<GraphicsChoicesEditor>,
) -> AnyElement {
    let s = editor.scope;
    let key = editor
        .pictures
        .as_deref()
        .zip(pictures.thumb.as_deref())
        .map(|(index, thumb)| ImageKey::card(&index.url(thumb)));
    let picture = rounded(
        s.id(ids::graphics_thumb_area(module)),
        s.id(ids::graphics_thumb_image(module)),
        key.as_ref(),
        px(7.),
        cx,
    );
    let open = editor.open_row() == Some(module);
    let (frame, label) = if open {
        (cx.theme().foreground, tr!("graphics.pictures.hide"))
    } else {
        (cx.theme().border, tr!("graphics.pictures.show"))
    };
    let target = module.to_owned();
    // The 1 px frame sits on a child, since the kit's hover style replaces the Button's own
    // border colour; the picture is rounded 7 px so its corners stay within the 8 px arc.
    Button::new(s.id(ids::graphics_thumb(module)))
        .ghost()
        .p_0()
        .flex_none()
        .h(px(THUMB_HEIGHT + 2.))
        .rounded(px(8.))
        .named(label)
        .on_click(
            cx.listener(move |editor, _, window, cx| editor.toggle_pictures(&target, window, cx)),
        )
        .child(
            div()
                .w(px(THUMB_WIDTH + 2.))
                .border_1()
                .border_color(frame)
                .rounded(px(8.))
                .child(picture),
        )
        .into_any_element()
}

/// The open row's pictures panel, the card's full width on the page's darker fill: the
/// compare view with the zoom toggle in its corner and the wipe slider under it, then the
/// strip of value pictures and its caption.
pub(super) fn panel(
    editor: &GraphicsChoicesEditor,
    module: &str,
    pictures: &RowPictures,
    cx: &mut Context<GraphicsChoicesEditor>,
) -> AnyElement {
    let s = editor.scope;
    let zoom = pictures.has_zoom.then(|| {
        let label = if editor.zoom() {
            tr!("graphics.pictures.full")
        } else {
            tr!("graphics.pictures.zoom")
        };
        Button::new(s.id(ids::graphics_zoom(module)))
            .ghost()
            .flex_none()
            .h(px(26.))
            .px(px(10.))
            .rounded_full()
            .bg(hsla(0., 0., 0., 0.8))
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(0., 0., 1., 1.))
            .label(label)
            .on_click(cx.listener(|editor, _, _, cx| editor.toggle_zoom(cx)))
    });
    let theme = cx.theme();
    let (fill, border) = (theme.background, theme.border);
    // The bottom rule overlaps the next row's top rule, so the two never double.
    v_flex()
        .id(s.id(ids::graphics_pictures(module)))
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
        .children(compare(editor, module, &pictures.compare, zoom, cx))
        .when(!pictures.strip.is_empty(), |panel| {
            panel
                .child(strip(editor, module, pictures, cx))
                .child(labelled(
                    div().text_xs().text_color(cx.theme().muted_foreground),
                    s.id(ids::graphics_strip_caption(module)),
                    tr!("graphics.pictures.caption"),
                ))
        })
        .into_any_element()
}

// One card per value with a picture, in catalogue order: its picture over its label, the
// chosen one outlined in the primary colour and the preset's tagged with the preset's name,
// or "Preset" when that name repeats the value's label.
fn strip(
    editor: &GraphicsChoicesEditor,
    module: &str,
    pictures: &RowPictures,
    cx: &mut Context<GraphicsChoicesEditor>,
) -> AnyElement {
    let s = editor.scope;
    let preset = preset_label(editor.choices.preset());
    let theme = cx.theme();
    let (ring, picked, border, muted) = (
        theme.primary,
        theme.secondary,
        theme.border,
        theme.muted_foreground,
    );
    let buttons = pictures.strip.iter().map(|entry| {
        let key = editor
            .pictures
            .as_deref()
            .map(|index| ImageKey::card(&index.url(&entry.picture)));
        let image = rounded(
            s.id(ids::graphics_strip_area(module, &entry.value)),
            s.id(ids::graphics_strip_image(module, &entry.value)),
            key.as_ref(),
            px(5.),
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
                .id(s.id(ids::graphics_strip_tag(module)))
                .aria_label(text.clone())
                .test_support()
                .text_color(muted)
                .child(text)
        });
        let (target, value) = (module.to_owned(), entry.value.clone());
        Button::new(s.id(ids::graphics_strip_picture(module, &entry.value)))
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
            .disabled(editor.disabled)
            .on_click(cx.listener(move |editor, _, _, cx| {
                let choice = ModuleChoice::Value(value.clone());
                editor.propose(|c, catalog| c.set(catalog, &target, choice), cx)
            }))
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
        .id(s.id(ids::graphics_strip(module)))
        .test_support()
        .flex_wrap()
        .gap_2()
        .children(buttons)
        .into_any_element()
}

// The compare view. The preset's picture fills the box; on a wipe, an overlay from the wipe
// line to the right edge shows the matching right part of yours, which keeps the box's full
// width so both pictures line up. The slider runs in a row under the picture, reserved even
// without a wipe, so a pick that starts or ends one moves nothing below.
fn compare(
    editor: &GraphicsChoicesEditor,
    module: &str,
    compare: &Compare,
    zoom: Option<Button>,
    cx: &mut Context<GraphicsChoicesEditor>,
) -> Option<AnyElement> {
    let s = editor.scope;
    let index = editor.pictures.as_deref()?;
    let key = |path: &Path| ImageKey::carousel(&index.url(path));
    // Each side rounds its own corners, since gpui clips children only to rectangles.
    let side = |name: &str, path: &Path, cx: &App| {
        rounded(
            s.id(ids::graphics_compare_area(module, name)),
            s.id(ids::graphics_compare_image(module, name)),
            Some(&key(path)),
            px(COMPARE_RADIUS),
            cx,
        )
    };
    let frame = div()
        .id(s.id(ids::graphics_compare(module)))
        .test_support()
        .absolute()
        .inset_0()
        .rounded(px(COMPARE_RADIUS))
        .overflow_hidden();
    let full = |child: AnyElement| div().absolute().inset_0().size_full().child(child);
    let corner = |zoom: Option<Button>| {
        div()
            .absolute()
            .right(px(10.))
            .bottom(px(10.))
            .children(zoom)
    };
    let (view, slider) = match compare {
        Compare::None => return None,
        Compare::Same {
            picture,
            pill: text,
        } => (
            frame
                .child(full(side("preset", picture, cx)))
                .child(pill(s, module, "same", text, false))
                .child(corner(zoom)),
            None,
        ),
        Compare::Single {
            picture,
            pill: text,
            yours,
        } => {
            let name = if *yours { "yours" } else { "preset" };
            (
                frame
                    .child(full(side(name, picture, cx)))
                    .child(pill(s, module, name, text, *yours))
                    .child(corner(zoom)),
                None,
            )
        }
        Compare::Wipe {
            preset,
            yours,
            preset_pill,
            yours_pill,
        } => {
            let w = editor.wipe(cx) / 100.;
            // The line sits apart from the clip: a border would narrow the clip's content box
            // and so the width yours takes from it.
            let line = (w < WIPE_ALL_PRESET).then(|| {
                div()
                    .id(s.id(ids::graphics_compare_line(module)))
                    .test_support()
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
                    .id(s.id(ids::graphics_compare_clip(module)))
                    .test_support()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(relative(1. - w))
                    .overflow_hidden()
                    .child(
                        div()
                            .id(s.id(ids::graphics_compare_yours(module)))
                            .test_support()
                            .absolute()
                            .top_0()
                            .right_0()
                            .h_full()
                            .w(relative(1. / (1. - w)))
                            .child(side("yours", yours, cx)),
                    )
            });
            // The track spans the picture's full width, so the thumb's centre is the line's x.
            let slider = div()
                .id(s.id(ids::graphics_compare_slider(module)))
                .aria_label(tr!("graphics.pictures.wipe"))
                .test_support()
                .w_full()
                .h(px(SLIDER_HEIGHT))
                .flex()
                .items_center()
                .child(
                    Slider::new(editor.wipe_state())
                        .accessibility_label(tr!("graphics.pictures.wipe")),
                )
                .into_any_element();
            (
                frame
                    .child(full(side("preset", preset, cx)))
                    .children(clip)
                    .children(line)
                    .child(pill(s, module, "preset", preset_pill, false))
                    .child(pill(s, module, "yours", yours_pill, true))
                    .child(corner(zoom)),
                Some(slider),
            )
        }
    };
    // In a row the stage's width settles on the main axis before its column lays out, so
    // the 16:9 picture box takes its height from the capped width. The slider sits outside
    // the frame's clip, so a thumb at either end is drawn whole.
    let row = slider.unwrap_or_else(|| div().w_full().h(px(SLIDER_HEIGHT)).into_any_element());
    let stage = v_flex()
        .relative()
        .flex_1()
        .max_w(px(COMPARE_MAX_WIDTH))
        .gap_2()
        .child(div().relative().w_full().aspect_ratio(16. / 9.).child(view))
        .child(row);
    Some(h_flex().w_full().mt_3().child(stage).into_any_element())
}

// A label over a compare picture's top corner: left for the preset's side, right for yours.
fn pill(
    s: IdScope,
    module: &str,
    side: &str,
    text: &SharedString,
    right: bool,
) -> impl IntoElement {
    div()
        .id(s.id(ids::graphics_compare_pill(module, side)))
        .aria_label(text.clone())
        .test_support()
        .absolute()
        .top(px(10.))
        .map(|pill| {
            if right {
                pill.right(px(10.))
            } else {
                pill.left(px(10.))
            }
        })
        .px_2()
        .py_0p5()
        .rounded_full()
        .bg(hsla(0., 0., 0., 0.8))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(hsla(0., 0., 1., 1.))
        .child(text.clone())
}
