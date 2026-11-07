use crate::support;

use std::sync::Arc;

use gpui_kit::TestSupportExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, ImageSource, ImgResourceLoader, TestAppContext, Window, div, img, px,
};
use tf2tools_app::previews::{CrosshairPreviews, DECODE_BATCH, Decoder, Preview, to_render_image};
use tf2tools_app::services::Services;
use tf2tools_core::{CrosshairId, CrosshairRef};

fn previews(cx: &mut TestAppContext, env: &support::Env) -> (Services, Entity<CrosshairPreviews>) {
    support::init(cx);
    let services = support::services(env, support::no_steam());
    support::install_services(cx, services.clone());
    let previews = cx.update(CrosshairPreviews::global);
    (services, previews)
}

fn drain(cx: &mut TestAppContext, previews: &Entity<CrosshairPreviews>) {
    support::wait_real(cx, "the decodes", |cx| {
        cx.update(|cx| previews.read(cx).pending() == 0)
    });
}

fn preview_of(
    cx: &mut TestAppContext,
    previews: &Entity<CrosshairPreviews>,
    crosshair: &CrosshairRef,
) -> Preview {
    cx.update(|cx| previews.read(cx).preview(crosshair, cx))
}

#[test]
fn to_render_image_swaps_red_and_blue() {
    let image = to_render_image(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    assert_eq!(image.as_bytes(0).unwrap(), &[3, 2, 1, 4, 7, 6, 5, 8]);
    let size = image.size(0);
    assert_eq!((i32::from(size.width), i32::from(size.height)), (2, 1));
    assert!(to_render_image(2, 2, vec![0; 8]).is_none());
}

#[gpui_kit::test]
fn the_cache_is_one_entity_per_session(cx: &mut TestAppContext) {
    let env = support::env();
    let (_, first) = previews(cx, &env);
    let second = cx.update(CrosshairPreviews::global);
    assert_eq!(first.entity_id(), second.entity_id());
}

#[gpui_kit::test]
fn a_bundled_builtin_previews_from_its_png_without_a_decode(cx: &mut TestAppContext) {
    let env = support::env();
    let (services, previews) = previews(cx, &env);
    let first = CrosshairRef::Builtin(services.builtins.all()[0].name().to_owned());
    assert!(matches!(
        preview_of(cx, &previews, &first),
        Preview::Ready(ImageSource::Resource(_))
    ));
    previews.update(cx, |p, cx| p.request([first], cx));
    assert_eq!(cx.update(|cx| previews.read(cx).pending()), 0);
    assert_eq!(cx.update(|cx| previews.read(cx).decodes()), 0);
}

#[gpui_kit::test]
fn a_library_crosshair_decodes_once(cx: &mut TestAppContext) {
    let env = support::env();
    let (services, previews) = previews(cx, &env);
    let dot = support::seed_crosshair(&services, &env, "Dot");
    let dot = CrosshairRef::Library(dot.id().clone());
    assert!(matches!(preview_of(cx, &previews, &dot), Preview::Pending));
    previews.update(cx, |p, cx| p.request([dot.clone(), dot.clone()], cx));
    drain(cx, &previews);
    match preview_of(cx, &previews, &dot) {
        Preview::Ready(ImageSource::Render(image)) => {
            let size = image.size(0);
            assert_eq!((i32::from(size.width), i32::from(size.height)), (64, 64));
        }
        _ => panic!("the decoded VTF"),
    }
    previews.update(cx, |p, cx| p.request([dot], cx));
    assert_eq!(cx.update(|cx| previews.read(cx).pending()), 0);
    assert_eq!(cx.update(|cx| previews.read(cx).decodes()), 1);
}

#[gpui_kit::test]
fn an_unknown_crosshair_ends_missing(cx: &mut TestAppContext) {
    let env = support::env();
    let (_, previews) = previews(cx, &env);
    let unknown = CrosshairRef::Builtin("nope".into());
    let gone = CrosshairRef::Library(CrosshairId::new("gone-1a2b3c4d"));
    previews.update(cx, |p, cx| p.request([unknown.clone(), gone.clone()], cx));
    drain(cx, &previews);
    assert!(matches!(
        preview_of(cx, &previews, &unknown),
        Preview::Missing
    ));
    assert!(matches!(preview_of(cx, &previews, &gone), Preview::Missing));
}

#[gpui_kit::test]
fn more_than_a_batch_all_decode(cx: &mut TestAppContext) {
    let env = support::env();
    let (services, previews) = previews(cx, &env);
    let count = DECODE_BATCH + 8;
    let refs: Vec<CrosshairRef> = (0..count)
        .map(|i| {
            let entry = support::seed_crosshair(&services, &env, &format!("Dot {i:02}"));
            CrosshairRef::Library(entry.id().clone())
        })
        .collect();
    previews.update(cx, |p, cx| p.request(refs.clone(), cx));
    drain(cx, &previews);
    for r in &refs {
        assert!(
            matches!(preview_of(cx, &previews, r), Preview::Ready(_)),
            "{r:?}"
        );
    }
    assert_eq!(cx.update(|cx| previews.read(cx).decodes()), count);
}

struct Shown(Vec<(&'static str, ImageSource)>);

impl Render for Shown {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().children(self.0.iter().map(|(id, source)| {
            div()
                .id(*id)
                .test_support()
                .size(px(64.))
                .child(img(source.clone()).size(px(64.)))
        }))
    }
}

#[gpui_kit::test]
fn both_kinds_of_preview_render_headlessly(cx: &mut TestAppContext) {
    let env = support::env();
    let (services, previews) = previews(cx, &env);
    let builtin = CrosshairRef::Builtin(services.builtins.all()[0].name().to_owned());
    let dot = support::seed_crosshair(&services, &env, "Dot");
    let dot = CrosshairRef::Library(dot.id().clone());
    previews.update(cx, |p, cx| p.request([dot.clone()], cx));
    drain(cx, &previews);
    let (Preview::Ready(png), Preview::Ready(decoded)) = (
        preview_of(cx, &previews, &builtin),
        preview_of(cx, &previews, &dot),
    ) else {
        panic!("both previews ready");
    };
    let ImageSource::Resource(resource) = png.clone() else {
        panic!("a bundled PNG is a file resource");
    };
    let (win, _) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |_, cx| {
                cx.new(|_| Shown(vec![("png", png), ("decoded", decoded)]))
            })
        })
        .unwrap();
    // `img` loads a path through this loader; wait until it has decoded the PNG, not merely drawn.
    support::wait_real(cx, "the bundled PNG to decode", |cx| {
        support::settle(cx, win);
        cx.update_window(win, |_, window, cx| {
            matches!(
                window.get_asset::<ImgResourceLoader>(&resource, cx),
                Some(Ok(_))
            )
        })
        .unwrap()
    });
    assert!(support::present(cx, win, "png"));
    assert!(support::present(cx, win, "decoded"));
}

