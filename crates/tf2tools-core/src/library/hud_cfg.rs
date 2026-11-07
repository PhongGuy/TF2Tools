//! Settings files the installed HUD's options write under `tf/cfg`. A save lists them in the
//! copy's `.tf2tools/game_cfg.json`; install writes them and records what each replaced, so
//! turning an option off, installing another HUD or uninstalling puts the old file back.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::hud_copy::sha256_hex;
use super::{IoFailure, Library, METADATA_DIR, create_dir_all, io_at, read_setting, save_setting};
use crate::hud_maker::CfgFile;
use crate::tf2::Tf2Install;

/// The list a save writes beside `maker.json`.
pub(super) const CFG_FILE: &str = "game_cfg.json";
/// The settings key holding what the install wrote.
const RECORD_KEY: &str = "installed_hud_cfg";
/// A file larger than this is never read, replaced or kept.
const MAX_CFG_BYTES: u64 = 64 * 1024;

/// One file in the saved list.
#[derive(Debug, Serialize, Deserialize)]
struct Listed {
    path: String,
    text: String,
}

/// One file the install wrote: its path, the hash of what it wrote and the bytes it replaced.
#[derive(Debug, Serialize, Deserialize)]
struct Written {
    path: String,
    sha256: String,
    before: Option<String>,
}

/// The saved list for `files`, as JSON.
pub(super) fn list_json(files: &[CfgFile]) -> Vec<u8> {
    let listed: Vec<Listed> = (files.iter())
        .filter_map(|f| {
            Some(Listed {
                path: f.path().to_owned(),
                text: String::from_utf8(f.bytes().to_vec()).ok()?,
            })
        })
        .collect();
    serde_json::to_vec_pretty(&listed).unwrap_or_default()
}

/// The files library HUD folder `src` lists; a missing, damaged or oversized list is none,
/// and each path is checked again.
fn listed(src: &Path) -> Vec<CfgFile> {
    let path = src.join(METADATA_DIR).join(CFG_FILE);
    let fits = std::fs::symlink_metadata(&path)
        .is_ok_and(|m| m.is_file() && m.len() <= MAX_CFG_BYTES * 16);
    let Some(bytes) = fits.then(|| std::fs::read(&path).ok()).flatten() else {
        return Vec::new();
    };
    let listed: Vec<Listed> = serde_json::from_slice(&bytes).unwrap_or_default();
    (listed.into_iter())
        .filter(|l| l.text.len() as u64 <= MAX_CFG_BYTES)
        .filter_map(|l| CfgFile::new(&l.path, l.text.into_bytes()))
        .collect()
}

impl Library {
    /// Takes back what an earlier install wrote, then writes the files HUD folder `src`
    /// lists under `tf/cfg` and records them. A file in the way that is no regular file, is
    /// too large or sits behind a link is left alone and not written.
    pub(super) fn write_hud_cfg(&self, src: &Path, tf2: &Tf2Install) -> Result<(), IoFailure> {
        self.revert_hud_cfg(tf2)?;
        let cfg = tf2.tf_dir().join("cfg");
        let mut record = Vec::new();
        for file in listed(src) {
            let Some(target) = safe_target(&cfg, file.path()) else {
                continue;
            };
            let before = match std::fs::symlink_metadata(&target) {
                Ok(m) if m.is_file() && m.len() <= MAX_CFG_BYTES => {
                    Some(std::fs::read(&target).map_err(io_at(&target))?)
                }
                Ok(_) => continue,
                Err(_) => None,
            };
            // Recorded before the write, so a crash after it still takes the file back; a
            // write that never happened leaves a file that is not ours, which stays.
            record.push(Written {
                path: file.path().to_owned(),
                sha256: sha256_hex(file.bytes()),
                before: before.as_deref().map(to_hex),
            });
            self.save_record(&record)?;
            if let Some(parent) = target.parent() {
                create_dir_all(parent)?;
            }
            std::fs::write(&target, file.bytes()).map_err(io_at(&target))?;
        }
        Ok(())
    }

    /// Puts back what the last install wrote under `tf/cfg`: each file still holding what
    /// TF2Tools wrote gets its old bytes back, or goes when there were none. A file the
    /// player changed since is left alone. The record is cleared.
    pub(super) fn revert_hud_cfg(&self, tf2: &Tf2Install) -> Result<(), IoFailure> {
        let record = self.read_record();
        if record.is_empty() {
            return Ok(());
        }
        let cfg = tf2.tf_dir().join("cfg");
        for written in record {
            let Some(target) = safe_target(&cfg, &written.path) else {
                continue;
            };
            let ours = std::fs::symlink_metadata(&target)
                .is_ok_and(|m| m.is_file() && m.len() <= MAX_CFG_BYTES)
                && std::fs::read(&target).is_ok_and(|b| sha256_hex(&b) == written.sha256);
            if !ours {
                continue;
            }
            match written.before.as_deref().and_then(from_hex) {
                Some(bytes) => std::fs::write(&target, bytes).map_err(io_at(&target))?,
                None => {
                    std::fs::remove_file(&target).map_err(io_at(&target))?;
                    // A folder the install made for the file goes too once empty.
                    if let Some(parent) = target.parent().filter(|p| *p != cfg) {
                        let _ = std::fs::remove_dir(parent);
                    }
                }
            }
        }
        self.save_record(&[])
    }

    fn read_record(&self) -> Vec<Written> {
        let text = self.db.read(|conn| read_setting(conn, RECORD_KEY));
        (text.ok().flatten())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save_record(&self, record: &[Written]) -> Result<(), IoFailure> {
        let text = match record.is_empty() {
            true => String::new(),
            false => serde_json::to_string(record).unwrap_or_default(),
        };
        (self.db)
            .write(|conn| save_setting(conn, RECORD_KEY, &text))
            .map_err(|e| IoFailure {
                path: PathBuf::from(RECORD_KEY),
                source: std::io::Error::other(e.to_string()),
            })
    }
}

/// `cfg/<path>` when the path passes the settings file rule and no folder on the way below
/// `tf` is a link, so a write never leaves `tf/cfg`.
fn safe_target(cfg: &Path, path: &str) -> Option<PathBuf> {
    let checked = CfgFile::new(path, Vec::new())?;
    let mut at = cfg.to_path_buf();
    let not_link = |p: &Path| !std::fs::symlink_metadata(p).is_ok_and(|m| m.is_symlink());
    if !not_link(&at) {
        return None;
    }
    for segment in checked.path().split('/') {
        at.push(segment);
        if !not_link(&at) {
            return None;
        }
    }
    Some(at)
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}
