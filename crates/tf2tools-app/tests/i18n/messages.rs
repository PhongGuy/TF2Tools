use std::path::PathBuf;

use gpui_kit::SharedString;
use tf2tools_app::audio::AudioError;
use tf2tools_app::i18n::{self, list};
use tf2tools_app::local_messages::{self as local, LocalFailure, PaintStep, SpaceFor};
use tf2tools_app::pages::graphics::jobs;
use tf2tools_app::pages::graphics::model as gfx;
use tf2tools_app::services::{OpError, StartupError};
use tf2tools_app::{map_messages as map, store_messages as store, sync_messages as sync};
use tf2tools_core::{
    ApplyModulesError, ApplyPresetError, ApplyProfileError, BrowseError, CopyHudError, DetectError,
    DownloadError, FilesError, FontInstallError, GbAddonFetchError, GbAddonInstallError, GbId,
    GbMapInstallError, GbMapRemoveError, HudLoadError, ImportError, InstallError,
    InstallMastercomfigError, KeepHudError, LibraryError, LogError, LogsCacheError, MakeHudError,
    MapDownloadError, MapRemoveError, MastercomfigStatusError, PaintError, PngReadError,
    PngWriteError, PreviewError, ProfileError, QueryError, RemoveMastercomfigError, SettingsError,
    StoreKind, SyncError, TakeOverError, UpdateCopyError,
};

use crate::LocaleGuard;

/// Russian text: some Cyrillic, every variable filled, and no catalogue key showing.
pub fn assert_russian(text: &str) {
    assert!(
        text.chars().any(|c| matches!(c, 'А'..='я' | 'Ё' | 'ё')),
        "not Russian: {text:?}"
    );
    assert!(!text.contains("%{"), "an unfilled variable: {text:?}");
    for prefix in ["messages.", "updater.", "common."] {
        assert!(!text.contains(prefix), "a raw key: {text:?}");
    }
}

fn items(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("i{i}")).collect()
}

#[test]
fn lists_join_in_both_languages() {
    let _guard = LocaleGuard::hold();
    assert_eq!(list(&items(1)), "i1");
    assert_eq!(list(&items(2)), "i1 and i2");
    assert_eq!(list(&items(5)), "i1, i2, i3, i4 and i5");
    let long = list(&items(21));
    assert!(
        long.starts_with("i1, i2, ") && long.ends_with(", i20 and i21"),
        "{long}"
    );
    i18n::apply("ru");
    assert_eq!(list(&items(1)), "i1");
    assert_eq!(list(&items(2)), "i1 и i2");
    assert_russian(&list(&items(2)));
    assert_eq!(list(&items(5)), "i1, i2, i3, i4 и i5");
    let long = list(&items(21));
    assert!(
        long.starts_with("i1, i2, ") && long.ends_with(", i20 и i21"),
        "{long}"
    );
}

#[test]
fn shared_texts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let shared = [
        local::internal(),
        local::hud_files(),
        local::hud_base_changed(),
        local::hud_options_unreadable(),
        local::map_restart(),
        local::map_skipped(),
        jobs::close_tf2(),
        jobs::close_tf2_text(),
    ];
    for text in &shared {
        assert_russian(text);
    }
}

#[test]
fn op_errors_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    for err in [OpError::NoTf2, OpError::Internal("boom".into())] {
        assert_russian(&err.title());
        assert_russian(&err.description());
        assert_russian(&err.to_string());
    }
    let op = LocalFailure::new(local::hud_files(), "x").into_op(SharedString::from("Худ"));
    assert_russian(&op.description());
    assert_russian(&op.to_string());
}

#[test]
fn a_failure_keeps_the_language_it_was_built_in() {
    let _guard = LocaleGuard::switch("ru");
    let failure = LocalFailure::new(local::hud_files(), "x");
    let op = LocalFailure::new(local::hud_files(), "x").into_op(SharedString::from("Сохранение"));
    i18n::apply("en");
    assert_russian(&failure.message);
    assert_russian(&op.description());
    // NoTf2 and Internal carry no text, so they read in the language of the moment.
    assert_eq!(OpError::NoTf2.title(), "TF2 not found");
}

fn path() -> PathBuf {
    PathBuf::from("/tmp/x")
}

