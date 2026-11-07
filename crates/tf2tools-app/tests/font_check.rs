use crate::support;

use std::path::PathBuf;
use std::sync::Arc;

use tf2tools_app::app::Boot;
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::font_check::{
    FontPrompt, dismiss_job, install_job, issue_text, prompt_job, skip_text,
};
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::Services;
use tf2tools_app::shell::toast::ToastKind;
use tf2tools_core::{
    FontHost, FontIssue, FontSkipReason, LogLevel, Platform, Tf2Install, scan_hud_fonts,
};

use support::fonts::{FONTS, FakeFontHost, write_font_hud};

#[test]
fn services_start_without_a_font_host() {
    let env = support::env();
    let services = support::services(&env, support::no_steam());
    assert!(services.font_host.is_none());
    let host: Arc<dyn FontHost> = Arc::new(FakeFontHost::new(&[], env.tmp.path()));
    assert!(services.with_font_host(Some(host)).font_host.is_some());
}

#[test]
fn a_timing_boot_drops_the_font_host() {
    let env = support::env();
    let host: Arc<dyn FontHost> = Arc::new(FakeFontHost::new(&[], env.tmp.path()));
    let boot = Boot {
        data: env.data.clone(),
        assets: Ok(support::repo_asset_root()),
        picker: Arc::new(ScriptedPicker::empty()),
        detector: support::no_steam(),
        audio: Arc::new(RecordingAudio::new()),
        endpoints: tf2tools_core::Endpoints::default(),
        auto_sync: false,
        offline: false,
        font_host: Some(host),
        updates: tf2tools_app::updater::UpdateBoot::Off,
    };
    assert!(boot.for_timing(None).font_host.is_none());
}

#[test]
fn the_fixture_hud_scans_as_its_fonts() {
    let env = support::env();
    let dir = env.game.join("tf/custom/fonthud");
    write_font_hud(&dir, "fonthud", FONTS);
    let fonts = scan_hud_fonts(&dir, Platform::Linux).unwrap();
    let names: Vec<&str> = fonts.fonts().iter().map(|f| f.name()).collect();
    assert_eq!(names, ["Surface", "Code Pro", "Marlett"]);
    assert!(fonts.issues().is_empty());
}

fn fonts_dir(env: &support::Env) -> PathBuf {
    env.tmp.path().join("user-fonts")
}

fn with_host(env: &support::Env, host: FakeFontHost) -> (Services, Arc<FakeFontHost>) {
    let host = Arc::new(host);
    let dyn_host: Arc<dyn FontHost> = Arc::<FakeFontHost>::clone(&host);
    let services = support::services(env, support::no_steam()).with_font_host(Some(dyn_host));
    (services, host)
}

fn tf2(env: &support::Env) -> Tf2Install {
    Tf2Install::from_path(&env.game).unwrap()
}

fn place(env: &support::Env, folder: &str, fonts: &[(&str, &str)]) {
    write_font_hud(&env.game.join("tf/custom").join(folder), folder, fonts);
}

fn missing_names(prompt: &FontPrompt) -> Vec<&str> {
    prompt.check.missing().iter().map(|f| f.name()).collect()
}

#[test]
fn a_hud_missing_fonts_prompts() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    let (s, _) = with_host(&env, FakeFontHost::new(&["Code Pro"], &fonts_dir(&env)));
    let prompt = prompt_job(&s, &tf2(&env), Some("fonthud")).expect("a prompt");
    assert_eq!(prompt.folder, "fonthud");
    assert_eq!(prompt.hud_dir, env.game.join("tf/custom/fonthud"));
    assert_eq!(missing_names(&prompt), ["Surface"]);
    assert_eq!(prompt.check.protected(), ["Marlett"]);
}

#[test]
fn no_font_host_means_no_prompt() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    let s = support::services(&env, support::no_steam());
    assert!(prompt_job(&s, &tf2(&env), Some("fonthud")).is_none());
}

#[test]
fn installed_fonts_mean_no_prompt() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    let host = FakeFontHost::new(&["surface", "CODE PRO"], &fonts_dir(&env));
    let (s, _) = with_host(&env, host);
    assert!(prompt_job(&s, &tf2(&env), Some("fonthud")).is_none());
}

#[test]
fn a_failing_host_means_no_prompt() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    let host = FakeFontHost {
        fail_names: true,
        ..FakeFontHost::new(&[], &fonts_dir(&env))
    };
    let (s, _) = with_host(&env, host);
    assert!(prompt_job(&s, &tf2(&env), Some("fonthud")).is_none());
}

