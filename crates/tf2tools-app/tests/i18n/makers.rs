//! The HUD Maker and the Crosshair Maker in Russian, with the Portuguese fit checks: core's
//! labels, the pages and their dialogs.

use gpui_kit::{AppContext as _, ElementId, Entity, TestAppContext};
use tf2tools_app::i18n::names::{
    board_mode_name, maker_label, maker_part_english, meter_name, scene_name, starting_point_about,
    starting_point_name,
};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshair_maker::CrosshairMakerPage;
use tf2tools_app::pages::crosshair_maker::model::{Tool, undo_tip, zoom_in_tip, zoom_out_tip};
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::pages::hud_maker::{install_tip, part_name};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::confirm::Confirm;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{BoardMode, MeterKind, OptionSpec, PartKey, Scene, StartingPoint, Tf2Install};

use crate::LocaleGuard;
use crate::support;

/// The HUD Maker over the stock game with `overrides`, loaded; the env keeps its folders.
fn open_hud_maker(
    cx: &mut TestAppContext,
    overrides: &[(&str, Option<&str>)],
) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, overrides);
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, state);
    let page = reopen_hud_maker(cx, &s);
    (env, s, page)
}

/// Opens the HUD Maker from the sidebar and waits for its HUD.
fn reopen_hud_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn label(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<ElementId>,
) -> Option<String> {
    support::label_of(cx, s.win, id)
}

#[test]
fn the_maker_enum_names_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let scenes = Scene::ALL.map(|s| scene_name(s).to_string());
    assert_eq!(
        scenes,
        [
            "HUD",
            "Таблица счёта",
            "Главное меню",
            "Смерть и стоп-кадр",
            "Итоги раунда",
            "Выбор класса",
            "Наблюдение"
        ]
    );
    let modes = BoardMode::ALL.map(|m| board_mode_name(m).to_string());
    assert_eq!(modes, ["Обычный", "Соревновательный", "MvM"]);
    assert_eq!(
        meter_name(MeterKind::Plain).to_string(),
        "Sandman, Jarate, знамёна и прочее"
    );
    assert_eq!(
        starting_point_name(StartingPoint::BigNumbers).to_string(),
        "Крупные цифры"
    );
    assert_eq!(
        starting_point_about(StartingPoint::CentreHud).to_string(),
        "Ставит здоровье и патроны рядом с прицелом."
    );
}

// Ids stay English, so each tab and mode is found by its English id and reads Russian.
#[gpui_kit::test]
fn the_scene_tabs_and_match_modes_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_hud_maker(cx, &[]);
    if !support::present(cx, s.win, ids::hud_maker_scene(Scene::Scoreboard)) {
        support::click_id(cx, s.win, ids::HUD_MAKER_SCENE_MENU);
    }
    let tabs = [Scene::Scoreboard, Scene::FreezeCam]
        .map(|scene| label(cx, &s, ids::hud_maker_scene(scene)));
    assert_eq!(
        tabs,
        [
            Some("Таблица счёта".to_owned()),
            Some("Смерть и стоп-кадр".to_owned())
        ]
    );
    support::click_id(cx, s.win, ids::hud_maker_scene(Scene::Scoreboard));
    assert_eq!(
        label(cx, &s, ids::HUD_MAKER_BOARD_MODE).as_deref(),
        Some("Обычный")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_BOARD_MODE);
    assert_eq!(
        label(cx, &s, ids::hud_maker_board_mode(BoardMode::Competitive)).as_deref(),
        Some("Соревновательный")
    );
}

#[gpui_kit::test]
fn the_meter_row_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_hud_maker(cx, &[]);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    assert_eq!(
        label(cx, &s, ids::HUD_MAKER_SITUATION_METER).as_deref(),
        Some("Шкала: Sandman, Jarate, знамёна и прочее")
    );
}

#[gpui_kit::test]
fn the_starting_points_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_hud_maker(cx, &[]);
    support::click_id(cx, s.win, ids::HUD_MAKER_START_READY);
    assert_eq!(
        label(cx, &s, ids::hud_maker_start_card_point("Big numbers")).as_deref(),
        Some("Крупные цифры")
    );
}

#[gpui_kit::test]
fn a_missing_scoreboard_says_so_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_hud_maker(cx, &[("resource/ui/scoreboard.res", None)]);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert_eq!(
        label(cx, &s, ids::HUD_MAKER_SCENE_NOTE).as_deref(),
        Some("Не удаётся показать таблицу счёта")
    );
}

