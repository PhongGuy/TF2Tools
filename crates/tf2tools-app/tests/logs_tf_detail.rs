//! The open log's sections, compared with the fixtures' logs.tf pages.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, InputEvent as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::pages::logs_tf::sort::{PlayerColumn, PlayerSort, sort_players};
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_core::TfClass;

use crate::support;
use crate::support::logstf::{
    ARIE, HIGHLANDER, SIXES, day_ago, list_body, open_log, open_page, requests_to,
    scroll_to_in_log, serve_two_logs, shell, shell_with_picker, sid,
};

/// Arie's two logs synced, log `log` open; the env and server live as long as the test.
fn opened(
    cx: &mut TestAppContext,
    log: u64,
) -> (
    support::Env,
    support::StoreServer,
    support::ShellWindow,
    Entity<LogsTfPage>,
) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, log);
    (env, server, s, page)
}

#[gpui_kit::test]
fn clicking_a_header_sorts_the_players_and_again_flips_it(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_sort("DA"));
    support::click_id(cx, s.win, ids::logstf_sort("DA"));
    assert_eq!(
        cx.update(|cx| page.read(cx).player_sort()),
        PlayerSort {
            column: PlayerColumn::Damage,
            descending: true
        }
    );
    let top = |cx: &mut TestAppContext, account: u64| {
        support::bounds_of(cx, s.win, ids::logstf_player_row(sid(account)))
            .unwrap()
            .top()
    };
    assert!(top(cx, 285_837_700) < top(cx, ARIE), "most damage first");
    support::click_id(cx, s.win, ids::logstf_sort("DA"));
    assert!(
        top(cx, ARIE) < top(cx, 285_837_700),
        "a second click flips it"
    );
    open_log(cx, &s, &page, 2);
    assert_eq!(
        cx.update(|cx| page.read(cx).player_sort()),
        PlayerSort {
            column: PlayerColumn::Damage,
            descending: false
        },
        "the sort is kept while the page lives"
    );
}

#[gpui_kit::test]
fn a_sort_by_a_column_the_next_log_hides_shows_it_in_the_default_order(cx: &mut TestAppContext) {
    // A 6v6 log whose hidden HS column still carries a number, so sorting by it would show.
    let mut sixes: serde_json::Value = serde_json::from_str(SIXES).unwrap();
    sixes["players"]["[U:1:231702]"]["headshots"] = 5.into();
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    server.mount_bytes("/json/3", HIGHLANDER.as_bytes().to_vec());
    server.mount_bytes("/json/2", serde_json::to_vec(&sixes).unwrap());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_sort("HS"));
    support::click_id(cx, s.win, ids::logstf_sort("HS"));
    let hs = PlayerSort {
        column: PlayerColumn::Headshots,
        descending: true,
    };
    assert_eq!(cx.update(|cx| page.read(cx).player_sort()), hs);

    open_log(cx, &s, &page, 2);
    assert_eq!(
        cx.update(|cx| page.read(cx).player_sort()),
        hs,
        "the sort is kept"
    );
    let players = cx.update(|cx| page.read(cx).detail().unwrap().players().to_vec());
    let tops: Vec<_> = sort_players(&players, PlayerSort::default())
        .into_iter()
        .map(|i| {
            let row = ids::logstf_player_row(players[i].steam_id());
            support::bounds_of(cx, s.win, row).unwrap().top()
        })
        .collect();
    assert!(
        tops.windows(2).all(|w| w[0] < w[1]),
        "default order: {tops:?}"
    );
}

#[gpui_kit::test]
fn hovering_a_class_icon_opens_its_card(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    let icon = ids::logstf_class_card(sid(ARIE), TfClass::Scout);
    scroll_to_in_log(cx, &s, &page, 3, icon.clone());
    assert!(
        !support::present(
            cx,
            s.win,
            ids::logstf_class_popover(sid(ARIE), TfClass::Scout)
        ),
        "closed until hovered"
    );
    cx.update_window(s.win, |_, w, cx| w.hover(icon.clone(), cx))
        .unwrap();
    support::settle(cx, s.win);
    cx.executor().advance_clock(Duration::from_millis(300));
    support::settle(cx, s.win);
    assert!(
        support::present(
            cx,
            s.win,
            ids::logstf_class_popover(sid(ARIE), TfClass::Scout)
        ),
        "the card opens on hover"
    );
    assert_eq!(
        support::label_of(
            cx,
            s.win,
            ids::logstf_weapon(sid(ARIE), TfClass::Scout, "scattergun")
        )
        .as_deref(),
        Some("scattergun 7 (100%) 3334 (86%) 56%")
    );
}

