//! Lowercase HUD installs: on Linux TF2 ignores a HUD whose folder or files carry capitals,
//! so the installed copy is renamed to lowercase and its own file references follow.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{IoFailure, io_at, tree_copy};

/// Whether HUDs install in lowercase on this platform.
pub(super) const LOWERCASE_INSTALLS: bool = cfg!(target_os = "linux");

/// Text files whose file references are lowercased to match the renamed files.
const TEXT_EXTENSIONS: [&str; 5] = [".res", ".txt", ".vdf", ".vmt", ".cfg"];

/// A quoted value ending in one of these names a file.
const FILE_EXTENSIONS: [&str; 7] = [".res", ".txt", ".ttf", ".otf", ".vdf", ".vtf", ".vmt"];

/// Where the game looks up a material named without its root folder (VGUI images sit
/// under `materials/vgui/`), and the extensions such a name may leave out.
const MATERIAL_LOOKUPS: [(&str, &[&str]); 2] = [
    ("materials", &["", ".vmt", ".vtf"]),
    ("materials/vgui", &["", ".vmt", ".vtf"]),
];

/// A sound is only ever named with one of these.
const SOUND_EXTENSIONS: [&str; 2] = [".wav", ".mp3"];

/// The characters a sound name may start with to set how it plays; not part of its path.
const SOUND_PREFIXES: &[char] = &[
    '*', '#', '@', '<', '>', '^', ')', '(', '}', '$', '!', '?', '&', '~',
];

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

/// What a lowercase install calls its own: the folder it installs under and the lowercase
/// paths of the files it ships, so references to them can follow the renames.
pub(super) struct Own {
    folder: String,
    files: HashSet<String>,
}

impl Own {
    /// The install under `folder` of the files `plan` keeps; `plan`'s folders are left out,
    /// since a folder name alone is never a reference.
    pub(super) fn new(folder: &str, plan: &BTreeMap<PathBuf, PathBuf>) -> Own {
        let folders: HashSet<&Path> = plan.values().filter_map(|lower| lower.parent()).collect();
        Own {
            folder: folder.to_ascii_lowercase(),
            files: plan
                .values()
                .filter(|lower| !folders.contains(lower.as_path()))
                .map(|lower| lower.to_string_lossy().replace('\\', "/"))
                .collect(),
        }
    }

    /// Whether `value` (lowercase, `/`-separated) names a file the HUD ships: as written, as a
    /// material, or as a sound under `sound/` past its play-mode prefix.
    fn ships(&self, value: &str) -> bool {
        let value = value.trim_start_matches('/');
        if value.is_empty() {
            return false;
        }
        let sound = value.trim_start_matches(SOUND_PREFIXES);
        self.files.contains(value)
            || self.found(value, &MATERIAL_LOOKUPS)
            || (SOUND_EXTENSIONS.iter().any(|ext| sound.ends_with(ext))
                && self.found(sound, &[("sound", &[""])]))
    }

    /// Whether `target` names a cfg the HUD ships, as `exec` looks it up under `cfg/`.
    fn ships_cfg(&self, target: &str) -> bool {
        let target = target.trim_start_matches('/');
        !target.is_empty() && self.found(target, &[("cfg", &["", ".cfg"])])
    }

    /// Whether `value` resolves under any of `lookups`' roots, with any of their extensions,
    /// to a shipped file.
    fn found(&self, value: &str, lookups: &[(&str, &[&str])]) -> bool {
        lookups.iter().any(|(base, exts)| {
            exts.iter().any(|ext| {
                resolve(&format!("{base}/{value}{ext}"))
                    .is_some_and(|path| self.files.contains(&path))
            })
        })
    }
}

/// `path` with its `.` and `..` segments resolved, or `None` when it climbs above its root.
fn resolve(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// Lowercases every name under `dir` as `plan` says, removing the entries it drops, then the
/// file references in its text files. `folder` is the name the HUD installs under.
pub(super) fn lowercase_tree(dir: &Path, folder: &str) -> Result<(), IoFailure> {
    let entries = tree_copy::entries(dir, &|_: &Path| false)?;
    let rels: Vec<PathBuf> = entries.iter().map(|(rel, _)| rel.clone()).collect();
    let kept = plan(&rels);
    let own = Own::new(folder, &kept);
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
        let rewritten = lowercase_bytes(lower, &bytes, &own);
        if rewritten != bytes {
            std::fs::write(&path, rewritten).map_err(io_at(&path))?;
        }
    }
    Ok(())
}

