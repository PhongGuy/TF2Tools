//! A copy opened with the schema for its repo; without a repo, switches only.

mod common;

use common::hud_copy::{game_for, import_stock_hud};
use common::hud_schema::schema;
use common::hud_switches::{dir, save, switch_layer};
use common::{LibraryEnv, library_env};
use tf2tools_core::{
    Change, HudEntry, HudSchemas, LibraryKind, OptionChange, OptionValue, Platform, Provenance,
    SetKey,
};

fn copy(env: &LibraryEnv, root: &str) -> HudEntry {
    let layer = switch_layer(&[]);
    let extra: Vec<(&str, &str)> = layer
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let base = import_stock_hud(env, root, &extra, Provenance::Local);
    env.lib.copy_hud(base.id()).unwrap()
}

// A local import records no repo; give the copy one as a curated copy would have.
fn set_repo(env: &LibraryEnv, copy: &HudEntry, repo: &str) {
    let maker = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name())
        .join(".tf2tools/maker.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&maker).unwrap()).unwrap();
    json["base"]["repo"] = repo.into();
    std::fs::write(&maker, serde_json::to_vec(&json).unwrap()).unwrap();
}

fn option_ids(env: &LibraryEnv, copy: &HudEntry) -> (Vec<String>, usize) {
    let tf = game_for(env, &[]);
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let made = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap();
    let ids = made
        .options()
        .specs()
        .iter()
        .map(|s| s.id().to_owned())
        .collect();
    (ids, made.options().misfits())
}

#[test]
fn a_copy_of_a_matching_repo_opens_with_its_schema_options() {
    let mut env = library_env();
    env.lib
        .set_hud_schemas(HudSchemas::bundled().with(schema()));
    let copy = copy(&env, "somehud");
    set_repo(&env, &copy, "https://github.com/Someone/SomeHUD");
    let (ids, misfits) = option_ids(&env, &copy);
    assert!(
        ids.contains(&"e:sh_tan".to_owned())
            && ids.contains(&"f:customizations/damage.res:0".to_owned()),
        "{ids:?}"
    );
    assert!(!ids.contains(&"b:#customization/bh_player_largehealth.res".to_owned()));
    assert_eq!(misfits, 2);
}

#[test]
fn a_copy_without_a_repo_or_with_another_gets_switches_only() {
    let mut env = library_env();
    env.lib
        .set_hud_schemas(HudSchemas::bundled().with(schema()));
    let plain = copy(&env, "somehud");
    assert!(
        option_ids(&env, &plain)
            .0
            .iter()
            .all(|id| !id.starts_with("e:"))
    );
    let other = copy(&env, "otherhud");
    set_repo(&env, &other, "https://github.com/someone/otherhud");
    assert!(
        option_ids(&env, &other)
            .0
            .iter()
            .all(|id| !id.starts_with("e:"))
    );
}

#[test]
fn the_default_library_uses_the_bundled_set() {
    let env = library_env();
    let copy = copy(&env, "somehud");
    set_repo(&env, &copy, "https://github.com/rbjaxter/budhud");
    // budhud's schema names files the fixture lacks, so its controls are misfits, not errors.
    let (ids, misfits) = option_ids(&env, &copy);
    assert!(misfits > 0, "{ids:?}");
}

#[test]
fn a_schema_key_edit_on_a_scene_file_the_model_also_edits_is_no_game_update() {
    let mut env = library_env();
    env.lib
        .set_hud_schemas(HudSchemas::bundled().with(schema()));
    let copy = copy(&env, "somehud");
    set_repo(&env, &copy, "https://github.com/Someone/SomeHUD");
    let tf = game_for(&env, &[]);
    // The schema moves the chat to the top in basechat.res, the file the model draws the chat
    // from, and the model edits the same file; its sources then hash the unswitched base bytes.
    let block = ["Resource/UI/BaseChat.res", "HudChat"];
    let changes = [
        Change::option(
            1,
            OptionChange::new(
                OptionChange::SCHEMA,
                "e:sh_chat_top",
                OptionValue::Bool(true),
            ),
        ),
        Change::set(
            2,
            SetKey::new(
                "resource/ui/basechat.res",
                &block,
                "xpos",
                None,
                Some("10"),
                "20",
            ),
        ),
    ];
    save(&env, &tf, &copy, &changes);
    let written =
        std::fs::read_to_string(dir(&env, &copy).join("resource/ui/basechat.res")).unwrap();
    assert!(
        written.contains("ypos 0") && written.contains("xpos 20"),
        "{written}"
    );
    let files = env.lib.made_hud_files(copy.id(), &tf).unwrap();
    let made = env
        .lib
        .open_made_hud(copy.id(), &files, Platform::Linux)
        .unwrap();
    assert_eq!((made.changes().len(), made.dropped()), (2, 0));
    assert!(!made.tf2_updated());
}