// Hovers Arie's Scout icon in log 3 and lets its open delay run in full.
fn open_scout_card(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<LogsTfPage>) {
    let icon = ids::logstf_class_card(sid(ARIE), TfClass::Scout);
    scroll_to_in_log(cx, s, page, 3, icon.clone());
    cx.update_window(s.win, |_, w, cx| w.hover(icon, cx))
        .unwrap();
    support::settle(cx, s.win);
    cx.executor().advance_clock(Duration::from_millis(300));
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn the_card_opens_after_a_tenth_of_a_second_and_closes_when_left(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    let icon = ids::logstf_class_card(sid(ARIE), TfClass::Scout);
    let card = ids::logstf_class_popover(sid(ARIE), TfClass::Scout);
    scroll_to_in_log(cx, &s, &page, 3, icon.clone());
    cx.update_window(s.win, |_, w, cx| w.hover(icon, cx))
        .unwrap();
    support::settle(cx, s.win);
    cx.executor().advance_clock(Duration::from_millis(50));
    assert!(!support::present(cx, s.win, card.clone()), "shut at 50 ms");
    cx.executor().advance_clock(Duration::from_millis(100));
    assert!(support::present(cx, s.win, card.clone()), "open at 150 ms");

    cx.update_window(s.win, |_, w, cx| {
        let away = gpui_kit::MouseMoveEvent {
            position: gpui_kit::point(gpui_kit::px(2.), gpui_kit::px(2.)),
            ..Default::default()
        };
        let _ = w.dispatch_event(away.to_platform_input(), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
    cx.executor().advance_clock(Duration::from_millis(500));
    assert!(
        !support::present(cx, s.win, card),
        "shut once the pointer leaves"
    );
}

// `text` shaped at text-sm in the theme font, as a card cell lays it out.
fn text_width(cx: &mut TestAppContext, win: gpui_kit::AnyWindowHandle, text: &str) -> f32 {
    use gpui_kit::component::ActiveTheme as _;
    cx.update_window(win, |_, w, cx| {
        let run = gpui_kit::TextRun {
            len: text.len(),
            font: gpui_kit::font(cx.theme().font_family.clone()),
            ..Default::default()
        };
        let size = gpui_kit::rems(0.875).to_pixels(w.rem_size());
        f32::from(
            w.text_system()
                .shape_line(text.to_owned().into(), size, &[run], None)
                .width,
        )
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_widest_card_cells_in_the_fixtures_fit_their_columns(cx: &mut TestAppContext) {
    use tf2tools_app::pages::logs_tf::tables::weapon_rows;
    let (_env, _server, s, page) = opened(cx, 3);
    open_scout_card(cx, &s, &page);
    // Every weapon row of every player and class in both fixtures, as the card words them.
    let mut kills = Vec::new();
    let mut dmg = Vec::new();
    for body in [SIXES, HIGHLANDER] {
        let log = tf2tools_core::parse_log(body).unwrap();
        for player in log.players() {
            for class in player.classes() {
                for w in weapon_rows(player, class) {
                    kills.push(format!("{} ({})", w.kills, w.kill_share));
                    dmg.push(format!("{} ({})", w.dmg, w.dmg_share));
                }
            }
        }
    }
    let card = support::bounds_of(
        cx,
        s.win,
        ids::logstf_class_popover(sid(ARIE), TfClass::Scout),
    )
    .expect("the card is open");
    let width = f32::from(card.size.width);
    assert!((320. ..=360.).contains(&width), "card width {width}");
    for (title, cells) in [("K", kills), ("DA", dmg)] {
        let column = ids::logstf_card_column(sid(ARIE), TfClass::Scout, title);
        let column = support::bounds_of(cx, s.win, column).expect("the column header");
        let widest = cells
            .iter()
            .map(|c| (text_width(cx, s.win, c), c))
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .expect("the fixtures have weapons");
        assert!(
            widest.0 <= f32::from(column.size.width),
            "{title}: {:?} is {} px in a {} px column",
            widest.1,
            widest.0,
            f32::from(column.size.width)
        );
    }
}

#[gpui_kit::test]
fn the_players_table_comes_before_the_team_table_and_the_rounds(cx: &mut TestAppContext) {
    // logs.tf's order under the title: Players, then Team, then Rounds.
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_PLAYERS_SCROLL);
    let top = |cx: &mut TestAppContext, id| {
        support::bounds_of(cx, s.win, id)
            .expect("the section is laid out")
            .top()
    };
    let players = top(cx, ids::LOGSTF_PLAYERS_SCROLL);
    let team = top(cx, ids::LOGSTF_TEAMS_SCROLL);
    let rounds = top(cx, ids::LOGSTF_ROUNDS);
    assert!(players < team, "players at {players:?}, team at {team:?}");
    assert!(team < rounds, "team at {team:?}, rounds at {rounds:?}");
}

#[gpui_kit::test]
fn rounds_open_any_number_and_show_players_and_events(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_ROUNDS);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_round_score(3, 0)).as_deref(),
        Some("0 - 1")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_round_midfight(3, 0)).as_deref(),
        Some("BLU")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_round_midfight(3, 5)).as_deref(),
        Some("-")
    );
    support::click_id(cx, s.win, ids::logstf_round(3, 0));
    // Round 0's panel pushes round 1's header down, possibly out of view.
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_round(3, 1));
    support::click_id(cx, s.win, ids::logstf_round(3, 1));
    assert_eq!(cx.update(|cx| page.read(cx).open_rounds()), [0, 1]);
    assert!(support::present(cx, s.win, ids::logstf_round_panel(3, 0)));
    assert!(support::present(cx, s.win, ids::logstf_round_panel(3, 1)));
    assert!(support::present(
        cx,
        s.win,
        ids::logstf_round_player(3, 0, sid(ARIE))
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_event(3, 0, 0)).as_deref(),
        Some("0:23 Medic Death Meta Knightynski killed by DrHappiness")
    );
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_round(3, 0));
    support::click_id(cx, s.win, ids::logstf_round(3, 0));
    assert_eq!(cx.update(|cx| page.read(cx).open_rounds()), [1]);
    assert!(!support::present(cx, s.win, ids::logstf_round_panel(3, 0)));
}

