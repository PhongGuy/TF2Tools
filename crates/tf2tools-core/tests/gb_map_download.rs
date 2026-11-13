//! Downloading a GameBanana map item's newest file and finding its maps.

use crate::common::{self, StoreEnv, gb_addon_file, gb_addon_profile, store_env};

use std::io::Write;

use tf2tools_core::{AddonProfile, GbAddonFetchError, GbId, GbMapDownload, StoreClient};

fn bsp(tag: &[u8]) -> Vec<u8> {
    let mut body = b"VBSP".to_vec();
    body.extend_from_slice(tag);
    body
}

fn bz2(bytes: &[u8]) -> Vec<u8> {
    let mut enc = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
    enc.write_all(bytes).unwrap();
    enc.finish().unwrap()
}

/// Map item `id` named "Pack" whose only file is `body`, saved as `file`.
async fn item(env: &StoreEnv, id: u64, file: &str, body: &[u8]) -> AddonProfile {
    env.server.reset().await;
    let f = gb_addon_file(env, id, 0, file, body, 100).await;
    gb_addon_profile(env, id, "Pack", vec![f]).await
}

async fn fetch(client: &StoreClient, p: &AddonProfile) -> Result<GbMapDownload, GbAddonFetchError> {
    client.download_gb_map(p, &|_| {}).await
}

fn names(d: &GbMapDownload) -> Vec<&str> {
    d.maps().iter().map(|m| m.name()).collect()
}

fn assert_send<T: Send + 'static>() {}

#[tokio::test]
async fn a_pack_downloads_and_lists_its_maps() {
    assert_send::<GbMapDownload>();
    let env = store_env().await;
    let body = common::zip_bytes(&[
        ("Pack/maps/cp_a.bsp", &bsp(b"a")),
        ("Pack/maps/cp_b.bsp", &bsp(b"b")),
        ("Pack/readme.txt", b"hi"),
    ]);
    let p = item(&env, 3, "pack.zip", &body).await;
    let d = fetch(&env.client, &p).await.unwrap();
    assert_eq!(names(&d), ["cp_a", "cp_b"]);
    assert_eq!(d.gb_id(), GbId::new(3));
    assert_eq!(d.name(), "Pack");
    assert_eq!((d.file_name(), d.file_date()), ("pack.zip", 100));
    assert!(!common::tmp_is_empty(&env.dir));
    drop(d);
    assert!(
        common::tmp_is_empty(&env.dir),
        "dropping it removes the scratch dir"
    );
}

#[tokio::test]
async fn a_bare_map_file_is_a_one_map_download() {
    let env = store_env().await;
    let p = item(&env, 4, "koth_bare.bsp", &bsp(b"k")).await;
    assert_eq!(names(&fetch(&env.client, &p).await.unwrap()), ["koth_bare"]);
    let p = item(&env, 5, "koth_bz.bsp.bz2", &bz2(&bsp(b"z"))).await;
    assert_eq!(names(&fetch(&env.client, &p).await.unwrap()), ["koth_bz"]);
}

#[tokio::test]
async fn a_download_without_a_map_is_an_unknown_layout() {
    let env = store_env().await;
    let inner = common::zip_bytes(&[("maps/cp_n.bsp", &bsp(b"n"))]);
    for body in [
        common::zip_bytes(&[("readme.txt", b"hi")]),
        common::zip_bytes(&[("inner.zip", &inner)]),
        b"plain text, not an archive".to_vec(),
    ] {
        let p = item(&env, 6, "pack.zip", &body).await;
        let err = fetch(&env.client, &p).await.unwrap_err();
        assert!(
            matches!(err, GbAddonFetchError::UnknownLayout { payloads: 0, .. }),
            "{err:?}"
        );
        assert!(common::tmp_is_empty(&env.dir));
    }
}

#[tokio::test]
async fn programs_alone_are_refused() {
    let env = store_env().await;
    let p = item(
        &env,
        7,
        "setup.zip",
        &common::zip_bytes(&[("setup.exe", b"MZ")]),
    )
    .await;
    let err = fetch(&env.client, &p).await.unwrap_err();
    assert!(matches!(err, GbAddonFetchError::OnlyExe { .. }), "{err:?}");
    let p = item(&env, 7, "setup.exe", b"MZ").await;
    let err = fetch(&env.client, &p).await.unwrap_err();
    assert!(matches!(err, GbAddonFetchError::OnlyExe { .. }), "{err:?}");
}

#[tokio::test]
async fn more_than_64_maps_is_an_unknown_layout() {
    let env = store_env().await;
    let files: Vec<(String, Vec<u8>)> = (0..65)
        .map(|i| (format!("maps/m_{i:02}.bsp"), bsp(b"m")))
        .collect();
    let entries: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let p = item(&env, 8, "pack.zip", &common::zip_bytes(&entries)).await;
    let err = fetch(&env.client, &p).await.unwrap_err();
    assert!(
        matches!(err, GbAddonFetchError::UnknownLayout { payloads: 65, .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn maps_use_the_map_cap_not_the_addon_cap() {
    let env = store_env().await;
    let p = item(&env, 9, "big.bsp", &bsp(&[0u8; 64])).await;
    let addon_capped = env
        .client
        .clone()
        .with_config(common::fast_config().with_gb_addon_size_cap(10));
    assert!(addon_capped.gb_map_refusal(&p).is_none());
    assert_eq!(names(&fetch(&addon_capped, &p).await.unwrap()), ["big"]);

    env.server.reset().await;
    let map_capped = env
        .client
        .clone()
        .with_config(common::fast_config().with_map_size_cap(10));
    assert!(matches!(
        map_capped.gb_map_refusal(&p),
        Some(GbAddonFetchError::TooLarge { limit: 10, .. })
    ));
    let err = fetch(&map_capped, &p).await.unwrap_err();
    assert!(
        matches!(err, GbAddonFetchError::TooLarge { limit: 10, .. }),
        "{err:?}"
    );
    assert_eq!(
        common::download_requests(&env).await,
        0,
        "the listed size refuses first"
    );
}
