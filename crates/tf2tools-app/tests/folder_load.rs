//! One background load per found TF2 folder, for any value type, side by side.

use crate::support;

use std::path::Path;
use std::sync::Arc;

use gpui_kit::TestAppContext;
use tf2tools_app::folder_load::{FolderLoad, FolderLoader};
use tf2tools_app::services::{Services, Tf2State, install_tf2_state};
use tf2tools_core::Tf2Install;

#[derive(Debug, Default, Clone, PartialEq)]
struct Folder(String);

#[derive(Debug, Default, Clone, PartialEq)]
struct Depth(usize);

fn folders() -> FolderLoader<Folder> {
    Arc::new(|_: &Services, tf: &Path| Folder(tf.display().to_string()))
}

fn depths() -> FolderLoader<Depth> {
    Arc::new(|_: &Services, tf: &Path| Depth(tf.components().count()))
}

#[gpui_kit::test]
fn two_kinds_load_side_by_side_each_under_its_own_global(cx: &mut TestAppContext) {
    let env = support::env();
    support::init(cx);
    support::install_services(cx, support::services(&env, support::no_steam()));
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let tf2 = cx.update(|cx| install_tf2_state(cx, found));
    let names = cx.update(|cx| FolderLoad::install_with(cx, folders()));
    let counts = cx.update(|cx| FolderLoad::install_with(cx, depths()));
    support::wait_real(cx, "both loads", |cx| {
        cx.update(|cx| {
            names.read(cx).loads() == 1
                && counts.read(cx).loads() == 1
                && !names.read(cx).loading()
                && !counts.read(cx).loading()
        })
    });
    let tf = env.game.join("tf");
    assert_eq!(
        cx.update(|cx| names.read(cx).value().clone()),
        Folder(tf.display().to_string())
    );
    assert_eq!(
        cx.update(|cx| counts.read(cx).value().clone()),
        Depth(tf.components().count())
    );
    let again = cx.update(|cx| FolderLoad::<Folder>::global_or(cx, || panic!("installed")));
    assert_eq!(again.entity_id(), names.entity_id());
    cx.update(|cx| {
        tf2.update(cx, |t, cx| {
            *t = Tf2State::Missing { saved: None };
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| names.read(cx).value().clone()),
        Folder::default()
    );
    assert_eq!(
        cx.update(|cx| counts.read(cx).value().clone()),
        Depth::default()
    );
}