#[gpui_kit::test]
fn a_round_names_its_players_and_says_who_won(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_ROUNDS);
    support::click_id(cx, s.win, ids::logstf_round(3, 0));
    let (name, kills, dmg, winner) = cx.update(|cx| {
        let log = page.read(cx).detail().unwrap();
        let round = &log.rounds()[0];
        let arie = |id: &tf2tools_core::SteamId| *id == sid(ARIE);
        let line = round
            .players()
            .iter()
            .find(|p| arie(&p.steam_id()))
            .unwrap();
        let player = log.players().iter().find(|p| arie(&p.steam_id())).unwrap();
        (
            player.name().to_owned(),
            line.kills(),
            line.dmg(),
            round.winner(),
        )
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_round_player(3, 0, sid(ARIE))),
        Some(format!("{name} {kills} {dmg}")),
        "the row shows the log's name for the player"
    );
    assert_eq!(winner, Some(tf2tools_core::Team::Red), "RED took round 1");
    let score = ids::logstf_round_score(3, 0);
    support::settle(cx, s.win);
    let said = cx
        .update_window(s.win, |_, w, _| {
            w.try_find(score)
                .and_then(|e| e.description().map(str::to_owned))
        })
        .unwrap();
    assert_eq!(
        said.as_deref(),
        Some("RED won"),
        "the score is in RED's colour"
    );
}

#[gpui_kit::test]
fn a_tracked_player_stays_highlighted_in_a_round(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_ROUNDS);
    support::click_id(cx, s.win, ids::logstf_round(3, 0));
    let other = cx.update(|cx| {
        let log = page.read(cx).detail().unwrap();
        let round = &log.rounds()[0];
        let other = round.players().iter().find(|p| p.steam_id() != sid(ARIE));
        other.unwrap().steam_id()
    });
    // The tint is said in words too, which is what a test can read.
    support::settle(cx, s.win);
    let said = |cx: &mut TestAppContext, id: tf2tools_core::SteamId| {
        let row = ids::logstf_round_player(3, 0, id);
        cx.update_window(s.win, |_, w, _| {
            w.try_find(row)
                .expect("the round's row")
                .description()
                .map(str::to_owned)
        })
        .unwrap()
    };
    assert_eq!(said(cx, sid(ARIE)).as_deref(), Some("Tracked player"));
    assert_eq!(said(cx, other), None);
}

