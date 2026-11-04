//! Attaching a part to a sibling from the side panel.

mod support;

use gpui_kit::TestAppContext;
use support::maker_page::{IMAGE, VALUE, click_in_panel, editor, on_page, open_maker};
use tf2tools_app::ids;
use tf2tools_core::{NewPart, PartKey, PinCorner, PinRefused};

#[test]
fn attach_and_detach_are_one_step_each() {
    let mut e = editor();
    e.select(Some(PartKey::new(VALUE)));
    let image = PartKey::new(IMAGE);
    assert!(e.pin_targets_selected().contains(&image));
    e.pin_selected(&image, PinCorner::TopLeft, PinCorner::BottomLeft)
        .unwrap();
    assert_eq!(
        e.pin_of_selected(),
        Some((
            "PlayerStatusHealthImage".to_owned(),
            PinCorner::TopLeft,
            PinCorner::BottomLeft
        ))
    );
    e.unpin_selected().unwrap();
    assert_eq!(e.pin_of_selected(), None);
    assert!(e.undo());
    assert!(e.pin_of_selected().is_some());
    e.select(Some(image.clone()));
    assert!(
        !e.pin_targets_selected().contains(&PartKey::new(VALUE)),
        "that would close a loop"
    );
    assert_eq!(
        e.pin_selected(&PartKey::new(VALUE), PinCorner::TopLeft, PinCorner::TopLeft),
        Err(PinRefused::Cycle)
    );
}

#[gpui_kit::test]
fn the_panel_attaches_moves_corners_and_detaches(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(PartKey::new(VALUE)));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    let pin =
        |cx: &mut TestAppContext| cx.update(|cx| page.read(cx).editor().unwrap().pin_of_selected());
    click_in_panel(cx, &s, ids::HUD_MAKER_ATTACH.into());
    support::click_id(cx, s.win, ids::hud_maker_attach_to(&PartKey::new(IMAGE)));
    assert_eq!(
        pin(cx).map(|p| p.0),
        Some("PlayerStatusHealthImage".to_owned())
    );
    click_in_panel(cx, &s, ids::hud_maker_pin_corner(PinCorner::BottomRight));
    assert_eq!(pin(cx).map(|p| p.2), Some(PinCorner::BottomRight));
    click_in_panel(cx, &s, ids::HUD_MAKER_DETACH.into());
    assert_eq!(pin(cx), None);
}

#[gpui_kit::test]
fn deleting_an_anchor_something_is_attached_to_says_so(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    let value = PartKey::new(VALUE);
    let mut anchor = None;
    on_page(cx, &s, &page, |p, _, cx| {
        let e = p.editor_mut().unwrap();
        e.select(Some(value.clone()));
        let a = e.add_part_selected(NewPart::Anchor).unwrap();
        e.select(Some(value.clone()));
        e.pin_selected(&a, PinCorner::TopLeft, PinCorner::TopLeft)
            .unwrap();
        e.select(Some(a.clone()));
        anchor = Some(a);
        cx.notify();
    });
    click_in_panel(cx, &s, ids::HUD_MAKER_DELETE_PART.into());
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t == "Detach the parts attached to it first.")
    );
    assert!(cx.update(|cx| {
        page.read(cx)
            .editor()
            .unwrap()
            .model()
            .part(anchor.as_ref().unwrap())
            .is_some()
    }));
}
