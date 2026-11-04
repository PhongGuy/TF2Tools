//! Scanning a HUD: the `Fonts` names its scheme declares, the shipped files that carry them,
//! and the Linux file-name problems TF2 trips on.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use super::Platform;
use super::keyvalues::{self, KvPair, KvValue};
use super::names::{font_names, is_protected};
use crate::error::FontCheckError;

/// HUD contents are untrusted, so every walk is bounded: real HUDs declare at most a few
/// dozen font files and ship no `.vpk` at all, far below these caps.
const MAX_DECLARED_FILES: usize = 512;
const MAX_WALK_ENTRIES: usize = 20_000;
const MAX_WALK_DEPTH: usize = 8;
const MAX_VPKS: usize = 32;

/// A `Fonts` name the HUD ships a file for; `files` are HUD-relative, sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HudFont {
    name: String,
    files: Vec<PathBuf>,
}

impl HudFont {
    /// The name as the scheme first spells it.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The HUD-relative files whose font names include this one, sorted.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }
}

/// A Linux file-name problem TF2 trips on; never produced for `Platform::Windows`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontIssue {
    /// A declared path matches a shipped file only case-insensitively.
    PathCaseMismatch {
        /// The scheme's path with `\` turned into `/`.
        declared: String,
        /// The HUD-relative file it matched.
        on_disk: PathBuf,
    },
    /// A shipped font file whose name has an uppercase letter.
    UppercaseFileName {
        /// The HUD-relative file.
        file: PathBuf,
    },
    /// A declared path found only inside a `.vpk`, which TF2 does not load fonts from.
    InsideVpk {
        /// The scheme's path with `\` turned into `/`.
        declared: String,
        /// The HUD-relative pak holding it.
        vpk: PathBuf,
    },
}

/// What a HUD declares and ships, as found by [`scan_hud_fonts`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct HudFonts {
    fonts: Vec<HudFont>,
    issues: Vec<FontIssue>,
    /// Candidate files carrying a protected name; read by the check.
    pub(crate) protected_files: BTreeSet<PathBuf>,
}

impl HudFonts {
    /// The referenced names the HUD ships files for, in first-declared order.
    pub fn fonts(&self) -> &[HudFont] {
        &self.fonts
    }

    /// The Linux file-name problems, each reported once.
    pub fn issues(&self) -> &[FontIssue] {
        &self.issues
    }
}

/// Reads `<hud_dir>/resource/clientscheme.res` for `platform`, merged with its `#base` and
/// `#include` chain by the engine's rules, then matches every `Fonts` name against the files
/// `CustomFontFiles` declares. A HUD without the scheme is empty. Fails with `Io` when
/// `hud_dir` cannot be canonicalised or the scheme cannot be read.
pub fn scan_hud_fonts(hud_dir: &Path, platform: Platform) -> Result<HudFonts, FontCheckError> {
    let io_err = |path: &Path| {
        let path = path.to_path_buf();
        move |source| FontCheckError::Io { path, source }
    };
    let root = hud_dir.canonicalize().map_err(io_err(hud_dir))?;
    let Some(scheme) = keyvalues::resolve_ci(&root, &root, "resource/clientscheme.res") else {
        return Ok(HudFonts::default());
    };
    let pairs = keyvalues::load(&root, &scheme, platform).map_err(io_err(&scheme))?;

    let mut declared = Declared::default();
    declared.collect(&pairs, false);

    let linux = platform == Platform::Linux;
    let mut issues = Vec::new();
    let mut push_issue = |issue: FontIssue| {
        if linux && !issues.contains(&issue) {
            issues.push(issue);
        }
    };
    let mut paks: Option<Vec<(PathBuf, vpk::VPK)>> = None;
    let mut candidates: BTreeSet<PathBuf> = BTreeSet::new();
    for path in &declared.files {
        // Exact case first, then a case-insensitive match, then a pak; anything else is a
        // stock TF2 path the game supplies itself.
        let (on_disk, mismatch) = match exact_file(&root, path) {
            Some(file) => (file, false),
            None => match keyvalues::resolve_ci(&root, &root, path).filter(|p| p.is_file()) {
                Some(file) => (file, true),
                None => {
                    let paks = paks.get_or_insert_with(|| hud_paks(&root));
                    if let Some(vpk) = pak_holding(paks, path) {
                        push_issue(FontIssue::InsideVpk {
                            declared: path.clone(),
                            vpk,
                        });
                    }
                    continue;
                }
            },
        };
        let Ok(rel) = on_disk.strip_prefix(&root).map(Path::to_path_buf) else {
            continue;
        };
        if mismatch {
            push_issue(FontIssue::PathCaseMismatch {
                declared: path.clone(),
                on_disk: rel.clone(),
            });
        }
        // TF2 opens the declared name, so an exact match is judged by that spelling even when
        // it is a symlink; a case-insensitive match can only be named by what is on disk.
        let opened = if mismatch {
            rel.clone()
        } else {
            PathBuf::from(normalized(path))
        };
        let uppercase = opened
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.chars().any(char::is_uppercase));
        if uppercase {
            push_issue(FontIssue::UppercaseFileName { file: opened });
        }
        candidates.insert(rel);
    }

    let mut shipped = Vec::new();
    let mut protected_files = BTreeSet::new();
    for rel in candidates {
        match font_names(&root.join(&rel)) {
            Ok(found) => {
                if is_protected(&found.names) {
                    protected_files.insert(rel.clone());
                }
                shipped.push((rel, found.names));
            }
            Err(error) => {
                tracing::warn!(path = %rel.display(), %error, "skipping unreadable HUD font")
            }
        }
    }

    // `shipped` is in path order, so each font's file list comes out sorted.
    let fonts = declared
        .names
        .into_iter()
        .filter_map(|name| {
            let files: Vec<PathBuf> = shipped
                .iter()
                .filter(|(_, names)| names.iter().any(|n| n.eq_ignore_ascii_case(&name)))
                .map(|(rel, _)| rel.clone())
                .collect();
            (!files.is_empty()).then_some(HudFont { name, files })
        })
        .collect();
    Ok(HudFonts {
        fonts,
        issues,
        protected_files,
    })
}

