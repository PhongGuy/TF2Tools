use tf2tools_app::updater::keys::{
    TestSeam, parse_test_base, parse_test_key, production_source, test_seam_with, trusted_keys,
};
use tf2tools_core::{Endpoints, UPDATE_PUBLIC_KEYS};

use crate::support;
use support::update_feed::{SignedFeed, fresh_key};

#[test]
fn a_minisign_public_key_is_56_base64_characters() {
    assert_eq!(fresh_key().pk.to_base64().len(), 56);
}

#[test]
fn exactly_one_key_is_accepted() {
    let key = fresh_key().pk.to_base64();
    let other = fresh_key().pk.to_base64();
    assert_eq!(parse_test_key(&format!("  {key}\n")), Some(key.clone()));
    for raw in [
        String::new(),
        format!("{key} {other}"),
        format!("{key},{other}"),
        format!("{key}\n{other}"),
        format!("{key}{other}"),
        format!("{key}A"),
        key[..55].to_owned(),
        "!".repeat(56),
    ] {
        assert_eq!(parse_test_key(&raw), None, "{raw:?}");
    }
}

#[test]
fn only_a_loopback_http_base_is_accepted() {
    assert_eq!(
        parse_test_base("http://127.0.0.1:8080/gh/"),
        Some("http://127.0.0.1:8080/gh".into())
    );
    assert_eq!(
        parse_test_base("http://localhost:9/gh"),
        Some("http://localhost:9/gh".into())
    );
    assert_eq!(
        parse_test_base("http://[::1]:9"),
        Some("http://[::1]:9".into())
    );
    for raw in [
        "https://github.com",
        "http://example.com/gh",
        "http://127.0.0.1.evil.com/",
        "127.0.0.1:9",
        "",
    ] {
        assert_eq!(parse_test_base(raw), None, "{raw:?}");
    }
}

#[test]
fn a_test_base_port_must_be_a_real_port() {
    assert_eq!(
        parse_test_base("http://127.0.0.1:65535/gh"),
        Some("http://127.0.0.1:65535/gh".into())
    );
    assert_eq!(
        parse_test_base("http://localhost/gh"),
        Some("http://localhost/gh".into())
    );
    for raw in [
        "http://127.0.0.1:/gh",
        "http://localhost:",
        "http://[::1]:",
        "http://127.0.0.1:65536",
        "http://localhost:99999/gh",
        "http://[::1]:0",
        "http://localhost:+80",
        "http://[::1]9",
    ] {
        assert_eq!(parse_test_base(raw), None, "{raw:?}");
    }
}

#[test]
fn trusted_keys_add_at_most_the_one_test_key() {
    assert!(trusted_keys(&[], None).is_empty());
    let seam = TestSeam {
        key: "T".into(),
        base: None,
    };
    assert_eq!(trusted_keys(&["P1", "P2"], Some(&seam)), ["P1", "P2", "T"]);
    assert_eq!(trusted_keys(&["P1"], None), ["P1"]);
}

#[test]
fn without_a_test_key_availability_follows_the_production_slots() {
    let source = production_source(&Endpoints::default(), false, None);
    assert_eq!(source.backend.is_some(), !UPDATE_PUBLIC_KEYS.is_empty());
    assert_eq!(source.current, env!("CARGO_PKG_VERSION"));
    assert_eq!(
        source.releases_page,
        "https://github.com/PhongGuy/TF2Tools/releases/latest"
    );
}

#[test]
fn an_undecodable_test_key_leaves_updates_unavailable_when_no_slot_is_set() {
    let seam = TestSeam {
        key: "A".repeat(56),
        base: None,
    };
    let source = production_source(&Endpoints::default(), false, Some(seam));
    // A bad key makes core refuse the whole list, so availability no longer depends on the slots.
    assert!(source.backend.is_none());
}

#[test]
fn a_test_seam_routes_checks_to_its_loopback_feed() {
    let feed = SignedFeed::start();
    feed.publish("99.0.0", b"new AppImage");
    let seam = TestSeam {
        key: feed.public_key(),
        base: Some(feed.github()),
    };
    let source = production_source(&Endpoints::default(), false, Some(seam));
    assert_eq!(
        source.releases_page,
        format!("{}/PhongGuy/TF2Tools/releases/latest", feed.github())
    );
    let backend = source
        .backend
        .expect("the test key makes updates available");
    let offer = feed
        .rt
        .block_on((backend.check)())
        .unwrap()
        .expect("an offer");
    assert_eq!(offer.version, "99.0.0");
    assert_eq!(offer.release_page, feed.release_page("99.0.0"));
}

#[cfg(not(feature = "update-test-key"))]
#[test]
fn without_the_feature_the_environment_is_never_read() {
    let key = fresh_key().pk.to_base64();
    let read = std::cell::Cell::new(false);
    let seam = test_seam_with(|_| {
        read.set(true);
        Some(key.clone())
    });
    assert_eq!(seam, None);
    assert!(!read.get());
}

#[cfg(not(feature = "update-test-key"))]
#[test]
fn the_default_binary_does_not_contain_the_test_key_variable() {
    let binary = std::fs::read(env!("CARGO_BIN_EXE_tf2tools")).unwrap();
    let needle = b"TF2TOOLS_UPDATE_TEST_PUBKEY";
    assert!(!binary.windows(needle.len()).any(|w| w == needle));
}

#[cfg(feature = "update-test-key")]
#[test]
fn with_the_feature_the_seam_reads_one_key_and_a_loopback_base() {
    let key = fresh_key().pk.to_base64();
    let env = |base: &'static str| {
        let key = key.clone();
        move |name: &str| match name {
            "TF2TOOLS_UPDATE_TEST_PUBKEY" => Some(key.clone()),
            "TF2TOOLS_UPDATE_TEST_BASE" => Some(base.to_owned()),
            _ => None,
        }
    };
    assert_eq!(
        test_seam_with(env("http://127.0.0.1:9/gh")),
        Some(TestSeam {
            key: key.clone(),
            base: Some("http://127.0.0.1:9/gh".into())
        })
    );
    assert_eq!(
        test_seam_with(env("https://github.com")),
        Some(TestSeam {
            key: key.clone(),
            base: None
        }),
        "a non-loopback base is dropped, never used"
    );
    assert_eq!(test_seam_with(|_| Some("two keys".into())), None);
    assert_eq!(test_seam_with(|_| None), None);
}
