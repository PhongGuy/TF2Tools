use gpui_kit::TestAppContext;
use tf2tools_app::updater::{UpdateState, updater};

use crate::support;
use support::update_fake::{
    CURRENT, RELEASES, act, events, failure, fake_updater, offer, reach_available, state, visible,
    wait_state,
};

#[gpui_kit::test]
fn install_makes_it_the_app_updater(cx: &mut TestAppContext) {
    let (u, _fake) = fake_updater(cx, true);
    let global = cx.update(|cx| updater(cx)).expect("an installed updater");
    assert_eq!(global.entity_id(), u.entity_id());
    assert_eq!(state(cx, &u), UpdateState::Idle);
    assert_eq!(cx.update(|cx| u.read(cx).current().to_owned()), CURRENT);
}

#[gpui_kit::test]
fn without_a_backend_updates_are_unavailable_and_checks_do_nothing(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, false);
    assert_eq!(state(cx, &u), UpdateState::Unavailable);
    assert!(!act(cx, &u, |u, cx| u.check(cx)));
    cx.run_until_parked();
    assert!(!fake.check_pending());
    assert!(!visible(cx, &u));
    cx.update(|cx| u.read(cx).open_release_page(cx));
    assert_eq!(cx.opened_url().as_deref(), Some(RELEASES));
}

#[gpui_kit::test]
fn a_check_runs_in_the_background_and_a_second_is_ignored(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, true);
    assert!(act(cx, &u, |u, cx| u.check(cx)));
    assert_eq!(
        state(cx, &u),
        UpdateState::Checking,
        "check returns before the server answers"
    );
    assert!(!act(cx, &u, |u, cx| u.check(cx)));
    let reply = fake.next_check(cx);
    assert!(
        !fake.check_pending(),
        "the ignored check reached the backend"
    );
    reply.send(Ok(None)).unwrap();
    wait_state(cx, &u, "UpToDate", |s| *s == UpdateState::UpToDate);
}

#[gpui_kit::test]
fn a_newer_version_becomes_available_and_shows_the_banner(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, true);
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    assert_eq!(state(cx, &u), UpdateState::Available(offer("6.0.1", true)));
    assert!(visible(cx, &u));
    cx.update(|cx| u.read(cx).open_release_page(cx));
    assert_eq!(cx.opened_url(), Some(offer("6.0.1", true).release_page));
}

#[gpui_kit::test]
fn a_failed_check_is_shown_in_settings_only(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, true);
    let seen = events(cx, &u);
    act(cx, &u, |u, cx| u.check(cx));
    let f = failure(
        "Couldn't reach the update server. Check your internet connection and try again.",
        false,
    );
    fake.next_check(cx).send(Err(f.clone())).unwrap();
    wait_state(cx, &u, "Failed", |s| matches!(s, UpdateState::Failed(_)));
    assert_eq!(state(cx, &u), UpdateState::Failed(f));
    assert!(seen.borrow().is_empty(), "a check failure raises no toast");
    assert!(!visible(cx, &u));
    assert!(
        act(cx, &u, |u, cx| u.check(cx)),
        "a failed check can be retried"
    );
}

#[gpui_kit::test]
fn dismiss_lasts_the_session_until_a_newer_version(cx: &mut TestAppContext) {
    let (u, fake) = fake_updater(cx, true);
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    act(cx, &u, |u, cx| u.dismiss(cx));
    assert!(!visible(cx, &u));
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    assert!(!visible(cx, &u), "the same version stays dismissed");
    reach_available(cx, &u, &fake, offer("6.0.2", true));
    assert!(visible(cx, &u), "a newer version shows again");
}

#[gpui_kit::test]
fn a_recheck_restores_an_installable_offer_after_a_release_page_only_one(cx: &mut TestAppContext) {
    // A transient download failure leaves the offer release-page-only; checking again must
    // hand the player Download & Install back when the server offers an asset.
    let (u, fake) = fake_updater(cx, true);
    reach_available(cx, &u, &fake, offer("6.0.1", false));
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    assert_eq!(state(cx, &u), UpdateState::Available(offer("6.0.1", true)));
}
