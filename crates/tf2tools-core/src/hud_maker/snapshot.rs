//! Snapshot files: a made HUD's changes under a small header, kept in its history folder.
//! The file sits where the player can reach it, so reading is strict about the shape, lenient
//! per change, bounded in size and count, and puts every path through the path rule.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::Change;
use super::maker_file::{change_json, parse_change};
use super::recipe::is_invisible;
use crate::error::HistoryError;

/// The only snapshot version this build reads and writes.
const VERSION: u64 = 1;
/// The longest snapshot name kept, in characters.
const MAX_NAME_CHARS: usize = 80;

/// One snapshot: a name, when it was taken (fixed-width UTC) and the changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnapshotFile {
    pub(crate) name: String,
    pub(crate) at: String,
    pub(crate) changes: Vec<Change>,
}

impl SnapshotFile {
    /// More changes than this are refused on save and skipped, unparsed, on read.
    pub(crate) const MAX_CHANGES: usize = 10_000;
    /// Larger files are refused unread; the same cap the capped reader applies to recipes.
    pub(crate) const MAX_BYTES: usize = crate::hud_maker::Recipe::MAX_BYTES;

    /// `name` without control or invisible characters, trimmed and cut to 80 characters;
    /// `None` when nothing is left.
    pub(crate) fn clean_name(name: &str) -> Option<String> {
        let kept: String = name
            .chars()
            .filter(|c| !c.is_control() && !is_invisible(*c))
            .collect();
        let cut: String = kept.trim().chars().take(MAX_NAME_CHARS).collect();
        match cut.trim_end() {
            "" => None,
            name => Some(name.to_owned()),
        }
    }

    /// Pretty JSON, version 1.
    pub(crate) fn to_json(&self) -> Vec<u8> {
        let file = json!({
            "version": VERSION,
            "kind": "snapshot",
            "name": self.name,
            "at": self.at,
            "changes": self.changes.iter().map(change_json).collect::<Vec<_>>(),
        });
        format!("{file:#}\n").into_bytes()
    }

    /// The file and how many changes were skipped.
    ///
    /// # Errors
    /// `Damaged` when the bytes are not a snapshot; `Newer` when the version is above 1.
    pub(crate) fn parse(bytes: &[u8]) -> Result<(SnapshotFile, usize), HistoryError> {
        if bytes.len() > Self::MAX_BYTES {
            return Err(damaged("over 32 MB"));
        }
        // serde_json's recursion limit turns deep nesting into an error, not a stack overflow.
        let value: Value =
            serde_json::from_slice(bytes).map_err(|e| damaged(&format!("not JSON: {e}")))?;
        let root = value.as_object().ok_or_else(|| damaged("not an object"))?;
        match root.get("version").and_then(Value::as_u64) {
            Some(0) | None => return Err(damaged("no positive version")),
            Some(v) if v > VERSION => return Err(HistoryError::Newer),
            Some(_) => {}
        }
        if root.get("kind").and_then(Value::as_str) != Some("snapshot") {
            return Err(damaged("kind is not snapshot"));
        }
        let name = root
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| damaged("name is not text"))?;
        let at = root
            .get("at")
            .and_then(Value::as_str)
            .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
            .ok_or_else(|| damaged("at is not a time"))?;
        let entries = root
            .get("changes")
            .and_then(Value::as_array)
            .ok_or_else(|| damaged("changes is not a list"))?;

        // A change this build cannot use is skipped and counted, never fatal.
        let mut changes = Vec::new();
        let mut skipped = entries.len().saturating_sub(Self::MAX_CHANGES);
        for entry in entries.iter().take(Self::MAX_CHANGES) {
            match parse_change(entry) {
                Ok(Some(change)) => changes.push(change),
                _ => skipped += 1,
            }
        }
        let file = SnapshotFile {
            name: Self::clean_name(name).unwrap_or_else(|| "Snapshot".to_owned()),
            at: crate::timestamp::format(at.with_timezone(&Utc)),
            changes,
        };
        Ok((file, skipped))
    }
}

fn damaged(reason: &str) -> HistoryError {
    HistoryError::Damaged {
        reason: reason.to_owned(),
    }
}