#[test]
fn local_failures_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let failures = [
        local::describe_gb_install(&GbAddonInstallError::NotInLibrary {
            gb_id: GbId::new(3),
        }),
        local::describe_gb_map_install(&GbMapInstallError::AlreadyPresent {
            names: vec!["cp_a".to_owned(), "cp_b".to_owned()],
        }),
        local::describe_gb_map_remove(&GbMapRemoveError::NotInLibrary {
            gb_id: GbId::new(3),
        }),
        local::describe_library(&LibraryError::NotFound {
            what: "HUD x".into(),
        }),
        local::describe_make_hud(&MakeHudError::EmptyName),
        local::describe_hud_load(&HudLoadError::Newer),
        local::describe_copy_hud(&CopyHudError::NotFound { id: "h".into() }),
        local::describe_update_copy(&UpdateCopyError::NotACopy { id: "h".into() }),
        local::describe_paint(&PaintError::EmptyName, PaintStep::SaveNew),
        local::describe_import(&ImportError::UnsupportedProvenance),
        local::describe_png_read(&PngReadError::Invalid {
            message: "x".into(),
        }),
        local::describe_png_write(&PngWriteError::Io {
            path: path(),
            source: std::io::Error::other("x"),
        }),
        local::describe_tf2_install(&InstallError::NotInLibrary { what: "x".into() }),
        local::describe_keep_hud(&KeepHudError::NotCopyable { path: path() }),
        local::describe_profile(&ProfileError::NameTaken {
            name: "Pubs".into(),
        }),
        local::describe_apply_profile(&ApplyProfileError::NotFound { id: "p".into() }),
        local::describe_settings(&SettingsError::Database {
            message: "x".into(),
        }),
        local::describe_tf2_folder(&DetectError::NotTf2 { path: path() }),
        local::describe_detect(&DetectError::NotTf2 { path: path() }),
        local::describe_store_status(&QueryError::Database {
            message: "x".into(),
        }),
        local::describe_font_install(&FontInstallError::Refresh { reason: "x".into() }),
        local::describe_audio(&AudioError::Closed),
        local::describe_log(&LogError::Database {
            message: "x".into(),
        }),
        local::describe_startup(&StartupError::NewerDatabase {
            path: path(),
            message: "x".into(),
        }),
    ];
    for failure in &failures {
        assert_russian(&failure.message);
    }
    let texts = [
        local::already_have(&["cp_a".to_owned(), "cp_b".to_owned()], false),
        local::already_have(&["cp_a".to_owned()], true),
        local::left_out(2, true).unwrap(),
        local::copy_made_text("x (copy)", "raysfire"),
        local::copy_made_text("x (copy)", ""),
        local::editing_copy_text("x", "raysfire"),
        local::no_space_text(SpaceFor::Save, 1 << 20),
        local::no_space_text(SpaceFor::Copy, 1 << 20),
        local::no_space_text(SpaceFor::Update, 1 << 20),
        local::copy_in_store_text("x"),
    ];
    for text in &texts {
        assert_russian(text);
    }
    let left = [("A", Some("xpos")), ("B", Some("odd_key")), ("C", None)];
    let notices = [
        local::reopen_notice(2, false).unwrap(),
        local::reopen_notice(0, true).unwrap(),
        local::copy_updated_notice("x", &[], 1),
        local::copy_updated_notice("x", &left, 0),
    ];
    for notice in &notices {
        assert_russian(&notice.title);
        assert_russian(&notice.text);
    }
    let many: Vec<(&str, Option<&str>)> = ["P1", "P2", "P3", "P4", "P5", "P6", "P7"]
        .iter()
        .map(|p| (*p, None))
        .collect();
    let notice = local::copy_updated_notice("x", &many, 0);
    assert_russian(&notice.text);
    assert!(notice.text.contains("P5 и "), "{}", notice.text);
}

#[test]
fn counted_messages_take_each_languages_forms() {
    let _guard = LocaleGuard::hold();
    let say = |n| local::left_out(n, false).unwrap();
    assert_eq!(say(1), "1 of your changes no longer fits and was left out.");
    for n in [2, 5, 21] {
        assert_eq!(
            say(n),
            format!("{n} of your changes no longer fit and were left out.")
        );
    }
    i18n::apply("ru");
    for n in [1, 2, 5, 21] {
        let text = say(n);
        assert_russian(&text);
        assert!(text.contains(&n.to_string()), "{text}");
    }
    assert!(say(21).contains("21"), "{}", say(21));
}

