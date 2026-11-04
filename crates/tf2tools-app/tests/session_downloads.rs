use tf2tools_app::store_download::{DownloadEvent, DownloadTarget, SessionDownloads};
use tf2tools_core::{AddonName, GbId, StoreItemId, StoreKind};

fn target(kind: StoreKind, item: StoreItemId) -> DownloadTarget {
    DownloadTarget {
        kind,
        item,
        name: "x".into(),
    }
}

fn finished(kind: StoreKind, id: u64) -> DownloadEvent {
    DownloadEvent::Finished {
        target: target(kind, StoreItemId::GameBanana(GbId::new(id))),
    }
}

#[test]
fn a_finished_download_of_its_kind_is_recorded() {
    let s = SessionDownloads::new(StoreKind::Sounds);
    assert!(!s.contains(GbId::new(7)));
    assert!(s.record(&finished(StoreKind::Sounds, 7)));
    assert!(s.contains(GbId::new(7)));
}

#[test]
fn other_kinds_failures_and_non_gamebanana_items_are_ignored() {
    let s = SessionDownloads::new(StoreKind::Sounds);
    assert!(!s.record(&finished(StoreKind::Crosshairs, 7)));
    let failed = DownloadEvent::Failed {
        target: target(StoreKind::Sounds, StoreItemId::GameBanana(GbId::new(8))),
        message: "boom".into(),
    };
    assert!(!s.record(&failed));
    let addon = DownloadEvent::Finished {
        target: target(
            StoreKind::Sounds,
            StoreItemId::Addon(AddonName::new("mastercomfig-addon-flat-mouse.vpk").unwrap()),
        ),
    };
    assert!(!s.record(&addon));
    assert!(!s.contains(GbId::new(7)) && !s.contains(GbId::new(8)));
}

#[test]
fn clones_share_one_set() {
    let s = SessionDownloads::new(StoreKind::GameBananaHuds);
    let renderer_copy = s.clone();
    s.record(&finished(StoreKind::GameBananaHuds, 3));
    assert!(renderer_copy.contains(GbId::new(3)));
}
