//! Downloading a GameBanana add-on's newest file into payloads, and every refusal.

use crate::common::{self, gb_addon_download, gb_addon_file, gb_addon_profile, store_env};

use rars::ArchiveVersion;
use tf2tools_core::{GbAddonFetchError, GbId, PayloadKind};

fn vpk() -> Vec<u8> {
    common::vpk_bytes(&[("materials/a.vmt", b"a")])
}

#[tokio::test]
async fn the_newest_file_downloads_and_lists_its_payloads() {
    let env = store_env().await;
    let old = common::zip_bytes(&[("old.vpk", &vpk())]);
    let new = common::rar_bytes(
        ArchiveVersion::Rar50,
        &[("Toon/toon.vpk", &vpk()), ("Toon/extra.vpk", b"x")],
        None,
    );
    let files = vec![
        gb_addon_file(&env, 7, 0, "old.zip", &old, 100).await,
        gb_addon_file(&env, 7, 1, "toon.rar", &new, 200).await,
    ];
    let profile = gb_addon_profile(&env, 7, "Toon FX", files).await;
    let (result, progress) = gb_addon_download(&env.client, "effects", &profile).await;
    let download = result.unwrap();
    assert_eq!(download.gb_id(), GbId::new(7));
    assert_eq!(download.section(), "effects");
    assert_eq!(download.name(), "Toon FX");
    assert_eq!(download.file_name(), "toon.rar");
    assert_eq!(download.file_date(), 200);
    let payloads: Vec<(&str, PayloadKind)> = download
        .payloads()
        .iter()
        .map(|p| (p.name(), p.kind()))
        .collect();
    assert_eq!(
        payloads,
        [
            ("Toon/toon.vpk", PayloadKind::Vpk),
            ("Toon/extra.vpk", PayloadKind::Vpk)
        ]
    );
    assert!(!progress.is_empty());
    assert!(
        !common::tmp_is_empty(&env.dir),
        "the download keeps its scratch dir"
    );
    drop(download);
    assert!(
        common::tmp_is_empty(&env.dir),
        "dropping it removes the scratch dir"
    );
}

#[tokio::test]
async fn an_item_without_files_has_nothing_to_install() {
    let env = store_env().await;
    let profile = gb_addon_profile(&env, 9, "Empty", vec![]).await;
    let err = gb_addon_download(&env.client, "skins", &profile)
        .await
        .0
        .unwrap_err();
    assert!(
        matches!(err, GbAddonFetchError::NoFiles { gb_id } if gb_id == GbId::new(9)),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_file_that_failed_the_virus_scan_is_never_downloaded() {
    let env = store_env().await;
    let mut file = gb_addon_file(&env, 10, 0, "bad.zip", b"PK", 100).await;
    file["_sAvResult"] = serde_json::json!("infected");
    let profile = gb_addon_profile(&env, 10, "Bad", vec![file]).await;
    let err = gb_addon_download(&env.client, "skins", &profile)
        .await
        .0
        .unwrap_err();
    assert!(
        matches!(&err, GbAddonFetchError::VirusScan { file_name, result } if file_name == "bad.zip" && result == "infected"),
        "{err:?}"
    );
    assert_eq!(common::download_requests(&env).await, 0);
}

#[tokio::test]
async fn a_file_over_the_cap_is_refused_before_and_while_downloading() {
    let env = store_env().await;
    let client = env
        .client
        .clone()
        .with_config(common::fast_config().with_gb_addon_size_cap(10));
    let body = common::zip_bytes(&[("big.vpk", &[0u8; 64])]);
    let file = gb_addon_file(&env, 11, 0, "big.zip", &body, 100).await;
    let profile = gb_addon_profile(&env, 11, "Big", vec![file.clone()]).await;
    let err = gb_addon_download(&client, "skins", &profile)
        .await
        .0
        .unwrap_err();
    assert!(
        matches!(err, GbAddonFetchError::TooLarge { limit: 10, .. }),
        "{err:?}"
    );
    assert_eq!(
        common::download_requests(&env).await,
        0,
        "the listed size refuses first"
    );

    env.server.reset().await;
    let mut unsized_file = gb_addon_file(&env, 11, 0, "big.zip", &body, 100).await;
    unsized_file.as_object_mut().unwrap().remove("_nFilesize");
    let profile = gb_addon_profile(&env, 11, "Big", vec![unsized_file]).await;
    let err = gb_addon_download(&client, "skins", &profile)
        .await
        .0
        .unwrap_err();
    assert!(
        matches!(err, GbAddonFetchError::TooLarge { limit: 10, .. }),
        "{err:?}"
    );
    assert!(common::tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn a_checksum_mismatch_installs_nothing() {
    let env = store_env().await;
    let mut file = gb_addon_file(&env, 12, 0, "a.vpk", &vpk(), 100).await;
    file["_sMd5Checksum"] = serde_json::json!("00000000000000000000000000000000");
    let profile = gb_addon_profile(&env, 12, "A", vec![file]).await;
    let err = gb_addon_download(&env.client, "skins", &profile)
        .await
        .0
        .unwrap_err();
    assert!(
        matches!(err, GbAddonFetchError::ChecksumMismatch { .. }),
        "{err:?}"
    );
    assert!(common::tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn a_failed_download_leaves_tmp_empty() {
    let env = store_env().await;
    let mut file = gb_addon_file(&env, 30, 0, "gone.zip", b"PK", 100).await;
    file["_sDownloadUrl"] = serde_json::json!(format!("{}/dl/addon/30/missing", env.server.uri()));
    let profile = gb_addon_profile(&env, 30, "Gone", vec![file]).await;
    let err = gb_addon_download(&env.client, "skins", &profile)
        .await
        .0
        .unwrap_err();
    assert!(
        matches!(err, GbAddonFetchError::Http { status: 404, .. }),
        "{err:?}"
    );
    assert!(common::tmp_is_empty(&env.dir));
}
