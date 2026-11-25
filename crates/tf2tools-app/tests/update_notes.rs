use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::updater::{NotesState, Offer, UpdateState, Updater, whats_new};

use crate::support;
use support::ShellWindow;
use support::update_fake::{
    FakeUpdates, HISTORY, fake_updater, offer_with_notes, reach_available, sample_notes, state,
};

struct Dialog {
    _env: support::Env,
    s: ShellWindow,
    u: Entity<Updater>,
    fake: FakeUpdates,
}

// The updater reaches Available with `offer`, then the dialog opens on it.
fn dialog(cx: &mut TestAppContext, offer: Offer) -> Dialog {
    let env = support::env();
    let (u, fake) = fake_updater(cx, true);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    reach_available(cx, &u, &fake, offer.clone());
    cx.update_window(s.win, |_, window, cx| whats_new::open(offer, window, cx))
        .unwrap();
    support::settle(cx, s.win);
    Dialog {
        _env: env,
        s,
        u,
        fake,
    }
}

fn label(
    cx: &mut TestAppContext,
    d: &Dialog,
    id: impl Into<gpui_kit::ElementId>,
) -> Option<String> {
    support::label_of(cx, d.s.win, id)
}

fn some(s: &str) -> Option<String> {
    Some(s.to_owned())
}

#[gpui_kit::test]
fn the_dialog_lists_versions_newest_first_with_their_groups(cx: &mut TestAppContext) {
    let d = dialog(
        cx,
        offer_with_notes("6.0.2", true, sample_notes(NotesState::Complete)),
    );
    assert_eq!(label(cx, &d, ids::UPDATE_NOTES_TITLE), some("What's new"));
    assert_eq!(
        label(cx, &d, ids::UPDATE_NOTES_SINCE),
        some("Changes since v6.0.0, newest first.")
    );
    assert!(!support::present(cx, d.s.win, ids::UPDATE_NOTES_ENGLISH));
    assert_eq!(label(cx, &d, ids::update_notes_release(0)), some("v6.0.2"));
    assert_eq!(label(cx, &d, ids::update_notes_date(0)), some("2026-11-20"));
    assert_eq!(
        label(cx, &d, ids::update_notes_intro(0)),
        some("Faster stores.")
    );
    assert_eq!(
        label(cx, &d, ids::update_notes_group(0, 0)),
        some("New features")
    );
    assert_eq!(
        label(cx, &d, ids::update_notes_group(0, 1)),
        some("Performance")
    );
    assert_eq!(
        label(cx, &d, ids::update_notes_group(0, 2)),
        some("Odds and ends")
    );
    assert_eq!(
        label(cx, &d, ids::update_notes_item(0, 0, 0)),
        some("Demo browser")
    );
    assert_eq!(label(cx, &d, ids::update_notes_release(1)), some("v6.0.1"));
    assert!(
        !support::present(cx, d.s.win, ids::update_notes_date(1)),
        "an unparsable date is left out"
    );
    assert!(!support::present(cx, d.s.win, ids::update_notes_intro(1)));
    assert_eq!(label(cx, &d, ids::update_notes_group(1, 0)), some("Fixes"));
    assert_eq!(
        label(cx, &d, ids::update_notes_group(1, 1)),
        some("Usability")
    );
    assert_eq!(label(cx, &d, ids::update_notes_group(1, 2)), some("Thanks"));
    let newer = support::bounds_of(cx, d.s.win, ids::update_notes_release(0)).unwrap();
    let older = support::bounds_of(cx, d.s.win, ids::update_notes_release(1)).unwrap();
    assert!(newer.top() < older.top(), "newest first");
    assert!(!support::present(cx, d.s.win, ids::UPDATE_NOTES_PARTIAL));
    assert!(!support::present(
        cx,
        d.s.win,
        ids::UPDATE_NOTES_UNAVAILABLE
    ));
    assert_eq!(label(cx, &d, ids::UPDATE_NOTES_LATER), some("Later"));
    assert_eq!(label(cx, &d, ids::UPDATE_NOTES_UPDATE), some("Update now"));
}