/// The text file at `rel` as `bytes` with the file paths in it lowercased. In a cfg only
/// `exec` targets naming a shipped cfg change. Elsewhere: `#base` and `#include` targets,
/// quoted values naming a file by its extension, values naming a file the HUD ships, and
/// quoted paths into `custom/<folder>/`. Only ASCII letters inside those spans change, which
/// is safe for UTF-8 and single-byte encodings such as Windows-1252; UTF-16 is left alone.
pub(super) fn lowercase_bytes(rel: &Path, bytes: &[u8], own: &Own) -> Vec<u8> {
    let mut out = bytes.to_vec();
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) || bytes.contains(&0) {
        return out;
    }
    let is_cfg = rel
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cfg"));
    if is_cfg {
        for range in exec_targets(bytes) {
            if own.ships_cfg(&normalized(&bytes[range.clone()])) {
                out[range].make_ascii_lowercase();
            }
        }
        return out;
    }
    let own_folder = format!("custom/{}/", own.folder);
    let mut at = 0;
    for line in bytes.split_inclusive(|&b| b == b'\n') {
        if let Some(range) = directive_target(line) {
            out[at + range.start..at + range.end].make_ascii_lowercase();
        }
        for (range, quoted) in tokens(line) {
            let value = normalized(&line[range.clone()]);
            let names_file = if quoted {
                FILE_EXTENSIONS.iter().any(|ext| value.ends_with(ext))
                    || value.contains(&own_folder)
                    || own.ships(&value)
            } else {
                own.ships(&value)
            };
            if names_file {
                out[at + range.start..at + range.end].make_ascii_lowercase();
            }
        }
        at += line.len();
    }
    out
}

/// The target of every `exec` command in cfg text `bytes`, including inside quoted alias
/// and bind bodies and `;`-separated sequences.
fn exec_targets(bytes: &[u8]) -> Vec<std::ops::Range<usize>> {
    let is_break = |b: u8| b.is_ascii_whitespace() || matches!(b, b'"' | b';');
    let mut found = Vec::new();
    let mut i = 0;
    while i + 4 < bytes.len() {
        let command = bytes[i..i + 4].eq_ignore_ascii_case(b"exec")
            && (i == 0 || is_break(bytes[i - 1]))
            && matches!(bytes[i + 4], b' ' | b'\t');
        if !command {
            i += 1;
            continue;
        }
        let mut start = i + 4;
        while start < bytes.len() && matches!(bytes[start], b' ' | b'\t') {
            start += 1;
        }
        if bytes.get(start) == Some(&b'"') {
            start += 1;
        }
        let len = bytes[start..]
            .iter()
            .position(|&b| is_break(b))
            .unwrap_or(bytes.len() - start);
        if len > 0 {
            found.push(start..start + len);
        }
        i = start + len;
    }
    found
}

/// `value` lowercased with `/` separators; KeyValues may escape a backslash as `\\`, so a
/// separator run counts as one.
fn normalized(value: &[u8]) -> String {
    let mut value = String::from_utf8_lossy(value)
        .to_ascii_lowercase()
        .replace('\\', "/");
    while value.contains("//") {
        value = value.replace("//", "/");
    }
    value
}

/// The values on `line`: each quoted string's inside, and each bare word between
/// whitespace, braces and quotes. A quote never pairs across lines, so an unpaired one costs
/// only the rest of its line.
fn tokens(line: &[u8]) -> Vec<(std::ops::Range<usize>, bool)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < line.len() {
        match line[i] {
            b'"' => {
                let open = i + 1;
                let Some(len) = line[open..].iter().position(|&b| b == b'"' || b == b'\n') else {
                    break;
                };
                if line[open + len] != b'"' {
                    break;
                }
                found.push((open..open + len, true));
                i = open + len + 1;
            }
            b'{' | b'}' => i += 1,
            b if b.is_ascii_whitespace() => i += 1,
            _ => {
                let len = line[i..]
                    .iter()
                    .position(|&b| b.is_ascii_whitespace() || matches!(b, b'"' | b'{' | b'}'))
                    .unwrap_or(line.len() - i);
                found.push((i..i + len, false));
                i += len;
            }
        }
    }
    found
}

