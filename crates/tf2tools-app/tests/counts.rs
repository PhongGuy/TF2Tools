use tf2tools_app::counts::format_count;

#[test]
fn counts_shorten_like_v5() {
    assert_eq!(format_count(0), "0");
    assert_eq!(format_count(999), "999");
    assert_eq!(format_count(1_000), "1.0K");
    assert_eq!(format_count(62_500), "62.5K");
    assert_eq!(format_count(178_749), "178.7K");
    assert_eq!(format_count(1_240_000), "1.2M");
}

#[test]
fn ties_round_up_as_to_fixed_does() {
    assert_eq!(format_count(1_250), "1.3K");
    assert_eq!(format_count(1_249), "1.2K");
    assert_eq!(format_count(1_350), "1.4K");
    assert_eq!(format_count(1_250_000), "1.3M");
    assert_eq!(format_count(999_950), "1000.0K");
}

#[test]
fn inexact_halves_follow_the_binary_value_as_to_fixed_does() {
    // node: (1150/1000).toFixed(1) == "1.1", (1450/1000).toFixed(1) == "1.4", (2150/1000).toFixed(1) == "2.1"
    assert_eq!(format_count(1_150), "1.1K");
    assert_eq!(format_count(1_450), "1.4K");
    assert_eq!(format_count(2_150), "2.1K");
    assert_eq!(format_count(1_750), "1.8K");
}
