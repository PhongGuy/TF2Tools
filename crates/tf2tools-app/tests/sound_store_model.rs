use tf2tools_app::pages::sound_store::model::{
    Category, banner_count, banner_percent, banner_text, category_label,
};
use tf2tools_app::sync::SyncTick;

#[test]
fn categories_follow_v5_s_pills() {
    let labels: Vec<_> = Category::ALL.iter().map(|c| c.label()).collect();
    assert_eq!(labels, ["All", "Hitsounds", "Killsounds"]);
    let slugs: Vec<_> = Category::ALL.iter().map(|c| c.slug()).collect();
    assert_eq!(slugs, ["all", "hitsound", "killsound"]);
    let filters: Vec<_> = Category::ALL.iter().map(|c| c.filter()).collect();
    assert_eq!(filters, [None, Some("hitsound"), Some("killsound")]);
}

#[test]
fn a_category_badge_reads_as_v5_s() {
    assert_eq!(category_label(Some("hitsound")), "Hitsound");
    assert_eq!(category_label(Some("killsound")), "Killsound");
    assert_eq!(category_label(Some("sound")), "Sound");
    assert_eq!(category_label(None), "Sound");
}

#[test]
fn the_banner_reads_as_v5_s() {
    assert_eq!(banner_text(false), "Syncing sounds from GameBanana...");
    assert_eq!(
        banner_text(true),
        "Syncing sounds from GameBanana... Not all items loaded yet."
    );
    let tick = SyncTick {
        fetched: 1215,
        total: 19718,
    };
    assert_eq!(banner_count(tick), "1215 / 19718");
    assert!((banner_percent(tick) - 6.162).abs() < 0.01);
    assert_eq!(
        banner_percent(SyncTick {
            fetched: 15,
            total: 0
        }),
        0.0
    );
    assert_eq!(
        banner_percent(SyncTick {
            fetched: 20,
            total: 10
        }),
        100.0
    );
}
