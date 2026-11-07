//! The MvM scoreboard on the canvas: the table, its titles and the notes.

use crate::support;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{BoardMode, PartKey, Scene, Tf2Install};

const LIST: &str = "scores/MvMScoreboard/MvMPlayerList";

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    support::click_nav(cx, s.win, tf2tools_app::shell::route::Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

// The dropdown's items exist only while it is open.
fn pick_mode(cx: &mut TestAppContext, s: &support::ShellWindow, m: BoardMode) {
    if !support::present(cx, s.win, ids::hud_maker_board_mode(m)) {
        support::click_id(cx, s.win, ids::HUD_MAKER_BOARD_MODE);
    }
    support::click_id(cx, s.win, ids::hud_maker_board_mode(m));
}

#[gpui_kit::test]
fn the_mvm_table_draws_a_header_and_one_row_per_defender(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    pick_mode(cx, &s, BoardMode::Mvm);
    let list = PartKey::new(LIST);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_list_header(&list)
    ));
    for column in [
        "name", "tour", "score", "damage", "tank", "healing", "support", "credits",
    ] {
        assert!(
            support::present(cx, s.win, ids::hud_maker_list_title(&list, column)),
            "{column}"
        );
    }
    for row in 0..6 {
        assert!(
            support::present(cx, s.win, ids::hud_maker_list_row(&list, row)),
            "row {row}"
        );
    }
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_list_row(&list, 6)
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_list_row(&list, 0)).as_deref(),
        Some("You")
    );
}

#[gpui_kit::test]
fn the_wave_bar_and_credit_tables_say_the_game_draws_them(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    pick_mode(cx, &s, BoardMode::Mvm);
    let wave = ids::hud_maker_part(&PartKey::new("scores/MvMScoreboard/WaveStatusPanel"));
    assert_eq!(
        support::label_of(cx, s.win, wave).as_deref(),
        Some("The wave bar draws in game")
    );
    let credits = ids::hud_maker_part(&PartKey::new(
        "scores/MvMScoreboard/CreditStatsContainer/PreviousWaveCreditInfoPanel",
    ));
    assert_eq!(
        support::label_of(cx, s.win, credits).as_deref(),
        Some("The credit tables draw in game")
    );
}

#[gpui_kit::test]
fn the_competitive_sample_draws_six_rows_a_team(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    pick_mode(cx, &s, BoardMode::Competitive);
    for list in ["scores/BluePlayerList", "scores/RedPlayerList"] {
        let key = PartKey::new(list);
        assert!(
            support::present(cx, s.win, ids::hud_maker_list_row(&key, 5)),
            "{list}"
        );
        assert!(
            !support::present(cx, s.win, ids::hud_maker_list_row(&key, 6)),
            "{list}"
        );
    }
}

fn utf16le(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}

#[gpui_kit::test]
fn the_mvm_header_uses_the_games_tokens_and_has_no_class_title(cx: &mut TestAppContext) {
    let env = support::env();
    let english = r#""lang" { "Tokens" { "TF_Scoreboard_Name" "Callsign" "TF_MvMScoreboard_Money" "Loot" "TF_MvMScoreboard_Credits" "Wrong" } }"#;
    let bytes = utf16le(english);
    support::seed_stock_game_with_bytes(
        &env.game,
        &[],
        &[("resource/tf_english.txt", bytes.as_slice())],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    pick_mode(cx, &s, BoardMode::Mvm);
    let list = PartKey::new(LIST);
    let title = |cx: &mut TestAppContext, column: &str| {
        support::label_of(cx, s.win, ids::hud_maker_list_title(&list, column))
    };
    assert_eq!(title(cx, "name").as_deref(), Some("Callsign"));
    assert_eq!(title(cx, "credits").as_deref(), Some("Loot"));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_list_title(&list, "class")
    ));
}
