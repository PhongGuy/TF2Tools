use tf2tools_app::updater::UpdateBoot;

// A QA run must never check a real feed once a signing key ships; only a build with the
// test-key feature reads the loopback seam.
#[cfg(not(feature = "update-test-key"))]
#[test]
fn a_pilot_run_turns_updates_off() {
    assert!(matches!(tf2tools_pilot::update_boot(), UpdateBoot::Off));
}

#[cfg(feature = "update-test-key")]
#[test]
fn a_test_key_pilot_reads_the_seam_from_the_environment() {
    assert!(matches!(
        tf2tools_pilot::update_boot(),
        UpdateBoot::Environment
    ));
}
