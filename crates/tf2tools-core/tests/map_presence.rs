//! Where the game already has a map: tf/maps, tf/download/maps and tf/custom folders.

use crate::common::{self, LibraryEnv, library_env, write_file};

use tf2tools_core::{GbId, MapSource};

fn put(env: &LibraryEnv, rel: &str) {
    write_file(&env.tf2.tf_dir().join(rel), b"VBSP");
}

fn presence(env: &LibraryEnv, name: &str, except: Option<u64>) -> Option<MapSource> {
    env.lib
        .map_presence(&env.tf2, name, except.map(GbId::new))
        .unwrap()
}

fn serveme_row(env: &LibraryEnv, name: &str) {
    common::sql(&env.dir)
        .execute(
            "INSERT INTO downloaded_maps (name, size, downloaded_at) VALUES (?1, 4, 'x')",
            [name],
        )
        .unwrap();
}

fn custom(folder: &str) -> Option<MapSource> {
    Some(MapSource::Custom {
        folder: folder.to_owned(),
    })
}

#[test]
fn a_map_the_game_lacks_has_no_source() {
    let env = library_env();
    assert_eq!(presence(&env, "cp_none", None), None);
}

#[test]
fn tf_maps_is_valve_unless_serveme_recorded_it() {
    let env = library_env();
    put(&env, "maps/cp_v.bsp");
    put(&env, "maps/cp_s.bsp");
    serveme_row(&env, "CP_S");
    serveme_row(&env, "cp_gone");
    assert_eq!(presence(&env, "cp_v", None), Some(MapSource::Valve));
    assert_eq!(presence(&env, "cp_s", None), Some(MapSource::Serveme));
    assert_eq!(
        presence(&env, "cp_gone", None),
        None,
        "a row without a file is not a map"
    );
}

#[test]
fn server_downloads_and_custom_folders_count_in_any_case() {
    let env = library_env();
    put(&env, "download/maps/CP_D.bsp");
    put(&env, "custom/myhud/Maps/Cp_C.BSP");
    put(&env, "custom/loose.bsp");
    write_file(&env.tf2.custom_dir().join("pack.vpk"), b"vpk");
    assert_eq!(presence(&env, "cp_d", None), Some(MapSource::Download));
    assert_eq!(presence(&env, "CP_C", None), custom("myhud"));
    assert_eq!(
        presence(&env, "loose", None),
        None,
        "only <folder>/maps counts"
    );
}

#[test]
fn tf_maps_wins_over_a_custom_folder() {
    let env = library_env();
    put(&env, "maps/cp_both.bsp");
    serveme_row(&env, "cp_both");
    put(&env, "custom/a/maps/cp_both.bsp");
    assert_eq!(presence(&env, "cp_both", None), Some(MapSource::Serveme));
}

#[test]
fn except_skips_only_the_items_own_folder() {
    let env = library_env();
    put(&env, "custom/tf2tools-gb-7-x/maps/cp_g.bsp");
    put(&env, "custom/tf2tools-gb-77-y/maps/cp_h.bsp");
    assert_eq!(presence(&env, "cp_g", None), custom("tf2tools-gb-7-x"));
    assert_eq!(presence(&env, "cp_g", Some(7)), None);
    assert_eq!(presence(&env, "cp_g", Some(77)), custom("tf2tools-gb-7-x"));
    assert_eq!(presence(&env, "cp_h", Some(7)), custom("tf2tools-gb-77-y"));
}
