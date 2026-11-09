//! Writing the player's module choices to tf/cfg/overrides/modules.cfg and saving them.

use crate::common;

use common::{
    StoreEnv, mastercomfig_base, module_catalog, set_mastercomfig_state, sql, store_env, tree,
    write_file,
};
use tempfile::TempDir;
use tf2tools_core::{ApplyModulesError, Choices, ModuleChoice, Preset, Tf2Install};

const HEADER: &str = "// Written by TF2Tools; changes here are replaced\n";

fn value(v: &str) -> ModuleChoice {
    ModuleChoice::Value(v.to_owned())
}

/// A TF2 tree with mastercomfig's base in tf/custom, managed by TF2Tools on `preset`.
fn managed_game(env: &StoreEnv, preset: &str) -> (TempDir, Tf2Install) {
    let (tmp, game) = common::temp_tf2_tree();
    let tf2 = Tf2Install::from_path(&game).unwrap();
    write_file(
        &tf2.custom_dir().join("mastercomfig-base.vpk"),
        &mastercomfig_base("9.100.1"),
    );
    set_mastercomfig_state(&env.dir, "9.100.1", preset);
    (tmp, tf2)
}

fn modules_cfg(tf2: &Tf2Install) -> String {
    std::fs::read_to_string(tf2.tf_dir().join("cfg/overrides/modules.cfg")).unwrap()
}

async fn apply(
    env: &StoreEnv,
    tf2: &Tf2Install,
    choices: &Choices,
    replace: bool,
) -> Result<Vec<String>, ApplyModulesError> {
    env.client
        .apply_mastercomfig_modules(tf2, &module_catalog(), choices, replace)
        .await
}

fn medium_choices() -> Choices {
    let catalog = module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "hud_avatars", value("friends"));
    choices.set(&catalog, "fpscap", value("120"));
    choices.set(&catalog, "lod", ModuleChoice::NotSet);
    choices
}

#[tokio::test]
async fn choices_are_written_in_catalog_order_and_saved() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env, "medium");
    let choices = medium_choices();
    assert!(apply(&env, &tf2, &choices, false).await.unwrap().is_empty());
    assert_eq!(
        modules_cfg(&tf2),
        format!("{HEADER}alias lod\nfpscap=120\nhud_avatars=friends\n")
    );
    assert_eq!(env.lib.mastercomfig_choices().unwrap(), Some(choices));
    let saved: String = sql(&env.dir)
        .query_row("SELECT modules FROM mastercomfig_state", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        saved,
        r#"{"fpscap":"120","hud_avatars":"friends","lod":null}"#
    );
}

#[tokio::test]
async fn a_file_tf2tools_wrote_is_replaced_and_no_choices_leave_only_the_first_line() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env, "medium");
    let overrides = tf2.tf_dir().join("cfg/overrides");
    write_file(
        &overrides.join("modules.cfg"),
        b"// Written by TF2Tools; changes here are replaced\r\nfpscap=60\r\n",
    );
    apply(&env, &tf2, &medium_choices(), false).await.unwrap();
    apply(&env, &tf2, &Choices::new(Preset::Medium), false)
        .await
        .unwrap();
    assert_eq!(modules_cfg(&tf2), HEADER);
    assert_eq!(
        tree(&overrides),
        ["modules.cfg"],
        "our own file gets no backup"
    );
    assert!(
        env.lib
            .mastercomfig_choices()
            .unwrap()
            .unwrap()
            .modules()
            .is_empty()
    );
}

#[tokio::test]
async fn a_setting_the_catalog_lacks_is_dropped_and_named() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env, "medium");
    let catalog = module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "gone", value("on"));
    choices.set(&catalog, "fpscap", value("60"));
    assert_eq!(apply(&env, &tf2, &choices, false).await.unwrap(), ["gone"]);
    assert_eq!(modules_cfg(&tf2), format!("{HEADER}fpscap=60\n"));
    assert_eq!(
        env.lib.mastercomfig_choices().unwrap().unwrap().get("gone"),
        None
    );
}

#[tokio::test]
async fn a_foreign_modules_cfg_is_refused_then_kept_as_numbered_backups() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env, "medium");
    let overrides = tf2.tf_dir().join("cfg/overrides");
    write_file(&overrides.join("modules.cfg"), b"fpscap=60\n");
    write_file(&overrides.join("autoexec.cfg"), b"echo mine\n");
    write_file(&overrides.join("setup_hook.cfg"), b"echo hook\n");
    let choices = medium_choices();
    let err = apply(&env, &tf2, &choices, false).await.unwrap_err();
    assert!(
        matches!(err, ApplyModulesError::ForeignModulesCfg { .. }),
        "{err:?}"
    );
    assert_eq!(modules_cfg(&tf2), "fpscap=60\n");
    assert!(
        env.lib
            .mastercomfig_choices()
            .unwrap()
            .unwrap()
            .modules()
            .is_empty(),
        "nothing saved"
    );

    apply(&env, &tf2, &choices, true).await.unwrap();
    assert!(modules_cfg(&tf2).starts_with(HEADER));
    for (n, mine) in [(1, "fpscap=30\n"), (2, "fpscap=75\n")] {
        write_file(&overrides.join("modules.cfg"), mine.as_bytes());
        apply(&env, &tf2, &choices, true).await.unwrap();
        let backup = overrides.join(format!("modules.cfg.bak.{n}"));
        assert_eq!(std::fs::read_to_string(backup).unwrap(), mine);
    }
    let first = std::fs::read_to_string(overrides.join("modules.cfg.bak")).unwrap();
    assert_eq!(first, "fpscap=60\n");
    assert_eq!(
        tree(&overrides),
        [
            "autoexec.cfg",
            "modules.cfg",
            "modules.cfg.bak",
            "modules.cfg.bak.1",
            "modules.cfg.bak.2",
            "setup_hook.cfg"
        ]
    );
    assert_eq!(
        std::fs::read_to_string(overrides.join("autoexec.cfg")).unwrap(),
        "echo mine\n"
    );
    assert_eq!(
        std::fs::read_to_string(overrides.join("setup_hook.cfg")).unwrap(),
        "echo hook\n"
    );
}

#[tokio::test]
async fn an_install_tf2tools_does_not_manage_is_refused() {
    let env = store_env().await;
    let (_game, game) = common::temp_tf2_tree();
    let tf2 = Tf2Install::from_path(&game).unwrap();
    write_file(
        &tf2.custom_dir().join("mastercomfig-base.vpk"),
        &mastercomfig_base("9.100.1"),
    );
    let err = apply(&env, &tf2, &medium_choices(), false)
        .await
        .unwrap_err();
    assert!(matches!(err, ApplyModulesError::NotManaged), "{err:?}");
    assert!(!tf2.tf_dir().join("cfg").exists());
    assert_eq!(env.lib.mastercomfig_choices().unwrap(), None);
}

#[tokio::test]
async fn choices_survive_a_preset_switch_even_when_they_now_match_it() {
    let env = store_env().await;
    let (_game, tf2) = managed_game(&env, "medium");
    let catalog = module_catalog();
    let mut choices = Choices::new(Preset::Medium);
    choices.set(&catalog, "lod", value("high"));
    apply(&env, &tf2, &choices, false).await.unwrap();
    env.lib.apply_preset(&tf2, Preset::High).unwrap();
    let saved = env.lib.mastercomfig_choices().unwrap().unwrap();
    assert_eq!(saved.preset(), Preset::High);
    assert_eq!(saved.get("lod"), Some(&value("high")));
    assert_eq!(modules_cfg(&tf2), format!("{HEADER}lod=high\n"));
}
