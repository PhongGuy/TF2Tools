//! The HUD Maker and the Crosshair Maker in Russian: core's labels, the pages and their dialogs.

use gpui_kit::{ElementId, Entity, TestAppContext};
use tf2tools_app::i18n::names::{
    board_mode_name, maker_label, meter_name, scene_name, starting_point_about, starting_point_name,
};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
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
            "Игра всегда скрывает эту часть"
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

// The item colours sit two to a line; a Russian name too long for half the panel takes a line
// of its own rather than running into its neighbour's swatch or past the panel.
#[gpui_kit::test]
fn a_long_russian_item_colour_keeps_clear_of_its_neighbour(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
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
    assert_eq!(items.len(), 16);
    let body = support::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_BODY).unwrap();
    for (id, _, name) in &items {
        assert!(
            name.right() <= body.right(),
            "{id} leaves the panel: {name:?} {body:?}"
        );
    }
    for pair in items.windows(2) {
        let ((id, swatch, name), (next, next_swatch, _)) = (&pair[0], &pair[1]);
        if next_swatch.top() == swatch.top() {
            assert!(name.right() <= next_swatch.left(), "{id} runs into {next}");
        }
    }
}
