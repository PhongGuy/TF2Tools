//! Steam ID and profile-link parsing for the logs.tf page.

use tf2tools_core::{
    PlayerInput, PlayerInputError, STEAM_ID64_BASE, SteamId, TfClass, logstf_class,
    parse_player_input,
};

const GABE: u64 = 76_561_197_960_287_930;
const ARIE: u64 = 76_561_197_960_497_430;

fn id(input: &str) -> u64 {
    match parse_player_input(input) {
        Ok(PlayerInput::Id(id)) => id.id64(),
        other => panic!("{input:?} gave {other:?}"),
    }
}

fn vanity(input: &str) -> String {
    match parse_player_input(input) {
        Ok(PlayerInput::Vanity(name)) => name,
        other => panic!("{input:?} gave {other:?}"),
    }
}

#[test]
fn a_steamid64_parses_after_trimming() {
    assert_eq!(id("76561197960497430"), ARIE);
    assert_eq!(id("  76561197960497430 \n"), ARIE);
}

#[test]
fn steam3_and_legacy_ids_convert_to_id64() {
    assert_eq!(id("[U:1:231702]"), ARIE);
    assert_eq!(id("STEAM_0:0:115851"), ARIE);
    assert_eq!(id("STEAM_1:0:11101"), GABE);
    assert_eq!(id("STEAM_0:1:115850"), ARIE - 1);
}

#[test]
fn profile_links_parse_with_any_scheme_www_and_trailing_slash() {
    assert_eq!(
        id("https://steamcommunity.com/profiles/76561197960287930"),
        GABE
    );
    assert_eq!(
        id("http://www.steamcommunity.com/profiles/76561197960287930/"),
        GABE
    );
    assert_eq!(id("steamcommunity.com/profiles/76561197960287930"), GABE);
    assert_eq!(id("https://logs.tf/profile/76561197960497430"), ARIE);
    assert_eq!(
        vanity("https://steamcommunity.com/id/gabelogannewell/"),
        "gabelogannewell"
    );
    assert_eq!(
        vanity("steamcommunity.com/id/gabelogannewell"),
        "gabelogannewell"
    );
}

#[test]
fn a_bare_word_is_a_vanity_name() {
    assert_eq!(vanity("gabelogannewell"), "gabelogannewell");
    assert_eq!(vanity("a-b_c9"), "a-b_c9");
    assert_eq!(vanity("ab"), "ab");
    assert_eq!(vanity(&"x".repeat(32)), "x".repeat(32));
}

#[test]
fn anything_else_is_not_steam() {
    let too_long = "x".repeat(33);
    for input in [
        "",
        "   ",
        "x",
        too_long.as_str(),
        "has space",
        "https://example.com/profiles/76561197960287930",
        "https://steamcommunity.com/profiles/notanumber",
        "https://steamcommunity.com/id/",
        "[U:1:abc]",
        "[U:2:5]",
        "STEAM_0:2:5",
        "76561190000000001",
    ] {
        assert_eq!(
            parse_player_input(input),
            Err(PlayerInputError::NotSteam),
            "{input:?}"
        );
    }
    assert_eq!(
        PlayerInputError::NotSteam.to_string(),
        "That doesn't look like a Steam profile link or Steam ID."
    );
}

#[test]
fn steam_id_round_trips_between_forms() {
    let arie = SteamId::from_id64(ARIE).unwrap();
    assert_eq!(arie.steam3(), "[U:1:231702]");
    assert_eq!(SteamId::from_steam3("[U:1:231702]"), Some(arie));
    assert_eq!(arie.to_string(), "76561197960497430");
    assert_eq!(SteamId::from_id64(STEAM_ID64_BASE - 1), None);
    assert_eq!(SteamId::from_steam3("[U:1:231702"), None);
}

#[test]
fn logs_tf_class_names_map_to_classes() {
    assert_eq!(logstf_class("scout"), Some(TfClass::Scout));
    assert_eq!(logstf_class("demoman"), Some(TfClass::Demoman));
    assert_eq!(logstf_class("heavyweapons"), Some(TfClass::Heavy));
    assert_eq!(logstf_class("spy"), Some(TfClass::Spy));
    assert_eq!(logstf_class("undefined"), None);
    assert_eq!(logstf_class("heavy"), None);
}

#[test]
fn ids_above_the_7656119_prefix_are_ids_not_vanity() {
    let high = STEAM_ID64_BASE + u64::from(u32::MAX);
    assert_eq!(id("76561202255233023"), high);
    assert_eq!(id("76561200000000000"), 76_561_200_000_000_000);
    assert_eq!(
        parse_player_input("76561202255233024"),
        Err(PlayerInputError::NotSteam)
    );
}

#[test]
fn pasted_links_may_carry_query_fragment_path_case_and_mobile_host() {
    assert_eq!(
        id("https://steamcommunity.com/profiles/76561197960287930/?l=english"),
        GABE
    );
    assert_eq!(
        id("https://steamcommunity.com/profiles/76561197960287930?l=english"),
        GABE
    );
    assert_eq!(
        id("https://steamcommunity.com/profiles/76561197960287930#top"),
        GABE
    );
    assert_eq!(
        id("https://steamcommunity.com/profiles/76561197960287930/games"),
        GABE
    );
    assert_eq!(
        id("HTTPS://SteamCommunity.com/profiles/76561197960287930"),
        GABE
    );
    assert_eq!(
        id("https://m.steamcommunity.com/profiles/76561197960287930"),
        GABE
    );
    assert_eq!(id("https://logs.tf/profile/76561197960497430#x"), ARIE);
    assert_eq!(
        vanity("https://steamcommunity.com/id/gabelogannewell/home"),
        "gabelogannewell"
    );
    assert_eq!(
        vanity("https://steamcommunity.com/id/gabelogannewell?l=english"),
        "gabelogannewell"
    );
    assert_eq!(vanity("Steamcommunity.com/id/Gabe"), "Gabe");
}
