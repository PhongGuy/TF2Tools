//! The HUD Maker's panels, menus and dialogs in Russian, read by their English ids.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Entity, ScrollDelta, TestAppContext, point};
use tf2tools_app::i18n::plural;
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_core::PartKey;

use crate::LocaleGuard;
use crate::support;
use crate::support::maker_page::{VALUE, on_page, open_maker};

const MEDIC: &str = "HudMedicCharge";

// Selects `keys` as Shift-clicks would, from an empty selection.
fn select(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    keys: &[&str],
) {
    on_page(cx, s, page, |p, _, cx| {
        let e = p.editor_mut().unwrap();
        e.select(None);
        for key in keys {
            e.toggle_select(PartKey::new(*key));
        }
        cx.notify();
    });
}

// The label of `id` in the side panel, wheeling the panel up, then down, until it is drawn.
fn panel_label(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<ElementId>,
) -> Option<String> {
    let id = id.into();
    for step in [3., -3.] {
        for _ in 0..40 {
            if support::present(cx, s.win, id.clone()) {
                return support::label_of(cx, s.win, id);
            }
            let turn = ScrollDelta::Lines(point(0., step));
            cx.update_window(s.win, |_, w, cx| {
                w.scroll(ids::HUD_MAKER_PANEL_BODY, turn, cx)
            })
            .unwrap();
        }
    }
    None
}

#[gpui_kit::test]
fn the_side_panel_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, page) = open_maker(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_EMPTY).as_deref(),
        Some("Нажмите на часть HUD, чтобы изменить её.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_DOCK_LEFT).as_deref(),
        Some("Закрепить слева")
    );
    select(cx, &s, &page, &[VALUE]);
    let shown = [
        panel_label(cx, &s, ids::HUD_MAKER_MIRROR),
        panel_label(cx, &s, ids::HUD_MAKER_FORWARD),
        panel_label(cx, &s, ids::hud_maker_align_button("north-west")),
        panel_label(cx, &s, ids::hud_maker_align_button("east")),
    ];
    let want = ["Отразить", "Поднять", "Вверху слева", "Справа"];
    assert_eq!(shown, want.map(|t| Some(t.to_owned())));
}

#[gpui_kit::test]
fn several_parts_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, page) = open_maker(cx);
    select(cx, &s, &page, &[VALUE, MEDIC]);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SEVERAL).as_deref(),
        Some("Выбрано 2 части. Перетащите любую из них, чтобы сдвинуть все.")
    );
    assert_eq!(
        panel_label(cx, &s, ids::hud_maker_line_up("left")).as_deref(),
        Some("Выровнять по левым краям")
    );
}

#[test]
fn the_several_parts_note_counts_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let said = [2, 5, 21].map(|n| plural("hud_maker.panel.several", n).to_string());
    let tail = "Перетащите любую из них, чтобы сдвинуть все.";
    assert_eq!(
        said,
        [
            format!("Выбрано 2 части. {tail}"),
            format!("Выбрано 5 частей. {tail}"),
            format!("Выбрана 21 часть. {tail}"),
        ]
    );
}

// Names the HUD and saves it, so it has a library folder to keep snapshots in.
fn save_hud(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    let name = cx.update(|cx| page.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        name.update(cx, |input, cx| input.set_value("Мой HUD", window, cx))
    })
    .unwrap();
    support::click_id(cx, s.win, ids::HUD_MAKER_SAVE);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving() && page.read(cx).saved_hud().is_some())
    });
    support::settle(cx, s.win);
}

fn open_history(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    support::click_id(cx, s.win, ids::HUD_MAKER_HISTORY_OPEN);
}

#[gpui_kit::test]
fn the_more_menu_and_an_unsaved_history_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_maker(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    let shown = [
        support::label_of(cx, s.win, ids::HUD_MAKER_BORROW_OPEN),
        support::label_of(cx, s.win, ids::hud_maker_vision("Red-blind")),
    ];
    assert_eq!(
        shown,
        ["Взять часть", "Без красного"].map(|t| Some(t.to_owned()))
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_HISTORY_OPEN);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_HISTORY_UNSAVED).as_deref(),
        Some("Сначала сохраните HUD, и здесь можно будет хранить его снимки.")
    );
}

#[gpui_kit::test]
fn a_saved_history_reads_in_russian_and_a_blank_name_toasts(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, page) = open_maker(cx);
    save_hud(cx, &s, &page);
    open_history(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_HISTORY_EMPTY).as_deref(),
        Some("У этого HUD пока нет снимков.")
    );
    support::maker_page::click_in_panel(cx, &s, ids::HUD_MAKER_SNAPSHOT_SAVE.into());
    support::wait_real(cx, "a toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    let toasts = support::toast_labels(cx, s.win);
    assert!(
        toasts.iter().any(|t| t.contains("Дайте снимку имя.")),
        "{toasts:?}"
    );
}

#[gpui_kit::test]
fn the_borrow_section_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, page) = open_maker(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    support::click_id(cx, s.win, ids::HUD_MAKER_BORROW_OPEN);
    support::wait_real(cx, "the borrowable HUDs", |cx| {
        cx.update(|cx| page.read(cx).borrow_listed())
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_BORROW_EMPTY).as_deref(),
        Some(
            "В библиотеке нет другого HUD, из которого можно взять часть. \
             Сначала скачайте HUD в магазине HUD."
        )
    );
}