#[test]
fn a_plain_file_named_like_a_hud_prompts_nothing() {
    let env = support::env();
    let custom = env.game.join("tf/custom");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(custom.join("fonthud"), b"not a HUD").unwrap();
    let (s, host) = with_host(&env, FakeFontHost::new(&[], &fonts_dir(&env)));
    assert!(prompt_job(&s, &tf2(&env), Some("fonthud")).is_none());
    assert!(support::log_rows(&s).is_empty(), "a stray file is no error");
    assert!(host.registered.lock().unwrap().is_empty());
    assert!(!fonts_dir(&env).exists());
}

#[test]
fn the_active_hud_is_found_when_no_folder_is_named() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    let (s, _) = with_host(&env, FakeFontHost::new(&[], &fonts_dir(&env)));
    let prompt = prompt_job(&s, &tf2(&env), None).expect("a prompt");
    assert_eq!(prompt.folder, "fonthud");
}

#[test]
fn two_installed_huds_have_no_active_prompt() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    place(&env, "otherhud", FONTS);
    let (s, _) = with_host(&env, FakeFontHost::new(&[], &fonts_dir(&env)));
    assert!(prompt_job(&s, &tf2(&env), None).is_none());
}

#[test]
fn a_dismissal_holds_until_the_hud_adds_a_font() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    let (s, _) = with_host(&env, FakeFontHost::new(&[], &fonts_dir(&env)));
    let prompt = prompt_job(&s, &tf2(&env), Some("fonthud")).unwrap();
    dismiss_job(&s, &prompt).unwrap();
    assert_eq!(
        s.db.font_check_dismissed("fonthud").unwrap(),
        ["font:code pro", "font:surface"]
    );
    assert!(prompt_job(&s, &tf2(&env), Some("fonthud")).is_none());
    let mut more = FONTS.to_vec();
    more.push(("Extra Sans", "resource/fonts/extra.ttf"));
    place(&env, "fonthud", &more);
    let again = prompt_job(&s, &tf2(&env), Some("fonthud")).expect("asked again");
    assert_eq!(missing_names(&again), ["Surface", "Code Pro", "Extra Sans"]);
}

#[test]
fn install_copies_the_missing_fonts_and_logs_a_row() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    let (s, host) = with_host(&env, FakeFontHost::new(&[], &fonts_dir(&env)));
    let prompt = prompt_job(&s, &tf2(&env), Some("fonthud")).unwrap();
    let toast = install_job(&s, &prompt).unwrap();
    assert_eq!(toast.kind, ToastKind::Success);
    assert_eq!(toast.title, "Fonts installed");
    assert_eq!(
        toast.description,
        "Installed 2 fonts for fonthud. Restart TF2 to use them."
    );
    assert!(fonts_dir(&env).join("surface.ttf").is_file());
    assert!(fonts_dir(&env).join("codepro.ttf").is_file());
    assert!(!fonts_dir(&env).join("marlett.ttf").exists());
    assert_eq!(*host.refreshes.lock().unwrap(), 1);
    let rows = support::log_rows(&s);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].category(), rows[0].action(), rows[0].message()),
        ("hud", "install_fonts", "Installed 2 fonts for HUD: fonthud")
    );
    assert_eq!(rows[0].level(), LogLevel::Info);
    assert_eq!(
        rows[0].details(),
        Some("Installed: resource/fonts/codepro.ttf, resource/fonts/surface.ttf")
    );
}

#[test]
fn a_different_existing_file_is_kept_and_reported() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    std::fs::create_dir_all(fonts_dir(&env)).unwrap();
    std::fs::write(fonts_dir(&env).join("surface.ttf"), b"other").unwrap();
    let (s, _) = with_host(&env, FakeFontHost::new(&[], &fonts_dir(&env)));
    let prompt = prompt_job(&s, &tf2(&env), Some("fonthud")).unwrap();
    let toast = install_job(&s, &prompt).unwrap();
    assert_eq!(toast.kind, ToastKind::Warning);
    assert_eq!(toast.title, "Some fonts were not installed");
    assert_eq!(
        toast.description,
        "Installed 1 of 2 fonts for fonthud; see the Activity Log. Restart TF2 to use them."
    );
    assert_eq!(
        std::fs::read(fonts_dir(&env).join("surface.ttf")).unwrap(),
        b"other"
    );
    let rows = support::log_rows(&s);
    assert_eq!(rows[0].message(), "Installed 1 font for HUD: fonthud");
    assert_eq!(rows[0].level(), LogLevel::Warning);
    assert_eq!(
        rows[0].details(),
        Some(
            "Installed: resource/fonts/codepro.ttf\nSkipped: resource/fonts/surface.ttf \
             (a different file with that name is already installed)"
        )
    );
}

