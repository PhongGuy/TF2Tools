use tf2tools_app::widgets::sync_banner::banner_text;
use tf2tools_core::StoreKind;

#[test]
fn each_gamebanana_store_names_itself_as_v5_does() {
    assert_eq!(
        banner_text(StoreKind::Sounds, false),
        "Syncing sounds from GameBanana..."
    );
    assert_eq!(
        banner_text(StoreKind::Crosshairs, false),
        "Syncing crosshairs from GameBanana..."
    );
    assert_eq!(
        banner_text(StoreKind::GameBananaHuds, true),
        "Syncing HUDs from GameBanana... Not all items loaded yet."
    );
}
