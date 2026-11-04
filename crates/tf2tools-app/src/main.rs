#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! The TF2Tools desktop app.

fn main() {
    // Answer a version query before GPUI starts, so packaging checks work without a display.
    if std::env::args().skip(1).any(|a| a == "--version") {
        println!("tf2tools {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    tf2tools_app::run();
}