const HEALTH: &str = "HudPlayerStatus/HudPlayerHealth";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

#[test]
fn core_table_labels_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let shown = [
        "Background colour",
        "Upper case",
        "Hide",
        "Square",
        "Rounded",
        "The game keeps this hidden",
        "Uber meter",
    ]
    .map(|t| maker_label(t).to_string());
    assert_eq!(
        shown,
        [
            "Цвет фона",
            "Заглавные буквы",
            "Скрыть",
            "Прямые",
            "Скруглённые",
            "Игра всегда скрывает эту часть",
            // Short enough to show whole in a top-level parts list row.
            "Убер-заряд"
        ]
    );
}

// The part list and the side panel read core's names in Russian; rows keep their key ids.
#[gpui_kit::test]
fn a_part_reads_its_core_name_and_when_it_shows_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_hud_maker(cx, &[]);
    support::click_id(cx, s.win, ids::HUD_MAKER_START_BLANK);
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::click_part_row(cx, s.win, &PartKey::new(HV));
    assert_eq!(
        label(cx, &s, ids::hud_maker_row(&PartKey::new(HEALTH))).as_deref(),
        Some("Здоровье")
    );
    assert_eq!(
        label(cx, &s, ids::HUD_MAKER_PANEL_SHOWS).as_deref(),
        Some("Видна, пока вы живы")
    );
    support::click_part_row(cx, s.win, &PartKey::new(HEALTH));
    assert_eq!(
        label(cx, &s, ids::HUD_MAKER_PANEL_PART).as_deref(),
        Some("Здоровье")
    );
}

// A Russian player finds a part by the element name its HUD file gives it, as well as by the
// Russian name the row shows.
#[gpui_kit::test]
fn the_part_filter_finds_a_part_by_its_file_name_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, page) = open_hud_maker(cx, &[]);
    support::click_id(cx, s.win, ids::HUD_MAKER_START_BLANK);
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    let input = cx.update(|cx| page.read(cx).parts_filter().clone());
    let filter = |cx: &mut TestAppContext, text: &str| {
        cx.update_window(s.win, |_, window, cx| {
            input.update(cx, |st, cx| st.set_value(text, window, cx))
        })
        .unwrap();
        support::settle(cx, s.win);
    };
    let row = |k: &str| ids::hud_maker_row(&PartKey::new(k));
    filter(cx, "HealthValue");
    assert!(support::present(cx, s.win, row(HV)));
    assert!(!support::present(cx, s.win, row("HudWeaponAmmo")));
    filter(cx, "hudplayerhealth");
    assert!(support::present(cx, s.win, row(HEALTH)));
    filter(cx, "цифры здоровья");
    assert!(support::present(cx, s.win, row(HV)));
}

// The option files the built-ins need, as tests/hud_maker_options.rs seeds them.
const SCHEME: &str = r#""Scheme" { Colors { TanDark "117 107 94 255" TanLight "235 226 202 255"
  QualityColorNormal "178 178 178 255" QualityColorUnique "255 215 0 255" } }"#;
const BASECHAT: &str = r#""Resource/UI/BaseChat.res" { HudChat { xpos 10 ypos 275 } }"#;
const MAINMENU: &str = r#""Resource/UI/MainMenuOverride.res" { FriendsContainer { visible 1 } }"#;

#[test]
fn option_labels_read_in_russian_and_glyphs_stay() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(maker_label("HUD crosshair").to_string(), "Прицел HUD");
    assert_eq!(maker_label("+").to_string(), "+");
}

// Options are found by their English ids; the group heading's id keeps core's group name.
#[gpui_kit::test]
fn the_built_in_options_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let files = [
        ("resource/clientscheme.res", Some(SCHEME)),
        ("scripts/hudlayout.res", Some(support::hud_stock::HUDLAYOUT)),
        ("resource/ui/basechat.res", Some(BASECHAT)),
        ("resource/ui/mainmenuoverride.res", Some(MAINMENU)),
    ];
    let (_env, s, _page) = open_hud_maker(cx, &files);
    assert_eq!(
        label(cx, &s, ids::hud_maker_option("chat_top")).as_deref(),
        Some("Чат вверху")
    );
    assert_eq!(
        label(cx, &s, ids::hud_maker_option_group("Item colours")).as_deref(),
        Some("Цвета предметов")
    );
    assert_eq!(
        label(cx, &s, ids::hud_maker_option("item_unique")).as_deref(),
        Some("Уникальный")
    );
}