#[gpui_kit::test]
fn a_medic_block_reads_like_logs_tf(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    let rahmed = sid(175_790_976);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_medic(rahmed));
    let facts = [
        ("Healing", "28170 (986/m)"),
        ("Charges:medigun", "Medigun: 12 (100%)"),
        ("Drops", "0"),
        ("Avg time to build", "63 s"),
        ("Avg time before using", "37 s"),
        ("Near full charge deaths", "0"),
        ("Avg uber length", "6.5 s"),
        ("Deaths after charge", "4"),
        ("Major advantages lost", "1"),
        ("Biggest advantage lost", "21 s"),
    ];
    for (label, value) in facts {
        assert_eq!(
            support::label_of(cx, s.win, ids::logstf_medic_fact(rahmed, label)).as_deref(),
            Some(value),
            "{label}"
        );
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_heal_target(rahmed, sid(251_597_547))).as_deref(),
        Some("30%")
    );
    assert!(
        support::present(cx, s.win, ids::logstf_medic(sid(98_265_577))),
        "both medics"
    );
}

#[gpui_kit::test]
fn the_matrix_tabs_switch_and_list_every_player(cx: &mut TestAppContext) {
    use tf2tools_core::MatrixKind;
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_MATRIX);
    assert_eq!(
        cx.update(|cx| page.read(cx).matrix_kind()),
        MatrixKind::Kills
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_matrix_tab(MatrixKind::Kills)),
        Some(true)
    );
    for account in [ARIE, 98_265_577, 285_837_700] {
        assert!(
            support::present(cx, s.win, ids::logstf_matrix_row(sid(account))),
            "{account}"
        );
    }
    support::click_id(cx, s.win, ids::logstf_matrix_tab(MatrixKind::Deaths));
    assert_eq!(
        cx.update(|cx| page.read(cx).matrix_kind()),
        MatrixKind::Deaths
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_matrix_tab(MatrixKind::Deaths)),
        Some(true)
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_matrix_tab(MatrixKind::Kills)),
        Some(false)
    );
}

#[gpui_kit::test]
fn chat_and_killstreaks_open_in_dialogs(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_CHAT_TOGGLE);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_STREAKS_TOGGLE).as_deref(),
        Some("Show Killstreaks (13)")
    );
    support::click_id(cx, s.win, ids::LOGSTF_CHAT_TOGGLE);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_chat_line(0)).as_deref(),
        Some("line between insane and genius: DRHAPPYWIPE")
    );
    // All 69 lines fit an 800x600 window only by scrolling inside the dialog.
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(600.)),
    );
    let list = support::bounds_of(cx, s.win, ids::LOGSTF_CHAT).expect("the chat list");
    let last = support::bounds_of(cx, s.win, ids::logstf_chat_line(68)).expect("the last line");
    assert!(list.bottom() <= gpui_kit::px(600.), "{list:?}");
    assert!(
        last.bottom() > list.bottom(),
        "the chat scrolls: {last:?} {list:?}"
    );
    support::click_dialog_x(cx, s.win);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_CHAT_TOGGLE);
    support::click_id(cx, s.win, ids::LOGSTF_STREAKS_TOGGLE);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_streak(0)).as_deref(),
        Some("0:39 ORANGEjuice 3")
    );
}

#[gpui_kit::test]
fn a_log_with_only_the_required_keys_opens_with_its_sections_left_out(cx: &mut TestAppContext) {
    const MINIMAL: &str = r#"{"length": 60, "teams": {"Red": {}, "Blue": {}},
        "players": {"[U:1:231702]": {"team": "Red",
            "class_stats": [{"type": "scout", "total_time": 60}]}}}"#;
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[(9, day_ago(1))]));
    server.mount_bytes("/json/9", MINIMAL.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 9);
    assert!(
        support::present(cx, s.win, ids::logstf_banner(9)),
        "the log opens"
    );
    assert!(
        !support::present(cx, s.win, ids::LOGSTF_ROUNDS),
        "no rounds section"
    );
    assert!(
        !support::present(cx, s.win, ids::LOGSTF_MEDICS),
        "no medic section"
    );
    assert!(
        support::present(cx, s.win, ids::logstf_matrix_row(sid(ARIE))),
        "matrix rows read as zeros"
    );
    assert!(
        support::disabled_of(cx, s.win, ids::LOGSTF_STREAKS_TOGGLE),
        "nothing to show"
    );
}