#[test]
fn skipping_every_font_logs_that_none_were_installed() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    std::fs::create_dir_all(fonts_dir(&env)).unwrap();
    std::fs::write(fonts_dir(&env).join("surface.ttf"), b"other").unwrap();
    std::fs::write(fonts_dir(&env).join("codepro.ttf"), b"other").unwrap();
    let (s, _) = with_host(&env, FakeFontHost::new(&[], &fonts_dir(&env)));
    let prompt = prompt_job(&s, &tf2(&env), Some("fonthud")).unwrap();
    let toast = install_job(&s, &prompt).unwrap();
    assert_eq!(toast.kind, ToastKind::Warning);
    assert_eq!(toast.title, "No fonts installed");
    assert_eq!(
        toast.description,
        "Skipped 2 fonts for fonthud; see the Activity Log."
    );
    let rows = support::log_rows(&s);
    assert_eq!(rows[0].message(), "No fonts installed for HUD: fonthud");
    assert_eq!(rows[0].level(), LogLevel::Warning);
}

#[test]
fn a_failed_refresh_says_the_fonts_were_copied() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    let host = FakeFontHost {
        fail_refresh: true,
        ..FakeFontHost::new(&[], &fonts_dir(&env))
    };
    let (s, _) = with_host(&env, host);
    let prompt = prompt_job(&s, &tf2(&env), Some("fonthud")).unwrap();
    let toast = install_job(&s, &prompt).unwrap();
    assert_eq!(toast.kind, ToastKind::Warning);
    assert_eq!(toast.title, "Fonts copied");
    assert_eq!(
        toast.description,
        "Log out and back in for TF2 to see the new fonts."
    );
    let rows = support::log_rows(&s);
    assert_eq!(rows[0].message(), "Copied fonts for HUD: fonthud");
    assert_eq!(rows[0].level(), LogLevel::Warning);
    assert!(rows[0].details().unwrap().contains("fc-cache failed"));
}

#[test]
fn a_failed_copy_is_an_error_with_a_row() {
    let env = support::env();
    place(&env, "fonthud", FONTS);
    // A plain file where the fonts directory should be makes every copy fail.
    std::fs::write(fonts_dir(&env), b"not a dir").unwrap();
    let (s, _) = with_host(&env, FakeFontHost::new(&[], &fonts_dir(&env)));
    let prompt = prompt_job(&s, &tf2(&env), Some("fonthud")).unwrap();
    let err = install_job(&s, &prompt).unwrap_err();
    assert_eq!(err.title(), "Failed to install fonts");
    support::assert_plain(err.description());
    let rows = support::log_rows(&s);
    assert_eq!(
        rows[0].message(),
        "Failed to install fonts for HUD: fonthud"
    );
    assert_eq!(rows[0].level(), LogLevel::Error);
}

#[test]
fn skip_reasons_read_in_plain_words() {
    assert_eq!(
        skip_text(&FontSkipReason::ProtectedName),
        "it carries a system font name"
    );
    assert_eq!(
        skip_text(&FontSkipReason::FileExists),
        "a different file with that name is already installed"
    );
    assert_eq!(
        skip_text(&FontSkipReason::OutsideHud),
        "it is outside the HUD folder"
    );
    assert_eq!(
        skip_text(&FontSkipReason::Unnamed),
        "its font name cannot be read"
    );
}

#[test]
fn issues_read_in_plain_words() {
    let case = FontIssue::PathCaseMismatch {
        declared: "resource/fonts/surface_semiBold.ttf".into(),
        on_disk: PathBuf::from("resource/fonts/surface_semibold.ttf"),
    };
    assert_eq!(
        issue_text(&case),
        "The HUD asks for resource/fonts/surface_semiBold.ttf but ships \
         resource/fonts/surface_semibold.ttf; Linux file names are case-sensitive, so TF2 may \
         not find it."
    );
    let upper = FontIssue::UppercaseFileName {
        file: PathBuf::from("resource/fonts/Surface.ttf"),
    };
    assert_eq!(
        issue_text(&upper),
        "resource/fonts/Surface.ttf has capital letters in its file name, which TF2 on Linux \
         may fail to load."
    );
    let vpk = FontIssue::InsideVpk {
        declared: "resource/fonts/packed.ttf".into(),
        vpk: PathBuf::from("fonts.vpk"),
    };
    assert_eq!(
        issue_text(&vpk),
        "resource/fonts/packed.ttf is packed in fonts.vpk, and TF2 does not load fonts from \
         VPK files."
    );
}
