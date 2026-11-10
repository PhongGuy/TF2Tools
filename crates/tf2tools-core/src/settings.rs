//! User settings stored in v5's `settings` key-value table, values as decimal text.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use crate::db::Database;
use crate::error::SettingsError;

const DEFAULT_THEME: &str = "dark";
const DEFAULT_SYNC_CACHE_HOURS: u32 = 168;

/// A snapshot of the user's settings, with v5's defaults for missing or unreadable keys.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Settings {
    tf2_path: Option<PathBuf>,
    volume: Volume,
    theme: String,
    sync_cache_hours: u32,
    sidebar_collapsed: bool,
    maker_grid_lines: bool,
    show_mature_addons: bool,
    hud_maker_panel: Option<String>,
    hud_maker_backdrop: Option<PathBuf>,
    old_v4_asked: bool,
}

impl Settings {
    /// The configured TF2 install directory, if any.
    pub fn tf2_path(&self) -> Option<&Path> {
        self.tf2_path.as_deref()
    }

    /// The sound playback volume.
    pub fn volume(&self) -> Volume {
        self.volume
    }

    /// The UI theme name, `"dark"` by default.
    pub fn theme(&self) -> &str {
        &self.theme
    }

    /// How many hours a store sync stays fresh.
    pub fn sync_cache_hours(&self) -> u32 {
        self.sync_cache_hours
    }

    /// Whether the sidebar shows icons only, `false` by default.
    pub fn sidebar_collapsed(&self) -> bool {
        self.sidebar_collapsed
    }

    /// Whether the Crosshair Maker draws lines between cells, `false` by default.
    pub fn maker_grid_lines(&self) -> bool {
        self.maker_grid_lines
    }

    /// Whether mature GameBanana add-ons show, `false` by default.
    pub fn show_mature_addons(&self) -> bool {
        self.show_mature_addons
    }

    /// Where the HUD Maker's side panel sits, as the app wrote it; `None` by default.
    pub fn hud_maker_panel(&self) -> Option<&str> {
        self.hud_maker_panel.as_deref()
    }

    /// The picture the HUD Maker draws behind the HUD; `None` by default.
    pub fn hud_maker_backdrop(&self) -> Option<&Path> {
        self.hud_maker_backdrop.as_deref()
    }

    /// Whether the player already answered the offer to remove TF2Tools 4, `false` by default.
    pub fn old_v4_asked(&self) -> bool {
        self.old_v4_asked
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tf2_path: None,
            volume: Volume::default(),
            theme: DEFAULT_THEME.to_owned(),
            sync_cache_hours: DEFAULT_SYNC_CACHE_HOURS,
            sidebar_collapsed: false,
            maker_grid_lines: false,
            show_mature_addons: false,
            hud_maker_panel: None,
            hud_maker_backdrop: None,
            old_v4_asked: false,
        }
    }
}

/// Playback volume in percent, 0..=100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Volume(u8);

impl Volume {
    /// A volume of `percent`, or `None` above 100.
    pub fn new(percent: u8) -> Option<Self> {
        (percent <= 100).then_some(Self(percent))
    }

    /// The volume in percent.
    pub fn get(self) -> u8 {
        self.0
    }

    /// v5 stored any `u32`; out-of-range values clamp and garbage falls back to the default.
    fn parse_lenient(value: &str) -> Self {
        value
            .parse::<u32>()
            .map_or_else(|_| Self::default(), |v| Self(v.min(100) as u8))
    }
}

impl Default for Volume {
    fn default() -> Self {
        Self(50)
    }
}

