//! The full log: rounds and events, weapons, medic details, class matrices, streaks and chat.

use tf2tools_core::{EventKind, LogDetail, Medigun, STEAM_ID64_BASE, SteamId, Team, parse_log};

use crate::common::logstf::logstf_fixture;

fn sid(account: u64) -> SteamId {
    SteamId::from_id64(STEAM_ID64_BASE + account).unwrap()
}

fn six() -> LogDetail {
    parse_log(&logstf_fixture("log-4137103-6v6.json")).unwrap()
}

fn hl() -> LogDetail {
    parse_log(&logstf_fixture("log-4150405-hl.json")).unwrap()
}

#[test]
fn six_v_six_rounds_carry_running_scores_winners_and_midfights() {
    use Team::{Blue, Red};
    let log = six();
    let rounds = log.rounds();
    let scores: Vec<(u32, u32)> = rounds
        .iter()
        .map(|r| (r.blue().score(), r.red().score()))
        .collect();
    assert_eq!(scores, [(0, 1), (0, 2), (0, 3), (1, 3), (2, 3), (2, 3)]);
    let winners: Vec<Option<Team>> = rounds.iter().map(|r| r.winner()).collect();
    assert_eq!(
        winners,
        [
            Some(Red),
            Some(Red),
            Some(Red),
            Some(Blue),
            Some(Blue),
            Some(Red)
        ]
    );
    let midfights: Vec<Option<Team>> = rounds.iter().map(|r| r.midfight()).collect();
    assert_eq!(
        midfights,
        [
            Some(Blue),
            Some(Blue),
            Some(Red),
            Some(Blue),
            Some(Blue),
            None
        ]
    );
    let lengths: Vec<u32> = rounds.iter().map(|r| r.length()).collect();
    assert_eq!(lengths, [242, 689, 101, 270, 391, 21]);
    assert_eq!(lengths.iter().sum::<u32>(), log.length());
    let first = &rounds[0];
    assert_eq!(
        (
            first.blue().kills(),
            first.blue().dmg(),
            first.blue().ubers()
        ),
        (15, 4845, 1)
    );
    assert_eq!(
        (first.red().kills(), first.red().dmg(), first.red().ubers()),
        (15, 4811, 1)
    );
}

#[test]
fn every_round_lists_every_player_most_kills_first() {
    let log = six();
    for round in log.rounds() {
        assert_eq!(round.players().len(), 12);
    }
    let top = &log.rounds()[0].players()[0];
    assert_eq!(
        (top.steam_id(), top.team(), top.kills(), top.dmg()),
        (sid(158_517_868), Team::Blue, 7, 1968)
    );
    let quiet = log.rounds()[0]
        .players()
        .iter()
        .find(|p| p.steam_id() == sid(170_965_550))
        .unwrap();
    assert_eq!(
        (quiet.team(), quiet.kills(), quiet.dmg()),
        (Team::Blue, 0, 385)
    );
    assert_eq!(hl().rounds()[0].players().len(), 18);
}

#[test]
fn events_read_their_kind_on_the_logs_clock() {
    let log = six();
    let counts: Vec<usize> = log.rounds().iter().map(|r| r.events().len()).collect();
    assert_eq!(counts, [11, 34, 7, 13, 17, 1]);
    let r0 = log.rounds()[0].events();
    assert_eq!(r0[0].at(), 23);
    assert_eq!(
        r0[0].kind(),
        &EventKind::MedicDeath {
            team: Team::Blue,
            medic: sid(98_265_577),
            killer: Some(sid(172_588_788)),
        }
    );
    assert_eq!(r0[1].at(), 41);
    assert_eq!(
        r0[1].kind(),
        &EventKind::PointCap {
            team: Team::Blue,
            point: 3
        }
    );
    let charge = r0
        .iter()
        .find(|e| matches!(e.kind(), EventKind::Charge { .. }))
        .unwrap();
    assert_eq!(charge.at(), 214);
    assert_eq!(
        charge.kind(),
        &EventKind::Charge {
            team: Team::Red,
            medic: sid(175_790_976),
            medigun: Medigun::MediGun,
        }
    );
    // The clock runs on across rounds, and each round ends on its round_win.
    let ends: Vec<u32> = log
        .rounds()
        .iter()
        .map(|r| r.events().last().unwrap().at())
        .collect();
    assert_eq!(ends, [242, 948, 1066, 1353, 1761, 1799]);
    assert_eq!(
        log.rounds()[3].events().last().unwrap().kind(),
        &EventKind::RoundWin { team: Team::Blue }
    );
}

#[test]
fn a_highlander_log_has_drops_and_charges() {
    let log = hl();
    let drop = log.rounds()[1]
        .events()
        .iter()
        .find(|e| matches!(e.kind(), EventKind::Drop { .. }))
        .unwrap();
    assert_eq!(drop.at(), 479);
    assert_eq!(
        drop.kind(),
        &EventKind::Drop {
            team: Team::Blue,
            medic: sid(306_968_798)
        }
    );
    let first = &log.rounds()[0].events()[0];
    assert_eq!(first.at(), 60);
    assert_eq!(
        first.kind(),
        &EventKind::Charge {
            team: Team::Red,
            medic: sid(302_123_669),
            medigun: Medigun::MediGun,
        }
    );
    let six = six();
    assert!(
        !six.rounds()
            .iter()
            .flat_map(|r| r.events())
            .any(|e| matches!(e.kind(), EventKind::Drop { .. }))
    );
}

