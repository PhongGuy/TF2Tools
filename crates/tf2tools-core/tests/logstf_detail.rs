//! Reading a logs.tf log body into the expanded view's shape.

use tf2tools_core::{LogParseError, Medigun, STEAM_ID64_BASE, SteamId, Team, TfClass, parse_log};

use crate::common::logstf::{PlayerSpec, log_body, logstf_fixture};

fn sid(account: u64) -> SteamId {
    SteamId::from_id64(STEAM_ID64_BASE + account).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn a_synthetic_medic_keeps_player_totals_and_drops_unknown_classes() {
    let medic = PlayerSpec {
        account: 5,
        name: "five",
        team: "Blue",
        heal: 5000,
        ubers: 3,
        drops: 1,
        classes: vec![
            ("medic", 500, 0, 1, 4, 80),
            ("scout", 100, 2, 1, 0, 300),
            ("undefined", 20, 0, 0, 0, 0),
        ],
    };
    let log = parse_log(&log_body(620, &[medic])).unwrap();
    let p = &log.players()[0];
    assert_eq!((p.heal(), p.ubers(), p.drops()), (5000, 3, 1));
    let classes: Vec<TfClass> = p.classes().iter().map(|c| c.class()).collect();
    assert_eq!(classes, [TfClass::Medic, TfClass::Scout]);
    assert_eq!(
        (p.kills(), p.deaths(), p.assists(), p.dmg()),
        (2, 2, 4, 380)
    );
}

#[test]
fn a_highlander_log_reads_header_teams_and_columns() {
    let log = parse_log(&logstf_fixture("log-4150405-hl.json")).unwrap();
    assert_eq!(log.length(), 2068);
    assert_eq!(log.title(), "vanguard.tf | Mix Highlander \u{2014} #834");
    assert_eq!(log.map(), "koth_proside_v1");
    assert_eq!(log.date(), 1_794_350_113);
    let red = log.red();
    assert_eq!(
        (
            red.score(),
            red.kills(),
            red.dmg(),
            red.charges(),
            red.drops(),
            red.caps()
        ),
        (3, 260, 108_447, 17, 2, 17)
    );
    let blue = log.blue();
    assert_eq!(
        (
            blue.score(),
            blue.kills(),
            blue.dmg(),
            blue.charges(),
            blue.drops(),
            blue.caps()
        ),
        (2, 230, 94_834, 19, 2, 15)
    );
    assert_eq!(log.players().len(), 18);
    assert!(log.players()[..9].iter().all(|p| p.team() == Team::Red));
    assert!(log.players()[9..].iter().all(|p| p.team() == Team::Blue));
    let c = log.columns();
    assert!(c.hp() && c.bs() && c.hs() && c.airshots() && c.caps() && c.dt());
}

#[test]
fn a_highlander_medic_has_charges_spread_and_class_kills() {
    let log = parse_log(&logstf_fixture("log-4150405-hl.json")).unwrap();
    assert_eq!(log.medics().len(), 2);
    let medic = log
        .medics()
        .iter()
        .find(|m| m.steam_id() == sid(302_123_669))
        .expect("Jefrey.exe");
    assert_eq!(medic.name(), "Jefrey.exe");
    assert_eq!(medic.heal(), 38_005);
    assert!(
        close(medic.heal_per_min(), 1102.66),
        "{}",
        medic.heal_per_min()
    );
    assert_eq!(medic.charges(), [(Medigun::MediGun, 17)]);
    assert_eq!(medic.drops(), 2);
    assert!(close(medic.avg_time_to_build(), 56.74));
    assert!(close(medic.avg_uber_length(), 6.56));
    assert_eq!(medic.deaths_after_uber(), 2);
    assert_eq!(medic.heal_spread().len(), 9);
    assert_eq!(medic.heal_spread()[0].steam_id(), sid(866_286_911));
    assert_eq!(medic.heal_spread()[0].healing(), 13_725);
    let kills = log
        .class_kills()
        .iter()
        .find(|k| k.steam_id() == sid(302_123_669))
        .unwrap();
    assert_eq!(kills.kills(), [(TfClass::Spy, 1)]);
}

#[test]
fn a_6v6_player_reads_classes_longest_first_and_hidden_columns() {
    let log = parse_log(&logstf_fixture("log-4137103-6v6.json")).unwrap();
    let c = log.columns();
    assert!(!c.hs() && !c.bs());
    assert!(c.hp() && c.airshots() && c.caps() && c.dt());
    let arie = log
        .players()
        .iter()
        .find(|p| p.steam_id() == sid(231_702))
        .unwrap();
    assert_eq!(arie.name(), "Arie - serveme.tf");
    assert_eq!(arie.team(), Team::Red);
    let classes: Vec<(TfClass, u32)> = arie
        .classes()
        .iter()
        .map(|c| (c.class(), c.seconds()))
        .collect();
    assert_eq!(
        classes,
        [
            (TfClass::Scout, 1559),
            (TfClass::Pyro, 114),
            (TfClass::Heavy, 41)
        ]
    );
    assert_eq!(arie.classes()[0].kills(), 7);
    assert_eq!(arie.classes()[0].dmg(), 3511);
    assert_eq!(
        (
            arie.kills(),
            arie.assists(),
            arie.deaths(),
            arie.dmg(),
            arie.da_per_min()
        ),
        (7, 6, 21, 3896, 136)
    );
    assert!(close(arie.kapd(), 0.6) && close(arie.kpd(), 0.3));
    assert_eq!((arie.dt(), arie.medkits(), arie.caps()), (5964, 19, 5));
    assert!(close(arie.dt_per_min(), 208.77), "{}", arie.dt_per_min());
}

#[test]
fn only_teams_players_and_length_are_required() {
    let log = parse_log(
        r#"{"length": 90, "teams": {"Red": {"score": 1}, "Blue": {}}, "players": {}, "extra": [1, 2]}"#,
    )
    .unwrap();
    assert_eq!(log.length(), 90);
    assert_eq!(log.red().score(), 1);
    assert_eq!(log.blue().score(), 0);
    assert!(log.players().is_empty() && log.medics().is_empty());
    assert!(!log.columns().hp());
    assert_eq!(log.title(), "");

    for body in [
        r#"{"length": 90, "players": {}}"#,
        r#"{"teams": {"Red": {}, "Blue": {}}, "players": {}}"#,
        r#"{"success": false, "error": "Log not found."}"#,
        "not json",
    ] {
        let err = parse_log(body).unwrap_err();
        assert!(
            matches!(err, LogParseError::Format { .. }),
            "{body}: {err:?}"
        );
    }
}

#[test]
fn bad_player_keys_and_unknown_classes_are_skipped() {
    let body = r#"{
        "length": 600,
        "teams": {"Red": {}, "Blue": {}},
        "names": {"[U:1:5]": "five"},
        "players": {
            "BOT": {"team": "Red", "kills": 1, "class_stats": []},
            "[U:1:x]": {"team": "Red", "kills": 1, "class_stats": []},
            "[U:1:5]": {"team": "Blue", "kills": 3, "kapd": "oops", "deaths": 0, "assists": 1,
                "class_stats": [
                    {"type": "undefined", "kills": 0, "deaths": 0, "assists": 0, "dmg": 0, "total_time": 30},
                    {"type": "soldier", "kills": 3, "deaths": 0, "assists": 1, "dmg": 900, "total_time": 570}
                ]}
        }
    }"#;
    let log = parse_log(body).unwrap();
    assert_eq!(log.players().len(), 1);
    let p = &log.players()[0];
    assert_eq!(p.name(), "five");
    assert_eq!(p.classes().len(), 1);
    assert_eq!(p.classes()[0].class(), TfClass::Soldier);
    assert!(close(p.kapd(), 4.0), "computed (3 + 1) / max(0, 1)");
}

