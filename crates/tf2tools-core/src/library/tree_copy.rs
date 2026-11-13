//! Link-safe folder copies: every file written anew, links never followed or copied.

use std::path::Path;

use walkdir::WalkDir;

use super::{IoFailure, create_dir_all, io_at, walk_failure};

/// Copies the regular files and folders under `src` into `dst`, leaving out each entry
/// whose relative path `skip` names (a skipped folder takes its contents with it).
/// Returns the bytes copied.
pub(crate) fn copy_tree(
    src: &Path,
    dst: &Path,
    skip: &dyn Fn(&Path) -> bool,
) -> Result<u64, IoFailure> {
    create_dir_all(dst)?;
    let mut bytes = 0u64;
    for (rel, is_dir) in entries(src, skip)? {
        let target = dst.join(&rel);
        if is_dir {
            create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                create_dir_all(parent)?;
            }
            let copied = std::fs::copy(src.join(&rel), &target).map_err(io_at(&target))?;
            bytes = bytes.saturating_add(copied);
        }
    }
    Ok(bytes)
}

/// The bytes `copy_tree` would copy with the same `skip`.
pub(crate) fn tree_bytes(src: &Path, skip: &dyn Fn(&Path) -> bool) -> Result<u64, IoFailure> {
    let mut bytes = 0u64;
    for (rel, is_dir) in entries(src, skip)? {
        if !is_dir {
            let path = src.join(&rel);
            let len = std::fs::symlink_metadata(&path)
                .map_err(io_at(&path))?
                .len();
            bytes = bytes.saturating_add(len);
        }
    }
    Ok(bytes)
}

/// The regular files `copy_tree` would copy with the same `skip`, as relative paths.
pub(crate) fn tree_files(
    src: &Path,
    skip: &dyn Fn(&Path) -> bool,
) -> Result<Vec<std::path::PathBuf>, IoFailure> {
    Ok(entries(src, skip)?
        .into_iter()
        .filter_map(|(rel, is_dir)| (!is_dir).then_some(rel))
        .collect())
}

/// The folders and regular files under `src` as relative paths, with whether each is a
/// folder; links and anything else are left out, and skipped folders are not entered.
pub(super) fn entries(
    src: &Path,
    skip: &dyn Fn(&Path) -> bool,
) -> Result<Vec<(std::path::PathBuf, bool)>, IoFailure> {
    let mut found = Vec::new();
    let walk = WalkDir::new(src)
        .min_depth(1)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.path().strip_prefix(src).is_ok_and(|rel| !skip(rel)));
    for entry in walk {
        let entry = entry.map_err(|e| walk_failure(src, e))?;
        let Ok(rel) = entry.path().strip_prefix(src) else {
            continue;
        };
        let kind = entry.file_type();
        if kind.is_dir() || kind.is_file() {
            found.push((rel.to_path_buf(), kind.is_dir()));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn it_copies_bytes_and_leaves_out_what_skip_names() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src");
        write(&src.join("a.res"), "a");
        write(&src.join(".tf2tools/original/a.res"), "o");
        write(&src.join(".tf2tools/maker.json"), "{}");
        let skip = |rel: &Path| rel == Path::new(".tf2tools/maker.json");
        assert_eq!(tree_bytes(&src, &skip).unwrap(), 2);
        let dst = t.path().join("dst");
        assert_eq!(copy_tree(&src, &dst, &skip).unwrap(), 2);
        assert_eq!(
            std::fs::read_to_string(dst.join(".tf2tools/original/a.res")).unwrap(),
            "o"
        );
        assert!(!dst.join(".tf2tools/maker.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn links_are_never_followed_and_copies_are_new_files() {
        use std::os::unix::fs::MetadataExt;
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src");
        write(&src.join("a.res"), "a");
        write(&t.path().join("out/secret.txt"), "no");
        std::os::unix::fs::symlink(t.path().join("out/secret.txt"), src.join("file_link")).unwrap();
        std::os::unix::fs::symlink(t.path().join("out"), src.join("dir_link")).unwrap();
        let dst = t.path().join("dst");
        copy_tree(&src, &dst, &|_: &Path| false).unwrap();
        assert!(!dst.join("file_link").exists() && !dst.join("dir_link").exists());
        let ino = |p: &Path| std::fs::metadata(p).unwrap().ino();
        assert_ne!(ino(&src.join("a.res")), ino(&dst.join("a.res")));
    }
}