#[gpui_kit::test]
fn a_part_the_game_fills_says_so_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, _page) = open_hud_maker(cx, &[]);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let tips = ids::hud_maker_part(&PartKey::new("class/ClassTipsPanel"));
    assert_eq!(
        label(cx, &s, tips).as_deref(),
        Some("Подсказки классов рисует игра")
    );
}

// The item colours sit two to a line; a name too long for half the panel takes a line
// of its own rather than running into its neighbour's swatch or past the panel.
fn a_long_item_colour_keeps_clear_of_its_neighbour(cx: &mut TestAppContext, lang: &str) {
    let _guard = LocaleGuard::switch(lang);
    let files = [
        ("resource/clientscheme.res", Some(SCHEME)),
        ("scripts/hudlayout.res", Some(support::hud_stock::HUDLAYOUT)),
    ];
    let (_env, s, _page) = open_hud_maker(cx, &files);
    let items = OptionSpec::built_ins()
        .iter()
        .filter(|spec| spec.group() == "Item colours")
        .map(|spec| {
            let swatch = support::bounds_of(cx, s.win, ids::hud_maker_option(spec.id()));
            let name = support::bounds_of(cx, s.win, ids::hud_maker_option_label(spec.id()));
            (spec.id(), swatch.expect("a swatch"), name.expect("a name"))
        })
        .collect::<Vec<_>>();
    assert_eq!(items.len(), 16, "{lang}: the item colour rows");
    let body = support::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_BODY).unwrap();
    for (id, _, name) in &items {
        assert!(
            name.right() <= body.right(),
            "{lang}: {id} leaves the panel: {name:?} {body:?}"
        );
    }
    for pair in items.windows(2) {
        let ((id, swatch, name), (next, next_swatch, _)) = (&pair[0], &pair[1]);
        if next_swatch.top() == swatch.top() {
            assert!(
                name.right() <= next_swatch.left(),
                "{lang}: {id} runs into {next}"
            );
        }
    }
}

#[gpui_kit::test]
fn a_long_russian_item_colour_keeps_clear_of_its_neighbour(cx: &mut TestAppContext) {
    a_long_item_colour_keeps_clear_of_its_neighbour(cx, "ru");
}

#[gpui_kit::test]
fn a_long_portuguese_item_colour_keeps_clear_of_its_neighbour(cx: &mut TestAppContext) {
    a_long_item_colour_keeps_clear_of_its_neighbour(cx, "pt-BR");
}

#[gpui_kit::test]
fn a_long_spanish_item_colour_keeps_clear_of_its_neighbour(cx: &mut TestAppContext) {
    a_long_item_colour_keeps_clear_of_its_neighbour(cx, "es");
}

#[gpui_kit::test]
fn a_long_german_item_colour_keeps_clear_of_its_neighbour(cx: &mut TestAppContext) {
    a_long_item_colour_keeps_clear_of_its_neighbour(cx, "de");
}

fn missing() -> Tf2State {
    Tf2State::Missing { saved: None }
}

/// Opens the Crosshair Maker from the sidebar.
fn open_crosshair_maker(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
) -> Entity<CrosshairMakerPage> {
    support::click_id(cx, s.win, ids::nav(Page::CrosshairMaker));
    cx.update(|cx| s.shell.read(cx).crosshair_maker_page())
        .expect("a Crosshair Maker page")
}

#[gpui_kit::test]
fn the_crosshair_maker_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let page = open_crosshair_maker(cx, &s);
    let shown = [
        ids::CROSSHAIR_MAKER_SAVE,
        ids::CROSSHAIR_MAKER_SAVE_AS_NEW,
        ids::CROSSHAIR_MAKER_NEW,
        ids::CROSSHAIR_MAKER_STARTERS,
        ids::CROSSHAIR_MAKER_IMPORT_PNG,
        ids::CROSSHAIR_MAKER_PENCIL,
        ids::CROSSHAIR_MAKER_COLOR_HINT,
        ids::CROSSHAIR_MAKER_OPACITY_LABEL,
        ids::CROSSHAIR_MAKER_PREVIEW,
        ids::CROSSHAIR_MAKER_BACKGROUND,
    ]
    .map(|id| label(cx, &s, id));
    let want = [
        "Сохранить",
        "Сохранить копию",
        "Новый",
        "Заготовки",
        "Импорт PNG",
        "Карандаш",
        "Принимает цвет вашего прицела",
        "Непрозрачность: 100%",
        "Превью на карте 2fort",
        "2fort",
    ];
    assert_eq!(shown, want.map(|t| Some(t.to_owned())));
    let placeholder = cx.update(|cx| {
        page.read(cx)
            .name_input()
            .read(cx)
            .presentation()
            .placeholder()
            .to_string()
    });
    assert_eq!(placeholder, "Название прицела");
    assert_eq!(Tool::Pencil.tooltip(), "Карандаш (P)");
    assert_eq!(undo_tip().to_string(), "Отменить (Ctrl+Z)");
    // The zoom tips name the keys as the HUD Maker's shortcuts sheet does.
    assert_eq!(zoom_in_tip().to_string(), "Увеличить (Ctrl + плюс)");
    assert_eq!(zoom_out_tip().to_string(), "Уменьшить (Ctrl + минус)");
}