/// The `Fonts` names and `CustomFontFiles` paths of a scheme, each deduplicated in first-seen
/// order: names case-insensitively, paths after normalising separators.
#[derive(Default)]
struct Declared {
    names: Vec<String>,
    files: Vec<String>,
    seen_names: HashSet<String>,
    seen_files: HashSet<String>,
}

impl Declared {
    /// Walks every block; `name` values count only beneath a `Fonts` block, and a
    /// `CustomFontFiles` entry is a path string or a block whose `font` key holds one.
    fn collect(&mut self, pairs: &[KvPair], in_fonts: bool) {
        for pair in pairs {
            match &pair.value {
                KvValue::Text(text) if in_fonts && pair.key.eq_ignore_ascii_case("name") => {
                    if self.seen_names.insert(text.to_ascii_lowercase()) {
                        self.names.push(text.clone());
                    }
                }
                KvValue::Text(_) => {}
                KvValue::Block(children) if pair.key.eq_ignore_ascii_case("CustomFontFiles") => {
                    for entry in children {
                        match &entry.value {
                            KvValue::Text(path) => self.add_file(path),
                            KvValue::Block(fields) => {
                                for field in fields {
                                    if let KvValue::Text(path) = &field.value
                                        && field.key.eq_ignore_ascii_case("font")
                                    {
                                        self.add_file(path);
                                    }
                                }
                            }
                        }
                    }
                }
                KvValue::Block(children) => {
                    let fonts = in_fonts || pair.key.eq_ignore_ascii_case("Fonts");
                    self.collect(children, fonts);
                }
            }
        }
    }

    fn add_file(&mut self, path: &str) {
        let path = path.replace('\\', "/");
        if self.files.len() < MAX_DECLARED_FILES && self.seen_files.insert(path.clone()) {
            self.files.push(path);
        }
    }
}

/// The canonical file `root/declared` when it exists, with exact case, inside `root`.
fn exact_file(root: &Path, declared: &str) -> Option<PathBuf> {
    let file = root.join(declared).canonicalize().ok()?;
    (file.starts_with(root) && file.is_file()).then_some(file)
}

/// Every `.vpk` under the HUD that opens, walked without following symlinks and within the
/// entry, depth and pak caps.
fn hud_paks(root: &Path) -> Vec<(PathBuf, vpk::VPK)> {
    walkdir::WalkDir::new(root)
        .max_depth(MAX_WALK_DEPTH)
        .into_iter()
        .take(MAX_WALK_ENTRIES)
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("vpk"))
        })
        .filter_map(|entry| {
            let pak = crate::pak::open_vpk(entry.path()).ok()?;
            let rel = entry.path().strip_prefix(root).ok()?.to_path_buf();
            Some((rel, pak))
        })
        .take(MAX_VPKS)
        .collect()
}

/// The first pak whose tree holds `declared`, compared case-insensitively.
fn pak_holding(paks: &[(PathBuf, vpk::VPK)], declared: &str) -> Option<PathBuf> {
    let wanted = normalized(declared);
    paks.iter()
        .find(|(_, pak)| pak.tree.keys().any(|key| key.eq_ignore_ascii_case(&wanted)))
        .map(|(rel, _)| rel.clone())
}

/// `declared` without empty and `.` segments, joined with `/`.
fn normalized(declared: &str) -> String {
    let segments: Vec<&str> = declared
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    segments.join("/")
}
