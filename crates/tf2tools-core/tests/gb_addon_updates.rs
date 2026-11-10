//! The daily GameBanana update check and which payloads an update keeps.

use crate::common::{self, gb_addon_file};
use crate::gb_addon_install::{Setup, download, no, setup, vpk};

use serde_json::json;
use tf2tools_core::GbId;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const NOW: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

async fn installed(s: &Setup, id: u64, date: i64) {
    let d = download(s, id, "A", "a.vpk", vpk(b"v"), date).await;
    s.env
        .lib
        .install_gb_addon(d, &["a.vpk".to_owned()], &s.tf2, &no)
        .unwrap();
}

fn set_checked(s: &Setup, id: u64, at: i64) {
    common::sql(&s.env.dir)
        .execute(
            "UPDATE library_gb_addons SET checked_at = ?1 WHERE gb_id = ?2",
            rusqlite::params![at, id as i64],
        )
        .unwrap();
}

async fn profile_body(s: &Setup, id: u64, body: serde_json::Value) {
    s.env.server.reset().await;
    Mock::given(method("GET"))
        .and(path(format!("/apiv11/Mod/{id}/ProfilePage")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&s.env.server)
        .await;
}

async fn profile_calls(s: &Setup) -> usize {
    s.env
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().ends_with("/ProfilePage"))
        .count()
}

fn entry(s: &Setup, id: u64) -> tf2tools_core::GbAddonEntry {
    s.env
        .lib
        .list_gb_addons(Some(&s.tf2))
        .unwrap()
        .into_iter()
        .find(|e| e.gb_id() == GbId::new(id))
        .unwrap()
}

#[tokio::test]
async fn due_rows_are_those_checked_over_a_day_ago() {
    let s = setup().await;
    installed(&s, 1, 100).await;
    installed(&s, 2, 100).await;
    installed(&s, 3, 100).await;
    set_checked(&s, 1, NOW - DAY - 1);
    set_checked(&s, 2, NOW - 10);
    set_checked(&s, 3, NOW - 3 * DAY);
    let due = s.env.lib.gb_addons_due_for_check(NOW).unwrap();
    assert_eq!(due, [GbId::new(3), GbId::new(1)]);
}

#[tokio::test]
async fn a_check_stores_the_newest_file_date_and_shows_the_update() {
    let s = setup().await;
    installed(&s, 4, 100).await;
    set_checked(&s, 4, NOW - 2 * DAY);
    s.env.server.reset().await;
    let newer = gb_addon_file(&s.env, 4, 0, "a2.vpk", &vpk(b"v2"), 300).await;
    let older = gb_addon_file(&s.env, 4, 1, "a.vpk", &vpk(b"v"), 100).await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Mod/4/ProfilePage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "_idRow": 4, "_sName": "A", "_aSubmitter": {"_sName": "vex"}, "_aFiles": [older, newer]})))
        .mount(&s.env.server)
        .await;
    assert_eq!(s.env.client.check_gb_addon_updates(NOW).await.unwrap(), 1);
    let e = entry(&s, 4);
    assert_eq!(
        (e.file_date(), e.latest_file_date(), e.checked_at()),
        (100, 300, NOW)
    );
    assert!(e.has_update() && !e.is_gone());
}

#[tokio::test]
async fn a_second_check_the_same_day_makes_no_calls() {
    let s = setup().await;
    installed(&s, 5, 100).await;
    set_checked(&s, 5, NOW - 2 * DAY);
    profile_body(&s, 5, json!({"_idRow": 5, "_sName": "A", "_aFiles": []})).await;
    assert_eq!(s.env.client.check_gb_addon_updates(NOW).await.unwrap(), 1);
    let calls = profile_calls(&s).await;
    assert_eq!(
        s.env.client.check_gb_addon_updates(NOW + 60).await.unwrap(),
        0
    );
    assert_eq!(profile_calls(&s).await, calls);
    let e = entry(&s, 5);
    assert_eq!(
        (e.latest_file_date(), e.checked_at()),
        (100, NOW),
        "no files keeps the dates"
    );
}

#[tokio::test]
async fn trashed_and_missing_items_are_marked_gone_and_kept() {
    let s = setup().await;
    installed(&s, 6, 100).await;
    set_checked(&s, 6, NOW - 2 * DAY);
    profile_body(
        &s,
        6,
        json!({"_idRow": 6, "_sName": "A", "_bIsTrashed": true}),
    )
    .await;
    s.env.client.check_gb_addon_updates(NOW).await.unwrap();
    assert!(entry(&s, 6).is_gone());

    installed(&s, 7, 100).await;
    set_checked(&s, 7, NOW - 2 * DAY);
    s.env.server.reset().await; // no ProfilePage mock: wiremock answers 404
    s.env.client.check_gb_addon_updates(NOW).await.unwrap();
    let e = entry(&s, 7);
    assert!(
        e.is_gone() && e.is_installed(),
        "gone keeps the install and its Remove"
    );
}

#[tokio::test]
async fn a_failing_item_waits_a_day_and_the_rest_are_still_checked() {
    let s = setup().await;
    installed(&s, 8, 100).await;
    installed(&s, 81, 100).await;
    set_checked(&s, 8, NOW - 3 * DAY);
    set_checked(&s, 81, NOW - 2 * DAY);
    s.env.server.reset().await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Mod/8/ProfilePage"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&s.env.server)
        .await;
    let newer = gb_addon_file(&s.env, 81, 0, "a2.vpk", &vpk(b"v2"), 300).await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Mod/81/ProfilePage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "_idRow": 81, "_sName": "A", "_aFiles": [newer]})))
        .mount(&s.env.server)
        .await;
    assert_eq!(s.env.client.check_gb_addon_updates(NOW).await.unwrap(), 2);
    let failed = entry(&s, 8);
    assert_eq!((failed.latest_file_date(), failed.checked_at()), (100, NOW));
    assert!(!failed.is_gone());
    assert_eq!(entry(&s, 81).latest_file_date(), 300);
    assert!(s.env.lib.gb_addons_due_for_check(NOW).unwrap().is_empty());
}

#[tokio::test]
async fn an_update_keeps_the_previous_payloads_only_when_all_still_exist() {
    let s = setup().await;
    let two = common::zip_bytes(&[("A.vpk", &vpk(&[1u8; 64])), ("B.vpk", &vpk(b"b"))]);
    let d = download(&s, 9, "AB", "ab.zip", two, 100).await;
    assert_eq!(
        d.kept_payloads(&["B.vpk".to_owned()]),
        Some(vec!["B.vpk".to_owned()])
    );
    assert_eq!(
        d.kept_payloads(&["A.vpk".to_owned(), "B.vpk".to_owned()]),
        Some(vec!["A.vpk".to_owned(), "B.vpk".to_owned()])
    );
    assert_eq!(
        d.kept_payloads(&["A.vpk".to_owned(), "C.vpk".to_owned()]),
        None
    );
    let one = download(&s, 10, "One", "one.vpk", vpk(b"v"), 100).await;
    assert_eq!(
        one.kept_payloads(&["old name".to_owned()]),
        Some(vec!["one.vpk".to_owned()])
    );
}

#[tokio::test]
async fn a_check_recorded_in_the_future_is_due_again() {
    let s = setup().await;
    installed(&s, 11, 100).await;
    set_checked(&s, 11, NOW + 30 * DAY);
    let due = s.env.lib.gb_addons_due_for_check(NOW).unwrap();
    assert_eq!(
        due,
        [GbId::new(11)],
        "a clock set back must not stall checks"
    );
}
