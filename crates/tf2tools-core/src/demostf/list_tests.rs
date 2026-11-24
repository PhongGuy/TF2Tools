use super::DemoFormat;
use crate::demostf::demo::parse_list;

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
