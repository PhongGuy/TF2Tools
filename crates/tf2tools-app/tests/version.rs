#[test]
fn version_flag_prints_the_crate_version_without_a_window() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tf2tools"))
        .arg("--version")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        format!("tf2tools {}", env!("CARGO_PKG_VERSION"))
    );
}