impl Database {
    /// Reads all settings; unknown keys are ignored and bad values fall back to defaults.
    pub fn settings(&self) -> Result<Settings, SettingsError> {
        let rows = self
            .read(|conn| {
                let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(db_error)?;

        let mut settings = Settings::default();
        for (key, value) in rows {
            match key.as_str() {
                // v5 could leave an empty path behind; treat it as unset.
                "tf2_path" => settings.tf2_path = (!value.is_empty()).then(|| value.into()),
                "volume" => settings.volume = Volume::parse_lenient(&value),
                "theme" => settings.theme = value,
                "sync_cache_hours" => {
                    settings.sync_cache_hours = value.parse().unwrap_or(DEFAULT_SYNC_CACHE_HOURS);
                }
                "sidebar_collapsed" => {
                    settings.sidebar_collapsed = matches!(value.as_str(), "true" | "1")
                }
                "maker_grid_lines" => {
                    settings.maker_grid_lines = matches!(value.as_str(), "true" | "1")
                }
                "show_mature_addons" => {
                    settings.show_mature_addons = matches!(value.as_str(), "true" | "1")
                }
                "hud_maker_panel" => settings.hud_maker_panel = Some(value),
                "hud_maker_backdrop" => {
                    settings.hud_maker_backdrop = (!value.is_empty()).then(|| value.into())
                }
                "old_v4_asked" => settings.old_v4_asked = matches!(value.as_str(), "true" | "1"),
                _ => {}
            }
        }
        Ok(settings)
    }

    /// Stores the TF2 install directory; `None` deletes the key.
    pub fn set_tf2_path(&self, path: Option<&Path>) -> Result<(), SettingsError> {
        match path {
            Some(path) => {
                let text = path.to_str().ok_or_else(|| SettingsError::NonUtf8Path {
                    path: path.to_path_buf(),
                })?;
                self.save_setting("tf2_path", text)
            }
            None => self
                .write(|conn| conn.execute("DELETE FROM settings WHERE key = 'tf2_path'", []))
                .map(drop)
                .map_err(db_error),
        }
    }

    /// Stores `path` only when no TF2 path is saved (an empty v5 value counts as unsaved), in one
    /// statement; returns whether it was stored.
    pub fn set_tf2_path_if_unset(&self, path: &Path) -> Result<bool, SettingsError> {
        let text = path.to_str().ok_or_else(|| SettingsError::NonUtf8Path {
            path: path.to_path_buf(),
        })?;
        // One upsert, so a concurrent `set_tf2_path` that lands first is never replaced, and
        // one that lands after simply replaces this.
        self.write(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('tf2_path', ?1) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value WHERE settings.value = ''",
                params![text],
            )
        })
        .map(|changed| changed == 1)
        .map_err(db_error)
    }

    /// Stores the playback volume.
    pub fn set_volume(&self, volume: Volume) -> Result<(), SettingsError> {
        self.save_setting("volume", &volume.get().to_string())
    }

    /// Stores how many hours a store sync stays fresh.
    pub fn set_sync_cache_hours(&self, hours: u32) -> Result<(), SettingsError> {
        self.save_setting("sync_cache_hours", &hours.to_string())
    }

    /// Stores the UI theme name.
    pub fn set_theme(&self, theme: &str) -> Result<(), SettingsError> {
        self.save_setting("theme", theme)
    }

    /// Stores whether the sidebar is collapsed, as `"true"` or `"false"`.
    pub fn set_sidebar_collapsed(&self, collapsed: bool) -> Result<(), SettingsError> {
        self.save_setting(
            "sidebar_collapsed",
            if collapsed { "true" } else { "false" },
        )
    }

    /// Stores the Crosshair Maker grid-lines choice, as `"true"` or `"false"`.
    pub fn set_maker_grid_lines(&self, on: bool) -> Result<(), SettingsError> {
        self.save_setting("maker_grid_lines", if on { "true" } else { "false" })
    }

    /// Stores whether mature GameBanana add-ons show, as `"true"` or `"false"`.
    pub fn set_show_mature_addons(&self, on: bool) -> Result<(), SettingsError> {
        self.save_setting("show_mature_addons", if on { "true" } else { "false" })
    }

    /// Stores where the HUD Maker's side panel sits; the text is the app's to read back.
    pub fn set_hud_maker_panel(&self, place: &str) -> Result<(), SettingsError> {
        self.save_setting("hud_maker_panel", place)
    }

