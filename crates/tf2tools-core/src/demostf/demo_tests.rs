//! The demos.tf wire shapes and the rule that picks a log's demo.

use serde_json::json;

use super::{DemoQuery, DemoSummary, parse_detail, parse_list, pick_demo};
use crate::error::DemosError;
use crate::logstf::{STEAM_ID64_BASE, SteamId, parse_log};

const MAP: &str = "cp_process_f12";
const DATE: i64 = 1_795_296_722;
const LENGTH: u32 = 1800;

fn sid(account: u64) -> SteamId {
    SteamId::from_id64(STEAM_ID64_BASE + account).unwrap()
}

fn query_of(length: u32) -> DemoQuery {
    DemoQuery {
        map: MAP.to_owned(),
        date: DATE,
        length,
        players: vec![sid(1), sid(2)],
    }
}

fn url(id: u64) -> String {
    format!("https://static.demos.tf/aa/bb/{id}.dem")
}

/// One list item as api.demos.tf sends it, fields the client ignores included.
fn wire(id: u64, map: &str, time: i64, duration: u32, url: &str) -> serde_json::Value {
    json!({"id": id, "url": url, "name": format!("match-{id}.dem"), "server": "serveme.tf #1",
           "duration": duration, "nick": "SourceTV Demo", "map": map, "time": time,
           "red": "RED", "blue": "BLU", "redScore": 0, "blueScore": 5, "playerCount": 12,
           "uploader": 1, "hash": "h", "backend": "static", "path": "/p", "private_until": null})
}

fn picked_for(query: &DemoQuery, items: &[serde_json::Value]) -> Option<u64> {
    let list = parse_list(json!(items).to_string().as_bytes()).unwrap();
    pick_demo(query, &list).map(DemoSummary::id)
}

fn picked(items: &[serde_json::Value]) -> Option<u64> {
    picked_for(&query_of(LENGTH), items)
}

/// A logs.tf body `parse_log` accepts, one Red scout per account.
fn log_json(map: &str, date: i64, length: u32, accounts: &[u64]) -> String {
    let stats = json!([{"type": "scout", "total_time": 600, "kills": 1, "deaths": 1,
                        "assists": 1, "dmg": 100}]);
    let players: serde_json::Map<String, serde_json::Value> = accounts
        .iter()
        .map(|a| {
            let line = json!({"team": "Red", "kills": 1, "deaths": 1, "assists": 1, "dmg": 100,
                              "heal": 0, "ubers": 0, "drops": 0, "class_stats": stats});
            (format!("[U:1:{a}]"), line)
        })
        .collect();
    json!({"version": 3, "length": length, "teams": {"Red": {}, "Blue": {}},
           "players": players, "names": {}, "info": {"map": map, "date": date, "title": "t"}})
    .to_string()
}

#[test]
fn a_list_reads_every_field_the_app_uses() {
    let body = json!([wire(7, MAP, DATE, 1790, &url(7))]).to_string();
    let list = parse_list(body.as_bytes()).unwrap();
    assert_eq!(list.len(), 1);
    let demo = &list[0];
    assert_eq!(demo.id(), 7);
    assert_eq!(demo.url(), url(7));
    assert_eq!(demo.name(), "match-7.dem");
    assert_eq!(demo.map(), MAP);
    assert_eq!(demo.time(), DATE);
    assert_eq!(demo.duration(), 1790);
    assert_eq!(demo.server(), "serveme.tf #1");
}

#[test]
fn a_private_demo_reads_with_a_blank_url() {
    let mut null_url = wire(7, MAP, DATE, 1790, "");
    null_url["url"] = serde_json::Value::Null;
    let body = json!([null_url, wire(8, MAP, DATE, 1790, "")]).to_string();
    let list = parse_list(body.as_bytes()).unwrap();
    assert_eq!(list[0].url(), "");
    assert_eq!(list[1].url(), "");
}

#[test]
fn a_body_that_is_not_a_list_of_demos_is_unreadable() {
    for body in [
        &b"<html>busy</html>"[..],
        &b"{}"[..],
        &b"[{\"id\": \"x\"}]"[..],
    ] {
        assert!(matches!(parse_list(body), Err(DemosError::Unreadable)));
    }
    assert!(matches!(parse_detail(b"[]"), Err(DemosError::Unreadable)));
}

#[test]
fn the_detail_reads_its_players_and_skips_a_bad_steam_id() {
    let mut body = wire(7, MAP, DATE, 1790, &url(7));
    body["players"] = json!([
        {"id": 1, "user_id": 2, "name": "a", "team": "red", "class": "medic",
         "steamid": (STEAM_ID64_BASE + 1).to_string(), "kills": 0, "assists": 1, "deaths": 2},
        {"id": 2, "user_id": 3, "name": "b", "team": "blue", "class": "scout",
         "steamid": "bogus", "kills": 0, "assists": 1, "deaths": 2}
    ]);
    let detail = parse_detail(body.to_string().as_bytes()).unwrap();
    assert_eq!(detail.summary().id(), 7);
    assert_eq!(detail.players().len(), 1);
    assert_eq!(detail.players()[0].steam_id(), sid(1));
    assert_eq!(detail.players()[0].name(), "a");
}

