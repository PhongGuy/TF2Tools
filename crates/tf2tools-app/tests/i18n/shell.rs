//! The app's frame in Russian: the header and its rings, the shell's dialogs and banners and
//! the parts checklist.

use gpui_kit::{ElementId, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{StoreKind, Tf2Install};

use crate::LocaleGuard;
use crate::support;

fn missing() -> Tf2State {
    Tf2State::Missing { saved: None }
}

fn label(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<ElementId>,
) -> Option<String> {
    support::label_of(cx, s.win, id)
}

#[gpui_kit::test]
fn the_header_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    cx.update(|cx| Tf2Probe::install(cx, || false));
    let s = support::shell_window(cx, &env, missing());
    assert_eq!(
        label(cx, &s, ids::TF2_BADGE).as_deref(),
        Some("TF2 не найдена")
    );
    assert_eq!(
        label(cx, &s, ids::SIDEBAR_TOGGLE).as_deref(),
        Some("Свернуть или развернуть боковую панель")
    );
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found);
    assert_eq!(
        label(cx, &s, ids::TF2_BADGE).as_deref(),
        Some("TF2 подключена")
    );
    assert_eq!(
        label(cx, &s, ids::PLAY_TF2).as_deref(),
        Some("Играть в TF2")
    );
}

#[gpui_kit::test]
fn the_sync_rings_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, missing());
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Sounds, false, cx))));
    // The run stays bound: dropping it ends the sync and its ring.
    let run = runs.try_recv().unwrap();
    run.report(120, 1000);
    support::next_repaint(cx);
    assert_eq!(
        label(cx, &s, ids::sync_ring(StoreKind::Sounds)).as_deref(),
        Some("Синхронизация звуков: 120 / 1000")
    );
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::CuratedHuds, false, cx))));
    assert_eq!(
        label(cx, &s, ids::sync_ring(StoreKind::CuratedHuds)).as_deref(),
        Some("Синхронизация HUD из подборки...")
    );
}
