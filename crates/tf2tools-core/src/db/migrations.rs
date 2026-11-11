//! v5's nine `init_db` steps plus v6's own, as data, and the runner that applies the missing ones.

use rusqlite::Connection;

/// The newest schema this build understands. v5 stopped at 9; v6 steps start at 10.
pub(crate) const SCHEMA_VERSION: u32 = 15;

/// One migration step: SQL to run, then columns to add when missing.
struct Step {
    version: u32,
    sql: &'static str,
    /// (table, column, declaration)
    add_columns: &'static [(&'static str, &'static str, &'static str)],
}

const STEPS: &[Step] = &[
    Step {
        version: 1,
        sql: STEP_1_SQL,
        add_columns: &[],
    },
    Step {
        version: 2,
        sql: "",
        add_columns: &[("library_huds", "author", "TEXT NOT NULL DEFAULT ''")],
    },
    Step {
        version: 3,
        sql: STEP_3_SQL,
        add_columns: &[
            ("library_sounds", "source", "TEXT NOT NULL DEFAULT 'local'"),
            ("library_sounds", "store_gb_id", "INTEGER"),
            (
                "library_crosshairs",
                "source",
                "TEXT NOT NULL DEFAULT 'local'",
            ),
            ("library_crosshairs", "store_gb_id", "INTEGER"),
        ],
    },
    Step {
        version: 4,
        sql: STEP_4_SQL,
        add_columns: &[("library_huds", "store_gb_id", "INTEGER")],
    },
    // v5 ignored the unique-key conflict here, leaving both keys; OR IGNORE does the same.
    Step {
        version: 5,
        sql: "UPDATE OR IGNORE store_cache_meta SET key = 'curated_last_synced' WHERE key = 'last_fetched';",
        add_columns: &[],
    },
    Step {
        version: 6,
        sql: STEP_6_SQL,
        add_columns: &[],
    },
    Step {
        version: 7,
        sql: "",
        add_columns: &[(
            "library_sounds",
            "sound_type",
            "TEXT NOT NULL DEFAULT 'wav'",
        )],
    },
    Step {
        version: 8,
        sql: STEP_8_SQL,
        add_columns: &[],
    },
    Step {
        version: 9,
        sql: STEP_9_SQL,
        add_columns: &[],
    },
    Step {
        version: 10,
        sql: STEP_10_SQL,
        add_columns: &[],
    },
    Step {
        version: 11,
        sql: STEP_11_SQL,
        add_columns: &[],
    },
    Step {
        version: 12,
        sql: STEP_12_SQL,
        add_columns: &[],
    },
    Step {
        version: 13,
        sql: STEP_13_SQL,
        add_columns: &[],
    },
    Step {
        version: 14,
        sql: STEP_14_SQL,
        add_columns: &[],
    },
    Step {
        version: 15,
        sql: STEP_15_SQL,
        add_columns: &[],
    },
];

/// Brings the database up to `SCHEMA_VERSION`. Each step and its version stamp commit together,
/// so an interrupted upgrade resumes at the first step that did not commit.
pub(crate) fn migrate(conn: &mut Connection, from: u32) -> Result<u32, (u32, rusqlite::Error)> {
    for step in STEPS.iter().filter(|s| s.version > from) {
        let run = |conn: &mut Connection| -> rusqlite::Result<()> {
            let tx = conn.transaction()?;
            tx.execute_batch(step.sql)?;
            // v5 ran ALTER and ignored "duplicate column"; checking first keeps real errors visible.
            for (table, column, declaration) in step.add_columns {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
                    [table, column],
                    |row| row.get(0),
                )?;
                if !exists {
                    tx.execute_batch(&format!(
                        "ALTER TABLE {table} ADD COLUMN {column} {declaration}"
                    ))?;
                }
            }
            tx.pragma_update(None, "user_version", step.version)?;
            tx.commit()
        };
        run(conn).map_err(|e| (step.version, e))?;
    }
    Ok(SCHEMA_VERSION.max(from))
}

