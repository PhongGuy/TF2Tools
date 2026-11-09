use crate::support;

use tf2tools_app::pages::graphics::model::{
    StatusCard, describe_install, detail, headline, preset_description, preset_label, status_card,
    update_to,
};
use tf2tools_core::{InstallMastercomfigError, MastercomfigStatus, Preset};

fn managed(version: &str) -> MastercomfigStatus {
    MastercomfigStatus::Managed {
        version: version.to_owned(),
        preset: Preset::Medium,
    }
}

#[test]
fn the_card_compares_the_install_with_the_recorded_release() {
    let outside = |v: Option<&str>| MastercomfigStatus::Outside {
        version: v.map(str::to_owned),
    };
    let cases = [
        (
            MastercomfigStatus::NotInstalled,
            Some("9.100.1"),
            "mastercomfig isn't installed",
            Some("Install it to pick a graphics preset."),
            None,
        ),
        (managed("9.100.1"), None, "mastercomfig 9.100.1", None, None),
        (
            managed("9.100.1"),
            Some("9.100.1"),
            "mastercomfig 9.100.1",
            Some("Up to date"),
            None,
        ),
        (
            managed("9.100.1"),
            Some("9.101.0"),
            "mastercomfig 9.100.1",
            Some("Update to 9.101.0"),
            Some("9.101.0"),
        ),
        (
            outside(Some("9.99.0")),
            Some("9.100.1"),
            "mastercomfig 9.99.0",
            Some("Managed outside TF2Tools"),
            None,
        ),
        (
            outside(None),
            None,
            "mastercomfig",
            Some("Managed outside TF2Tools"),
            None,
        ),
    ];
    for (status, latest, head, line, update) in cases {
        let card = status_card(&status, latest);
        assert_eq!(headline(&card), head, "{status:?}");
        assert_eq!(detail(&card).as_deref(), line, "{status:?}");
        assert_eq!(update_to(&card), update, "{status:?}");
    }
    assert_eq!(
        status_card(&managed("9.100.1"), Some("9.101.0")),
        StatusCard::Installed {
            version: "9.100.1".to_owned(),
            latest: Some("9.101.0".to_owned())
        }
    );
}

#[test]
fn preset_cards_carry_comfig_app_s_descriptions() {
    let shown: Vec<(&str, &str)> = Preset::PICKER
        .iter()
        .map(|p| (preset_label(*p), preset_description(*p)))
        .collect();
    assert_eq!(
        shown,
        [
            ("Ultra", "Absolute maximum graphical quality"),
            ("High", "High graphical quality"),
            ("Medium", "Medium graphical quality"),
            ("Low", "Low graphical quality"),
            ("Custom", "Skip setting quality options unless set by you"),
        ]
    );
}

#[test]
fn install_failures_read_as_player_words() {
    let busy = describe_install(&InstallMastercomfigError::RateLimited {
        url: "https://github.com/x".to_owned(),
    });
    assert_eq!(
        busy.message,
        "GitHub is busy right now. Wait a few minutes, then try again."
    );
    let held = describe_install(&InstallMastercomfigError::Io {
        path: "/games/tf/custom".into(),
        source: std::io::Error::other("in use"),
    });
    assert!(
        held.message.contains("Make sure TF2 is closed"),
        "{}",
        held.message
    );
    for failure in [
        busy,
        held,
        describe_install(&InstallMastercomfigError::NoRelease),
    ] {
        support::assert_plain(&failure.message);
    }
}
