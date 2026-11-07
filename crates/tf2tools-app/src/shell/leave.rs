//! The one unsaved-changes check every way out of a maker (crosshair or HUD) goes through: the
//! sidebar, closing the window and the update's Restart buttons.

use gpui_kit::{App, Global, WeakEntity, Window};

use crate::pages::crosshair_maker::CrosshairMakerPage;
use crate::pages::hud_maker::HudMakerPage;
use crate::shell::confirm::{Choice, Confirm, confirm, confirm_or};

// Cleared on navigation rather than trusted to die with the page, since anything holding the
// entity (a test, a pending task) keeps the weak handle alive after the shell drops it.
#[derive(Default)]
enum OpenMaker {
    #[default]
    None,
    Crosshair(WeakEntity<CrosshairMakerPage>),
    Hud(WeakEntity<HudMakerPage>),
}
impl Global for OpenMaker {}

// Set while the discard dialog waits for an answer, so a repeat request (a second click on the
// window's close button) does not stack another dialog on it.
#[derive(Default)]
struct Asking(bool);
impl Global for Asking {}

/// The open maker, if any; the shell registers it when it builds one and forgets it on navigation.
pub fn register_crosshair_maker(page: WeakEntity<CrosshairMakerPage>, cx: &mut App) {
    cx.set_global(OpenMaker::Crosshair(page));
}

/// Registers the open HUD Maker, as `register_crosshair_maker` does the Crosshair Maker.
pub fn register_hud_maker(page: WeakEntity<HudMakerPage>, cx: &mut App) {
    cx.set_global(OpenMaker::Hud(page));
}

/// Forgets the registered maker, so leaving no longer asks about it.
pub fn forget_maker(cx: &mut App) {
    cx.set_global(OpenMaker::None);
}

/// True unless a registered maker holds unsaved changes.
pub fn can_leave(cx: &App) -> bool {
    match cx.try_global::<OpenMaker>() {
        Some(OpenMaker::Crosshair(page)) => {
            page.upgrade().is_none_or(|page| page.read(cx).can_leave())
        }
        Some(OpenMaker::Hud(page)) => page.upgrade().is_none_or(|page| page.read(cx).can_leave()),
        Some(OpenMaker::None) | None => true,
    }
}

/// Runs `proceed` (deferred to after the current update) when `can_leave`; otherwise opens
/// `Confirm::discard_changes` and runs it on Discard. Keep editing drops it, and so does a
/// request made while that dialog is already open. Leaving the HUD Maker asks with
/// `Confirm::leave_unsaved` instead, whose Save and leave runs `proceed` once the save lands.
pub fn leave_then(
    window: &mut Window,
    cx: &mut App,
    proceed: impl FnOnce(&mut Window, &mut App) + 'static,
) {
    if can_leave(cx) {
        window.defer(cx, proceed);
        return;
    }
    if cx.try_global::<Asking>().is_some_and(|a| a.0) {
        return;
    }
    cx.set_global(Asking(true));
    // The press that asked (a sidebar item) has taken focus off the maker, and the dialog hands
    // focus back to whatever held it at open, so the maker takes it first to get it after Keep editing.
    reclaim_maker_focus(window, cx);
    let hud = match cx.try_global::<OpenMaker>() {
        Some(OpenMaker::Hud(page)) => page.upgrade(),
        _ => None,
    };
    let handle = window.window_handle();
    let Some(hud) = hud else {
        let answer = confirm(Confirm::discard_changes(), window, cx);
        cx.spawn(async move |cx| {
            let discard = answer.recv().await.unwrap_or(false);
            cx.update(|cx| cx.set_global(Asking(false)));
            if discard {
                cx.update(|cx| {
                    let _ = handle.update(cx, |_, window, cx| proceed(window, cx));
                });
            }
        })
        .detach();
        return;
    };
    let can_save = hud.read(cx).can_save(cx);
    let save = can_save.then(|| "Save and leave".into());
    let answer = confirm_or(Confirm::leave_unsaved(can_save), save, window, cx);
    let hud = hud.downgrade();
    cx.spawn(async move |cx| {
        let said = answer.recv().await.unwrap_or(Choice::Stay);
        cx.update(|cx| cx.set_global(Asking(false)));
        cx.update(|cx| {
            let _ = handle.update(cx, |_, window, cx| match said {
                Choice::Stay => {}
                Choice::Confirm => proceed(window, cx),
                Choice::Other => {
                    let _ = hud.update(cx, |page, cx| page.save_then(proceed, window, cx));
                }
            });
        });
    })
    .detach();
}

fn reclaim_maker_focus(window: &mut Window, cx: &mut App) {
    match cx.try_global::<OpenMaker>() {
        Some(OpenMaker::Crosshair(page)) => {
            let _ = page
                .clone()
                .update(cx, |page, cx| page.reclaim_focus(window, cx));
        }
        Some(OpenMaker::Hud(page)) => {
            let _ = page
                .clone()
                .update(cx, |page, cx| page.reclaim_focus(window, cx));
        }
        Some(OpenMaker::None) | None => {}
    }
}

/// The window's should-close answer: true when `can_leave`; otherwise false, with the dialog
/// open and Discard closing the window.
pub fn should_close(window: &mut Window, cx: &mut App) -> bool {
    if can_leave(cx) {
        return true;
    }
    leave_then(window, cx, |window, _| window.remove_window());
    false
}
