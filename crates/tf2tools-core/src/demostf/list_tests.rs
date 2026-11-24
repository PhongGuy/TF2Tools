use super::{
    DemoCursor, DemoFormat, DemoListQuery, DemosRange, merge_demos, next_cursor, query_pairs,
};
use crate::demostf::demo::parse_list;
use crate::error::DemosError;
use crate::logstf::{STEAM_ID64_BASE, SteamId};

const LIST: &[u8] = include_bytes!("../../tests/fixtures/demostf/demos_list.json");

#[test]
fn the_format_table_follows_demos_tf() {
    use DemoFormat::*;
    let cases: [(&str, u32, DemoFormat); 22] = [
        ("ctf_bball_alpine_b4", 2, BBall),
        ("ctf_ballin_sky", 4, BBall),
        ("CTF_BBALL_X", 12, BBall),
        ("koth_ultiduo_r_b7", 4, Ultiduo),
        ("koth_ultiduo_r_b7", 6, Ultiduo),
        ("ultitrio_dockport_final6", 6, Threes),
        ("koth_proot_b5b", 19, Highlander),
        ("koth_proot_b5b", 18, Highlander),
        ("koth_proot_b5b", 17, Highlander),
        ("cp_x", 16, Other),
        ("cp_x", 15, Prolander),
        ("cp_x", 14, Prolander),
        ("cp_x", 13, Sixes),
        ("cp_x", 12, Sixes),
        ("cp_x", 11, Sixes),
        ("cp_x", 10, Other),
        ("cp_x", 9, Fours),
        ("cp_x", 7, Fours),
        ("cp_x", 5, Other),
        ("cp_x", 4, Twos),
        ("cp_x", 2, Ones),
        ("cp_x", 0, Other),
    ];
    for (map, count, want) in cases {
        assert_eq!(DemoFormat::derive(map, count), want, "{map} with {count}");
    }
    assert_eq!(DemoFormat::derive("cp_x", 20), Other);
    assert_eq!(DemoFormat::derive("cp_x", 3), Other);
}

#[test]
fn only_server_side_formats_have_a_type_key() {
    let keys: Vec<_> = DemoFormat::ALL.iter().map(|f| f.type_key()).collect();
    assert_eq!(
        keys,
        [
            Some("hl"),
            Some("prolander"),
            Some("6v6"),
            Some("4v4"),
            Some("3v3"),
            Some("2v2"),
            Some("1v1"),
            Some("2v2"),
            None,
            None
        ]
    );
}

#[test]
fn the_live_list_reads_teams_counts_and_formats() {
    let demos = parse_list(LIST).unwrap();
    let got: Vec<_> = demos
        .iter()
        .map(|d| (d.id(), d.player_count(), d.format()))
        .collect();
    use DemoFormat::*;
    assert_eq!(
        got,
        [
            (1538013, 12, Sixes),
            (1538010, 18, Highlander),
            (1536035, 6, Threes),
            (1536063, 12, Sixes),
            (1536842, 4, Ultiduo),
            (1537281, 4, BBall),
            (1516947, 2, BBall),
            (1538012, 12, Sixes),
        ]
    );
    assert_eq!((demos[0].red(), demos[0].blue()), ("RED", "BLU"));
    let private = &demos[7];
    assert_eq!(
        (private.red(), private.blue(), private.url()),
        ("shuffl", "jazzy", "")
    );
    assert!(demos[3].url().starts_with("https://freezer.demos.tf/"));
}

#[test]
fn a_row_without_teams_or_count_still_reads() {
    let body = br#"[{"id":1,"time":2,"duration":3}]"#;
    let demo = parse_list(body).unwrap().remove(0);
    assert_eq!(
        (demo.red(), demo.blue(), demo.player_count(), demo.format()),
        ("", "", 0, DemoFormat::Other)
    );
}

const NOW: i64 = 1_795_500_000;

fn sid(a: u64) -> SteamId {
    SteamId::from_id64(STEAM_ID64_BASE + a).unwrap()
}

fn pairs(q: &DemoListQuery, c: DemoCursor) -> Vec<(&'static str, String)> {
    query_pairs(q, c).unwrap()
}

fn s(k: &'static str, v: impl ToString) -> (&'static str, String) {
    (k, v.to_string())
}

#[test]
fn ranges_reach_back_from_now() {
    let days: Vec<_> = DemosRange::STOPS
        .iter()
        .map(|r| r.reach_secs().map(|s| s / 86_400))
        .collect();
    assert_eq!(
        days,
        [Some(1), Some(7), Some(30), Some(90), Some(365), None]
    );
    assert_eq!(DemosRange::default(), DemosRange::Week1);
    assert_eq!(
        pairs(
            &DemoListQuery::new(DemosRange::Day1, NOW),
            DemoCursor::First
        ),
        [s("after", NOW - 86_400)]
    );
    assert!(pairs(&DemoListQuery::new(DemosRange::All, NOW), DemoCursor::First).is_empty());
}

#[test]
fn every_filter_lands_in_the_query_in_order() {
    let q = DemoListQuery::new(DemosRange::Week1, NOW)
        .with_format(Some(DemoFormat::Sixes))
        .with_map("  cp_process_f12 ")
        .with_players(&[sid(1), sid(2)]);
    assert_eq!(
        pairs(&q, DemoCursor::Page(3)),
        [
            s("after", NOW - 7 * 86_400),
            s("map", "cp_process_f12"),
            s("type", "6v6"),
            s(
                "players",
                format!("{},{}", STEAM_ID64_BASE + 1, STEAM_ID64_BASE + 2)
            ),
            s("page", 3),
        ]
    );
}

#[test]
fn empty_filters_and_client_side_formats_send_nothing() {
    let q = DemoListQuery::new(DemosRange::All, NOW)
        .with_map("   ")
        .with_players(&[])
        .with_format(Some(DemoFormat::BBall));
    assert_eq!(q.map(), None);
    assert_eq!(pairs(&q, DemoCursor::BeforeId(99)), [s("before_id", 99)]);
}

#[test]
fn a_players_query_never_sends_before_id() {
    let q = DemoListQuery::new(DemosRange::All, NOW).with_players(&[sid(1)]);
    assert!(matches!(
        query_pairs(&q, DemoCursor::BeforeId(5)),
        Err(DemosError::Cursor)
    ));
    let first = parse_list(LIST).unwrap();
    assert_eq!(
        next_cursor(&q, DemoCursor::First, &first),
        Some(DemoCursor::Page(2))
    );
    assert_eq!(
        next_cursor(&q, DemoCursor::Page(2), &first),
        Some(DemoCursor::Page(3))
    );
    assert_eq!(next_cursor(&q, DemoCursor::Page(2), &[]), None);
}

#[test]
fn without_players_the_next_page_starts_below_the_lowest_id() {
    let q = DemoListQuery::new(DemosRange::All, NOW);
    let raw = parse_list(LIST).unwrap();
    assert_eq!(
        next_cursor(&q, DemoCursor::First, &raw),
        Some(DemoCursor::BeforeId(1516947))
    );
    assert_eq!(next_cursor(&q, DemoCursor::BeforeId(1516947), &[]), None);
}

#[test]
fn merging_a_shifted_page_keeps_each_id_once() {
    let all = parse_list(LIST).unwrap();
    let mut list = all[..5].to_vec();
    assert_eq!(merge_demos(&mut list, &all[3..]), 3);
    let ids: Vec<_> = list.iter().map(|d| d.id()).collect();
    assert_eq!(ids, all.iter().map(|d| d.id()).collect::<Vec<_>>());
}
