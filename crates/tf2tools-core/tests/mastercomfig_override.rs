//! The override VPK that low sound or no decals needs, swapped as the choices change.

use crate::common;

use common::{
    StoreEnv, mastercomfig_base, module_catalog, set_mastercomfig_state, sql, store_env, tree,
    vpk_bytes, write_file,
};
use tempfile::TempDir;
use tf2tools_core::{ApplyModulesError, Choices, ModuleChoice, Preset, Tf2Install};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const AT: &str = "/gh/dl";
const BASE: &str = "mastercomfig-base.vpk";

fn value(v: &str) -> ModuleChoice {
    ModuleChoice::Value(v.to_owned())
}

/// A TF2 tree with mastercomfig's base in tf/custom, managed by TF2Tools on Medium.
fn managed_game(env: &StoreEnv) -> (TempDir, Tf2Install) {
    let (tmp, game) = common::temp_tf2_tree();
    let tf2 = Tf2Install::from_path(&game).unwrap();
    write_file(&tf2.custom_dir().join(BASE), &mastercomfig_base("9.100.1"));
    set_mastercomfig_state(&env.dir, "9.100.1", "medium");
    (tmp, tf2)
}

fn override_vpk(n: u8) -> Vec<u8> {
    let entry = format!("cfg/addons/override-{n}.cfg");
    vpk_bytes(&[(entry.as_str(), b"echo override".as_slice())])
}

/// Records release 9.100.1 whose three override assets live under AT.
fn record_release(env: &StoreEnv) {
    let uri = env.server.uri();
    let urls: Vec<String> = (1..=3)
        .map(|n| format!("{uri}{AT}/override-{n}.vpk"))
        .collect();
    sql(&env.dir)
        .execute(
            "INSERT OR REPLACE INTO mastercomfig_release (id, tag, base_url, override_urls)
             VALUES (1, '9.100.1', ?1, ?2)",
            [
                format!("{uri}{AT}/base.vpk"),
                serde_json::to_string(&urls).unwrap(),
            ],
        )
        .unwrap();
}

async fn serve_overrides(env: &StoreEnv) {
    for n in 1..=3u8 {
        Mock::given(method("GET"))
            .and(path(format!("{AT}/override-{n}.vpk")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(override_vpk(n)))
            .mount(&env.server)
            .await;
    }
}

fn overrides(tf2: &Tf2Install) -> Vec<String> {
    tree(&tf2.custom_dir())
        .into_iter()
        .filter(|f| f.starts_with("mastercomfig-addon-override-"))
        .collect()
}

async fn requests(env: &StoreEnv) -> usize {
    env.server.received_requests().await.unwrap().len()
}

async fn apply(
    env: &StoreEnv,
    tf2: &Tf2Install,
    choices: &Choices,
) -> Result<Vec<String>, ApplyModulesError> {
    env.client
        .apply_mastercomfig_modules(tf2, &module_catalog(), choices, false)
        .await
}

#[tokio::test]
async fn low_sound_and_no_decals_bring_in_the_matching_override() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env);
    record_release(&env);
    serve_overrides(&env).await;
    let catalog = module_catalog();
    let custom = tf2.custom_dir();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "sound", value("low"));
    apply(&env, &tf2, &choices).await.unwrap();
    assert_eq!(overrides(&tf2), ["mastercomfig-addon-override-1.vpk"]);
    let one = std::fs::read(custom.join("mastercomfig-addon-override-1.vpk")).unwrap();
    assert_eq!(one, override_vpk(1));
    write_file(
        &custom.join("mastercomfig-addon-override-1.vpk.sound.cache"),
        b"stale",
    );
    choices.set(&catalog, "decals", value("off"));
    apply(&env, &tf2, &choices).await.unwrap();
    assert_eq!(
        overrides(&tf2),
        ["mastercomfig-addon-override-3.vpk"],
        "the old one and its cache go"
    );
    choices.clear();
    apply(&env, &tf2, &choices).await.unwrap();
    assert!(overrides(&tf2).is_empty());
    assert_eq!(tree(&custom), [BASE], "no temp file is left");
}

