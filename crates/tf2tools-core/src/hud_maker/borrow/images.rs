//! The pictures a borrowed part draws: each `image` value's material and the texture it names,
//! brought along as kept files, reused when the player's HUD has the same bytes, or renamed
//! when it has different ones.

use super::{BorrowRefused, Brings, Change, ChangeTarget, LeftBehind, Run};
use crate::fonts::keyvalues::{self, KvValue};
use crate::hud_maker::images::game_key;
use crate::hud_maker::{AssetFile, HudFiles, KvDocument, Part, Team, drawn_image};
use crate::hud_maker::{BORROWED_DIR, MAX_IMAGE_BYTES, within_caps};

/// The highest `-b<n>` suffix a renamed picture may take.
const LAST_SUFFIX: u32 = 99;
/// The keys whose value names a picture.
const IMAGE_KEYS: [&str; 3] = ["image", "teambg_2", "teambg_3"];

/// A material and the texture it names, both read from the source.
struct Pair {
    vmt_path: String,
    vmt: Vec<u8>,
    /// The material's `$basetexture` as written.
    base: String,
    vtf_path: String,
    vtf: Vec<u8>,
}

/// `text` without a trailing `ext` (any case).
fn strip_ext<'a>(text: &'a str, ext: &str) -> &'a str {
    match text.len().checked_sub(ext.len()) {
        Some(at) if text.is_char_boundary(at) && text[at..].eq_ignore_ascii_case(ext) => {
            &text[..at]
        }
        _ => text,
    }
}

/// `path` with `-b<n>` before its extension.
fn with_suffix(path: &str, n: u32) -> String {
    match path.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}-b{n}.{ext}"),
        None => format!("{path}-b{n}"),
    }
}

