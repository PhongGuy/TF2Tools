//! The addon-store sync records mastercomfig's release and leaves the override VPKs out.

use crate::common;

use common::{StoreEnv, count, sql, store_env, store_fixture};
use tf2tools_core::StoreKind;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const RELEASE: &str = "/gh-api/repos/mastercomfig/mastercomfig/releases/latest";
const DL: &str = "https://github.com/mastercomfig/mastercomfig/releases/download";

async fn serve(env: &StoreEnv, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(RELEASE))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&env.server)
        .await;
}

fn release_with(tag: &str, names: &[&str]) -> Vec<u8> {
    let assets: Vec<serde_json::Value> = names
        .iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "browser_download_url": format!("{DL}/{tag}/{name}"),
                "size": 100,
                "download_count": 7
            })
        })
        .collect();
    serde_json::json!({ "tag_name": tag, "assets": assets })
        .to_string()
        .into_bytes()
}

fn store_names(env: &StoreEnv) -> Vec<String> {
    let conn = sql(&env.dir);
    let mut stmt = conn
        .prepare("SELECT asset_name FROM store_addons ORDER BY asset_name")
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    names
}

#[tokio::test]
async fn the_sync_records_the_base_and_override_urls_and_lists_no_overrides() {
    let env = store_env().await;
    let names = [
        "mastercomfig-base.vpk",
        "mastercomfig-addon-override-1.vpk",
        "mastercomfig-addon-override-3.vpk",
        "mastercomfig-addon-flat-mouse.vpk",
        "autoexec.cfg",
    ];
    serve(&env, release_with("9.100.1", &names)).await;
    let outcome = env
        .client
        .sync(StoreKind::Addons, true, &|_| {})
        .await
        .unwrap();
    assert_eq!(outcome.rows(), 1);
    assert_eq!(store_names(&env), ["mastercomfig-addon-flat-mouse.vpk"]);
    let release = env
        .client
        .mastercomfig_release()
        .unwrap()
        .expect("a release");
    let at = |name: &str| Some(format!("{DL}/9.100.1/{name}"));
    assert_eq!(release.tag(), "9.100.1");
    assert_eq!(
        Some(release.base_url().to_owned()),
        at("mastercomfig-base.vpk")
    );
    assert_eq!(
        release.override_url(1).map(str::to_owned),
        at("mastercomfig-addon-override-1.vpk")
    );
    assert_eq!(release.override_url(2), None);
    assert_eq!(
        release.override_url(3).map(str::to_owned),
        at("mastercomfig-addon-override-3.vpk")
    );
    assert_eq!(release.override_url(0), None);
    assert_eq!(release.override_url(4), None);
}

#[tokio::test]
async fn a_newer_release_replaces_the_recorded_one() {
    let env = store_env().await;
    serve(&env, release_with("9.100.1", &["mastercomfig-base.vpk"])).await;
    env.client
        .sync(StoreKind::Addons, true, &|_| {})
        .await
        .unwrap();
    env.server.reset().await;
    serve(&env, release_with("9.101.0", &["mastercomfig-base.vpk"])).await;
    env.client
        .sync(StoreKind::Addons, true, &|_| {})
        .await
        .unwrap();
    let release = env
        .client
        .mastercomfig_release()
        .unwrap()
        .expect("a release");
    assert_eq!(release.tag(), "9.101.0");
    assert_eq!(count(&env.dir, "mastercomfig_release"), 1);
}

#[tokio::test]
async fn a_release_without_the_base_keeps_the_recorded_one() {
    let env = store_env().await;
    sql(&env.dir)
        .execute(
            "INSERT INTO mastercomfig_release (id, tag, base_url, override_urls)
             VALUES (1, '9.99.0', 'https://example.invalid/base.vpk', '[null,null,null]')",
            [],
        )
        .unwrap();
    serve(&env, store_fixture("github-mastercomfig-latest.json")).await;
    env.client
        .sync(StoreKind::Addons, true, &|_| {})
        .await
        .unwrap();
    let release = env.client.mastercomfig_release().unwrap().expect("kept");
    assert_eq!(release.tag(), "9.99.0");
    assert_eq!(store_names(&env).len(), 2);
}

#[tokio::test]
async fn a_store_never_synced_has_no_release() {
    let env = store_env().await;
    assert_eq!(env.client.mastercomfig_release().unwrap(), None);
}
