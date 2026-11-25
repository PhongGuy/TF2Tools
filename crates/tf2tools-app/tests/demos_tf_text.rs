//! The Demos.tf list's words: a demo's title, its format and why the list failed.

use tf2tools_app::pages::demos_tf::text::{demo_title, format_label, list_error};
use tf2tools_core::{DemoFormat, DemosError};

#[test]
fn a_title_names_the_server_and_both_teams() {
    assert_eq!(
        demo_title("Matcha Bookable", "shuffl", "jazzy"),
        "Matcha Bookable - shuffl vs jazzy"
    );
    assert_eq!(demo_title("Matcha Bookable", "", "BLU"), "Matcha Bookable");
    assert_eq!(demo_title("Matcha Bookable", "RED", ""), "Matcha Bookable");
}

#[test]
fn every_format_has_its_name() {
    let names: Vec<String> = DemoFormat::ALL
        .iter()
        .map(|f| format_label(*f).to_string())
        .collect();
    assert_eq!(
        names,
        [
            "Highlander",
            "Prolander",
            "6v6",
            "4v4",
            "3v3",
            "2v2",
            "1v1",
            "Ultiduo",
            "BBall",
            "Other"
        ]
    );
}

#[test]
fn a_list_error_says_what_to_do() {
    let offline = DemosError::Network {
        url: "u".into(),
        message: "m".into(),
    };
    assert_eq!(list_error(&offline), "Check your internet connection.");
    assert_eq!(
        list_error(&DemosError::RateLimited { url: "u".into() }),
        "demos.tf is busy. Try again in a few minutes."
    );
    assert_eq!(
        list_error(&DemosError::Http {
            url: "u".into(),
            status: 500
        }),
        "demos.tf isn't responding right now. Try again later."
    );
}