#[test]
fn computing_kapd_from_huge_kills_and_assists_does_not_overflow() {
    let body = r#"{
        "length": 600,
        "teams": {"Red": {}, "Blue": {}},
        "names": {"[U:1:5]": "five"},
        "players": {
            "[U:1:5]": {"team": "Blue", "kills": 4294967295, "assists": 4294967295, "deaths": 0,
                "class_stats": []}
        }
    }"#;
    let log = parse_log(body).unwrap();
    let p = &log.players()[0];
    assert!(close(p.kapd(), 2.0 * f64::from(u32::MAX)));
}

#[test]
fn classes_with_equal_time_list_by_class_name_like_the_log_row() {
    let tied = PlayerSpec {
        account: 5,
        name: "five",
        team: "Red",
        heal: 0,
        ubers: 0,
        drops: 0,
        classes: vec![
            ("soldier", 300, 1, 1, 1, 100),
            ("medic", 300, 1, 1, 1, 100),
            ("scout", 600, 1, 1, 1, 100),
        ],
    };
    let log = parse_log(&log_body(1200, &[tied])).unwrap();
    let classes: Vec<TfClass> = log.players()[0]
        .classes()
        .iter()
        .map(|c| c.class())
        .collect();
    assert_eq!(classes, [TfClass::Scout, TfClass::Medic, TfClass::Soldier]);
}
