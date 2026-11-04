mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::TestAppContext;
use tf2tools_app::services::OpError;
use tf2tools_app::store_preview::{load_preview, parse_preview_key, preview_key};
use tf2tools_core::GbId;

fn ready(cx: &mut TestAppContext, env: &support::Env, server: &support::StoreServer) {
    support::init(cx);
    support::install_services(cx, support::services(env, support::no_steam()));
    support::store_ready(cx, server);
}

fn load(cx: &mut TestAppContext, id: u64) -> Result<Vec<u8>, OpError> {
    let out = Rc::new(RefCell::new(None));
    let slot = out.clone();
    cx.update(|cx| {
        let task = load_preview(GbId::new(id), cx);
        cx.spawn(async move |_| *slot.borrow_mut() = Some(task.await))
            .detach();
    });
    support::wait_real(cx, "the preview", |_| out.borrow().is_some());
    out.take().unwrap()
}

#[test]
fn a_preview_key_names_the_store_sound() {
    assert_eq!(preview_key(GbId::new(7)).as_ref(), "store-sound:7");
}

#[gpui_kit::test]
fn a_preview_is_fetched_from_the_sound_s_preview_url(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    ready(cx, &env, &server);
    let url = server.mount_preview(1, vec![1, 2, 3]);
    support::seed_sounds(&env, &[(1, "Ding", "hitsound", Some(url.as_str()))]);
    assert_eq!(load(cx, 1), Ok(vec![1, 2, 3]));
}

#[gpui_kit::test]
fn a_missing_row_preview_or_file_fails_as_could_not_play_preview(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    ready(cx, &env, &server);
    let gone = format!("{}/preview/3.mp3", server.uri());
    support::seed_sounds(
        &env,
        &[
            (2, "Silent", "sound", None),
            (3, "Gone", "sound", Some(gone.as_str())),
            (4, "Elsewhere", "sound", Some("https://evil.example/4.mp3")),
        ],
    );
    for id in [2, 3, 4, 99] {
        let err = load(cx, id).unwrap_err();
        assert_eq!(err.title(), "Could not play preview", "sound {id}: {err}");
        support::assert_plain(err.description());
    }
    assert_eq!(
        load(cx, 2).unwrap_err().description(),
        "This sound has no preview."
    );
    assert_eq!(
        load(cx, 4).unwrap_err().description(),
        "This preview isn't hosted on GameBanana, so TF2Tools won't play it."
    );
    assert_eq!(
        load(cx, 99).unwrap_err().description(),
        "This sound is no longer in the store."
    );
}

#[test]
fn a_preview_key_parses_back_to_its_id() {
    assert_eq!(
        parse_preview_key(&preview_key(GbId::new(7))),
        Some(GbId::new(7))
    );
    assert_eq!(parse_preview_key("store-sound:x"), None);
    assert_eq!(parse_preview_key("library-sound:7"), None);
}
