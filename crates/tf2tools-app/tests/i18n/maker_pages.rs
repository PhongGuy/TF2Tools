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
