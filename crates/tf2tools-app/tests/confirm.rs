use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext, px};
use tf2tools_app::shell::confirm::{Confirm, confirm};

use crate::support;

fn open(
    cx: &mut TestAppContext,
    spec: Confirm,
) -> (AnyWindowHandle, async_channel::Receiver<bool>) {
    support::init(cx);
    let win = support::blank_window(cx);
    let rx = cx
        .update_window(win, |_, w, cx| confirm(spec, w, cx))
        .unwrap();
    support::settle(cx, win);
    (win, rx)
}

#[gpui_kit::test]
fn delete_hud_reads_like_v5(cx: &mut TestAppContext) {
    let (win, _rx) = open(cx, Confirm::delete_hud("budhud"));
    assert!(support::present(cx, win, "confirm-dialog"));
    assert_eq!(
        support::label_of(cx, win, "confirm-title").as_deref(),
        Some("Delete HUD")
    );
    assert_eq!(
        support::label_of(cx, win, "confirm-description").as_deref(),
        Some("Are you sure you want to delete budhud from your library? This cannot be undone.")
    );
    assert!(support::present(cx, win, "confirm-ok"));
    assert!(support::present(cx, win, "confirm-cancel"));
}

#[gpui_kit::test]
fn the_title_and_description_sit_together_like_v5s_header(cx: &mut TestAppContext) {
    let (win, _rx) = open(cx, Confirm::delete_hud("budhud"));
    let title = support::bounds_of(cx, win, "confirm-title").unwrap();
    let description = support::bounds_of(cx, win, "confirm-description").unwrap();
    let cancel = support::bounds_of(cx, win, "confirm-cancel").unwrap();
    assert_eq!(
        description.top() - title.bottom(),
        px(8.),
        "v5's gap-2 header"
    );
    assert_eq!(
        cancel.top() - description.bottom(),
        px(16.),
        "v5's gap-4 content"
    );
}

#[test]
fn delete_sound_reads_like_v5() {
    let spec = Confirm::delete_sound();
    assert_eq!(spec.title.as_ref(), "Delete Sound");
    assert_eq!(
        spec.description.as_ref(),
        "Are you sure you want to delete this sound from your library? This cannot be undone."
    );
    assert_eq!(spec.confirm_label.as_ref(), "Delete");
    assert_eq!(Confirm::delete_hud("x").confirm_label.as_ref(), "Delete");
}

#[gpui_kit::test]
fn ok_answers_true_and_closes(cx: &mut TestAppContext) {
    let (win, rx) = open(cx, Confirm::delete_sound());
    support::click_id(cx, win, "confirm-ok");
    assert_eq!(rx.try_recv(), Ok(true));
    assert!(!support::present(cx, win, "confirm-dialog"));
}

#[gpui_kit::test]
fn cancel_answers_false_and_closes(cx: &mut TestAppContext) {
    let (win, rx) = open(cx, Confirm::delete_sound());
    support::click_id(cx, win, "confirm-cancel");
    assert_eq!(rx.try_recv(), Ok(false));
    assert!(!support::present(cx, win, "confirm-dialog"));
}

#[gpui_kit::test]
fn escape_answers_false(cx: &mut TestAppContext) {
    let (win, rx) = open(cx, Confirm::delete_sound());
    support::press(cx, win, "escape");
    assert_eq!(rx.try_recv(), Ok(false));
    assert!(!support::present(cx, win, "confirm-dialog"));
}

#[gpui_kit::test]
fn enter_does_not_confirm(cx: &mut TestAppContext) {
    let (win, rx) = open(cx, Confirm::delete_sound());
    support::press(cx, win, "enter");
    assert_eq!(rx.try_recv(), Ok(false));
    assert!(!support::present(cx, win, "confirm-dialog"));
}

#[gpui_kit::test]
fn closing_unanswered_errs(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    let (win, rx) = open(cx, Confirm::delete_sound());
    cx.update_window(win, |_, w, cx| w.close_dialog(cx))
        .unwrap();
    support::settle(cx, win);
    assert_eq!(rx.try_recv(), Err(async_channel::TryRecvError::Closed));
}

#[test]
fn delete_addon_reads_like_v5() {
    let spec = Confirm::delete_addon();
    assert_eq!(spec.title.as_ref(), "Delete Addon");
    assert_eq!(
        spec.description.as_ref(),
        "Are you sure you want to remove this addon from your library? If installed, it will also be removed from TF2."
    );
    assert_eq!(spec.confirm_label.as_ref(), "Delete");
}