#[test]
fn one_close_demo_is_the_match() {
    assert_eq!(
        picked(&[wire(7, MAP, DATE + 120, LENGTH + 30, &url(7))]),
        Some(7)
    );
}

#[test]
fn no_demo_or_one_on_another_map_is_no_match() {
    assert_eq!(picked(&[]), None);
    assert_eq!(
        picked(&[wire(7, "cp_gullywash_f9", DATE, LENGTH, &url(7))]),
        None
    );
}

#[test]
fn the_upload_times_may_differ_by_five_minutes_at_most() {
    assert_eq!(
        picked(&[wire(7, MAP, DATE + 300, LENGTH, &url(7))]),
        Some(7)
    );
    assert_eq!(
        picked(&[wire(7, MAP, DATE - 300, LENGTH, &url(7))]),
        Some(7)
    );
    assert_eq!(picked(&[wire(7, MAP, DATE + 301, LENGTH, &url(7))]), None);
    assert_eq!(picked(&[wire(7, MAP, DATE - 301, LENGTH, &url(7))]), None);
}

#[test]
fn the_lengths_may_differ_by_a_tenth_or_ninety_seconds_whichever_is_more() {
    // 1800 s allows 180 s either way.
    assert_eq!(
        picked(&[wire(7, MAP, DATE, LENGTH + 180, &url(7))]),
        Some(7)
    );
    assert_eq!(
        picked(&[wire(7, MAP, DATE, LENGTH - 180, &url(7))]),
        Some(7)
    );
    assert_eq!(picked(&[wire(7, MAP, DATE, LENGTH + 181, &url(7))]), None);
    // 600 s allows 90 s, not 60.
    let short = query_of(600);
    assert_eq!(
        picked_for(&short, &[wire(7, MAP, DATE, 690, &url(7))]),
        Some(7)
    );
    assert_eq!(
        picked_for(&short, &[wire(7, MAP, DATE, 510, &url(7))]),
        Some(7)
    );
    assert_eq!(
        picked_for(&short, &[wire(7, MAP, DATE, 691, &url(7))]),
        None
    );
}

#[test]
fn a_short_log_is_not_matched_to_the_full_match_beside_it() {
    let short = query_of(600);
    assert_eq!(
        picked_for(&short, &[wire(7, MAP, DATE, 1800, &url(7))]),
        None
    );
    let both = [
        wire(7, MAP, DATE, 1800, &url(7)),
        wire(8, MAP, DATE, 620, &url(8)),
    ];
    assert_eq!(picked_for(&short, &both), Some(8));
}

#[test]
fn a_rematch_with_the_same_players_is_no_match() {
    let two = [
        wire(7, MAP, DATE - 200, LENGTH, &url(7)),
        wire(8, MAP, DATE + 250, LENGTH + 60, &url(8)),
    ];
    assert_eq!(picked(&two), None);
}

#[test]
fn a_private_demo_is_never_the_match() {
    assert_eq!(picked(&[wire(7, MAP, DATE, LENGTH, "")]), None);
    let both = [
        wire(7, MAP, DATE, LENGTH, ""),
        wire(8, MAP, DATE, LENGTH, &url(8)),
    ];
    assert_eq!(picked(&both), Some(8));
}

#[test]
fn a_log_queries_its_map_date_length_and_first_two_players() {
    let detail = parse_log(&log_json(MAP, DATE, 1800, &[231_702, 1, 5])).unwrap();
    let query = DemoQuery::from_log(&detail).unwrap();
    assert_eq!(query.map(), MAP);
    assert_eq!(query.date(), DATE);
    assert_eq!(query.length(), 1800);
    let first_two = [
        detail.players()[0].steam_id(),
        detail.players()[1].steam_id(),
    ];
    assert_eq!(query.players(), first_two);
}

#[test]
fn steam3_players_query_as_id64() {
    let detail = parse_log(&log_json(MAP, DATE, 1800, &[231_702, 1])).unwrap();
    let query = DemoQuery::from_log(&detail).unwrap();
    let mut ids: Vec<u64> = query.players().iter().map(|p| p.id64()).collect();
    ids.sort_unstable();
    assert_eq!(ids, [76_561_197_960_265_729, 76_561_197_960_497_430]);
}

#[test]
fn a_log_missing_what_a_lookup_needs_gives_no_query() {
    let none = |body: String| DemoQuery::from_log(&parse_log(&body).unwrap());
    assert_eq!(none(log_json("", DATE, 1800, &[1, 2])), None);
    assert_eq!(none(log_json(MAP, 0, 1800, &[1, 2])), None);
    assert_eq!(none(log_json(MAP, DATE, 0, &[1, 2])), None);
    assert_eq!(none(log_json(MAP, DATE, 1800, &[1])), None);
}
