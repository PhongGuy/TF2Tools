//! A meter's count label shows a made-up count, as a health label shows the sample's health.

use tf2tools_core::{GameSituation, SampleSet};

#[test]
fn the_progress_count_reads_three() {
    let s = GameSituation::default();
    assert_eq!(SampleSet::Empty.fill_in("%progresscount%", &s), "3");
    assert_eq!(SampleSet::Empty.fill_in("x%ProgressCount%", &s), "x3");
    assert_eq!(SampleSet::Casual.fill_in("%progresscount%", &s), "3");
    assert_eq!(
        SampleSet::Empty.fill_in("%health%", &s),
        "150",
        "the other numbers stay"
    );
}