#[tokio::test]
async fn an_unchanged_override_fetches_nothing_and_an_old_one_stays() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env);
    record_release(&env);
    serve_overrides(&env).await;
    let old = write_file(
        &tf2.custom_dir().join("mastercomfig-addon-override-2.vpk"),
        b"from the addon days",
    );
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&module_catalog(), "fpscap", value("120"));
    apply(&env, &tf2, &choices).await.unwrap();
    assert_eq!(overrides(&tf2), ["mastercomfig-addon-override-2.vpk"]);
    assert_eq!(std::fs::read(old).unwrap(), b"from the addon days");
    assert_eq!(requests(&env).await, 0);
}

#[tokio::test]
async fn a_failed_override_download_changes_nothing() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env);
    record_release(&env);
    sql(&env.dir)
        .execute(
            "UPDATE mastercomfig_state SET modules = ?1",
            [r#"{"decals":"off"}"#],
        )
        .unwrap();
    write_file(
        &tf2.custom_dir().join("mastercomfig-addon-override-2.vpk"),
        &override_vpk(2),
    );
    let catalog = module_catalog();
    let mut choices = env.lib.mastercomfig_choices().unwrap().unwrap();
    choices.reset("decals");
    choices.set(&catalog, "sound", value("low"));
    let err = apply(&env, &tf2, &choices).await.unwrap_err();
    assert!(
        matches!(err, ApplyModulesError::Http { status: 404, .. }),
        "{err:?}"
    );
    assert_eq!(overrides(&tf2), ["mastercomfig-addon-override-2.vpk"]);
    assert!(!tf2.tf_dir().join("cfg/overrides/modules.cfg").exists());
    let saved = env.lib.mastercomfig_choices().unwrap().unwrap();
    assert_eq!(saved.get("decals"), Some(&value("off")));
    assert_eq!(saved.get("sound"), None);
}

#[tokio::test]
async fn a_download_that_is_not_a_vpk_leaves_no_file() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env);
    record_release(&env);
    Mock::given(method("GET"))
        .and(path(format!("{AT}/override-1.vpk")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"<html>gone</html>".to_vec()))
        .mount(&env.server)
        .await;
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&module_catalog(), "sound", value("low"));
    let err = apply(&env, &tf2, &choices).await.unwrap_err();
    assert!(matches!(err, ApplyModulesError::NotAVpk { .. }), "{err:?}");
    assert_eq!(tree(&tf2.custom_dir()), [BASE]);
}

#[tokio::test]
async fn a_failed_download_leaves_a_foreign_modules_cfg_where_it_was() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env);
    record_release(&env);
    let cfg_dir = tf2.tf_dir().join("cfg/overrides");
    write_file(&cfg_dir.join("modules.cfg"), b"fpscap=60\n");
    let catalog = module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "sound", value("low"));
    let err = env
        .client
        .apply_mastercomfig_modules(&tf2, &catalog, &choices, true)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ApplyModulesError::Http { status: 404, .. }),
        "{err:?}"
    );
    assert_eq!(tree(&cfg_dir), ["modules.cfg"], "no backup was made");
    assert_eq!(
        std::fs::read_to_string(cfg_dir.join("modules.cfg")).unwrap(),
        "fpscap=60\n"
    );
}

#[tokio::test]
async fn without_a_recorded_release_the_override_cannot_change() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env);
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&module_catalog(), "sound", value("low"));
    let err = apply(&env, &tf2, &choices).await.unwrap_err();
    assert!(matches!(err, ApplyModulesError::NoRelease), "{err:?}");
    assert_eq!(tree(&tf2.custom_dir()), [BASE]);
    assert!(!tf2.tf_dir().join("cfg/overrides/modules.cfg").exists());
    assert_eq!(requests(&env).await, 0);
}
