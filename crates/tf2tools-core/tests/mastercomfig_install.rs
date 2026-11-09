//! Installing and updating mastercomfig-base.vpk from the recorded release.

use crate::common;

use common::{
    StoreEnv, count, mastercomfig_base, set_mastercomfig_state, sql, store_env, tree, write_file,
};
use tempfile::TempDir;
use tf2tools_core::{InstallMastercomfigError, MastercomfigStatus, Preset, Tf2Install};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const BASE: &str = "mastercomfig-base.vpk";
const HOOK: &str = "comfig-custom/cfg/app/setup_hook.cfg";
const AT: &str = "/gh/dl/mastercomfig-base.vpk";

fn game() -> (TempDir, Tf2Install) {
    let (tmp, game) = common::temp_tf2_tree();
    let install = Tf2Install::from_path(&game).unwrap();
    (tmp, install)
}

async fn release(env: &StoreEnv, tag: &str, status: u16, body: Vec<u8>) {
    sql(&env.dir)
        .execute(
            "INSERT OR REPLACE INTO mastercomfig_release (id, tag, base_url, override_urls)
             VALUES (1, ?1, ?2, '[null,null,null]')",
            [tag.to_owned(), format!("{}{AT}", env.server.uri())],
        )
        .unwrap();
    Mock::given(method("GET"))
        .and(path(AT))
        .respond_with(ResponseTemplate::new(status).set_body_bytes(body))
        .mount(&env.server)
        .await;
}

async fn install(env: &StoreEnv, tf2: &Tf2Install) -> Result<String, InstallMastercomfigError> {
    env.client.install_mastercomfig(tf2, &|_, _| {}).await
}

fn hook(tf2: &Tf2Install) -> String {
    std::fs::read_to_string(tf2.custom_dir().join(HOOK)).unwrap()
}

#[tokio::test]
async fn install_downloads_the_base_and_manages_it_on_medium() {
    let env = store_env().await;
    let (_game, tf2) = game();
    release(&env, "9.100.1", 200, mastercomfig_base("9.100.1")).await;
    write_file(
        &tf2.custom_dir().join("mastercomfig-base.vpk.sound.cache"),
        b"stale",
    );
    assert_eq!(install(&env, &tf2).await.unwrap(), "9.100.1");
    let custom = tf2.custom_dir();
    assert_eq!(
        std::fs::read(custom.join(BASE)).unwrap(),
        mastercomfig_base("9.100.1")
    );
    assert_eq!(tree(&custom), [HOOK, BASE]);
    assert_eq!(hook(&tf2), "preset=medium\n");
    assert_eq!(
        env.lib.mastercomfig_status(&tf2).unwrap(),
        MastercomfigStatus::Managed {
            version: "9.100.1".to_owned(),
            preset: Preset::Medium
        }
    );
}

#[tokio::test]
async fn an_update_replaces_the_base_and_keeps_the_preset() {
    let env = store_env().await;
    let (_game, tf2) = game();
    write_file(&tf2.custom_dir().join(BASE), &mastercomfig_base("9.99.0"));
    set_mastercomfig_state(&env.dir, "9.99.0", "high");
    release(&env, "9.100.1", 200, mastercomfig_base("9.100.1")).await;
    install(&env, &tf2).await.unwrap();
    assert_eq!(
        std::fs::read(tf2.custom_dir().join(BASE)).unwrap(),
        mastercomfig_base("9.100.1")
    );
    assert_eq!(hook(&tf2), "preset=high\n");
    assert_eq!(
        env.lib.mastercomfig_status(&tf2).unwrap(),
        MastercomfigStatus::Managed {
            version: "9.100.1".to_owned(),
            preset: Preset::High
        }
    );
}

#[tokio::test]
async fn a_download_that_is_not_a_vpk_keeps_the_old_base() {
    let env = store_env().await;
    let (_game, tf2) = game();
    let old = write_file(&tf2.custom_dir().join(BASE), &mastercomfig_base("9.99.0"));
    release(&env, "9.100.1", 200, b"<html>not here</html>".to_vec()).await;
    let err = install(&env, &tf2).await.unwrap_err();
    assert!(
        matches!(err, InstallMastercomfigError::NotAVpk { .. }),
        "{err:?}"
    );
    assert_eq!(std::fs::read(&old).unwrap(), mastercomfig_base("9.99.0"));
    assert_eq!(tree(&tf2.custom_dir()), [BASE], "no partial file is left");
    assert_eq!(count(&env.dir, "mastercomfig_state"), 0);
}

#[tokio::test]
async fn a_failed_download_leaves_nothing_behind() {
    let env = store_env().await;
    let (_game, tf2) = game();
    release(&env, "9.100.1", 404, Vec::new()).await;
    let err = install(&env, &tf2).await.unwrap_err();
    assert!(
        matches!(err, InstallMastercomfigError::Http { status: 404, .. }),
        "{err:?}"
    );
    assert!(tree(&tf2.custom_dir()).is_empty());
}

#[tokio::test]
async fn without_a_recorded_release_nothing_is_fetched() {
    let env = store_env().await;
    let (_game, tf2) = game();
    let err = install(&env, &tf2).await.unwrap_err();
    assert!(
        matches!(err, InstallMastercomfigError::NoRelease),
        "{err:?}"
    );
    assert!(env.server.received_requests().await.unwrap().is_empty());
}
