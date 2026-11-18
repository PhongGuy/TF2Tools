//! The app's frame in Russian: the header and its rings, the shell's dialogs and banners and
//! the parts checklist.

use std::sync::Arc;

use gpui_kit::AppContext as _;
use gpui_kit::{ElementId, TestAppContext};
use tf2tools_app::crash::notice::CrashNotice;
use tf2tools_app::ids;
use tf2tools_app::old_v4::{OldV4Notice, UninstallRunner};
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::payload_checklist::{ChecklistItem, open_map_checklist, open_payload_checklist};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::confirm::Confirm;
use tf2tools_core::{OldV4, StoreKind, Tf2Install, UninstallEntry, find_old_v4};

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

fn texts(c: &Confirm) -> [String; 4] {
    [&c.title, &c.description, &c.confirm_label, &c.cancel_label].map(|t| t.to_string())
}

#[test]
fn the_shell_dialogs_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(
        texts(&Confirm::discard_changes()),
        [
            "Несохранённые изменения",
            "Есть несохранённые изменения. Отбросить их?",
            "Отбросить",
            "Продолжить редактирование"
        ]
    );
    assert_eq!(
        Confirm::leave_unsaved(true).description,
        "Есть несохранённые изменения. Сохранить их перед уходом?"
    );
    assert_eq!(
        Confirm::leave_unsaved(false).description,
        "Есть несохранённые изменения. Дайте HUD имя, чтобы сохранить его, или отбросьте их."
    );
    assert_eq!(
        texts(&Confirm::remove_old_v4()),
        [
            "Удалить старый TF2Tools",
            "TF2Tools закроется на время удаления старой версии. Откройте его снова, когда всё закончится.",
            "Удалить и закрыть",
            "Отмена"
        ]
    );
    assert_eq!(
        texts(&Confirm::clear_logs()),
        [
            "Очистить журнал",
            "Очистить весь журнал действий? Это действие нельзя отменить.",
            "Очистить",
            "Отмена"
        ]
    );
    assert_eq!(
        tf2tools_app::shell::leave::save_and_leave(),
        "Сохранить и выйти"
    );
}

#[gpui_kit::test]
fn the_crash_banner_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    cx.update(|cx| CrashNotice::install(cx, Some("boom".to_owned())));
    let s = support::shell_window(cx, &env, missing());
    let shown = [
        ids::CRASH_BANNER_TEXT,
        ids::CRASH_BANNER_COPY,
        ids::CRASH_BANNER_REPORT,
        ids::CRASH_BANNER_DISMISS,
    ]
    .map(|id| label(cx, &s, id));
    let want = [
        "В прошлый раз TF2Tools неожиданно закрылся.",
        "Скопировать",
        "Сообщить о проблеме",
        "Закрыть",
    ];
    assert_eq!(shown, want.map(|t| Some(t.to_owned())));
}

fn v4() -> OldV4 {
    let entry = UninstallEntry::new("v4")
        .display_name("TF2Tools 4.8.0")
        .display_version("4.8.0")
        .uninstall_string(r#""C:\tf2tools\Uninstall TF2Tools.exe" /currentuser"#.to_owned());
    find_old_v4(&[entry], |_| true).unwrap()
}

#[gpui_kit::test]
fn the_old_version_banner_and_its_confirm_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let runner: UninstallRunner = Arc::new(|_: &std::path::Path, _: &[String]| Ok(()));
    cx.update(|cx| OldV4Notice::install(cx, Some(v4()), runner));
    let s = support::shell_window(cx, &env, missing());
    assert_eq!(
        label(cx, &s, ids::OLD_V4_BANNER_TEXT).as_deref(),
        Some("Старая версия TF2Tools всё ещё установлена.")
    );
    assert_eq!(
        label(cx, &s, ids::OLD_V4_BANNER_LATER).as_deref(),
        Some("Не сейчас")
    );
    assert_eq!(
        label(cx, &s, ids::OLD_V4_BANNER_REMOVE).as_deref(),
        Some("Удалить")
    );
    support::click_id(cx, s.win, ids::OLD_V4_BANNER_REMOVE);
    assert_eq!(
        label(cx, &s, ids::CONFIRM_TITLE).as_deref(),
        Some("Удалить старый TF2Tools")
    );
}

fn items() -> Vec<ChecklistItem> {
    vec![
        ChecklistItem {
            name: "a.vpk".into(),
            label: "A".into(),
            present: false,
        },
        ChecklistItem {
            name: "b.vpk".into(),
            label: "B".into(),
            present: true,
        },
    ]
}

#[gpui_kit::test]
fn the_parts_checklist_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    support::init(cx);
    let win = support::blank_window(cx);
    let _open = cx
        .update_window(win, |_, w, cx| {
            open_payload_checklist("Toon FX", items(), w, cx)
        })
        .unwrap();
    support::settle(cx, win);
    let shown = [
        ids::PAYLOAD_CHECKLIST_TITLE,
        ids::PAYLOAD_CHECKLIST_DESCRIPTION,
        ids::PAYLOAD_CANCEL,
        ids::PAYLOAD_INSTALL,
    ]
    .map(|id| support::label_of(cx, win, id));
    let want = [
        "Выберите, что установить",
        "У Toon FX несколько частей. Выберите, какие установить.",
        "Отмена",
        "Установить",
    ];
    assert_eq!(shown, want.map(|t| Some(t.to_owned())));
    assert_eq!(
        support::label_of(cx, win, ids::payload_check(1)).as_deref(),
        Some("B · Уже в игре")
    );
}

#[gpui_kit::test]
fn the_map_checklist_says_what_was_skipped_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    support::init(cx);
    let win = support::blank_window(cx);
    let _open = cx
        .update_window(win, |_, w, cx| {
            open_map_checklist("Pack", items(), true, w, cx)
        })
        .unwrap();
    support::settle(cx, win);
    assert_eq!(
        support::label_of(cx, win, ids::PAYLOAD_CHECKLIST_DESCRIPTION).as_deref(),
        Some(
            "В Pack несколько карт. Выберите, какие установить. Часть дополнительных файлов из этой загрузки пропущена."
        )
    );
}