#[gpui_kit::test]
fn the_starter_dialog_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let _page = open_crosshair_maker(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_STARTERS);
    assert_eq!(
        label(cx, &s, ids::crosshair_maker_starter("cross-gap")).as_deref(),
        Some("Крест с зазором")
    );
    assert_eq!(
        label(cx, &s, ids::CROSSHAIR_MAKER_STARTER_USE).as_deref(),
        Some("Использовать")
    );
    let length = label(cx, &s, ids::crosshair_maker_starter_value("length")).unwrap();
    assert!(
        length.starts_with("Длина: ") && length.ends_with(" px"),
        "{length}"
    );
}

// The maker is dropped on leaving, so one opened after a switch is built in the new language.
#[gpui_kit::test]
fn the_crosshair_maker_reopened_after_a_switch_reads_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let _page = open_crosshair_maker(cx, &s);
    assert_eq!(
        label(cx, &s, ids::CROSSHAIR_MAKER_SAVE).as_deref(),
        Some("Save")
    );
    crate::settings::open_settings(cx, &s);
    crate::settings::pick(cx, s.win, "ru");
    let _page = open_crosshair_maker(cx, &s);
    assert_eq!(
        label(cx, &s, ids::CROSSHAIR_MAKER_SAVE).as_deref(),
        Some("Сохранить")
    );
}

// The longest button row of the confirms: all three buttons stay inside the dialog.
fn the_unsaved_changes_buttons_fit_their_dialog(cx: &mut TestAppContext, lang: &str) {
    let _guard = LocaleGuard::switch(lang);
    let (_env, s, page) = open_hud_maker(cx, &[]);
    let name = cx.update(|cx| page.read(cx).name_input().clone());
    let draft = if lang == "ru" {
        "Набросок"
    } else {
        "Rascunho"
    };
    cx.update_window(s.win, |_, window, cx| {
        name.update(cx, |st, cx| st.set_value(draft, window, cx))
    })
    .unwrap();
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(HV)));
            e.nudge_selected(5, 0).unwrap();
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let dialog = support::bounds_of(cx, s.win, ids::CONFIRM_DIALOG).expect("the confirm");
    for id in [ids::CONFIRM_CANCEL, ids::CONFIRM_OK, ids::CONFIRM_SAVE] {
        let button = support::bounds_of(cx, s.win, id).expect("a button");
        assert!(
            button.left() >= dialog.left() && button.right() <= dialog.right(),
            "{lang}: {id} leaves the dialog: {button:?} {dialog:?}"
        );
    }
}

#[gpui_kit::test]
fn the_unsaved_changes_buttons_fit_their_dialog_in_russian(cx: &mut TestAppContext) {
    the_unsaved_changes_buttons_fit_their_dialog(cx, "ru");
}

#[gpui_kit::test]
fn the_unsaved_changes_buttons_fit_their_dialog_in_portuguese(cx: &mut TestAppContext) {
    the_unsaved_changes_buttons_fit_their_dialog(cx, "pt-BR");
}

#[gpui_kit::test]
fn the_unsaved_changes_buttons_fit_their_dialog_in_spanish(cx: &mut TestAppContext) {
    the_unsaved_changes_buttons_fit_their_dialog(cx, "es");
}

#[gpui_kit::test]
fn the_unsaved_changes_buttons_fit_their_dialog_in_german(cx: &mut TestAppContext) {
    the_unsaved_changes_buttons_fit_their_dialog(cx, "de");
}

