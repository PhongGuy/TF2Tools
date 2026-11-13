//! Lowercase HUD installs: on Linux TF2 ignores a HUD whose folder or files carry capitals,
//! so the installed copy is renamed to lowercase and its own file references follow.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{IoFailure, io_at, tree_copy};

/// Whether HUDs install in lowercase on this platform.
pub(super) const LOWERCASE_INSTALLS: bool = cfg!(target_os = "linux");

/// Text files whose file references are lowercased to match the renamed files.
const TEXT_EXTENSIONS: [&str; 4] = [".res", ".txt", ".vdf", ".vmt"];

/// A quoted value ending in one of these names a file.
const FILE_EXTENSIONS: [&str; 7] = [".res", ".txt", ".ttf", ".otf", ".vdf", ".vtf", ".vmt"];

/// Whether the install at `tf/custom/<folder>` is one this platform lowercases. An install
/// made before lowercasing keeps its capitals, so a live update leaves it as it was.
pub(super) fn lowercases(folder: &str) -> bool {
    LOWERCASE_INSTALLS && folder == folder.to_ascii_lowercase()
}

/// Whether the file at `rel` is a text file whose references are lowercased.
pub(super) fn is_text_file(rel: &Path) -> bool {
    rel.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|name| TEXT_EXTENSIONS.iter().any(|ext| name.ends_with(ext)))
}

/// Which of `entries` (paths relative to one root) survive lowercasing, each mapped to its
/// lowercase path. Among siblings that differ only in case, the one already lowercase wins,
/// else the first in byte order; a losing folder takes its contents with it.
pub(super) fn plan(entries: &[PathBuf]) -> BTreeMap<PathBuf, PathBuf> {
    let mut sorted: Vec<&PathBuf> = entries.iter().collect();
    sorted.sort();
    let mut winners: HashMap<(PathBuf, OsString), OsString> = HashMap::new();
    for rel in &sorted {
        let (Some(parent), Some(name)) = (rel.parent(), rel.file_name()) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        let winner = winners
            .entry((parent.to_path_buf(), lower.clone()))
            .or_insert_with(|| name.to_os_string());
        if name == lower {
            *winner = name.to_os_string();
        }
    }
    let wins = |rel: &Path| {
        rel.ancestors()
            .filter(|a| !a.as_os_str().is_empty())
            .all(|a| match (a.parent(), a.file_name()) {
                (Some(parent), Some(name)) => winners
                    .get(&(parent.to_path_buf(), name.to_ascii_lowercase()))
                    .is_some_and(|winner| winner == name),
                _ => false,
            })
    };
    sorted
        .into_iter()
        .filter(|rel| wins(rel))
        .map(|rel| {
            (
                rel.clone(),
                PathBuf::from(rel.as_os_str().to_ascii_lowercase()),
            )
        })
        .collect()
}

