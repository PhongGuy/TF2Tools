//! The library pages, the names they share and the navigation, in Russian.

use gpui_kit::{SharedString, TestAppContext};
use tf2tools_app::i18n::{self, names};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::widgets::class_icon::class_short_name;
use tf2tools_core::{SoundKind, TfClass, WeaponSlot};

use crate::LocaleGuard;
use crate::support;

fn classes(name: fn(TfClass) -> SharedString) -> Vec<String> {
    TfClass::ALL.iter().map(|c| name(*c).to_string()).collect()
}

#[test]
fn class_slot_and_sound_names_read_in_both_languages() {
    let _guard = LocaleGuard::hold();
    assert_eq!(names::class_name(TfClass::Heavy), "Heavy");
    assert_eq!(class_short_name(TfClass::Engineer), "Engie");
    assert_eq!(names::slot_name(WeaponSlot::Melee), "Melee");
    assert_eq!(names::sound_kind_name(SoundKind::Killsound), "Killsound");
    i18n::apply("ru");
    assert_eq!(
        classes(names::class_name),
        [
            "Разведчик",
            "Солдат",
            "Поджигатель",
            "Подрывник",
            "Пулемётчик",
            "Инженер",
            "Медик",
            "Снайпер",
            "Шпион"
        ]
    );
    assert_eq!(
        classes(class_short_name),
        [
            "Разведчик",
            "Солдат",
            "Поджигатель",
            "Подрыв.",
            "Пулемётчик",
            "Инж.",
            "Медик",
            "Снайпер",
            "Шпион"
        ]
    );
    let slots = [
        WeaponSlot::Primary,
        WeaponSlot::Secondary,
        WeaponSlot::Melee,
    ]
    .map(|s| names::slot_name(s).to_string());
    assert_eq!(slots, ["Основное", "Дополнительное", "Ближний бой"]);
    let kinds =
        [SoundKind::Hitsound, SoundKind::Killsound].map(|k| names::sound_kind_name(k).to_string());
    assert_eq!(kinds, ["Хитсаунд", "Килсаунд"]);
}

#[gpui_kit::test]
fn the_sidebar_header_and_tf2_notice_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let label = |cx: &mut TestAppContext, id: gpui_kit::ElementId| support::label_of(cx, s.win, id);
    assert_eq!(label(cx, ids::nav(Page::Huds)).as_deref(), Some("HUD"));
    assert_eq!(
        label(cx, ids::nav(Page::Crosshairs)).as_deref(),
        Some("Прицелы")
    );
    assert_eq!(
        label(cx, ids::nav(Page::AddonStore)).as_deref(),
        Some("Магазин дополнений")
    );
    assert_eq!(label(cx, ids::HEADER_TITLE.into()).as_deref(), Some("HUD"));
    support::wait_real(cx, "the no-TF2 notice", |cx| {
        support::present(cx, s.win, ids::HUDS_NO_TF2)
    });
    assert_eq!(
        label(cx, ids::HUDS_NO_TF2.into()).as_deref(),
        Some("TF2 не найден")
    );
    support::click_id(cx, s.win, ids::nav(Page::Logs));
    assert_eq!(
        label(cx, ids::HEADER_TITLE.into()).as_deref(),
        Some("Журнал действий")
    );
}