    /// Stores the HUD Maker's backdrop picture; `None` deletes the key.
    pub fn set_hud_maker_backdrop(&self, path: Option<&Path>) -> Result<(), SettingsError> {
        match path {
            Some(path) => {
                let text = path.to_str().ok_or_else(|| SettingsError::NonUtf8Path {
                    path: path.to_path_buf(),
                })?;
                self.save_setting("hud_maker_backdrop", text)
            }
            None => self
                .write(|conn| {
                    conn.execute("DELETE FROM settings WHERE key = 'hud_maker_backdrop'", [])
                })
                .map(drop)
                .map_err(db_error),
        }
    }

    /// Records that the player answered the offer to remove TF2Tools 4, so it never shows again.
    pub fn set_old_v4_asked(&self) -> Result<(), SettingsError> {
        self.save_setting("old_v4_asked", "true")
    }

    /// The fingerprint keys stored for `hud_folder`; empty when none, or when the
    /// `font_check_dismissed` value is not a JSON object of string lists.
    pub fn font_check_dismissed(&self, hud_folder: &str) -> Result<Vec<String>, SettingsError> {
        let stored = self
            .read(|conn| read_setting(conn, FONT_CHECK_DISMISSED))
            .map_err(db_error)?;
        Ok(parse_dismissals(stored.as_deref())
            .remove(hud_folder)
            .unwrap_or_default())
    }

    /// Stores `fingerprint` for `hud_folder`, keeping other HUDs' entries; a malformed
    /// value is replaced.
    pub fn set_font_check_dismissed(
        &self,
        hud_folder: &str,
        fingerprint: &[String],
    ) -> Result<(), SettingsError> {
        // Read, merge and write in one transaction on the writer, so two HUDs dismissed at
        // once both keep their entry.
        self.write(|conn| {
            let tx = conn.transaction()?;
            let mut all = parse_dismissals(read_setting(&tx, FONT_CHECK_DISMISSED)?.as_deref());
            all.insert(hud_folder.to_owned(), fingerprint.to_vec());
            let text = serde_json::to_string(&all)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params![FONT_CHECK_DISMISSED, text],
            )?;
            tx.commit()
        })
        .map_err(db_error)
    }

    fn save_setting(&self, key: &str, value: &str) -> Result<(), SettingsError> {
        self.write(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
        })
        .map(drop)
        .map_err(db_error)
    }
}

const FONT_CHECK_DISMISSED: &str = "font_check_dismissed";

fn read_setting(conn: &rusqlite::Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
}

/// The per-HUD dismissal lists, empty when the stored value is missing or malformed.
fn parse_dismissals(stored: Option<&str>) -> BTreeMap<String, Vec<String>> {
    stored
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_default()
}

fn db_error(e: rusqlite::Error) -> SettingsError {
    SettingsError::Database {
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use crate::{DataDir, Database};

    #[test]
    fn settings_read_does_not_wait_for_an_open_write_transaction() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = Database::open(&DataDir::at(tmp.path())).expect("open");
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let writer = db.clone();
        let handle = std::thread::spawn(move || {
            writer.write(|conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES ('theme', 'light')",
                    [],
                )?;
                started_tx.send(()).ok();
                release_rx.recv().ok();
                tx.commit()
            })
        });
        started_rx.recv().expect("writer started");
        let begun = Instant::now();
        // The read runs on its own thread so a regression to the writer fails instead of hanging.
        let reader = db.clone();
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            done_tx.send(reader.settings()).ok();
        });
        let during = match done_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => result.expect("read during write"),
            Err(_) => {
                release_tx.send(()).ok();
                panic!("settings() blocked behind an open write transaction");
            }
        };
        assert!(
            begun.elapsed() < Duration::from_secs(1),
            "reader waited {:?}",
            begun.elapsed()
        );
        assert_eq!(during.theme(), "dark");
        release_tx.send(()).expect("release");
        handle.join().expect("join").expect("commit");
        assert_eq!(db.settings().expect("read after").theme(), "light");
    }
}