/// Lowercases every name under `dir` as `plan` says, removing the entries it drops, then the
/// file references in its text files. `folder` is the name the HUD installs under.
pub(super) fn lowercase_tree(dir: &Path, folder: &str) -> Result<(), IoFailure> {
    let entries = tree_copy::entries(dir, &|_: &Path| false)?;
    let rels: Vec<PathBuf> = entries.iter().map(|(rel, _)| rel.clone()).collect();
    let kept = plan(&rels);
    // A dropped entry under a kept folder goes; one under a dropped folder goes with it.
    for (rel, is_dir) in &entries {
        let parent_kept = rel
            .parent()
            .is_none_or(|p| p.as_os_str().is_empty() || kept.contains_key(p));
        if kept.contains_key(rel) || !parent_kept {
            continue;
        }
        let path = dir.join(rel);
        let removed = if *is_dir {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        removed.map_err(io_at(&path))?;
    }
    // Deepest first, so each rename happens inside a parent still under its old name.
    let mut renames: Vec<(&PathBuf, &PathBuf)> = kept.iter().collect();
    renames.sort_by_key(|(rel, _)| std::cmp::Reverse(rel.components().count()));
    for (rel, lower) in renames {
        if let (Some(name), Some(lower_name)) = (rel.file_name(), lower.file_name())
            && name != lower_name
        {
            let from = dir.join(rel);
            std::fs::rename(&from, from.with_file_name(lower_name)).map_err(io_at(&from))?;
        }
    }
    for (_, lower) in kept.iter().filter(|(_, lower)| is_text_file(lower)) {
        let path = dir.join(lower);
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(io_at(&path))?;
        let rewritten = lowercase_bytes(&bytes, folder);
        if rewritten != bytes {
            std::fs::write(&path, rewritten).map_err(io_at(&path))?;
        }
    }
    Ok(())
}

/// `bytes` with their file references lowercased when they are UTF-8 text, else as they are.
pub(super) fn lowercase_bytes(bytes: &[u8], folder: &str) -> Vec<u8> {
    match std::str::from_utf8(bytes) {
        Ok(text) => lowercase_references(text, folder).into_bytes(),
        Err(_) => bytes.to_vec(),
    }
}

/// `text` with the file paths in it lowercased: `#base` and `#include` targets, quoted
/// values naming a file by its extension, and quoted paths into `custom/<folder>/`. ASCII
/// lowercasing keeps every byte offset, so everything else stays byte for byte.
pub(super) fn lowercase_references(text: &str, folder: &str) -> String {
    let mut out = text.as_bytes().to_vec();
    let own = format!("custom/{}/", folder.to_ascii_lowercase());
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if let Some(range) = directive_target(line) {
            out[at + range.start..at + range.end].make_ascii_lowercase();
        }
        // Quoted values never span lines, so an unpaired quote costs only its own line.
        let mut rest = 0;
        while let Some(open) = line[rest..].find('"').map(|i| rest + i + 1) {
            let Some(close) = line[open..].find(['"', '\n']).map(|i| open + i) else {
                break;
            };
            if line.as_bytes()[close] != b'"' {
                break;
            }
            // KeyValues may escape a backslash as `\\`, so a separator run counts as one.
            let mut value = line[open..close].to_ascii_lowercase().replace('\\', "/");
            while value.contains("//") {
                value = value.replace("//", "/");
            }
            if FILE_EXTENSIONS.iter().any(|ext| value.ends_with(ext)) || value.contains(&own) {
                out[at + open..at + close].make_ascii_lowercase();
            }
            rest = close + 1;
        }
        at += line.len();
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

/// Where in `line` a `#base` or `#include` directive's target sits, quoted or bare.
fn directive_target(line: &str) -> Option<std::ops::Range<usize>> {
    let lead = line.len() - line.trim_start().len();
    let lower = line[lead..].to_ascii_lowercase();
    let word = ["#base", "#include"]
        .into_iter()
        .find(|word| lower.starts_with(word))?;
    let after = lead + word.len();
    let start = after + (line[after..].len() - line[after..].trim_start().len());
    let target = &line[start..];
    if let Some(quoted) = target.strip_prefix('"') {
        let end = quoted.find(['"', '\n']).unwrap_or(quoted.len());
        Some(start + 1..start + 1 + end)
    } else {
        let end = target.find(char::is_whitespace).unwrap_or(target.len());
        Some(start..start + end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    fn planned(list: &[&str]) -> Vec<(String, String)> {
        plan(&paths(list))
            .into_iter()
            .map(|(a, b)| (a.display().to_string(), b.display().to_string()))
            .collect()
    }

    #[test]
    fn every_name_is_lowercased() {
        assert_eq!(
            planned(&["Resource", "Resource/UI", "Resource/UI/HudLayout.res"]),
            [
                ("Resource".into(), "resource".into()),
                ("Resource/UI".into(), "resource/ui".into()),
                (
                    "Resource/UI/HudLayout.res".into(),
                    "resource/ui/hudlayout.res".into()
                ),
            ]
        );
    }

    #[test]
    fn a_collision_keeps_the_lowercase_entry_with_its_contents() {
        assert_eq!(
            planned(&["Resource", "Resource/a.res", "resource", "resource/b.res"]),
            [
                ("resource".into(), "resource".into()),
                ("resource/b.res".into(), "resource/b.res".into()),
            ]
        );
    }

    #[test]
    fn a_collision_with_no_lowercase_entry_keeps_the_first_in_order() {
        assert_eq!(
            planned(&["HudLayout.res", "hudLayout.res"]),
            [("HudLayout.res".into(), "hudlayout.res".into())]
        );
    }

    #[test]
    fn base_and_include_targets_are_lowercased() {
        let text = "#base \"Scheme/Base.res\"\n  #include Other/File.res // Keep\n#BASE \"X\"\n";
        assert_eq!(
            lowercase_references(text, "kn-edit"),
            "#base \"scheme/base.res\"\n  #include other/file.res // Keep\n#BASE \"x\"\n"
        );
    }

    #[test]
    fn quoted_file_paths_are_lowercased() {
        let text = "\"font\" \"Resource/Fonts/TF2Build.TTF\" \"x\" \"A.otf\" \"vdf\" \"B.VDF\"\n\
                    \"t\" \"C.vtf\" \"m\" \"D.vmt\" \"s\" \"E.txt\" \"w\" \"F\\\\G.res\"";
        assert_eq!(
            lowercase_references(text, "kn-edit"),
            "\"font\" \"resource/fonts/tf2build.ttf\" \"x\" \"a.otf\" \"vdf\" \"b.vdf\"\n\
             \"t\" \"c.vtf\" \"m\" \"d.vmt\" \"s\" \"e.txt\" \"w\" \"f\\\\g.res\""
        );
    }

    #[test]
    fn paths_into_the_huds_own_folder_are_lowercased() {
        let text = "\"image\" \"../../custom/KN-Edit/Materials/Bg\"\n\
                    \"image\" \"../../custom/OtherHud/Materials/Bg\"\n\
                    \"image\" \"..\\\\..\\\\custom\\\\KN-Edit\\\\Bg\"";
        assert_eq!(
            lowercase_references(text, "kn-edit"),
            "\"image\" \"../../custom/kn-edit/materials/bg\"\n\
             \"image\" \"../../custom/OtherHud/Materials/Bg\"\n\
             \"image\" \"..\\\\..\\\\custom\\\\kn-edit\\\\bg\""
        );
    }

    #[test]
    fn labels_colours_fonts_and_controls_stay_as_written() {
        let text = "\"ControlName\" \"CExLabel\"\n\"labelText\" \"%Health%\"\n\
                    \"name\" \"TF2 Build\"\n\"TanLight\" \"235 226 202 255\"\n\
                    // An \"Unclosed quote\nthen \"Next.Res\"";
        assert_eq!(
            lowercase_references(text, "kn-edit"),
            text.replace("Next.Res", "next.res")
        );
    }

    #[test]
    fn a_tree_is_renamed_deepest_first_and_its_text_rewritten() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("kn-edit");
        let write = |rel: &str, bytes: &[u8]| {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        };
        write("Resource/UI/HudLayout.res", b"#base \"Base.res\" \"Label\"");
        write("Resource/Fonts/Font.TTF", b"\"Binary.res\"");
        write("Resource/UI/Odd.res", b"\"Odd.res\" \xff");
        lowercase_tree(&dir, "kn-edit").unwrap();
        let read = |rel: &str| std::fs::read(dir.join(rel)).unwrap();
        assert_eq!(
            read("resource/ui/hudlayout.res"),
            b"#base \"base.res\" \"Label\""
        );
        assert_eq!(read("resource/fonts/font.ttf"), b"\"Binary.res\"");
        assert_eq!(read("resource/ui/odd.res"), b"\"Odd.res\" \xff");
        let mut names: Vec<String> = walkdir::WalkDir::new(&dir)
            .min_depth(1)
            .into_iter()
            .map(|e| {
                e.unwrap()
                    .path()
                    .strip_prefix(&dir)
                    .unwrap()
                    .display()
                    .to_string()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "resource",
                "resource/fonts",
                "resource/fonts/font.ttf",
                "resource/ui",
                "resource/ui/hudlayout.res",
                "resource/ui/odd.res",
            ]
        );
    }

    // Two names differing only in case need a case-sensitive file system.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_tree_collision_drops_the_capitalised_entry() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("hud");
        std::fs::create_dir_all(dir.join("Resource")).unwrap();
        std::fs::create_dir_all(dir.join("resource")).unwrap();
        std::fs::write(dir.join("Resource/a.res"), "a").unwrap();
        std::fs::write(dir.join("resource/b.res"), "b").unwrap();
        std::fs::write(dir.join("Info.vdf"), "upper").unwrap();
        std::fs::write(dir.join("info.vdf"), "lower").unwrap();
        lowercase_tree(&dir, "hud").unwrap();
        assert!(!dir.join("Resource").exists());
        assert!(!dir.join("resource/a.res").exists());
        assert_eq!(std::fs::read(dir.join("resource/b.res")).unwrap(), b"b");
        assert_eq!(std::fs::read(dir.join("info.vdf")).unwrap(), b"lower");
        assert!(!dir.join("Info.vdf").exists());
    }

    #[test]
    fn only_lowercase_folders_are_lowercased_installs() {
        assert_eq!(lowercases("kn-edit"), LOWERCASE_INSTALLS);
        assert!(!lowercases("KN-Edit"));
    }
}