#[gpui_kit::test]
fn partial_notes_say_so_and_link_every_release(cx: &mut TestAppContext) {
    let d = dialog(
        cx,
        offer_with_notes("6.0.3", true, sample_notes(NotesState::Partial)),
    );
    assert_eq!(
        label(cx, &d, ids::UPDATE_NOTES_PARTIAL),
        some("Notes for some versions are missing. See the release page for all of them.")
    );
    assert_eq!(label(cx, &d, ids::update_notes_release(0)), some("v6.0.2"));
    assert_eq!(
        label(cx, &d, ids::UPDATE_NOTES_HISTORY),
        some("Open release page")
    );
    assert_eq!(label(cx, &d, ids::UPDATE_NOTES_UPDATE), some("Update now"));
    support::click_id(cx, d.s.win, ids::UPDATE_NOTES_HISTORY);
    assert_eq!(cx.opened_url().as_deref(), Some(HISTORY));
}

#[gpui_kit::test]
fn notes_that_failed_to_load_still_offer_the_update(cx: &mut TestAppContext) {
    let notes = tf2tools_app::updater::Notes {
        status: NotesState::Unavailable,
        releases: std::sync::Arc::from(Vec::new()),
    };
    let d = dialog(cx, offer_with_notes("6.0.2", true, notes));
    assert_eq!(
        label(cx, &d, ids::UPDATE_NOTES_UNAVAILABLE),
        some("Release notes couldn't be loaded. You can still update.")
    );
    assert!(!support::present(cx, d.s.win, ids::update_notes_release(0)));
    assert!(!support::present(cx, d.s.win, ids::UPDATE_NOTES_HISTORY));
    assert_eq!(label(cx, &d, ids::UPDATE_NOTES_UPDATE), some("Update now"));
}

#[gpui_kit::test]
fn update_now_starts_the_download_and_closes(cx: &mut TestAppContext) {
    let d = dialog(
        cx,
        offer_with_notes("6.0.2", true, sample_notes(NotesState::Complete)),
    );
    support::click_id(cx, d.s.win, ids::UPDATE_NOTES_UPDATE);
    let _download = d.fake.next_download(cx);
    support::settle(cx, d.s.win);
    assert!(!support::present(cx, d.s.win, ids::UPDATE_NOTES));
}

#[gpui_kit::test]
fn later_closes_and_keeps_the_banner(cx: &mut TestAppContext) {
    let offer = offer_with_notes("6.0.2", true, sample_notes(NotesState::Complete));
    let d = dialog(cx, offer.clone());
    support::click_id(cx, d.s.win, ids::UPDATE_NOTES_LATER);
    support::settle(cx, d.s.win);
    assert!(!support::present(cx, d.s.win, ids::UPDATE_NOTES));
    assert!(support::present(cx, d.s.win, ids::UPDATE_BANNER));
    assert_eq!(state(cx, &d.u), UpdateState::Available(offer));
    assert!(!d.fake.download_pending());
}

#[gpui_kit::test]
fn a_copy_that_cannot_update_itself_opens_the_release_page(cx: &mut TestAppContext) {
    let offer = offer_with_notes("6.0.2", false, sample_notes(NotesState::Complete));
    let d = dialog(cx, offer.clone());
    assert_eq!(
        label(cx, &d, ids::UPDATE_NOTES_UPDATE),
        some("Open release page")
    );
    support::click_id(cx, d.s.win, ids::UPDATE_NOTES_UPDATE);
    assert_eq!(cx.opened_url(), Some(offer.release_page));
    assert!(!d.fake.download_pending());
    support::settle(cx, d.s.win);
    assert!(!support::present(cx, d.s.win, ids::UPDATE_NOTES));
}