#[test]
fn store_map_and_sync_failures_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let kind = StoreKind::Sounds;
    let messages = [
        store::describe_download(kind, &DownloadError::AlreadyInLibrary { what: "x".into() })
            .message,
        store::describe_download(kind, &DownloadError::RateLimited { url: "u".into() }).message,
        store::describe_files(kind, &FilesError::NotInStore { what: "x".into() }).message,
        store::describe_browse(&BrowseError::Gone {
            gb_id: GbId::new(1),
        })
        .message,
        store::describe_gb_fetch(&GbAddonFetchError::NoFiles {
            gb_id: GbId::new(1),
        })
        .message,
        store::describe_gb_map_fetch(&GbAddonFetchError::UnknownLayout {
            file_name: "m.zip".into(),
            payloads: 0,
        })
        .message,
        store::describe_preview(&PreviewError::NoPreview).message,
        store::describe_install(&InstallError::NotInLibrary { what: "x".into() }).message,
        store::describe_store_read(&OpError::NoTf2).message,
        store::read_failed(&OpError::NoTf2),
        store::download_stopped("x").message,
        store::listing_stopped("x".into()).message,
        store::addons_stopped("x".into()).message,
        store::addon_stopped("x".into()).message,
        store::preview_gone(GbId::new(1)).message,
        store::preview_unavailable("x".into()).message,
        store::no_tf2().message,
        store::release_has_no_file().message,
        map::describe_map_sync(&SyncError::Database {
            message: "x".into(),
        })
        .message,
        map::describe_map_download(&MapDownloadError::InGame {
            name: "cp_a".into(),
        })
        .message,
        map::describe_map_remove(&MapRemoveError::NotDownloaded {
            name: "cp_a".into(),
        })
        .message,
        map::map_stopped("x".into()).message,
        sync::describe_sync(kind, &SyncError::Throttled { url: "u".into() }).message,
        sync::sync_stopped("x".into()).message,
    ];
    for message in &messages {
        assert_russian(message);
    }
    for title in [
        store::details_failed_title(),
        store::busy_title(),
        store::busy_text(),
    ] {
        assert_russian(&title);
    }
    // A store keeps the site name players know it by.
    let busy = sync::describe_sync(kind, &SyncError::RateLimited { url: "u".into() }).message;
    assert_russian(&busy);
    assert!(busy.contains("GameBanana"), "{busy}");
}

#[test]
fn mastercomfig_failures_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let failures = [
        gfx::describe_status(&MastercomfigStatusError::Database {
            message: "x".into(),
        }),
        gfx::describe_install(&InstallMastercomfigError::NoRelease),
        gfx::describe_remove(&RemoveMastercomfigError::NotManaged),
        gfx::describe_apply(&ApplyPresetError::NotManaged),
        gfx::describe_apply_modules(&ApplyModulesError::ForeignModulesCfg { path: path() }),
        gfx::describe_apply_modules(&ApplyModulesError::NoRelease),
        gfx::describe_take_over(&TakeOverError::NotInstalled),
    ];
    for failure in &failures {
        assert_russian(&failure.message);
    }
}

#[test]
fn page_action_titles_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    assert_russian(&jobs::save_failed());
}

#[test]
fn sync_and_update_failure_titles_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let kinds = [
        StoreKind::CuratedHuds,
        StoreKind::GameBananaHuds,
        StoreKind::Sounds,
        StoreKind::Crosshairs,
        StoreKind::Addons,
    ];
    for kind in kinds {
        assert_russian(&sync::sync_failed_title(kind));
    }
    assert_russian(&tf2tools_app::updater::backend::failure_title());
    i18n::apply("en");
    assert_eq!(
        sync::sync_failed_title(StoreKind::Sounds),
        "Failed to sync Sounds"
    );
}

#[test]
fn logs_cache_and_stopped_job_failures_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let cache = local::describe_logs_cache(&LogsCacheError::Database {
        message: "disk I/O error".into(),
    });
    assert_russian(&cache.message);
    assert!(!cache.message.contains("disk I/O"), "{}", cache.message);
    let stopped = local::describe_join(&crate::support::cancelled_join());
    assert_russian(&stopped.message);
    let store = local::describe_store_status(&QueryError::Database {
        message: "no such table".into(),
    });
    assert_russian(&store.message);
    assert!(
        !store.message.contains("no such table"),
        "{}",
        store.message
    );
}