/// Where in `line` a `#base` or `#include` directive's target sits, quoted or bare.
fn directive_target(line: &[u8]) -> Option<std::ops::Range<usize>> {
    let lead = line.iter().take_while(|b| b.is_ascii_whitespace()).count();
    let word = [&b"#base"[..], b"#include"].into_iter().find(|word| {
        line.len() >= lead + word.len() && line[lead..lead + word.len()].eq_ignore_ascii_case(word)
    })?;
    let after = lead + word.len();
    let start = after
        + line[after..]
            .iter()
            .take_while(|b| b.is_ascii_whitespace())
            .count();
    let target = &line[start..];
    if let Some(quoted) = target.strip_prefix(b"\"") {
        let end = quoted
            .iter()
            .position(|&b| b == b'"' || b == b'\n')
            .unwrap_or(quoted.len());
        Some(start + 1..start + 1 + end)
    } else {
        let end = target
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap_or(target.len());
        Some(start..start + end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    /// The `Own` of an install shipping `files`, with their folders listed as a walk would.
    fn own(folder: &str, files: &[&str]) -> Own {
        let mut all: Vec<PathBuf> = paths(files)
            .iter()
            .flat_map(|file| file.ancestors().map(Path::to_path_buf))
            .filter(|path| !path.as_os_str().is_empty())
            .collect();
        all.sort();
        all.dedup();
        Own::new(folder, &plan(&all))
    }

    /// `text` as a lowercase install writes it.
    fn fix(text: &str, own: &Own) -> String {
        String::from_utf8(lowercase_bytes(Path::new("x.res"), text.as_bytes(), own)).unwrap()
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
            fix(text, &own("kn-edit", &[])),
            "#base \"scheme/base.res\"\n  #include other/file.res // Keep\n#BASE \"x\"\n"
        );
    }

    #[test]
    fn quoted_file_paths_are_lowercased() {
        let text = "\"font\" \"Resource/Fonts/TF2Build.TTF\" \"x\" \"A.otf\" \"vdf\" \"B.VDF\"\n\
                    \"t\" \"C.vtf\" \"m\" \"D.vmt\" \"s\" \"E.txt\" \"w\" \"F\\\\G.res\"";
        assert_eq!(
            fix(text, &own("kn-edit", &[])),
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
            fix(text, &own("kn-edit", &[])),
            "\"image\" \"../../custom/kn-edit/materials/bg\"\n\
             \"image\" \"../../custom/OtherHud/Materials/Bg\"\n\
             \"image\" \"..\\\\..\\\\custom\\\\kn-edit\\\\bg\""
        );
    }

    #[test]
    fn references_to_materials_the_hud_ships_are_lowercased() {
        let own = own(
            "kn-edit",
            &[
                "materials/vgui/Replay/Thumbnails/Foo.vmt",
                "materials/vgui/Replay/Thumbnails/Foo.vtf",
                "materials/Hud/Bar.vtf",
                "materials/Sign.vmt",
            ],
        );
        let text = "\"image\" \"Replay/Thumbnails/Foo\"\n\
                    \"$baseTexture\" \"VGUI\\\\Replay/Thumbnails/Foo\"\n\
                    \"image\" \"../Hud/Bar\"\n\
                    \"material\" \"Sign\"\n\
                    \"image\" \"Stock/Thing\"\n\
                    \"labelText\" \"Foo\"\n";
        assert_eq!(
            fix(text, &own),
            "\"image\" \"replay/thumbnails/foo\"\n\
             \"$baseTexture\" \"vgui\\\\replay/thumbnails/foo\"\n\
             \"image\" \"../hud/bar\"\n\
             \"material\" \"sign\"\n\
             \"image\" \"Stock/Thing\"\n\
             \"labelText\" \"Foo\"\n"
        );
    }

    #[test]
    fn text_in_any_ascii_compatible_encoding_is_rewritten() {
        let own = own("kn-edit", &[]);
        assert_eq!(
            lowercase_bytes(
                Path::new("x.res"),
                b"// \xa9 Caf\xe9\n#base \"Scheme/Base.res\"\n",
                &own
            ),
            b"// \xa9 Caf\xe9\n#base \"scheme/base.res\"\n"
        );
    }

    #[test]
    fn utf16_text_is_left_alone() {
        let own = own("kn-edit", &[]);
        let bom = b"\xff\xfe#\0b\0";
        assert_eq!(lowercase_bytes(Path::new("x.res"), bom, &own), bom);
        let nul = b"\"A.res\"\0";
        assert_eq!(lowercase_bytes(Path::new("x.res"), nul, &own), nul);
    }

    #[test]
    fn references_to_sounds_the_hud_ships_are_lowercased() {
        let own = own("kn-edit", &["sound/UI/HitSound.wav", "sound/UI/Kill.mp3"]);
        let text = "\"wave\" \"UI/HitSound.wav\"\n\"wave\" \")UI/Kill.mp3\"\n\
                    \"wave\" \"Stock/Other.WAV\"\n\"labelText\" \"UI/HitSound\"\n";
        assert_eq!(
            fix(text, &own),
            "\"wave\" \"ui/hitsound.wav\"\n\"wave\" \")ui/kill.mp3\"\n\
             \"wave\" \"Stock/Other.WAV\"\n\"labelText\" \"UI/HitSound\"\n"
        );
    }

    #[test]
    fn a_label_naming_a_shipped_cfg_stays_as_written() {
        let own = own("kn-edit", &["cfg/Menu.cfg"]);
        let text = "\"labelText\" \"Menu\"\nlabelText Menu\n";
        assert_eq!(fix(text, &own), text);
    }

    #[test]
    fn exec_targets_in_cfgs_naming_shipped_cfgs_are_lowercased() {
        let own = own("kn-edit", &["cfg/KN/Extra.cfg", "cfg/Menu.cfg"]);
        let text = "exec KN/Extra\nEXEC \"KN/Extra.cfg\"\n\
                    alias x \"exec KN/Extra;echo hi\"\nbind k \"exec KN/Extra\"\n\
                    exec Stock_Thing\necho \"Loaded Foo.txt\"\necho Menu\n";
        let fixed = lowercase_bytes(Path::new("cfg/kn/menu.cfg"), text.as_bytes(), &own);
        assert_eq!(
            String::from_utf8(fixed).unwrap(),
            "exec kn/extra\nEXEC \"kn/extra.cfg\"\n\
             alias x \"exec kn/extra;echo hi\"\nbind k \"exec kn/extra\"\n\
             exec Stock_Thing\necho \"Loaded Foo.txt\"\necho Menu\n"
        );
    }

    #[test]
    fn a_folder_name_alone_is_no_reference() {
        let own = own("kn-edit", &["resource/x.res", "materials/vgui/Foo.vtf"]);
        let text = "\"name\" \"Resource\"\n\"name\" \"VGUI\"\n";
        assert_eq!(fix(text, &own), text);
    }

    #[test]
    fn unquoted_values_naming_shipped_files_are_lowercased() {
        let own = own(
            "kn-edit",
            &["Resource/UI/X.res", "materials/vgui/Replay/Foo.vmt"],
        );
        let text = "Panel\n{\n  file Resource/UI/X.res\n  image Replay/Foo\n  \
                    labelText Foo\n  image Stock/Thing\n  ControlName CExLabel\n}\n";
        assert_eq!(
            fix(text, &own),
            "Panel\n{\n  file resource/ui/x.res\n  image replay/foo\n  \
             labelText Foo\n  image Stock/Thing\n  ControlName CExLabel\n}\n"
        );
    }

    #[test]
    fn labels_colours_fonts_and_controls_stay_as_written() {
        let text = "\"ControlName\" \"CExLabel\"\n\"labelText\" \"%Health%\"\n\
                    \"name\" \"TF2 Build\"\n\"TanLight\" \"235 226 202 255\"\n\
                    // An \"Unclosed quote\nthen \"Next.Res\"";
        assert_eq!(
            fix(text, &own("kn-edit", &[])),
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
        write("cfg/KN/Menu.cfg", b"exec KN/Extra\nexec Stock_Thing\n");
        write("cfg/KN/Extra.cfg", b"");
        lowercase_tree(&dir, "kn-edit").unwrap();
        let read = |rel: &str| std::fs::read(dir.join(rel)).unwrap();
        assert_eq!(
            read("resource/ui/hudlayout.res"),
            b"#base \"base.res\" \"Label\""
        );
        assert_eq!(read("resource/fonts/font.ttf"), b"\"Binary.res\"");
        assert_eq!(read("resource/ui/odd.res"), b"\"odd.res\" \xff");
        assert_eq!(
            read("cfg/kn/menu.cfg"),
            b"exec kn/extra\nexec Stock_Thing\n"
        );
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
                "cfg",
                "cfg/kn",
                "cfg/kn/extra.cfg",
                "cfg/kn/menu.cfg",
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
