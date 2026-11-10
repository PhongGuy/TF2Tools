//! Downloading a GameBanana add-on's newest file into payloads, and every refusal.

use crate::common::{self, gb_addon_download, gb_addon_file, gb_addon_profile, store_env};

use rars::ArchiveVersion;
use tf2tools_core::{ArchiveError, GbAddonFetchError, GbId, PayloadKind};

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

#[tokio::test]
async fn a_bare_vpk_download_is_one_payload() {
    let env = store_env().await;
    let files = vec![gb_addon_file(&env, 8, 0, "Crit.vpk", &vpk(), 100).await];
    let profile = gb_addon_profile(&env, 8, "Crit", files).await;
    let download = gb_addon_download(&env.client, "effects", &profile)
        .await
        .0
        .unwrap();
    let names: Vec<&str> = download.payloads().iter().map(|p| p.name()).collect();
    assert_eq!(names, ["Crit.vpk"]);
}

#[tokio::test]
async fn refusals_name_their_reason_and_leave_tmp_empty() {
    let env = store_env().await;
    let exe = common::zip_bytes(&[("Setup.exe", b"MZ"), ("readme.txt", b"r")]);
    let loose = common::zip_bytes(&[("readme.txt", b"r")]);
    let many: Vec<(String, Vec<u8>)> = (0..17).map(|i| (format!("v{i:02}.vpk"), vpk())).collect();
    let many_refs: Vec<(&str, &[u8])> = many
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let crowded = common::zip_bytes(&many_refs);
    let locked = common::rar_bytes(ArchiveVersion::Rar50, &[("a.vpk", &vpk())], Some(b"pw"));
    let evil = common::rar5_stored(&["ok.txt", "..\\evil.txt"], 0);
    let cases: [(u64, &str, &[u8]); 5] = [
        (20, "exe.zip", &exe),
        (21, "loose.zip", &loose),
        (22, "crowded.zip", &crowded),
        (23, "locked.rar", &locked),
        (24, "evil.rar", &evil),
    ];
    let mut errors = Vec::new();
    for (id, name, body) in cases {
        let file = gb_addon_file(&env, id, 0, name, body, 100).await;
        let profile = gb_addon_profile(&env, id, name, vec![file]).await;
        errors.push(
            gb_addon_download(&env.client, "skins", &profile)
                .await
                .0
                .unwrap_err(),
        );
        assert!(common::tmp_is_empty(&env.dir), "{name} left files in tmp");
    }
    assert!(
        matches!(&errors[0], GbAddonFetchError::OnlyExe { file_name } if file_name == "exe.zip"),
        "{:?}",
        errors[0]
    );
    assert!(
        matches!(
            errors[1],
            GbAddonFetchError::UnknownLayout { payloads: 0, .. }
        ),
        "{:?}",
        errors[1]
    );
    assert!(
        matches!(
            errors[2],
            GbAddonFetchError::UnknownLayout { payloads: 17, .. }
        ),
        "{:?}",
        errors[2]
    );
    assert!(
        matches!(
            errors[3],
            GbAddonFetchError::Archive(ArchiveError::Encrypted { .. })
        ),
        "{:?}",
        errors[3]
    );
    assert!(
        matches!(
            errors[4],
            GbAddonFetchError::Archive(ArchiveError::UnsafePath { .. })
        ),
        "{:?}",
        errors[4]
    );
}

#[tokio::test]
async fn a_download_that_is_not_an_archive_is_refused_by_kind() {
    let env = store_env().await;
    let cases: [(u64, &str, &[u8]); 3] = [
        (40, "notes.txt", b"just text"),
        (41, "fake.vpk", b"not a vpk"),
        (42, "Setup.exe", b"MZ\x90\x00"),
    ];
    let mut errors = Vec::new();
    for (id, name, body) in cases {
        let file = gb_addon_file(&env, id, 0, name, body, 100).await;
        let profile = gb_addon_profile(&env, id, name, vec![file]).await;
        errors.push(
            gb_addon_download(&env.client, "skins", &profile)
                .await
                .0
                .unwrap_err(),
        );
        assert!(common::tmp_is_empty(&env.dir), "{name} left files in tmp");
    }
    assert!(
        matches!(
            errors[0],
            GbAddonFetchError::UnknownLayout { payloads: 0, .. }
        ),
        "{:?}",
        errors[0]
    );
    assert!(
        matches!(
            errors[1],
            GbAddonFetchError::UnknownLayout { payloads: 0, .. }
        ),
        "{:?}",
        errors[1]
    );
    assert!(
        matches!(&errors[2], GbAddonFetchError::OnlyExe { file_name } if file_name == "Setup.exe"),
        "{:?}",
        errors[2]
    );
}

#[tokio::test]
async fn an_unknown_layout_says_whether_it_found_nothing_or_too_much() {
    let env = store_env().await;
    let many: Vec<(String, Vec<u8>)> = (0..17).map(|i| (format!("v{i:02}.vpk"), vpk())).collect();
    let many_refs: Vec<(&str, &[u8])> = many
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let crowded = common::zip_bytes(&many_refs);
    let cases: [(u64, &str, &[u8]); 2] = [
        (50, "notes.txt", b"just text"),
        (51, "crowded.zip", &crowded),
    ];
    let mut texts = Vec::new();
    for (id, name, body) in cases {
        let file = gb_addon_file(&env, id, 0, name, body, 100).await;
        let profile = gb_addon_profile(&env, id, name, vec![file]).await;
        let err = gb_addon_download(&env.client, "skins", &profile)
            .await
            .0
            .unwrap_err();
        texts.push(err.to_string());
    }
    assert_eq!(
        texts[0],
        "notes.txt holds nothing TF2Tools knows how to install"
    );
    assert_eq!(
        texts[1],
        "crowded.zip has 17 possible parts, too many to tell which to install"
    );
}
