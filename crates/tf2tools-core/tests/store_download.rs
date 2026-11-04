//! The download pipeline: streaming into tmp with caps and checksums, one download at a
//! time, and HUD and addon imports into the library.

mod common;

use std::time::{Duration, Instant};

use common::{
    count, download_collecting, download_requests, fast_config, gb_file, md5_hex, sql, store_env,
    tmp_is_empty, vpk_bytes, zip_bytes,
};
use tf2tools_core::{
    AddonName, DownloadError, DownloadPhase, GbId, LibraryEntry, LibraryKind, Provenance,
    StoreItemId, StoreKind,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

fn hud_zip(folder: &str) -> Vec<u8> {
    let info = format!("{folder}/info.vdf");
    let layout = format!("{folder}/resource/ui/hudlayout.res");
    zip_bytes(&[
        (info.as_str(), b"\"hud\" {}".as_slice()),
        (layout.as_str(), b"x".as_slice()),
    ])
}

const HUDS: StoreKind = StoreKind::GameBananaHuds;

#[tokio::test]
async fn a_gamebanana_hud_downloads_into_the_library() {
    let env = store_env().await;
    sql(&env.dir)
        .execute(
            "INSERT INTO store_huds_gb (gb_id, name, author, fetched_at) VALUES (463048, 'DarHUD', 'DarSitam', 'x')",
            [],
        )
        .unwrap();
    let body = hud_zip("darhud");
    // GameBanana's checksum is compared without regard to case.
    let file = gb_file(
        &env,
        HUDS,
        463048,
        "darhud_1.zip",
        body.clone(),
        &md5_hex(&body).to_uppercase(),
    )
    .await;
    let (result, seen) = download_collecting(&env, HUDS, &file).await;
    let entries = result.unwrap();
    let [LibraryEntry::Hud(hud)] = entries.as_slice() else {
        panic!("{entries:?}")
    };
    assert_eq!(
        (hud.folder_name(), hud.name(), hud.author()),
        ("darhud", "DarHUD", "DarSitam")
    );
    assert_eq!(
        hud.provenance(),
        &Provenance::GameBanana {
            gb_id: GbId::new(463048),
            author: "DarSitam".to_owned()
        }
    );
    assert!(
        env.dir
            .library_dir(LibraryKind::Huds)
            .join("darhud/info.vdf")
            .is_file()
    );
    assert!(tmp_is_empty(&env.dir));

    let len = body.len() as u64;
    let (last, downloading) = seen.split_last().unwrap();
    assert_eq!(
        (last.phase(), last.downloaded(), last.total()),
        (DownloadPhase::Importing, len, Some(len))
    );
    assert!(!downloading.is_empty());
    assert!(
        downloading
            .iter()
            .all(|p| p.phase() == DownloadPhase::Downloading)
    );
    assert_eq!(downloading.last().unwrap().downloaded(), len);
}

#[tokio::test]
async fn a_curated_hud_downloads_with_curated_provenance() {
    let env = store_env().await;
    let repo = format!("{}/gh/raysfire/rayshud", env.server.uri());
    sql(&env.dir)
        .execute(
            "INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
             VALUES ('rayshud', 'rayshud', 'raysfire', ?1, 'a22e', '{}', 'x')",
            [&repo],
        )
        .unwrap();
    Mock::given(method("GET"))
        .and(path("/gh/raysfire/rayshud/archive/a22e.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(hud_zip("rayshud-a22e")))
        .mount(&env.server)
        .await;
    let id = StoreItemId::Curated("rayshud".to_owned());
    let file = env
        .client
        .item_files(StoreKind::CuratedHuds, &id)
        .await
        .unwrap()
        .remove(0);
    let entries = download_collecting(&env, StoreKind::CuratedHuds, &file)
        .await
        .0
        .unwrap();
    let [LibraryEntry::Hud(hud)] = entries.as_slice() else {
        panic!("{entries:?}")
    };
    assert_eq!(
        (hud.folder_name(), hud.name(), hud.author()),
        ("rayshud-a22e", "rayshud", "raysfire")
    );
    assert_eq!(
        hud.provenance(),
        &Provenance::Curated {
            hud_id: "rayshud".to_owned()
        }
    );
}

#[tokio::test]
async fn an_addon_downloads_with_its_release_tag() {
    let env = store_env().await;
    let url = format!("{}/dl/flat.vpk", env.server.uri());
    sql(&env.dir)
        .execute(
            "INSERT INTO store_addons (asset_name, display_name, download_url, file_size, release_tag, fetched_at)
             VALUES ('mastercomfig-addon-flat-mouse.vpk', 'Flat Mouse', ?1, 299, '9.100.1', 'x')",
            [&url],
        )
        .unwrap();
    let vpk = vpk_bytes(&[("cfg/flat.cfg", b"m_rawinput 1".as_slice())]);
    Mock::given(method("GET"))
        .and(path("/dl/flat.vpk"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vpk.clone()))
        .mount(&env.server)
        .await;
    let id = StoreItemId::Addon(AddonName::new("mastercomfig-addon-flat-mouse.vpk").unwrap());
    let file = env
        .client
        .item_files(StoreKind::Addons, &id)
        .await
        .unwrap()
        .remove(0);
    let entries = download_collecting(&env, StoreKind::Addons, &file)
        .await
        .0
        .unwrap();
    let [LibraryEntry::Addon(addon)] = entries.as_slice() else {
        panic!("{entries:?}")
    };
    assert_eq!(
        (addon.name().as_str(), addon.installed_tag()),
        ("mastercomfig-addon-flat-mouse.vpk", "9.100.1")
    );
    let stored = env
        .dir
        .library_dir(LibraryKind::Addons)
        .join("mastercomfig-addon-flat-mouse.vpk");
    assert_eq!(std::fs::read(stored).unwrap(), vpk);
}

#[tokio::test]
async fn a_download_over_the_cap_is_refused_and_leaves_nothing() {
    let env = store_env().await;
    let client = env
        .client
        .clone()
        .with_config(fast_config().with_size_cap(HUDS, 1024));
    let file = gb_file(&env, HUDS, 1, "big.zip", vec![0u8; 4096], "").await;
    let result = client.download(HUDS, &file, &|_| {}).await;
    assert!(
        matches!(result, Err(DownloadError::TooLarge { limit: 1024, .. })),
        "{result:?}"
    );
    assert!(tmp_is_empty(&env.dir));
    assert_eq!(count(&env.dir, "library_huds"), 0);
}

#[tokio::test]
async fn a_checksum_mismatch_is_refused_and_leaves_nothing() {
    let env = store_env().await;
    let file = gb_file(
        &env,
        HUDS,
        2,
        "hud.zip",
        hud_zip("x"),
        "00000000000000000000000000000000",
    )
    .await;
    let (result, _) = download_collecting(&env, HUDS, &file).await;
    assert!(
        matches!(result, Err(DownloadError::ChecksumMismatch { .. })),
        "{result:?}"
    );
    assert!(tmp_is_empty(&env.dir));
    assert_eq!(count(&env.dir, "library_huds"), 0);
    let huds = std::fs::read_dir(env.dir.library_dir(LibraryKind::Huds)).unwrap();
    assert_eq!(huds.count(), 0);
}

#[tokio::test]
async fn a_gamebanana_item_already_in_the_library_is_refused() {
    let env = store_env().await;
    let file = gb_file(&env, HUDS, 3, "hud.zip", hud_zip("dup"), "").await;
    download_collecting(&env, HUDS, &file).await.0.unwrap();
    let (again, _) = download_collecting(&env, HUDS, &file).await;
    assert!(
        matches!(again, Err(DownloadError::AlreadyInLibrary { .. })),
        "{again:?}"
    );
    assert_eq!(download_requests(&env).await, 1);
}

#[tokio::test]
async fn a_failed_request_reports_its_status() {
    let env = store_env().await;
    let file = gb_file(&env, HUDS, 4, "hud.zip", Vec::new(), "").await;
    env.server.reset().await; // the listed URL now answers 404
    let (result, _) = download_collecting(&env, HUDS, &file).await;
    assert!(
        matches!(result, Err(DownloadError::Http { status: 404, .. })),
        "{result:?}"
    );
    assert!(tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn a_hostile_file_name_cannot_leave_the_scratch_dir() {
    let env = store_env().await;
    let file = gb_file(&env, HUDS, 5, "../../evil.zip", hud_zip("safehud"), "").await;
    let entries = download_collecting(&env, HUDS, &file).await.0.unwrap();
    let [LibraryEntry::Hud(hud)] = entries.as_slice() else {
        panic!("{entries:?}")
    };
    assert_eq!(hud.folder_name(), "safehud");
    assert!(!env.dir.root().join("evil.zip").exists());
    assert!(!env.dir.tmp_dir().join("evil.zip").exists());
    assert!(tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn a_dropped_download_leaves_no_scratch_dir() {
    let env = store_env().await;
    Mock::given(method("GET"))
        .and(path("/dl/6"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(hud_zip("slow"))
                .set_delay(Duration::from_secs(5)),
        )
        .with_priority(1)
        .mount(&env.server)
        .await;
    let file = gb_file(&env, HUDS, 6, "hud.zip", hud_zip("slow"), "").await;
    let attempt = tokio::time::timeout(
        Duration::from_millis(300),
        env.client.download(HUDS, &file, &|_| {}),
    )
    .await;
    assert!(attempt.is_err(), "the download should still be waiting");
    assert!(tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn downloads_take_turns() {
    let env = store_env().await;
    for id in [7, 8] {
        Mock::given(method("GET"))
            .and(path(format!("/dl/{id}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(hud_zip(&format!("hud{id}")))
                    .set_delay(Duration::from_millis(300)),
            )
            .with_priority(1)
            .mount(&env.server)
            .await;
    }
    let a = gb_file(&env, HUDS, 7, "a.zip", Vec::new(), "").await;
    let b = gb_file(&env, HUDS, 8, "b.zip", Vec::new(), "").await;
    let start = Instant::now();
    let (first, second) = tokio::join!(
        env.client.download(HUDS, &a, &|_| {}),
        env.client.download(HUDS, &b, &|_| {}),
    );
    first.unwrap();
    second.unwrap();
    assert!(
        start.elapsed() >= Duration::from_millis(600),
        "{:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn a_file_from_another_store_is_refused() {
    let env = store_env().await;
    let file = gb_file(&env, StoreKind::Crosshairs, 9, "x.zip", hud_zip("x"), "").await;
    let (result, _) = download_collecting(&env, HUDS, &file).await;
    assert!(
        matches!(result, Err(DownloadError::WrongKind { .. })),
        "{result:?}"
    );
    assert_eq!(download_requests(&env).await, 0);
}

#[tokio::test]
async fn a_download_can_run_on_a_spawned_task() {
    let env = store_env().await;
    let file = gb_file(&env, HUDS, 10, "hud.zip", hud_zip("spawned"), "").await;
    let client = env.client.clone();
    let entries = tokio::spawn(async move { client.download(HUDS, &file, &|_| {}).await })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entries.len(), 1);
}