#[test]
fn the_profile_confirms_read_like_v5() {
    let delete = Confirm::delete_profile("Competitive");
    assert_eq!(delete.title.as_ref(), "Delete Profile");
    assert_eq!(
        delete.description.as_ref(),
        "Are you sure you want to delete \"Competitive\"? This cannot be undone."
    );
    assert_eq!(delete.confirm_label.as_ref(), "Delete");
    let empty = Confirm::activate_empty();
    assert_eq!(empty.title.as_ref(), "Activate Empty Profile");
    assert_eq!(
        empty.description.as_ref(),
        "This profile has no customizations. Activating it will remove all installed HUDs, sounds, and crosshairs."
    );
    assert_eq!(empty.confirm_label.as_ref(), "Activate Anyway");
    let running = Confirm::tf2_running();
    assert_eq!(running.title.as_ref(), "TF2 is running");
    assert_eq!(
        running.description.as_ref(),
        "TF2 is running; changes take effect after a restart."
    );
    assert_eq!(running.confirm_label.as_ref(), "Continue");
}

#[gpui_kit::test]
fn the_confirm_is_v5_s_512_px_and_centred(cx: &mut TestAppContext) {
    let (win, _rx) = open(cx, Confirm::delete_addon());
    let b = support::dialog_box(cx, win);
    let view = support::viewport(cx, win);
    assert_eq!(b.size.width, px(512.));
    let off = b.origin.y + b.size.height / 2. - view.height / 2.;
    assert!(off.abs() <= px(1.), "box {b:?} in {view:?}");
}

// v5's AlertDialog content is p-6: 24 px inside the box's 1 px border.
#[gpui_kit::test]
fn the_confirm_is_padded_like_v5(cx: &mut TestAppContext) {
    let (win, _rx) = open(cx, Confirm::delete_addon());
    let b = support::dialog_box(cx, win);
    let title = support::bounds_of(cx, win, "confirm-title").unwrap();
    let ok = support::bounds_of(cx, win, "confirm-ok").unwrap();
    assert_eq!(title.left() - b.left(), px(25.), "{title:?} in {b:?}");
    assert_eq!(title.top() - b.top(), px(25.), "{title:?} in {b:?}");
    assert_eq!(b.right() - ok.right(), px(25.), "{ok:?} in {b:?}");
    assert_eq!(b.bottom() - ok.bottom(), px(25.), "{ok:?} in {b:?}");
}

// p-6 narrows the text by 16 px; the addon delete text keeps the three lines it has at 16 px.
#[gpui_kit::test]
fn the_addon_delete_text_keeps_its_three_lines(cx: &mut TestAppContext) {
    let (win, _rx) = open(cx, Confirm::delete_addon());
    let _ = support::dialog_box(cx, win);
    let d = support::bounds_of(cx, win, "confirm-description").unwrap();
    assert_eq!(d.size.height, px(67.5), "{d:?}");
}

#[gpui_kit::test]
fn discard_changes_offers_discard_and_keep_editing(cx: &mut TestAppContext) {
    let spec = Confirm::discard_changes();
    assert_eq!(
        (
            spec.title.as_ref(),
            spec.description.as_ref(),
            spec.confirm_label.as_ref(),
            spec.cancel_label.as_ref()
        ),
        (
            "Unsaved changes",
            "You have unsaved changes. Discard them?",
            "Discard",
            "Keep editing"
        )
    );
    assert_eq!(Confirm::delete_sound().cancel_label.as_ref(), "Cancel");
    let (win, _rx) = open(cx, Confirm::discard_changes());
    assert_eq!(
        support::label_of(cx, win, "confirm-cancel").as_deref(),
        Some("Keep editing")
    );
    assert_eq!(
        support::label_of(cx, win, "confirm-ok").as_deref(),
        Some("Discard")
    );
}

#[test]
fn delete_crosshair_says_what_happens_to_its_weapons() {
    let cases = [
        (
            0,
            false,
            "Are you sure you want to delete Dot from your library? This cannot be undone.",
        ),
        (
            1,
            false,
            "Are you sure you want to delete Dot from your library? It is used by 1 weapon. It goes back to the default crosshair. This cannot be undone.",
        ),
        (
            0,
            true,
            "Are you sure you want to delete Dot from your library? It is also removed from TF2. This cannot be undone.",
        ),
        (
            3,
            true,
            "Are you sure you want to delete Dot from your library? It is used by 3 weapons. They go back to the default crosshair. It is also removed from TF2. This cannot be undone.",
        ),
    ];
    for (weapons, installed, text) in cases {
        let spec = Confirm::delete_crosshair("Dot", weapons, installed);
        assert_eq!(spec.title.as_ref(), "Delete Crosshair");
        assert_eq!(spec.description.as_ref(), text);
        assert_eq!(spec.confirm_label.as_ref(), "Delete");
        assert_eq!(spec.cancel_label.as_ref(), "Cancel");
    }
}

#[test]
fn clear_profile_crosshairs_asks_before_removing_them_all() {
    let spec = Confirm::clear_profile_crosshairs();
    assert_eq!(spec.title.as_ref(), "Clear Crosshairs");
    assert_eq!(
        spec.description.as_ref(),
        "Remove all crosshairs from this profile?"
    );
    assert_eq!(spec.confirm_label.as_ref(), "Clear all");
    assert_eq!(spec.cancel_label.as_ref(), "Cancel");
}