impl Run<'_> {
    /// Brings the pictures the changes so far name, and the code-set pictures of the borrowed
    /// parts, as kept files. A picture the source lacks is listed and its key kept.
    pub(super) fn carry_images(
        &mut self,
        target_files: &HudFiles,
        set: &[&Part],
    ) -> Result<(), BorrowRefused> {
        let named = self.named_images();
        let coded: Vec<String> = set
            .iter()
            .filter(|p| p.value("image").is_none() && p.value("teambg_2").is_none())
            .filter_map(|p| drawn_image(p, Team::Red))
            .map(str::to_owned)
            .collect();
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        let mut swaps: Vec<(String, String)> = Vec::new();
        let mut renamed = Vec::new();
        for (value, code) in named
            .iter()
            .map(|v| (v, false))
            .chain(coded.iter().map(|v| (v, true)))
        {
            let Some(pair) = self.read_pair(value)? else {
                self.left_behind
                    .push(LeftBehind::MissingFile(value.clone()));
                continue;
            };
            let known =
                |files: &[(String, Vec<u8>)], path: &str| files.iter().any(|(p, _)| p == path);
            let differs = |path: &str, bytes: &[u8]| self.differs(target_files, path, bytes);
            // The material and its texture are renamed together, so the pair stays a pair.
            let clash =
                differs(&pair.vtf_path, &pair.vtf) || (!code && differs(&pair.vmt_path, &pair.vmt));
            let (mut vmt, mut vmt_path, mut vtf_path) = (
                pair.vmt.clone(),
                pair.vmt_path.clone(),
                pair.vtf_path.clone(),
            );
            if clash {
                let n = (2..=LAST_SUFFIX)
                    .find(|&n| {
                        let new_vtf = with_suffix(&pair.vtf_path, n);
                        let new_vmt = if code {
                            vmt_path.clone()
                        } else {
                            with_suffix(&pair.vmt_path, n)
                        };
                        !self.occupied(target_files, &new_vtf, &files)
                            && (code || !self.occupied(target_files, &new_vmt, &files))
                    })
                    .ok_or(BorrowRefused::NoRoom)?;
                vtf_path = with_suffix(&pair.vtf_path, n);
                let new_base = format!("{}-b{n}", strip_ext(&pair.base, ".vtf"));
                vmt = rewrite_basetexture(&pair.vmt, &new_base).ok_or(BorrowRefused::NoRoom)?;
                renamed.push((pair.base.clone(), new_base));
                if !code {
                    vmt_path = with_suffix(&pair.vmt_path, n);
                    let new_value = format!("{}-b{n}", strip_ext(value, ".vmt"));
                    renamed.push((value.clone(), new_value.clone()));
                    swaps.push((value.to_ascii_lowercase(), new_value));
                }
            }
            if !known(&files, &vtf_path)
                && !(self.same_in(target_files, &vtf_path, &pair.vtf) && !clash)
            {
                files.push((vtf_path, pair.vtf));
            }
            if !known(&files, &vmt_path) && !self.same_in(target_files, &vmt_path, &vmt) {
                files.push((vmt_path, vmt));
            }
            let brought = if code {
                Brings::CodeImage(value.rsplit('/').next().unwrap_or(value).to_owned())
            } else {
                Brings::Image(value.clone())
            };
            self.brings.push(brought);
        }

        // Pictures already in the player's HUD count toward the same limits.
        let kept = format!("{BORROWED_DIR}/");
        let before = self
            .target
            .assets()
            .iter()
            .filter(|(_, s)| s.starts_with(&kept))
            .count();
        let mut sizes = vec![0u64; before];
        sizes.extend(files.iter().map(|(_, b)| b.len() as u64));
        if !within_caps(&sizes) {
            return Err(BorrowRefused::TooMuch);
        }
        for (path, bytes) in files {
            let source = format!("{BORROWED_DIR}/{path}");
            self.changes
                .push(Change::asset(self.group, AssetFile::new(&path, &source)));
            self.assets.insert(&source, bytes);
        }
        self.renamed.extend(renamed);
        if !swaps.is_empty() {
            self.rewrite_values(&|key, value| {
                if !IMAGE_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key)) {
                    return None;
                }
                let value = value.trim().to_ascii_lowercase();
                swaps
                    .iter()
                    .find(|(old, _)| *old == value)
                    .map(|(_, new)| new.clone())
            });
        }
        Ok(())
    }

    /// Every non-blank picture name in the changes so far, each once, in order of use.
    fn named_images(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let mut note = |key: &str, value: &str| {
            let value = value.trim();
            let wanted = IMAGE_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key));
            if wanted && !value.is_empty() && !names.iter().any(|n| n.eq_ignore_ascii_case(value)) {
                names.push(value.to_owned());
            }
        };
        for change in &self.changes {
            match change.target() {
                ChangeTarget::Set(s) => note(s.key(), s.value()),
                ChangeTarget::AddBlock(a) => {
                    a.lines().iter().for_each(|l| note(l.key(), l.value()))
                }
                ChangeTarget::Remove(_)
                | ChangeTarget::Asset(_)
                | ChangeTarget::Option(_)
                | ChangeTarget::Credit(_)
                | ChangeTarget::Animation(_)
                | ChangeTarget::AnimationAppend(_) => {}
            }
        }
        names
    }

    /// The source's material for `name` and the texture it names; `None` when either is
    /// missing, unreadable or outside `materials/`. A file over the size cap refuses the borrow.
    fn read_pair(&self, name: &str) -> Result<Option<Pair>, BorrowRefused> {
        let Some(vmt_path) = game_key("materials/vgui", name, ".vmt") else {
            return Ok(None);
        };
        let read = |path: &str| {
            self.source
                .files
                .read(path)
                .ok()
                .flatten()
                .map(|f| f.into_bytes())
        };
        let Some(vmt) = read(&vmt_path) else {
            return Ok(None);
        };
        let parsed = keyvalues::parse(&String::from_utf8_lossy(&vmt), self.source.model.platform());
        let base = parsed.pairs.first().and_then(|pair| match &pair.value {
            KvValue::Block(children) => children.iter().find_map(|c| match &c.value {
                KvValue::Text(t) if c.key.eq_ignore_ascii_case("$basetexture") => Some(t.clone()),
                KvValue::Text(_) | KvValue::Block(_) => None,
            }),
            KvValue::Text(_) => None,
        });
        let Some(base) = base else { return Ok(None) };
        let Some(vtf_path) = game_key("materials", &base, ".vtf") else {
            return Ok(None);
        };
        let Some(vtf) = read(&vtf_path) else {
            return Ok(None);
        };
        if vmt.len() as u64 > MAX_IMAGE_BYTES || vtf.len() as u64 > MAX_IMAGE_BYTES {
            return Err(BorrowRefused::TooMuch);
        }
        Ok(Some(Pair {
            vmt_path,
            vmt,
            base,
            vtf_path,
            vtf,
        }))
    }

    /// Whether the player's HUD has a file at `path` that is not `bytes`: a game or HUD file with
    /// other bytes, or a picture an earlier action added (whose bytes are not known here).
    fn differs(&self, target_files: &HudFiles, path: &str, bytes: &[u8]) -> bool {
        let added = self
            .target
            .assets()
            .iter()
            .any(|(p, _)| p.eq_ignore_ascii_case(path));
        added
            || target_files
                .read(path)
                .ok()
                .flatten()
                .is_some_and(|f| f.bytes() != bytes)
    }

    /// Whether the player's HUD already has exactly `bytes` at `path`.
    fn same_in(&self, target_files: &HudFiles, path: &str, bytes: &[u8]) -> bool {
        !self.differs(target_files, path, bytes)
            && target_files
                .read(path)
                .ok()
                .flatten()
                .is_some_and(|f| f.bytes() == bytes)
    }

    /// Whether `path` is taken in the player's HUD, by an earlier action or by this borrow.
    fn occupied(&self, target_files: &HudFiles, path: &str, files: &[(String, Vec<u8>)]) -> bool {
        target_files.contains(path)
            || self
                .target
                .assets()
                .iter()
                .any(|(p, _)| p.eq_ignore_ascii_case(path))
            || files.iter().any(|(p, _)| p.eq_ignore_ascii_case(path))
    }
}

/// The material with its `$basetexture` set to `base`, other lines untouched.
fn rewrite_basetexture(vmt: &[u8], base: &str) -> Option<Vec<u8>> {
    let mut doc = KvDocument::parse(vmt).ok()?;
    let shader = doc.entries().first()?.key().to_owned();
    let id = doc
        .find(&[&shader, "$basetexture"])
        .into_iter()
        .find(|e| e.text().is_some())?
        .value_id();
    doc.set_text(id, base).then(|| doc.as_bytes().to_vec())
}