#[test]
fn unknown_events_and_mediguns_keep_their_raw_names() {
    let body = r#"{"length": 100, "teams": {"Red": {}, "Blue": {}}, "players": {},
        "rounds": [{"length": 100, "winner": null, "firstcap": null,
            "team": {"Blue": {"score": 0}, "Red": {"score": 0}}, "players": {},
            "events": [
                {"type": "charge", "time": 5, "team": "Red", "steamid": "[U:1:7]", "medigun": "kritzkrieg"},
                {"type": "charge", "time": 6, "team": "Red", "steamid": "[U:1:7]", "medigun": "quickfix"},
                {"type": "charge", "time": 7, "team": "Red", "steamid": "[U:1:7]", "medigun": "vaccinator"},
                {"type": "charge", "time": 8, "team": "Red", "steamid": "[U:1:7]", "medigun": "new_gun"},
                {"type": "medic_death", "time": 9, "team": "Blue", "steamid": "[U:1:8]", "killer": null},
                {"type": "capture_flag", "time": 10, "team": "Blue"},
                {"type": "pointcap", "time": 11, "point": 2}
            ]}]}"#;
    let log = parse_log(body).unwrap();
    let round = &log.rounds()[0];
    assert_eq!((round.winner(), round.midfight()), (None, None));
    let gun = |i: usize| match round.events()[i].kind() {
        EventKind::Charge { medigun, .. } => medigun.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        [gun(0), gun(1), gun(2), gun(3)],
        [
            Medigun::Kritzkrieg,
            Medigun::QuickFix,
            Medigun::Vaccinator,
            Medigun::Other("new_gun".to_owned()),
        ]
    );
    assert_eq!(Medigun::from_key("quickfix").key(), "quickfix");
    assert_eq!(Medigun::Other("new_gun".to_owned()).key(), "new_gun");
    assert_eq!(
        round.events()[4].kind(),
        &EventKind::MedicDeath {
            team: Team::Blue,
            medic: sid(8),
            killer: None
        }
    );
    assert_eq!(
        round.events()[5].kind(),
        &EventKind::Other {
            name: "capture_flag".to_owned()
        }
    );
    // A known type missing its team keeps its raw name.
    assert_eq!(
        round.events()[6].kind(),
        &EventKind::Other {
            name: "pointcap".to_owned()
        }
    );
}

#[test]
fn a_malformed_rounds_section_reads_as_empty_and_a_bad_round_is_skipped() {
    for rounds in ["null", "5", r#"{"a": 1}"#] {
        let body = format!(
            r#"{{"length": 9, "teams": {{"Red": {{}}, "Blue": {{}}}}, "players": {{}}, "rounds": {rounds}}}"#
        );
        assert!(parse_log(&body).unwrap().rounds().is_empty(), "{rounds}");
    }
    let missing = r#"{"length": 9, "teams": {"Red": {}, "Blue": {}}, "players": {}}"#;
    assert!(parse_log(missing).unwrap().rounds().is_empty());
    let body = r#"{"length": 9, "teams": {"Red": {}, "Blue": {}}, "players": {},
        "rounds": [7, {"length": 9, "winner": "Blue", "events": null, "players": 3}]}"#;
    let log = parse_log(body).unwrap();
    assert_eq!(log.rounds().len(), 1);
    assert_eq!(log.rounds()[0].winner(), Some(Team::Blue));
    assert_eq!(log.rounds()[0].blue().score(), 0);
    assert!(log.rounds()[0].events().is_empty() && log.rounds()[0].players().is_empty());
}

#[test]
fn team_totals_read_midfights_from_firstcaps() {
    let (six, hl) = (six(), hl());
    assert_eq!((six.blue().midfights(), six.red().midfights()), (4, 1));
    assert_eq!((hl.blue().midfights(), hl.red().midfights()), (1, 4));
}

#[test]
fn class_lines_list_weapons_by_kills_then_damage() {
    let log = six();
    let arie = log
        .players()
        .iter()
        .find(|p| p.steam_id() == sid(231_702))
        .unwrap();
    let scout: Vec<(&str, u32, u32, u32, u32)> = arie.classes()[0]
        .weapons()
        .iter()
        .map(|w| (w.name(), w.kills(), w.dmg(), w.shots(), w.hits()))
        .collect();
    assert_eq!(
        scout,
        [
            ("scattergun", 7, 3334, 290, 161),
            ("pistol_scout", 0, 177, 201, 21)
        ]
    );
    let pyro: Vec<&str> = arie.classes()[1]
        .weapons()
        .iter()
        .map(|w| w.name())
        .collect();
    assert_eq!(pyro, ["shotgun_pyro", "flamethrower"]);
    let heavy = &arie.classes()[2].weapons()[0];
    assert_eq!(
        (heavy.name(), heavy.shots(), heavy.hits()),
        ("long_heatmaker", 0, 0)
    );
}

#[test]
fn a_class_without_a_readable_weapon_map_has_no_weapons() {
    let body = r#"{"length": 60, "teams": {"Red": {}, "Blue": {}}, "players": {
        "[U:1:5]": {"team": "Red", "class_stats": [
            {"type": "scout", "total_time": 30, "weapon": null},
            {"type": "soldier", "total_time": 20,
             "weapon": {"rocket": {"kills": "2", "dmg": 90.4}, "bad": 5}}
        ]}}}"#;
    let log = parse_log(body).unwrap();
    let classes = log.players()[0].classes();
    assert!(classes[0].weapons().is_empty());
    assert_eq!(
        classes[1].weapons().len(),
        1,
        "a non-object entry is skipped"
    );
    let rocket = &classes[1].weapons()[0];
    assert_eq!(
        (rocket.name(), rocket.kills(), rocket.dmg(), rocket.shots()),
        ("rocket", 2, 90, 0)
    );
}
