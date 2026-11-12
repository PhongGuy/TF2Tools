//! The KOTH team clocks read as a round in progress: BLU counting down, RED still at 3:00.

use tf2tools_core::SampleSet;

const BLUE: &str = "HudKothTimeStatus/BlueTimer/TimePanelValue";
const RED: &str = "HudKothTimeStatus/RedTimer/TimePanelValue";

/// A `m:ss` clock as seconds.
fn seconds(clock: &str) -> u32 {
    let (m, s) = clock.split_once(':').expect("m:ss");
    m.parse::<u32>().unwrap() * 60 + s.parse::<u32>().unwrap()
}

#[test]
fn the_two_clocks_differ_red_at_the_start_and_blue_counting_down() {
    let blue = SampleSet::koth_clock(BLUE).expect("BLU clock text");
    let red = SampleSet::koth_clock(RED).expect("RED clock text");
    assert_ne!(blue, red, "only the team holding the point counts down");
    assert_eq!(red, "3:00", "RED has not held the point yet");
    assert!(seconds(blue) < 180, "BLU part-way down: {blue}");
}

#[test]
fn the_clock_matches_any_case_and_only_the_koth_values() {
    assert_eq!(
        SampleSet::koth_clock(&RED.to_ascii_lowercase()),
        Some("3:00")
    );
    let round = "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue";
    assert_eq!(SampleSet::koth_clock(round), None);
    assert_eq!(SampleSet::koth_clock("HudKothTimeStatus/BlueTimer"), None);
}