#[gpui_kit::test]
fn the_matrix_total_column_is_named_for_its_tab(cx: &mut TestAppContext) {
    use tf2tools_core::MatrixKind;
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_MATRIX);
    let total = |cx: &mut TestAppContext| support::label_of(cx, s.win, ids::LOGSTF_MATRIX_TOTAL);
    assert_eq!(total(cx).as_deref(), Some("K"));
    support::click_id(cx, s.win, ids::logstf_matrix_tab(MatrixKind::KillsAssists));
    assert_eq!(total(cx).as_deref(), Some("KA"));
    support::click_id(cx, s.win, ids::logstf_matrix_tab(MatrixKind::Deaths));
    assert_eq!(total(cx).as_deref(), Some("D"));
}

fn download_shell(
    cx: &mut TestAppContext,
    picks: Vec<std::path::PathBuf>,
    zip: Option<Vec<u8>>,
) -> (
    support::Env,
    support::StoreServer,
    support::ShellWindow,
    Entity<LogsTfPage>,
    Arc<ScriptedPicker>,
) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    match zip {
        Some(bytes) => server.mount_bytes("/logs/log_3.log.zip", bytes),
        None => server.mount_status("/logs/log_3.log.zip", 404),
    }
    let picker = Arc::new(ScriptedPicker::new(picks));
    let s = shell_with_picker(
        cx,
        &env,
        &server.uri(),
        &[(ARIE, "Arie")],
        Arc::clone(&picker),
    );
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_download(3));
    (env, server, s, page, picker)
}

#[gpui_kit::test]
fn download_log_saves_the_zip_where_the_player_chose(cx: &mut TestAppContext) {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("my log.zip");
    let (_env, _server, s, _page, picker) =
        download_shell(cx, vec![path.clone()], Some(b"PK\x03\x04raw".to_vec()));
    support::click_id(cx, s.win, ids::logstf_download(3));
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Log downloaded"), "{toast}");
    assert_eq!(std::fs::read(&path).unwrap(), b"PK\x03\x04raw");
    assert_eq!(picker.suggested(), ["log_3.log.zip"]);
}

#[gpui_kit::test]
fn a_log_logs_tf_lost_is_not_saved_and_says_why(cx: &mut TestAppContext) {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("log.zip");
    let (_env, _server, s, _page, _picker) = download_shell(cx, vec![path.clone()], None);
    support::click_id(cx, s.win, ids::logstf_download(3));
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Couldn't download this log."), "{toast}");
    assert!(toast.contains("logs.tf no longer has this log"), "{toast}");
    support::assert_plain(&toast);
    assert!(!path.exists(), "nothing is written");
}

#[gpui_kit::test]
fn a_file_that_cannot_be_written_says_so(cx: &mut TestAppContext) {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("no such folder").join("log.zip");
    let (_env, _server, s, _page, _picker) =
        download_shell(cx, vec![path.clone()], Some(b"PK\x03\x04raw".to_vec()));
    support::click_id(cx, s.win, ids::logstf_download(3));
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Couldn't download this log."), "{toast}");
    assert!(toast.contains("Couldn't write the file"), "{toast}");
    assert!(!path.exists(), "nothing is written");
}

#[gpui_kit::test]
fn a_cancelled_save_does_nothing(cx: &mut TestAppContext) {
    let (_env, server, s, _page, picker) = download_shell(cx, Vec::new(), Some(b"zip".to_vec()));
    support::click_id(cx, s.win, ids::logstf_download(3));
    // Wait until the picker was asked, then let the cancelled answer run out before
    // asserting, so an ignored cancel would have time to download and toast.
    support::wait_real(cx, "the save picker is asked", |_| {
        !picker.suggested().is_empty()
    });
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::TOAST), "no toast");
    assert_eq!(
        requests_to(&server, "/logs/log_3.log.zip"),
        0,
        "nothing downloaded"
    );
}
