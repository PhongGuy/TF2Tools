//! Isolation checks that keep every timing run away from the user's real data.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub const REAL_DATA_VAR: &str = "TF2TOOLS_REAL_DATA";
pub const SESSION_VAR: &str = "TIMING_SCRATCH";

/// The real data dir: `exported` if given (must be absolute); else, inside a session, an error
/// naming REAL_DATA_VAR; else `platform()`.
pub fn resolve_real_data(
    exported: Option<PathBuf>,
    in_session: bool,
    platform: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<PathBuf, String> {
    // Inside the session every XDG dir is scratch, so the platform default would name the
    // scratch itself; the outer process must hand the real path down instead.
    match exported {
        Some(path) if path.is_absolute() => Ok(path),
        Some(path) => Err(format!(
            "{REAL_DATA_VAR} must be absolute: {}",
            path.display()
        )),
        None if in_session => Err(format!(
            "{REAL_DATA_VAR} is not set inside a timing session ({SESSION_VAR} is set)"
        )),
        None => platform(),
    }
}

/// `resolve_real_data` over this process's env and `DataDir::platform_default()`'s root.
pub fn real_data_dir() -> Result<PathBuf, String> {
    resolve_real_data(
        std::env::var_os(REAL_DATA_VAR).map(PathBuf::from),
        std::env::var_os(SESSION_VAR).is_some(),
        || {
            tf2tools_core::DataDir::platform_default()
                .map(|dir| dir.root().to_path_buf())
                .map_err(|e| e.to_string())
        },
    )
}

/// Absolute, no `..`, and neither inside nor containing `real`.
pub fn assert_scratch(scratch: &Path, real: &Path) -> Result<(), String> {
    let shown = scratch.display();
    if !scratch.is_absolute() || scratch.components().any(|c| c == Component::ParentDir) {
        return Err(format!(
            "scratch must be an absolute path without '..': {shown}"
        ));
    }
    if overlaps(scratch, real) {
        return Err(format!(
            "scratch overlaps the real data dir {}: {shown}",
            real.display()
        ));
    }
    Ok(())
}

/// Missing or an empty dir.
pub fn assert_fresh(dir: &Path) -> Result<(), String> {
    // KWin keeps its output config in the scratch, so a reused one could change the output size.
    match std::fs::read_dir(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot read scratch {}: {e}", dir.display())),
        Ok(mut entries) => match entries.next() {
            None => Ok(()),
            Some(_) => Err(format!(
                "scratch {} is not empty; use a fresh one",
                dir.display()
            )),
        },
    }
}

/// Every absolute-path argument lies apart from `real` (inside or containing it is refused).
pub fn refuse_real_args(args: &[OsString], real: &Path) -> Result<(), String> {
    for arg in args {
        let path = Path::new(arg);
        if path.is_absolute() && overlaps(path, real) {
            return Err(format!(
                "argument {} overlaps the real data dir {}",
                path.display(),
                real.display()
            ));
        }
    }
    Ok(())
}

/// One path lies inside the other (or they are equal).
fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    const REAL: &str = "/home/u/.local/share/com.tf2tools.app";

    fn args(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn scratch_must_be_absolute_and_apart_from_the_real_data() {
        let real = Path::new(REAL);
        assert!(assert_scratch(Path::new("rel/scratch"), real).is_err());
        assert!(assert_scratch(Path::new("/tmp/a/../b"), real).is_err());
        assert!(assert_scratch(Path::new("/home/u/.local/share"), real).is_err());
        assert!(
            assert_scratch(Path::new("/home/u/.local/share/com.tf2tools.app/x"), real).is_err()
        );
        assert!(
            assert_scratch(Path::new("/home/u/.cache/tf2tools-scratch-m2f/sess"), real).is_ok()
        );
    }

    #[test]
    fn a_used_scratch_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(assert_fresh(tmp.path()).is_ok());
        assert!(assert_fresh(&tmp.path().join("missing")).is_ok());
        std::fs::write(tmp.path().join("kwinoutputconfig.json"), "{}").unwrap();
        assert!(assert_fresh(tmp.path()).is_err());
    }

    #[test]
    fn no_argument_may_name_the_real_data() {
        let real = Path::new(REAL);
        let ok = args(&[
            "/bin/app_timing",
            "--seed",
            "/home/u/.cache/s/tpl-1/data/com.tf2tools.app",
            "--runs",
            "10",
        ]);
        assert!(refuse_real_args(&ok, real).is_ok());
        let err = refuse_real_args(&args(&["/bin/app_timing", "--seed", REAL]), real).unwrap_err();
        assert!(err.contains(REAL), "{err}");
        let inside = args(&["x", "/home/u/.local/share/com.tf2tools.app/library"]);
        assert!(refuse_real_args(&inside, real).is_err());
        assert!(refuse_real_args(&args(&["x", "/home/u/.local"]), real).is_err());
    }

    #[test]
    fn inside_a_session_the_real_dir_must_be_exported() {
        let platform = || Ok(PathBuf::from("/scratch/sess/data/com.tf2tools.app"));
        let err = resolve_real_data(None, true, platform).unwrap_err();
        assert!(err.contains(REAL_DATA_VAR), "{err}");
        assert!(resolve_real_data(Some(PathBuf::from("rel/data")), true, platform).is_err());
        assert_eq!(
            resolve_real_data(Some(PathBuf::from(REAL)), true, platform),
            Ok(PathBuf::from(REAL))
        );
        assert_eq!(
            resolve_real_data(Some(PathBuf::from(REAL)), false, platform),
            Ok(PathBuf::from(REAL))
        );
        assert_eq!(
            resolve_real_data(None, false, platform),
            Ok(PathBuf::from("/scratch/sess/data/com.tf2tools.app"))
        );
    }
}
