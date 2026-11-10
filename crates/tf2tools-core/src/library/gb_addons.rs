//! GameBanana add-ons installed from the Addon Store: their rows in `library_gb_addons`,
//! their copies under `library/gb-addons/<gb_id>` and their files in `tf/custom`.

use super::{GbId, Library, library_db};
use crate::error::LibraryError;
use crate::tf2::Tf2Install;

/// A recorded GameBanana add-on, as the library lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GbAddonEntry {
    gb_id: GbId,
    section: String,
    name: String,
    author: String,
    file_date: i64,
    latest_file_date: i64,
    checked_at: i64,
    payloads: Vec<String>,
    files: Vec<String>,
    installed: bool,
    gone: bool,
    present: bool,
}

impl GbAddonEntry {
    /// The GameBanana item id.
    pub fn gb_id(&self) -> GbId {
        self.gb_id
    }

    /// The `AddonSection` key the add-on was installed from.
    pub fn section(&self) -> &str {
        &self.section
    }

    /// The item's name on GameBanana.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The item's submitter.
    pub fn author(&self) -> &str {
        &self.author
    }

    /// The upload date of the installed file, in Unix seconds.
    pub fn file_date(&self) -> i64 {
        self.file_date
    }

    /// The upload date of the newest file the last check found, in Unix seconds.
    pub fn latest_file_date(&self) -> i64 {
        self.latest_file_date
    }

    /// When GameBanana was last asked about the item, in Unix seconds.
    pub fn checked_at(&self) -> i64 {
        self.checked_at
    }

    /// The names of the payloads chosen at install.
    pub fn payloads(&self) -> &[String] {
        &self.payloads
    }

    /// The installed files and folders, relative to `tf/` and `/`-separated.
    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// The last install completed and every recorded file is in the game.
    pub fn is_installed(&self) -> bool {
        self.installed && self.present
    }

    /// GameBanana has a newer file than the installed one.
    pub fn has_update(&self) -> bool {
        self.latest_file_date > self.file_date
    }

    /// The item is no longer on GameBanana.
    pub fn is_gone(&self) -> bool {
        self.gone
    }
}

/// Reads one JSON array column, naming the column when it is malformed.
fn json_list(row: &rusqlite::Row<'_>, column: &str) -> rusqlite::Result<Vec<String>> {
    let text: String = row.get(column)?;
    serde_json::from_str(&text).map_err(|e| malformed(row, column, &e))
}

fn malformed(row: &rusqlite::Row<'_>, column: &str, e: &dyn std::fmt::Display) -> rusqlite::Error {
    let index = row.as_ref().column_index(column).unwrap_or(0);
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        format!("library_gb_addons.{column}: {e}").into(),
    )
}

fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GbAddonEntry> {
    let gb_id: i64 = row.get("gb_id")?;
    let gb_id = u64::try_from(gb_id).map_err(|e| malformed(row, "gb_id", &e))?;
    Ok(GbAddonEntry {
        gb_id: GbId::new(gb_id),
        section: row.get("section")?,
        name: row.get("name")?,
        author: row.get("author")?,
        file_date: row.get("file_date")?,
        latest_file_date: row.get("latest_file_date")?,
        checked_at: row.get("checked_at")?,
        payloads: json_list(row, "payloads")?,
        files: json_list(row, "files")?,
        installed: row.get("installed")?,
        gone: row.get("gone")?,
        present: false,
    })
}

impl Library {
    /// Every recorded add-on, by section key then name (case-insensitive). Without `tf2`
    /// none counts as installed.
    pub fn list_gb_addons(
        &self,
        tf2: Option<&Tf2Install>,
    ) -> Result<Vec<GbAddonEntry>, LibraryError> {
        let mut entries = self
            .db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT gb_id, section, name, author, file_date, latest_file_date,
                     checked_at, payloads, files, installed, gone FROM library_gb_addons
                     ORDER BY section COLLATE NOCASE, name COLLATE NOCASE",
                )?;
                let rows = stmt.query_map([], entry_from_row)?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .map_err(library_db)?;
        // An empty record never counts as present: there is nothing in the game to find.
        if let Some(tf2) = tf2 {
            let tf = tf2.tf_dir();
            for entry in &mut entries {
                entry.present =
                    !entry.files.is_empty() && entry.files.iter().all(|f| tf.join(f).exists());
            }
        }
        Ok(entries)
    }
}
