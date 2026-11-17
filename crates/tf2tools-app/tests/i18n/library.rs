//! The library pages, the names they share and the navigation, in Russian.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, Entity, SharedString, TestAppContext};
use tf2tools_app::crosshair_sync::installed_text;
use tf2tools_app::i18n::{self, names};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshairs::CrosshairsPage;
use tf2tools_app::pages::huds::HudsPage;
use tf2tools_app::pages::profiles::ProfilesPage;
use tf2tools_app::pages::profiles::editor_model::{Slot, SlotState, next_profile_name, slot_text};
use tf2tools_app::pages::profiles::model::{crosshair_count, not_applied};
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::pages::sounds::SoundsPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::confirm::Confirm;
use tf2tools_app::shell::route::Page;
use tf2tools_app::widgets::class_icon::class_short_name;
use tf2tools_app::widgets::profile_menu::added_title;
use tf2tools_core::{ProfileDraft, Provenance, SoundKind, Tf2Install, TfClass, WeaponSlot};

use crate::LocaleGuard;
use crate::support;

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

// The HUDs page, the shell's start page, once its first load is done.
fn open_huds(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudsPage> {
    let page = cx
        .update(|cx| s.shell.read(cx).huds_page())
        .expect("a HUDs page");
    support::wait_real(cx, "HUDs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

// The Sounds page, once its load and the saved volume are in.
fn open_sounds(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<SoundsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    let page = cx
        .update(|cx| s.shell.read(cx).sounds_page())
        .expect("a Sounds page");
    support::wait_real(cx, "Sounds to load", |cx| {
        cx.update(|cx| {
            let page = page.read(cx);
            page.is_loaded() && page.player().read(cx).is_ready()
        })
    });
    support::settle(cx, s.win);
    page
}

// The Crosshairs page, once its first load is done.
fn open_crosshairs(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshairs_page())
        .expect("a Crosshairs page");
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn search(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairsPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

// The Profiles page with the TF2 probe pinned to "not running", once its load is done.
fn open_profiles(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<ProfilesPage> {
    cx.update(|cx| Tf2Probe::install(cx, || false));
    support::click_id(cx, s.win, ids::nav(Page::Profiles));
    let page = cx
        .update(|cx| s.shell.read(cx).profiles_page())
        .expect("a Profiles page");
    support::wait_real(cx, "Profiles to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn toast_is(cx: &mut TestAppContext, s: &support::ShellWindow, text: &str) {
    support::wait_real(cx, text, |cx| {
        support::label_of(cx, s.win, ids::TOAST).as_deref() == Some(text)
    });
}

// A confirm dialog's four texts, in reading order.
fn texts(c: &Confirm) -> [&str; 4] {
    [
        c.title.as_ref(),
        c.description.as_ref(),
        c.confirm_label.as_ref(),
        c.cancel_label.as_ref(),
    ]
}

// A minimal HUD archive, `<name>/info.vdf` plus a layout file, written into `dir`.
fn hud_zip(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{name}/info.vdf"), options).unwrap();
    write!(zip, "\"{name}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n").unwrap();
    zip.start_file(format!("{name}/resource/ui/hudlayout.res"), options)
        .unwrap();
    write!(zip, "\"Resource/HudLayout.res\"\n{{\n}}\n").unwrap();
    zip.finish().unwrap();
    path
}

fn classes(name: fn(TfClass) -> SharedString) -> Vec<String> {
    TfClass::ALL.iter().map(|c| name(*c).to_string()).collect()
}

#[test]
fn class_slot_and_sound_names_read_in_both_languages() {
    let _guard = LocaleGuard::hold();
    assert_eq!(names::class_name(TfClass::Heavy), "Heavy");
    assert_eq!(class_short_name(TfClass::Engineer), "Engie");
    assert_eq!(
        classes(names::class_compact_name),
        classes(names::class_name)
    );
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
            "Развед.",
            "Солдат",
            "Поджиг.",
            "Подрыв.",
            "Пулем.",
            "Инж.",
            "Медик",
            "Снайпер",
            "Шпион"
        ]
    );
    assert_eq!(
        classes(names::class_compact_name),
        [
            "Развед.",
            "Солдат",
            "Поджиг.",
            "Подрыв.",
            "Пулем.",
            "Инженер",
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

#[gpui_kit::test]
fn the_huds_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_huds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_EMPTY).as_deref(),
        Some("HUD не установлен")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_EMPTY).as_deref(),
        Some("Ваша библиотека пуста")
    );
    let archive = hud_zip(env.tmp.path(), "budhud");
    s.services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let _page = open_huds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_badge("budhud")).as_deref(),
        Some("Локальный")
    );
    support::click_id(cx, s.win, ids::hud_delete("budhud"));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Удалить HUD")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("Удалить budhud из библиотеки? Это действие нельзя отменить.")
    );
}