#[gpui_kit::test]
fn a_panicking_decode_marks_only_itself_missing(cx: &mut TestAppContext) {
    let env = support::env();
    let (_, previews) = previews(cx, &env);
    let refs: Vec<CrosshairRef> = ["dot-00000001", "boom-00000002", "ring-00000003"]
        .into_iter()
        .map(|id| CrosshairRef::Library(CrosshairId::new(id)))
        .collect();
    let boom = refs[1].clone();
    // The middle crosshair panics as a corrupt VTF would inside the decoder.
    let decoder: Decoder = Arc::new(move |_: &Services, c: &CrosshairRef| {
        assert_ne!(c, &boom, "a corrupt VTF");
        to_render_image(1, 1, vec![0, 0, 0, 255])
    });
    previews.update(cx, |p, _| p.set_decoder(decoder));
    previews.update(cx, |p, cx| p.request(refs.clone(), cx));
    drain(cx, &previews);
    assert!(matches!(
        preview_of(cx, &previews, &refs[0]),
        Preview::Ready(_)
    ));
    assert!(matches!(
        preview_of(cx, &previews, &refs[1]),
        Preview::Missing
    ));
    assert!(matches!(
        preview_of(cx, &previews, &refs[2]),
        Preview::Ready(_)
    ));
    assert_eq!(cx.update(|cx| previews.read(cx).decodes()), 3);
}
