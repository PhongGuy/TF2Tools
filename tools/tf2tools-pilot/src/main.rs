//! The piloted TF2Tools binary: the gpui-pilot host runs the app first thing.

use tf2tools_pilot::{BootPlan, REAL_VAR, boot_plan, real_home_env};

fn main() -> std::process::ExitCode {
    // Real mode resolves the user's home the way the shipped app does, so the launch's private
    // home has to go before any thread starts and before the app reads it.
    let real = std::env::var_os(REAL_VAR);
    if matches!(
        boot_plan(real.as_deref(), None, std::path::Path::new("")),
        BootPlan::Real { .. }
    ) {
        let unset = real_home_env(None).unset;
        // SAFETY: first thing in main, while this is the only thread.
        for var in unset {
            unsafe { std::env::remove_var(var) };
        }
        // With HOME gone, `home_dir` reads the passwd entry.
        if let Some(home) = real_home_env(std::env::home_dir()).home {
            unsafe { std::env::set_var("HOME", home) };
        }
    }
    gpui_pilot_host::run(tf2tools_pilot::Tf2Tools)
}
