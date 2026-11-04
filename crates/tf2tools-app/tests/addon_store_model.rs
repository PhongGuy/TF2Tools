mod support;

use tf2tools_app::pages::addon_store::model::{
    AddonState, addon_state, downloads_label, ordered, store_release,
};
use tf2tools_core::{StoreKind, Tf2Install};

const FLAT: &str = "mastercomfig-addon-flat-mouse.vpk";
const LOW: &str = "mastercomfig-addon-low-memory.vpk";
const TUT: &str = "mastercomfig-addon-no-tutorial.vpk";
const FOOT: &str = "mastercomfig-addon-no-footsteps.vpk";
const NOWHERE: &str = "http://127.0.0.1:9";

#[test]
fn addon_state_follows_v5_s_order() {
    let env = support::env();
    let s = support::services(&env, support::no_steam());
    support::seed_addons(
        &env,
        NOWHERE,
        &[
            (FLAT, "Flat Mouse", "9.100.1"),
            (LOW, "Low Memory", "9.100.1"),
            (TUT, "No Tutorial", "9.100.1"),
            (FOOT, "No Footsteps", "9.100.1"),
        ],
    );
    let vpk = support::vpk_bytes(&[("cfg/addons/x.cfg", b"v1".as_slice())]);
    support::seed_library_addon(&s, &env, LOW, "9.100.1", &vpk, false);
    support::seed_library_addon(&s, &env, TUT, "9.100.1", &vpk, true);
    support::seed_library_addon(&s, &env, FOOT, "9.0.0", &vpk, true);
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    let entries = s.library.list_addons(Some(&tf2)).unwrap();
    let state = |asset: &str| addon_state(entries.iter().find(|e| e.name().as_str() == asset));
    assert_eq!(state(FLAT), AddonState::Download);
    assert_eq!(state(LOW), AddonState::InLibrary);
    assert_eq!(state(TUT), AddonState::Installed);
    assert_eq!(state(FOOT), AddonState::Update);
    // Without a TF2 install nothing reads as installed, so the old copy is only In Library.
    let bare = s.library.list_addons(None).unwrap();
    assert_eq!(
        addon_state(bare.iter().find(|e| e.name().as_str() == FOOT)),
        AddonState::InLibrary
    );
}

#[test]
fn labels_read_as_v5() {
    let labels: Vec<_> = [
        AddonState::Download,
        AddonState::InLibrary,
        AddonState::Installed,
        AddonState::Update,
    ]
    .map(AddonState::label)
    .into();
    assert_eq!(labels, ["Download", "In Library", "Installed", "Update"]);
    assert!(AddonState::Download.is_action() && AddonState::Update.is_action());
    assert!(!AddonState::Installed.is_action() && !AddonState::InLibrary.is_action());
    assert_eq!(downloads_label(0), "0 downloads");
    assert_eq!(downloads_label(989), "989 downloads");
    assert_eq!(downloads_label(40_838), "40,838 downloads");
    assert_eq!(downloads_label(1_234_567), "1,234,567 downloads");
}

#[test]
fn the_store_lists_by_name_with_the_first_release_as_the_header_tag() {
    let env = support::env();
    let s = support::services(&env, support::no_steam());
    assert_eq!(store_release(&[]), None);
    // Each row has its own tag: by name the first is 9.100.1, the last 9.98.0 and the
    // string maximum 9.99.0, so only a first-row choice reads 9.100.1.
    support::seed_addons(
        &env,
        NOWHERE,
        &[
            (TUT, "No Tutorial", "9.98.0"),
            (FLAT, "flat Mouse", "9.100.1"),
            (LOW, "Low Memory", "9.99.0"),
        ],
    );
    let items = ordered(s.store.store_items(StoreKind::Addons).unwrap());
    let names: Vec<&str> = items.iter().map(|i| i.name()).collect();
    assert_eq!(names, ["flat Mouse", "Low Memory", "No Tutorial"]);
    assert_eq!(store_release(&items), Some("9.100.1"));
}
