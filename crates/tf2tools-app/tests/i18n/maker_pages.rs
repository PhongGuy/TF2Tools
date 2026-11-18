//! The HUD Maker's panels, menus and dialogs in Russian, read by their English ids.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Entity, ScrollDelta, TestAppContext, point, px};
use tf2tools_app::i18n::names::maker_label;
use tf2tools_app::i18n::plural;
use tf2tools_app::ids;
use tf2tools_app::local_messages::pictures_left_out;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::pages::hud_maker::animations::anim_sentence;
use tf2tools_app::pages::hud_maker::editor::{Align, HudEditor};
use tf2tools_app::pages::hud_maker::live::{live_failure, live_note};
use tf2tools_app::pages::hud_maker::shortcuts;
use tf2tools_core::{AnimCommand, Interp, PartKey, ReinstallError, Scene, SituationSwitch};

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

// An editor over the stock HUD holding `keys`, added as Shift-clicks would.
fn holding(keys: &[&str]) -> HudEditor {
    let mut e = HudEditor::new(support::stock_model(), Vec::new());
    for key in keys {
        e.toggle_select(PartKey::new(*key));
    }
    e
}

#[test]
fn animation_rows_read_as_russian_sentences() {
    let _guard = LocaleGuard::switch("ru");
    let fade = AnimCommand::Animate {
        panel: "P".into(),
        field: "Alpha".into(),
        value: "0".into(),
        interp: Interp::Linear,
        start: 0.2,
        duration: 0.2,
    };
    assert_eq!(
        anim_sentence(&fade),
        "Меняет прозрачность до 0 за 0,2\u{a0}с, начиная с 0,2\u{a0}с"
    );
    let run = AnimCommand::RunEvent {
        event: "Loop".into(),
        delay: 0.25,
    };
    assert_eq!(anim_sentence(&run), "Запускает Loop в 0,25\u{a0}с");
}

#[gpui_kit::test]
fn the_animations_section_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, page) = open_maker(cx);
    select(cx, &s, &page, &[VALUE]);
    assert_eq!(
        panel_label(cx, &s, ids::hud_maker_anim_row("HudHealthDyingPulse", 0)).as_deref(),
        Some("Меняет цвет на HUDRedTeamSolid за 0,1\u{a0}с, начиная с 0\u{a0}с")
    );
}

#[test]
fn undo_steps_are_named_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let mut e = holding(&[VALUE]);
    e.layer_selected(false).unwrap();
    let layer = format!("смена слоя «{}»", maker_label("Health value"));
    assert_eq!(e.undo_name(), Some(layer.as_str()));
    let mut e = holding(&[VALUE, MEDIC]);
    e.align_selected(Align::Left).unwrap();
    assert_eq!(e.undo_name(), Some("выравнивание частей: 2"));
    let mut e = holding(&[VALUE, MEDIC]);
    e.hide_selected().unwrap();
    assert_eq!(e.undo_name(), Some("скрытие частей: 2"));
}

#[test]
fn live_update_lines_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(
        live_note(),
        "Установленный HUD обновлён. Введите hud_reloadscheme в консоли TF2, чтобы увидеть \
         изменения. Шрифты, картинки и анимации обновятся после перезапуска TF2."
    );
    let changed = ReinstallError::Changed {
        folder: "rayshud".into(),
    };
    assert_eq!(
        live_failure(&changed),
        "Сохранено, но в установленном HUD есть свои изменения, поэтому он не обновлён. \
         Установите этот HUD снова, чтобы выбрать, что делать."
    );
}

#[gpui_kit::test]
fn the_situation_menu_and_the_parts_list_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_maker(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_SITUATION).as_deref(),
        Some("Солдат, здоровье 150")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    assert_eq!(
        support::label_of(
            cx,
            s.win,
            ids::hud_maker_situation_switch(SituationSwitch::Typing)
        )
        .as_deref(),
        Some("Набор в чате")
    );
    support::press(cx, s.win, "escape");
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    let row = PartKey::new(VALUE);
    support::reveal_part_row(cx, s.win, &row);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row_eye(&row)).as_deref(),
        Some("Скрыть эту часть")
    );
}

#[gpui_kit::test]
fn the_start_card_zoom_and_shortcuts_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_maker(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_START_BLANK).as_deref(),
        Some("С нуля")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_START_BLANK);
    support::click_id(cx, s.win, ids::HUD_MAKER_ZOOM);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_ZOOM_FIT).as_deref(),
        Some("Вписать")
    );
    support::press(cx, s.win, "escape");
    support::click_id(cx, s.win, ids::HUD_MAKER_SHORTCUTS);
    let rows: Vec<String> = (0..shortcuts().len())
        .filter_map(|i| support::label_of(cx, s.win, ids::hud_maker_shortcut(i)))
        .collect();
    for want in [
        "Стрелки: Сдвинуть выбранные части",
        "Escape: Снять выделение",
        "Ctrl+Z: Отменить",
        "Тянуть по пустому месту: Выделить все части в рамке",
    ] {
        assert!(rows.iter().any(|r| r == want), "{want} not in {rows:?}");
    }
}

// Ids are built from core's and the app's English, so the pilot and the English tests find
// the same elements in Russian.
#[gpui_kit::test]
fn maker_ids_stay_english_under_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, page) = open_maker(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_START_READY);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_start_card_point("Competitive minimal")
    ));
    support::click_id(cx, s.win, ids::HUD_MAKER_START_BLANK);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert_eq!(
        cx.update(|cx| page.read(cx).scene()),
        Some(Scene::Scoreboard)
    );
    support::click_scene(cx, s.win, Scene::Hud);
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_vision("Green-blind")
    ));
    support::press(cx, s.win, "escape");
    select(cx, &s, &page, &[VALUE]);
    assert!(panel_label(cx, &s, ids::hud_maker_align_button("south-east")).is_some());
}

// At 800x600 the scene bar keeps its tabs inside the bar or falls back to its menu. The side
// panel's title width is checked offline in Inter: the headless font cannot show it.
#[gpui_kit::test]
fn the_russian_maker_fits_at_800_by_600(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_maker(cx);
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    support::settle(cx, s.win);
    let bar = support::bounds_of(cx, s.win, ids::HUD_MAKER_SCENES).unwrap();
    match support::bounds_of(cx, s.win, ids::HUD_MAKER_SCENE_TABS) {
        Some(tabs) => assert!(tabs.right() <= bar.right(), "{tabs:?} past {bar:?}"),
        None => assert!(support::present(cx, s.win, ids::HUD_MAKER_SCENE_MENU)),
    }
}

// Reopening a HUD that held too many borrowed pictures says so in the player's language.
#[test]
fn the_pictures_left_out_note_reads_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(pictures_left_out(0), None);
    assert_eq!(
        pictures_left_out(1).as_deref(),
        Some("Часть взятых картинок не вошла: в этом HUD их было больше, чем он может хранить.")
    );
}

// The kill feed's sample names stay as the game shows them; the sentence around them is Russian.
#[gpui_kit::test]
fn a_kill_feed_line_is_named_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_maker(cx);
    support::put_start_card_away(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_killfeed_line(1)).as_deref(),
        Some("Market Garden убивает Rocket Jumper")
    );
}