fn texts(c: &Confirm) -> [String; 4] {
    [&c.title, &c.description, &c.confirm_label, &c.cancel_label].map(|t| t.to_string())
}

#[test]
fn the_maker_confirm_dialogs_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(
        texts(&Confirm::shrink_canvas()),
        [
            "Уменьшить холст",
            "При уменьшении до 32x32 края рисунка обрежутся.",
            "Уменьшить",
            "Отмена"
        ]
    );
    assert_eq!(
        texts(&Confirm::delete_snapshot("Before")),
        [
            "Удалить этот снимок?",
            "Снимок «Before» будет удалён навсегда. Ваш HUD не изменится.",
            "Удалить",
            "Оставить"
        ]
    );
    assert_eq!(
        texts(&Confirm::reset_all_options())[..3],
        [
            "Сбросить все настройки?",
            "Все настройки вернутся к исходным значениям HUD. Остальные изменения останутся, а «Отменить» вернёт настройки.",
            "Сбросить все",
        ]
    );
}

#[gpui_kit::test]
fn the_hud_maker_frame_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, page) = open_hud_maker(cx, &[]);
    let buttons = [ids::HUD_MAKER_SAVE, ids::HUD_MAKER_INSTALL].map(|id| label(cx, &s, id));
    assert_eq!(
        buttons,
        [Some("Сохранить".to_owned()), Some("Установить".to_owned())]
    );
    assert_eq!(
        cx.update(|cx| page.read(cx).undo_tip()),
        "Отменить (Ctrl+Z)"
    );
    let placeholders = cx.update(|cx| {
        let p = page.read(cx);
        [
            p.name_input(),
            p.parts_filter(),
            p.snapshot_name_input(),
            p.preset_name_input(),
        ]
        .map(|input| input.read(cx).presentation().placeholder().to_string())
    });
    // The preset and snapshot hints are short enough to show whole in their narrow fields.
    assert_eq!(
        placeholders,
        ["Назовите HUD", "Найти часть...", "Имя снимка", "Имя"]
    );
    assert_eq!(
        install_tip(false, true).to_string(),
        "Сначала сохраните HUD"
    );
    assert_eq!(install_tip(false, false).to_string(), "TF2 не найдена");
}

#[gpui_kit::test]
fn the_minimal_hud_chip_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (_env, s, page) = open_hud_maker(cx, &[]);
    support::maker_page::on_page(cx, &s, &page, |p, window, cx| {
        p.set_minmode(true, window, cx)
    });
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert_eq!(
        label(cx, &s, ids::HUD_MAKER_MINMODE_CHIP).as_deref(),
        Some("Минимальный HUD")
    );
}

// The HUD Maker is dropped on leaving, so one opened after a switch is built in the new language.
#[gpui_kit::test]
fn the_hud_maker_reopened_after_a_switch_reads_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let (_env, s, _page) = open_hud_maker(cx, &[]);
    assert_eq!(label(cx, &s, ids::HUD_MAKER_SAVE).as_deref(), Some("Save"));
    crate::settings::open_settings(cx, &s);
    crate::settings::pick(cx, s.win, "ru");
    let _page = reopen_hud_maker(cx, &s);
    assert_eq!(
        label(cx, &s, ids::HUD_MAKER_SAVE).as_deref(),
        Some("Сохранить")
    );
}

#[test]
fn a_part_names_english_is_read_in_any_language() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(
        maker_part_english("Health value").to_string(),
        "Health value"
    );
    assert_eq!(maker_part_english("Panelty").to_string(), "Panelty");
}

#[test]
fn a_curated_panel_reads_in_russian_and_an_unknown_one_keeps_its_own_name() {
    let _guard = LocaleGuard::switch("ru");
    let (known, unknown, glyph) = (
        part_name("PlayerStatusHealthValue"),
        part_name("Panelty"),
        part_name("___"),
    );
    tf2tools_app::i18n::apply("en");
    let english = part_name("PlayerStatusHealthValue");
    let russian = tf2tools_app::_rust_i18n_try_translate("ru", "hud_maker.part_name.health_value")
        .expect("health_value in ru.yml")
        .into_owned();
    assert_eq!(known, russian);
    assert!(
        known
            .chars()
            .any(|c| ('а'..='я').contains(&c.to_lowercase().next().unwrap())),
        "{known}"
    );
    assert_eq!(unknown, "Panelty");
    assert_eq!(glyph, "___");
    assert_eq!(english, "Health value");
}