#[test]
fn hud_counts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let fonts = |n: u64| {
        i18n::counted_plural(
            "font_check.activity.installed",
            n,
            &n.to_string(),
            &[("folder", "budhud")],
        )
    };
    assert_eq!(fonts(1), "Установлен 1 шрифт для HUD: budhud");
    assert_eq!(fonts(2), "Установлено 2 шрифта для HUD: budhud");
    assert_eq!(fonts(5), "Установлено 5 шрифтов для HUD: budhud");
    assert_eq!(fonts(21), "Установлен 21 шрифт для HUD: budhud");
    let more = |n: u64| {
        i18n::counted_plural(
            "external_huds.names_more",
            n,
            &n.to_string(),
            &[("names", "a, b")],
        )
    };
    for n in [1, 2, 5, 21] {
        assert_eq!(more(n), format!("a, b и ещё {n}"));
    }
}

#[gpui_kit::test]
fn the_sounds_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Звуки")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUNDS_EMPTY).as_deref(),
        Some("В библиотеке пока нет звуков")
    );
    let hit = SoundKind::Hitsound;
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_status(hit)).as_deref(),
        Some("Нет")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_name(hit)).as_deref(),
        Some("Хитсаунд не установлен")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_name(SoundKind::Killsound)).as_deref(),
        Some("Килсаунд не установлен")
    );
    assert_eq!(
        texts(&Confirm::delete_sound()),
        [
            "Удалить звук",
            "Удалить этот звук из библиотеки? Это действие нельзя отменить.",
            "Удалить",
            "Отмена"
        ]
    );
    assert_eq!(
        added_title("Ding", "Comp", Some(hit), Some("Old")),
        "Добавлено в профиль Comp как хитсаунд: Ding (вместо Old)"
    );
}

#[gpui_kit::test]
fn the_crosshairs_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_crosshair(&s.services, &env, "Zebra Dot");
    let page = open_crosshairs(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_WAITING).as_deref(),
        Some("TF2 использует свои прицелы")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_PREVIEW).as_deref(),
        Some("Нет прицела")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_BACKGROUND).as_deref(),
        Some("Тёмный")
    );
    search(cx, &s, &page, "zebra");
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_GROUP_LIBRARY).as_deref(),
        Some("Мои прицелы (1)")
    );
    search(cx, &s, &page, "zzzz");
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_NO_MATCH).as_deref(),
        Some("Нет прицелов по вашему запросу")
    );
    assert_eq!(
        texts(&Confirm::delete_crosshair("Zebra Dot", 2, true))[..2],
        [
            "Удалить прицел",
            "Удалить Zebra Dot из библиотеки? Его используют 2 оружия. Они вернутся к прицелу по умолчанию. \
             Он также будет удалён из TF2. Это действие нельзя отменить."
        ]
    );
    assert_eq!(installed_text(1), "Установлено 1 назначение прицела");
    assert_eq!(installed_text(2), "Установлено 2 назначения прицелов");
    assert_eq!(installed_text(5), "Установлено 5 назначений прицелов");
    assert_eq!(installed_text(21), "Установлено 21 назначение прицела");
}

#[gpui_kit::test]
fn the_profiles_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_profiles(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Профили")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_EMPTY).as_deref(),
        Some("Профилей пока нет")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_NO_ACTIVE).as_deref(),
        Some("Нет активного профиля")
    );
    let comp = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Competitive"))
        .unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_delete(comp.id()));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Удалить профиль")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("Удалить «Competitive»? Это действие нельзя отменить.")
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(cx, &s, "Профиль удалён");
}

#[test]
fn profile_texts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(
        [1, 2, 5, 21].map(crosshair_count),
        ["1 прицел", "2 прицела", "5 прицелов", "21 прицел"]
    );
    assert_eq!(not_applied(), "Изменения ещё не применены");
    assert_eq!(
        texts(&Confirm::tf2_running())[..3],
        [
            "TF2 запущен",
            "TF2 запущен; изменения вступят в силу после перезапуска.",
            "Продолжить"
        ]
    );
    let env = support::env();
    let services = support::services(&env, support::no_steam());
    assert_eq!(next_profile_name(&[]), "Профиль 1");
    for name in ["профиль 1", "Profile 2"] {
        services
            .library
            .create_profile(&ProfileDraft::new(name))
            .unwrap();
    }
    assert_eq!(
        next_profile_name(&services.library.profiles().unwrap()),
        "Профиль 2"
    );
    let labels: Vec<String> = Slot::ALL.iter().map(|s| s.label().to_string()).collect();
    assert_eq!(labels, ["HUD", "Хитсаунд", "Килсаунд", "Прицелы"]);
    assert_eq!(slot_text(&SlotState::Empty, Slot::Hud), "Нет");
    assert_eq!(
        slot_text(&SlotState::Empty, Slot::Crosshairs),
        "Стандартные прицелы"
    );
    assert_eq!(
        slot_text(&SlotState::Missing("budhud".into()), Slot::Hud),
        "Отсутствует: budhud"
    );
    assert_eq!(
        texts(&Confirm::clear_profile_crosshairs())[..3],
        [
            "Очистить прицелы",
            "Убрать все прицелы из этого профиля?",
            "Очистить все"
        ]
    );
}
