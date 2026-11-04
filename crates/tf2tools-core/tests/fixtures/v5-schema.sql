-- Schema of a real v5 database (user_version 9), schema only, no user data.
-- Captured 2026-10-16 from sqlite_master; sqlite_sequence omitted (AUTOINCREMENT creates it).
CREATE TABLE settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
CREATE TABLE library_huds (
                id TEXT PRIMARY KEY,
                folder_name TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'local',
                store_hud_id TEXT,
                has_thumbnail INTEGER NOT NULL DEFAULT 0,
                has_gallery INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            , author TEXT NOT NULL DEFAULT '', store_gb_id INTEGER);
CREATE TABLE library_sounds (
                id TEXT PRIMARY KEY,
                folder_name TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL,
                original_filename TEXT NOT NULL,
                created_at TEXT NOT NULL
            , source TEXT NOT NULL DEFAULT 'local', store_gb_id INTEGER, sound_type TEXT NOT NULL DEFAULT 'wav');
CREATE TABLE library_crosshairs (
                id TEXT PRIMARY KEY,
                folder_name TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL,
                source_type TEXT NOT NULL,
                created_at TEXT NOT NULL
            , source TEXT NOT NULL DEFAULT 'local', store_gb_id INTEGER);
CREATE TABLE crosshair_assignments (
                weapon_script TEXT PRIMARY KEY,
                crosshair_source TEXT NOT NULL,
                crosshair_ref TEXT NOT NULL
            );
CREATE TABLE store_huds (
                hud_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                author TEXT NOT NULL,
                repo TEXT,
                hash TEXT,
                data_json TEXT NOT NULL,
                fetched_at TEXT NOT NULL
            );
CREATE TABLE store_cache_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
CREATE TABLE logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                level TEXT NOT NULL,
                category TEXT NOT NULL,
                action TEXT NOT NULL,
                message TEXT NOT NULL,
                details TEXT
            );
CREATE INDEX idx_logs_timestamp ON logs(timestamp DESC);
CREATE INDEX idx_logs_category ON logs(category);
CREATE TABLE store_sounds (
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
CREATE TABLE store_crosshairs (
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
CREATE TABLE store_huds_gb (
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
CREATE TABLE profiles (
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
CREATE TABLE profile_crosshair_assignments (
                profile_id TEXT NOT NULL,
                weapon_script TEXT NOT NULL,
                crosshair_source TEXT NOT NULL,
                crosshair_ref TEXT NOT NULL,
                crosshair_store_gb_id INTEGER,
                PRIMARY KEY (profile_id, weapon_script),
                FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
            );
CREATE TABLE store_addons (
                asset_name TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                download_url TEXT NOT NULL,
                file_size INTEGER NOT NULL DEFAULT 0,
                download_count INTEGER NOT NULL DEFAULT 0,
                release_tag TEXT NOT NULL,
                fetched_at TEXT NOT NULL
            );
CREATE TABLE library_addons (
                asset_name TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                installed_tag TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
PRAGMA user_version = 9;