/// v5 step 1: the original tables and log indexes.
const STEP_1_SQL: &str = "
    -- Key-value settings store (replaces plugin-store / config.json)
    CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    -- HUD library metadata
    CREATE TABLE IF NOT EXISTS library_huds (
        id TEXT PRIMARY KEY,
        folder_name TEXT UNIQUE NOT NULL,
        name TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'local',
        store_hud_id TEXT,
        has_thumbnail INTEGER NOT NULL DEFAULT 0,
        has_gallery INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    -- Sound library metadata
    CREATE TABLE IF NOT EXISTS library_sounds (
        id TEXT PRIMARY KEY,
        folder_name TEXT UNIQUE NOT NULL,
        name TEXT NOT NULL,
        original_filename TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'local',
        store_gb_id INTEGER,
        sound_type TEXT NOT NULL DEFAULT 'wav',
        created_at TEXT NOT NULL
    );

    -- Crosshair library metadata
    CREATE TABLE IF NOT EXISTS library_crosshairs (
        id TEXT PRIMARY KEY,
        folder_name TEXT UNIQUE NOT NULL,
        name TEXT NOT NULL,
        source_type TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'local',
        store_gb_id INTEGER,
        created_at TEXT NOT NULL
    );

    -- Crosshair-to-weapon assignments (no FK — builtin crosshairs aren't in DB)
    CREATE TABLE IF NOT EXISTS crosshair_assignments (
        weapon_script TEXT PRIMARY KEY,
        crosshair_source TEXT NOT NULL,
        crosshair_ref TEXT NOT NULL
    );

    -- HUD store cache from mastercomfig/hud-db
    CREATE TABLE IF NOT EXISTS store_huds (
        hud_id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        author TEXT NOT NULL,
        repo TEXT,
        hash TEXT,
        data_json TEXT NOT NULL,
        fetched_at TEXT NOT NULL
    );

    -- Store cache metadata (e.g., last_fetched timestamp)
    CREATE TABLE IF NOT EXISTS store_cache_meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    -- Activity logs for all operations
    CREATE TABLE IF NOT EXISTS logs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp TEXT NOT NULL,
        level TEXT NOT NULL,
        category TEXT NOT NULL,
        action TEXT NOT NULL,
        message TEXT NOT NULL,
        details TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_logs_timestamp ON logs(timestamp DESC);
    CREATE INDEX IF NOT EXISTS idx_logs_category ON logs(category);

    -- Profiles (saved collections of HUD + sounds + crosshair assignments)
    CREATE TABLE IF NOT EXISTS profiles (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        hud_folder_name TEXT,
        hud_name TEXT,
        hud_source TEXT,
        hud_store_gb_id INTEGER,
        hud_store_hud_id TEXT,
        hitsound_folder_name TEXT,
        hitsound_name TEXT,
        hitsound_source TEXT,
        hitsound_store_gb_id INTEGER,
        killsound_folder_name TEXT,
        killsound_name TEXT,
        killsound_source TEXT,
        killsound_store_gb_id INTEGER,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS profile_crosshair_assignments (
        profile_id TEXT NOT NULL,
        weapon_script TEXT NOT NULL,
        crosshair_source TEXT NOT NULL,
        crosshair_ref TEXT NOT NULL,
        crosshair_store_gb_id INTEGER,
        PRIMARY KEY (profile_id, weapon_script),
        FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
    );
";

/// v5 step 3: GameBanana sound and crosshair store caches.
const STEP_3_SQL: &str = "
    -- Store cache tables for GameBanana sounds
    CREATE TABLE IF NOT EXISTS store_sounds (
        gb_id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        author TEXT NOT NULL DEFAULT '',
        category TEXT NOT NULL DEFAULT '',
        preview_url TEXT,
        thumbnail_url TEXT,
        like_count INTEGER NOT NULL DEFAULT 0,
        view_count INTEGER NOT NULL DEFAULT 0,
        tags_json TEXT NOT NULL DEFAULT '[]',
        profile_url TEXT NOT NULL DEFAULT '',
        date_added INTEGER NOT NULL DEFAULT 0,
        fetched_at TEXT NOT NULL
    );

    -- Store cache tables for GameBanana crosshairs
    CREATE TABLE IF NOT EXISTS store_crosshairs (
        gb_id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        author TEXT NOT NULL DEFAULT '',
        thumbnail_url TEXT,
        images_json TEXT NOT NULL DEFAULT '[]',
        like_count INTEGER NOT NULL DEFAULT 0,
        view_count INTEGER NOT NULL DEFAULT 0,
        tags_json TEXT NOT NULL DEFAULT '[]',
        profile_url TEXT NOT NULL DEFAULT '',
        date_added INTEGER NOT NULL DEFAULT 0,
        fetched_at TEXT NOT NULL
    );
";

/// v5 step 4: GameBanana HUD store cache; drops stale per-page sync keys.
const STEP_4_SQL: &str = "
    -- Store cache table for GameBanana HUD mods
    CREATE TABLE IF NOT EXISTS store_huds_gb (
        gb_id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        author TEXT NOT NULL DEFAULT '',
        thumbnail_url TEXT,
        images_json TEXT NOT NULL DEFAULT '[]',
        like_count INTEGER NOT NULL DEFAULT 0,
        view_count INTEGER NOT NULL DEFAULT 0,
        tags_json TEXT NOT NULL DEFAULT '[]',
        profile_url TEXT NOT NULL DEFAULT '',
        date_added INTEGER NOT NULL DEFAULT 0,
        fetched_at TEXT NOT NULL
    );

    -- Clean up stale per-page cache keys from old pattern
    DELETE FROM store_cache_meta WHERE key LIKE 'sounds_%_page_%';
    DELETE FROM store_cache_meta WHERE key LIKE 'crosshairs_%_page_%';
";

/// v5 step 6: profiles, for databases created before step 1 included them.
const STEP_6_SQL: &str = "
    CREATE TABLE IF NOT EXISTS profiles (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        hud_folder_name TEXT,
        hud_name TEXT,
        hud_source TEXT,
        hud_store_gb_id INTEGER,
        hud_store_hud_id TEXT,
        hitsound_folder_name TEXT,
        hitsound_name TEXT,
        hitsound_source TEXT,
        hitsound_store_gb_id INTEGER,
        killsound_folder_name TEXT,
        killsound_name TEXT,
        killsound_source TEXT,
        killsound_store_gb_id INTEGER,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS profile_crosshair_assignments (
        profile_id TEXT NOT NULL,
        weapon_script TEXT NOT NULL,
        crosshair_source TEXT NOT NULL,
        crosshair_ref TEXT NOT NULL,
        crosshair_store_gb_id INTEGER,
        PRIMARY KEY (profile_id, weapon_script),
        FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
    );
";

/// v5 step 8: mastercomfig addon store cache.
const STEP_8_SQL: &str = "
    CREATE TABLE IF NOT EXISTS store_addons (
        asset_name TEXT PRIMARY KEY,
        display_name TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        download_url TEXT NOT NULL,
        file_size INTEGER NOT NULL DEFAULT 0,
        download_count INTEGER NOT NULL DEFAULT 0,
        release_tag TEXT NOT NULL,
        fetched_at TEXT NOT NULL
    );
";

/// v5 step 9: installed addons; drops the old global installed-tag key.
const STEP_9_SQL: &str = "
    CREATE TABLE IF NOT EXISTS library_addons (
        asset_name TEXT PRIMARY KEY,
        display_name TEXT NOT NULL,
        installed_tag TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    -- Remove broken global tag (replaced by per-addon installed_tag)
    DELETE FROM store_cache_meta WHERE key = 'addons_installed_tag';
";

/// v6 step 10: indexes for the store grids' keyset sorts and the library's store-id lookups.
/// v5 gates its steps on `version < N`, so it still opens a v10 file.
const STEP_10_SQL: &str = "
    CREATE INDEX IF NOT EXISTS idx_store_sounds_likes ON store_sounds(like_count DESC, gb_id DESC);
    CREATE INDEX IF NOT EXISTS idx_store_sounds_views ON store_sounds(view_count DESC, gb_id DESC);
    CREATE INDEX IF NOT EXISTS idx_store_sounds_date ON store_sounds(date_added DESC, gb_id DESC);
    CREATE INDEX IF NOT EXISTS idx_store_sounds_cat_likes ON store_sounds(category, like_count DESC, gb_id DESC);
    CREATE INDEX IF NOT EXISTS idx_store_crosshairs_likes ON store_crosshairs(like_count DESC, gb_id DESC);
    CREATE INDEX IF NOT EXISTS idx_store_crosshairs_views ON store_crosshairs(view_count DESC, gb_id DESC);
    CREATE INDEX IF NOT EXISTS idx_store_crosshairs_date ON store_crosshairs(date_added DESC, gb_id DESC);
    CREATE INDEX IF NOT EXISTS idx_store_huds_gb_likes ON store_huds_gb(like_count DESC, gb_id DESC);
    CREATE INDEX IF NOT EXISTS idx_store_huds_gb_views ON store_huds_gb(view_count DESC, gb_id DESC);
    CREATE INDEX IF NOT EXISTS idx_store_huds_gb_date ON store_huds_gb(date_added DESC, gb_id DESC);
    CREATE INDEX IF NOT EXISTS idx_library_sounds_gb ON library_sounds(store_gb_id) WHERE store_gb_id IS NOT NULL;
    CREATE INDEX IF NOT EXISTS idx_library_crosshairs_gb ON library_crosshairs(store_gb_id) WHERE store_gb_id IS NOT NULL;
    CREATE INDEX IF NOT EXISTS idx_library_huds_gb ON library_huds(store_gb_id) WHERE store_gb_id IS NOT NULL;
    ANALYZE;
";

/// v6 step 11: earlier syncs dropped most GameBanana screenshots, so the crosshair and HUD
/// stores forget their sync stamp and checkpoint; the next visit refetches every row in place.
const STEP_11_SQL: &str = "
    DELETE FROM store_cache_meta WHERE key IN (
        'crosshairs_last_synced', 'crosshairs_sync_page', 'crosshairs_sync_start', 'crosshairs_sync_fetched',
        'huds_gb_last_synced', 'huds_gb_sync_page', 'huds_gb_sync_start', 'huds_gb_sync_fetched'
    );
";

/// v6 step 12: the cached serveme.tf map list, and the maps the app put in `tf/maps`, which
/// are the only ones it may remove.
const STEP_12_SQL: &str = "
    CREATE TABLE IF NOT EXISTS store_maps (
        name TEXT PRIMARY KEY,
        times_played INTEGER NOT NULL DEFAULT 0,
        first_played TEXT,
        last_played TEXT,
        size INTEGER NOT NULL DEFAULT 0,
        fetched_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS downloaded_maps (
        name TEXT PRIMARY KEY,
        size INTEGER NOT NULL,
        downloaded_at TEXT NOT NULL
    );
";

/// v6 step 13: mastercomfig's managed state and its latest release, and the override VPKs
/// leave the addon tables, since they are module overrides, not addons. Forgetting the
/// addon sync makes the next sync record the release.
const STEP_13_SQL: &str = "
    CREATE TABLE IF NOT EXISTS mastercomfig_state (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        version TEXT NOT NULL,
        preset TEXT NOT NULL,
        modules TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS mastercomfig_release (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        tag TEXT NOT NULL,
        base_url TEXT NOT NULL,
        override_urls TEXT NOT NULL
    );
    DELETE FROM library_addons WHERE asset_name LIKE 'mastercomfig-addon-override-%';
    DELETE FROM store_addons WHERE asset_name LIKE 'mastercomfig-addon-override-%';
    DELETE FROM store_cache_meta WHERE key = 'addons_last_synced';
";

/// v6 step 14: GameBanana add-ons installed from the Addon Store. `payloads` and `files`
/// are JSON arrays; `files` is relative to `tf/`.
const STEP_14_SQL: &str = "
    CREATE TABLE IF NOT EXISTS library_gb_addons (
        gb_id INTEGER PRIMARY KEY,
        section TEXT NOT NULL,
        name TEXT NOT NULL,
        author TEXT NOT NULL,
        file_id INTEGER NOT NULL,
        file_date INTEGER NOT NULL,
        latest_file_date INTEGER NOT NULL,
        checked_at INTEGER NOT NULL,
        payloads TEXT NOT NULL,
        files TEXT NOT NULL,
        installed INTEGER NOT NULL,
        gone INTEGER NOT NULL DEFAULT 0
    );
";

/// v6 step 15: the logs.tf cache. Bodies are kept, zlib-compressed, so the expanded view needs
/// no download; databases from before the BLOB declaration hold some plain TEXT bodies, which
/// still read. Class rows hold every player of a downloaded log.
const STEP_15_SQL: &str = "
    CREATE TABLE IF NOT EXISTS logstf_players (
        steam_id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        position INTEGER NOT NULL,
        last_synced INTEGER
    );
    CREATE TABLE IF NOT EXISTS logstf_logs (
        id INTEGER PRIMARY KEY,
        title TEXT NOT NULL,
        map TEXT NOT NULL,
        date INTEGER NOT NULL,
        length INTEGER,
        body BLOB,
        fetched_at INTEGER,
        state INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS logstf_player_logs (
        steam_id INTEGER NOT NULL,
        log_id INTEGER NOT NULL REFERENCES logstf_logs(id) ON DELETE CASCADE,
        PRIMARY KEY (steam_id, log_id)
    );
    CREATE TABLE IF NOT EXISTS logstf_class_rows (
        log_id INTEGER NOT NULL REFERENCES logstf_logs(id) ON DELETE CASCADE,
        steam_id INTEGER NOT NULL,
        class TEXT NOT NULL,
        seconds INTEGER NOT NULL,
        kills INTEGER NOT NULL,
        deaths INTEGER NOT NULL,
        assists INTEGER NOT NULL,
        dmg INTEGER NOT NULL,
        heal INTEGER NOT NULL DEFAULT 0,
        ubers INTEGER NOT NULL DEFAULT 0,
        drops INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (log_id, steam_id, class)
    );
    CREATE INDEX IF NOT EXISTS logstf_class_rows_player ON logstf_class_rows(steam_id);
    CREATE INDEX IF NOT EXISTS logstf_player_logs_log ON logstf_player_logs(log_id);
";
