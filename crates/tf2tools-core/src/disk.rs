//! Free space on the disk holding a path, so large copies are refused before they start.

use std::path::{Path, PathBuf};

/// Free bytes on the disk whose mount point is the longest component prefix of `path`.
/// A path on no listed disk gives `u64::MAX`, so an odd mount never blocks a copy.
///
/// # Errors
/// When `path` cannot be canonicalised.
pub(crate) fn free_bytes(path: &Path) -> std::io::Result<u64> {
    let path = path.canonicalize()?;
    // A platform that cannot list disks reads as no disks, which never blocks a copy.
    let disks: Vec<(PathBuf, u64)> = sysinfo::Disks::new_with_refreshed_list()
        .map(|disks| {
            disks
                .iter()
                .map(|d| (d.mount_point().to_path_buf(), d.available_space()))
                .collect()
        })
        .unwrap_or_default();
    Ok(pick(&path, &disks).unwrap_or(u64::MAX))
}

/// The free bytes of the disk mounted deepest above `path`; `Path::starts_with` compares
/// whole components, so `/home/u2` never claims `/home/u`.
fn pick(path: &Path, disks: &[(PathBuf, u64)]) -> Option<u64> {
    disks
        .iter()
        .filter(|(mount, _)| path.starts_with(mount))
        .max_by_key(|(mount, _)| mount.components().count())
        .map(|(_, free)| *free)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_longest_mount_point_prefix_wins_by_component() {
        let disks = [("/".into(), 1), ("/home".into(), 2), ("/home/u2".into(), 3)];
        assert_eq!(pick(Path::new("/home/u/data"), &disks), Some(2));
        assert_eq!(pick(Path::new("/home/u2/x"), &disks), Some(3));
    }

    #[test]
    fn a_real_folder_has_free_space() {
        let dir = tempfile::tempdir().unwrap();
        assert!(free_bytes(dir.path()).unwrap() > 0);
    }
}
