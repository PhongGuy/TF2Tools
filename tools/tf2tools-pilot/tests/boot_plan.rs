use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use tf2tools_pilot::{BootPlan, PRIVATE_HOME_VARS, REAL_VAR, boot_plan, real_home_env};

// Paths here are never opened; the plan only names them.
const LAUNCH: &str = "/launch/data";

#[test]
fn without_the_real_switch_the_launch_dir_is_seeded() {
    assert_eq!(REAL_VAR, "TF2TOOLS_PILOT_REAL");
    for real in [None, Some(OsStr::new("")), Some(OsStr::new("0"))] {
        assert_eq!(
            boot_plan(real, None, Path::new(LAUNCH)),
            BootPlan::Seeded {
                data: PathBuf::from(LAUNCH)
            },
            "{real:?}"
        );
    }
}

#[test]
fn the_data_variable_still_picks_the_seeded_dir() {
    assert_eq!(
        boot_plan(None, Some(OsStr::new("/kept")), Path::new(LAUNCH)),
        BootPlan::Seeded {
            data: PathBuf::from("/kept")
        }
    );
    assert_eq!(
        boot_plan(None, Some(OsStr::new("")), Path::new(LAUNCH)),
        BootPlan::Seeded {
            data: PathBuf::from(LAUNCH)
        }
    );
}

#[test]
fn the_real_switch_selects_the_shipped_boot() {
    assert_eq!(
        boot_plan(Some(OsStr::new("1")), None, Path::new(LAUNCH)),
        BootPlan::Real { data: None }
    );
    assert_eq!(
        boot_plan(
            Some(OsStr::new("1")),
            Some(OsStr::new("")),
            Path::new(LAUNCH)
        ),
        BootPlan::Real { data: None }
    );
}

// The host hides XDG_DATA_HOME, so the data variable is the only way to keep a library.
#[test]
fn the_real_switch_still_uses_a_data_dir_the_launch_passed() {
    assert_eq!(
        boot_plan(
            Some(OsStr::new("1")),
            Some(OsStr::new("/kept")),
            Path::new(LAUNCH)
        ),
        BootPlan::Real {
            data: Some(PathBuf::from("/kept"))
        }
    );
}

#[test]
fn real_mode_drops_the_launch_private_home_and_keeps_the_runtime_dirs() {
    for var in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
    ] {
        assert!(PRIVATE_HOME_VARS.contains(&var), "{var}");
    }
    for var in ["XDG_RUNTIME_DIR", "TMPDIR", "GPUI_PILOT_SOCKET"] {
        assert!(!PRIVATE_HOME_VARS.contains(&var), "{var}");
    }
}

// fontconfig has no passwd fallback, so HOME must come back as the user's own home.
#[test]
fn real_mode_points_home_at_the_passwd_home_and_leaves_xdg_unset() {
    let env = real_home_env(Some(PathBuf::from("/home/player")));
    assert_eq!(env.home, Some(PathBuf::from("/home/player")));
    assert_eq!(env.unset, PRIVATE_HOME_VARS);
    assert!(
        env.unset
            .iter()
            .all(|v| *v == "HOME" || v.starts_with("XDG_"))
    );
}

#[test]
fn with_no_passwd_home_real_mode_leaves_home_unset() {
    assert_eq!(real_home_env(None).home, None);
}
