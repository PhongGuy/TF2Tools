use gpui_kit::ElementId;
use tf2tools_app::ids;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{AddonName, GbId, ProfileId, StoreItemId, StoreKind};

fn name(id: ElementId) -> String {
    match id {
        ElementId::Name(n) => n.to_string(),
        other => panic!("expected a named id, got {other:?}"),
    }
}

#[test]
fn page_ids_use_kebab_slugs() {
    let slugs: Vec<_> = Page::ALL.iter().map(|p| p.slug()).collect();
    assert_eq!(
        slugs,
        [
            "huds",
            "sounds",
            "crosshairs",
            "profiles",
            "addons",
            "graphics",
            "hud-store",
            "sound-store",
            "crosshair-store",
            "addon-store",
            "crosshair-maker",
            "hud-maker",
            "maps",
            "settings",
            "logs"
        ]
    );
    assert_eq!(name(ids::nav(Page::HudStore)), "nav-hud-store");
    assert_eq!(name(ids::page(Page::Logs)), "page-logs");
}

#[test]
fn keyed_ids_carry_the_stable_key() {
    assert_eq!(name(ids::hud_card("budhud")), "hud-card:budhud");
    assert_eq!(name(ids::hud_uninstall("budhud")), "hud-uninstall:budhud");
    assert_eq!(
        name(ids::hud_update_mark("budhud")),
        "hud-update-mark:budhud"
    );
    assert_eq!(name(ids::hud_visit("budhud")), "hud-visit:budhud");
    assert_eq!(
        name(ids::hud_installed_name("budhud")),
        "hud-installed-name:budhud"
    );
    assert_eq!(
        name(ids::sound_card("ding-1a2b3c4d")),
        "sound-card:ding-1a2b3c4d"
    );
    assert_eq!(
        name(ids::crosshair_builtin("cross1")),
        "crosshair-option:builtin:cross1"
    );
    assert_eq!(
        name(ids::crosshair_library("dot-9f")),
        "crosshair-option:library:dot-9f"
    );
    assert_eq!(name(ids::class_tab("scout")), "class-tab:scout");
    assert_eq!(
        name(ids::weapon_row("tf_weapon_scattergun")),
        "weapon-row:tf_weapon_scattergun"
    );
}

#[test]
fn labels_and_titles_match_v5() {
    let table: Vec<_> = Page::ALL.iter().map(|p| (p.label(), p.title())).collect();
    assert_eq!(
        table,
        [
            ("HUDs", "HUDs"),
            ("Sounds", "Sounds"),
            ("Crosshairs", "Crosshairs"),
            ("Profiles", "Profiles"),
            ("Addons", "Addons"),
            ("Graphics", "Graphics"),
            ("HUD Store", "HUD Store"),
            ("Sound Store", "Sound Store"),
            ("Crosshair Store", "Crosshair Store"),
            ("Addon Store", "Addon Store"),
            ("Crosshair Maker", "Crosshair Maker"),
            ("HUD Maker", "HUD Maker"),
            ("Maps", "Maps"),
            ("Settings", "Settings"),
            ("Logs", "Activity Log"),
        ]
    );
}

#[test]
fn sound_store_ids_are_keyed_by_store_and_item() {
    let id = StoreItemId::GameBanana(GbId::new(42));
    assert_eq!(
        name(ids::store_play(StoreKind::Sounds, &id)),
        "store-play:sounds:42"
    );
    assert_eq!(
        name(ids::store_category("killsound")),
        "store-category:killsound"
    );
    assert_eq!(ids::SYNC_BANNER, "sync-banner");
    assert_eq!(ids::SYNC_BANNER_COUNT, "sync-banner-count");
    assert_eq!(ids::STORE_PREVIEW, "store-preview");
}

#[test]
fn addon_store_ids_are_stable() {
    assert_eq!(ids::STORE_REFRESH, "store-refresh");
    assert_eq!(ids::ADDON_STORE_TAG, "addon-store-tag");
    assert_eq!(ids::ADDON_STORE_INFO, "addon-store-info");
    assert_eq!(ids::ADDON_STORE_NO_TF2, "addon-store-no-tf2");
    let flat = StoreItemId::Addon(AddonName::new("mastercomfig-addon-flat-mouse.vpk").unwrap());
    assert_eq!(
        name(ids::store_state(StoreKind::Addons, &flat)),
        "store-state:addons:mastercomfig-addon-flat-mouse.vpk"
    );
}

#[test]
fn log_ids_are_keyed_by_row_id() {
    assert_eq!(name(ids::log_row(42)), "log-row:42");
    assert_eq!(name(ids::log_level(42)), "log-level:42");
    assert_eq!(name(ids::log_details_toggle(7)), "log-details-toggle:7");
    assert_eq!(name(ids::log_details(7)), "log-details:7");
    assert_eq!(ids::LOGS_CLEAR, "logs-clear");
    assert_eq!(ids::LOGS_MORE, "logs-more");
}

#[test]
fn addon_ids_are_keyed_by_asset() {
    let asset = "mastercomfig-addon-no-footsteps.vpk";
    let a = AddonName::new(asset).unwrap();
    for (id, kind) in [
        (ids::addon_card(&a), "addon-card"),
        (ids::addon_check(&a), "addon-check"),
        (ids::addon_tag(&a), "addon-tag"),
        (ids::addon_install(&a), "addon-install"),
        (ids::addon_uninstall(&a), "addon-uninstall"),
        (ids::addon_update(&a), "addon-update"),
        (ids::addon_delete(&a), "addon-delete"),
        (ids::addon_busy(&a), "addon-busy"),
    ] {
        assert_eq!(name(id), format!("{kind}:{asset}"));
    }
    assert_eq!(
        [
            ids::ADDONS_NO_TF2,
            ids::ADDONS_COUNT,
            ids::ADDONS_BROWSE,
            ids::ADDONS_EMPTY,
            ids::ADDONS_EMPTY_BROWSE,
            ids::ADDONS_INSTALLED,
            ids::ADDONS_INSTALLED_EMPTY,
            ids::ADDONS_LIBRARY,
        ],
        [
            "addons-no-tf2",
            "addons-count",
            "addons-browse",
            "addons-empty",
            "addons-empty-browse",
            "addons-installed",
            "addons-installed-empty",
            "addons-library",
        ]
    );
}

#[test]
fn profile_ids_are_keyed_by_profile_id() {
    let id = ProfileId::new("1700000000000-ab12");
    for (el, kind) in [
        (ids::profile_card(&id), "profile-card"),
        (ids::profile_active_badge(&id), "profile-active-badge"),
        (ids::profile_hud(&id), "profile-hud"),
        (ids::profile_hitsound(&id), "profile-hitsound"),
        (ids::profile_killsound(&id), "profile-killsound"),
        (ids::profile_crosshairs(&id), "profile-crosshairs"),
        (ids::profile_empty(&id), "profile-empty"),
        (ids::profile_activate(&id), "profile-activate"),
        (ids::profile_edit(&id), "profile-edit"),
        (ids::profile_delete(&id), "profile-delete"),
    ] {
        assert_eq!(name(el), format!("{kind}:1700000000000-ab12"));
    }
    assert_eq!(name(ids::missing_item(0)), "missing-item:0");
    assert_eq!(name(ids::missing_redownload(2)), "missing-redownload:2");
    assert_eq!(name(ids::missing_local(1)), "missing-local:1");
    assert_eq!(
        [
            ids::PROFILES_NO_TF2,
            ids::PROFILES_ACTIVE,
            ids::PROFILES_ACTIVE_NAME,
            ids::PROFILES_ACTIVE_BADGE,
            ids::PROFILES_ACTIVE_HUD,
            ids::PROFILES_ACTIVE_HITSOUND,
            ids::PROFILES_ACTIVE_KILLSOUND,
            ids::PROFILES_ACTIVE_CROSSHAIRS,
            ids::PROFILES_ACTIVE_EMPTY,
            ids::PROFILES_NO_ACTIVE,
            ids::PROFILES_SEARCH,
            ids::PROFILES_CREATE,
            ids::PROFILES_SAVE_CURRENT,
            ids::PROFILES_EMPTY,
            ids::PROFILES_EMPTY_CREATE,
            ids::PROFILES_EMPTY_SAVE,
            ids::PROFILES_NO_MATCH,
            ids::PROFILES_SAVE_DIALOG,
            ids::PROFILES_SAVE_NAME,
            ids::PROFILES_SAVE_CANCEL,
            ids::PROFILES_SAVE_OK,
            ids::PROFILES_MISSING_DIALOG,
            ids::PROFILES_MISSING_DESCRIPTION,
            ids::PROFILES_MISSING_CANCEL,
            ids::PROFILES_MISSING_INSTALL,
        ],
        [
            "profiles-no-tf2",
            "profiles-active",
            "profiles-active-name",
            "profiles-active-badge",
            "profiles-active-hud",
            "profiles-active-hitsound",
            "profiles-active-killsound",
            "profiles-active-crosshairs",
            "profiles-active-empty",
            "profiles-no-active",
            "profiles-search",
            "profiles-create",
            "profiles-save-current",
            "profiles-empty",
            "profiles-empty-create",
            "profiles-empty-save",
            "profiles-no-match",
            "profiles-save-dialog",
            "profiles-save-name",
            "profiles-save-cancel",
            "profiles-save-ok",
            "profiles-missing-dialog",
            "profiles-missing-description",
            "profiles-missing-cancel",
            "profiles-missing-install",
        ]
    );
}

#[test]
fn wizard_ids_are_stable() {
    assert_eq!(name(ids::wizard_step(1)), "wizard-step:1");
    assert_eq!(name(ids::wizard_step(4)), "wizard-step:4");
    assert_eq!(
        name(ids::wizard_option("hud", "none")),
        "wizard-hud-option:none"
    );
    assert_eq!(
        name(ids::wizard_option("killsound", "missing")),
        "wizard-killsound-option:missing"
    );
    assert_eq!(
        [
            ids::WIZARD_DIALOG,
            ids::WIZARD_TITLE,
            ids::WIZARD_DESCRIPTION,
            ids::WIZARD_NAME,
            ids::WIZARD_HUD,
            ids::WIZARD_HUD_EMPTY,
            ids::WIZARD_HITSOUND,
            ids::WIZARD_KILLSOUND,
            ids::WIZARD_HITSOUND_PLAY,
            ids::WIZARD_KILLSOUND_PLAY,
            ids::WIZARD_SOUNDS_EMPTY,
            ids::WIZARD_MODE_NONE,
            ids::WIZARD_MODE_COPY,
            ids::WIZARD_MODE_CUSTOM,
            ids::WIZARD_COPY_NOTE,
            ids::WIZARD_ASSIGNED_COUNT,
            ids::WIZARD_CANCEL,
            ids::WIZARD_BACK,
            ids::WIZARD_NEXT,
            ids::WIZARD_SAVE,
        ],
        [
            "wizard-dialog",
            "wizard-title",
            "wizard-description",
            "wizard-name",
            "wizard-hud",
            "wizard-hud-empty",
            "wizard-hitsound",
            "wizard-killsound",
            "wizard-hitsound-play",
            "wizard-killsound-play",
            "wizard-sounds-empty",
            "wizard-mode-none",
            "wizard-mode-copy",
            "wizard-mode-custom",
            "wizard-copy-note",
            "wizard-assigned-count",
            "wizard-cancel",
            "wizard-back",
            "wizard-next",
            "wizard-save",
        ]
    );
}

#[test]
fn hud_font_ids_are_stable() {
    assert_eq!(ids::HUD_FONTS_DIALOG, "hud-fonts-dialog");
    assert_eq!(ids::HUD_FONTS_DONT_ASK, "hud-fonts-dont-ask");
    assert_eq!(name(ids::hud_fonts_font(2)), "hud-fonts-font:2");
    assert_eq!(name(ids::hud_fonts_issue(0)), "hud-fonts-issue:0");
}

#[test]
fn hud_fonts_list_id_is_stable() {
    assert_eq!(ids::HUD_FONTS_LIST, "hud-fonts-list");
}

#[test]
fn the_curated_os_filter_ids_are_stable() {
    assert_eq!(ids::STORE_FILTER_OS_LABEL, "store-filter-os-label");
    assert_eq!(ids::STORE_FILTER_OS_GROUP, "store-filter-os-group");
    assert_eq!(
        name(ids::store_filter_os(
            tf2tools_app::pages::hud_store::curated::Os::Mac.key()
        )),
        "store-filter-os:mac"
    );
}

#[test]
fn the_curated_facet_filter_ids_are_stable() {
    use tf2tools_app::pages::hud_store::curated::Facet;
    for (facet, key) in [
        (Facet::Flag, "flag"),
        (Facet::Tag, "tag"),
        (Facet::Gamemode, "gamemode"),
        (Facet::Resolution, "resolution"),
    ] {
        assert_eq!(
            name(ids::store_filter(facet.key(), "menus")),
            format!("store-filter:{key}:menus")
        );
    }
}

#[test]
fn the_kit_wrapper_ids_are_stable() {
    assert_eq!(ids::STORE_DETAIL_BAR, "store-detail-bar");
    assert_eq!(ids::STORE_PROGRESS_BAR, "store-progress-bar");
    assert_eq!(ids::GAMEBANANA_CARD, "gamebanana-card");
}

#[test]
fn the_sort_label_id_is_stable() {
    assert_eq!(ids::STORE_SORT_LABEL, "store-sort-label");
}

#[test]
fn store_card_badge_ids_are_keyed_by_store_item_and_position() {
    assert_eq!(
        name(ids::store_card_tag(
            StoreKind::CuratedHuds,
            &StoreItemId::Curated("rayshud".into()),
            2
        )),
        "store-card-tag:curated-huds:rayshud:2"
    );
    assert_eq!(
        name(ids::store_card_category(
            StoreKind::Sounds,
            &StoreItemId::GameBanana(GbId::new(7))
        )),
        "store-card-category:sounds:7"
    );
}

#[test]
fn the_store_card_image_id_is_keyed_by_store_and_item() {
    assert_eq!(
        name(ids::store_card_image(
            StoreKind::CuratedHuds,
            &StoreItemId::Curated("rayshud".into())
        )),
        "store-card-image:curated-huds:rayshud"
    );
}

#[test]
fn the_card_name_and_repo_ids_are_keyed_by_store_and_item() {
    let rays = StoreItemId::Curated("rayshud".into());
    assert_eq!(
        name(ids::store_card_name(StoreKind::CuratedHuds, &rays)),
        "store-card-name:curated-huds:rayshud"
    );
    assert_eq!(
        name(ids::store_card_repo(StoreKind::CuratedHuds, &rays)),
        "store-card-repo:curated-huds:rayshud"
    );
}

#[test]
fn store_detail_badge_ids_are_keyed_by_row_and_position() {
    assert_eq!(
        name(ids::store_detail_tag("flags", 1)),
        "store-detail-tag:flags:1"
    );
    assert_eq!(
        name(ids::store_detail_tag("tags", 0)),
        "store-detail-tag:tags:0"
    );
    assert_eq!(ids::STORE_DETAIL_CATEGORY, "store-detail-category");
}

#[test]
fn store_card_tag_rows_are_keyed_by_store_item() {
    assert_eq!(
        name(ids::store_card_tags(
            StoreKind::GameBananaHuds,
            &StoreItemId::GameBanana(GbId::new(8))
        )),
        "store-card-tags:gamebanana-huds:8"
    );
}

#[test]
fn the_grid_and_list_ids_are_stable() {
    assert_eq!(ids::HUDS_GRID, "huds-grid");
    assert_eq!(ids::HUDS_LIBRARY, "huds-library");
    assert_eq!(ids::SOUNDS_GRID, "sounds-grid");
    assert_eq!(ids::PROFILES_GRID, "profiles-grid");
    assert_eq!(ids::PROFILES_MISSING_LIST, "profiles-missing-list");
    assert_eq!(ids::ADDON_STORE_INFO_TEXT, "addon-store-info-text");
    assert_eq!(ids::SOUND_STORE_CARD_TARGET, "sound-card");
}

// ids.rs is the one place element ids are spelt, so pilot QA and tests read them from there.
fn literal_ids_in(dir: &std::path::Path, hits: &mut Vec<String>) {
    const LITERALS: [&str; 6] = [
        "\"sound-card\"",
        "\"huds-grid\"",
        "\"sounds-grid\"",
        "\"profiles-grid\"",
        "\"profiles-missing-list\"",
        "\"addon-store-info-text\"",
    ];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            literal_ids_in(&path, hits);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") || path.ends_with("src/ids.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            for lit in LITERALS.iter().filter(|lit| line.contains(*lit)) {
                hits.push(format!("{}:{}: {lit}", path.display(), n + 1));
            }
        }
    }
}

#[test]
fn no_source_outside_ids_rs_spells_these_ids() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    for dir in ["src", "examples"] {
        literal_ids_in(&root.join(dir), &mut hits);
    }
    assert!(hits.is_empty(), "literal ids outside ids.rs: {hits:#?}");
}

#[test]
fn crosshair_maker_ids_are_kebab_case() {
    let table = [
        (ids::CROSSHAIR_MAKER_GRID, "crosshair-maker-grid"),
        (ids::CROSSHAIR_MAKER_PANEL, "crosshair-maker-panel"),
        (ids::CROSSHAIR_MAKER_PENCIL, "crosshair-maker-pencil"),
        (ids::CROSSHAIR_MAKER_ERASER, "crosshair-maker-eraser"),
        (ids::CROSSHAIR_MAKER_FILL, "crosshair-maker-fill"),
        (ids::CROSSHAIR_MAKER_LINE, "crosshair-maker-line"),
        (ids::CROSSHAIR_MAKER_RECTANGLE, "crosshair-maker-rectangle"),
        (ids::CROSSHAIR_MAKER_CIRCLE, "crosshair-maker-circle"),
        (ids::CROSSHAIR_MAKER_FILLED, "crosshair-maker-filled"),
        (ids::CROSSHAIR_MAKER_MIRROR_X, "crosshair-maker-mirror-x"),
        (ids::CROSSHAIR_MAKER_MIRROR_Y, "crosshair-maker-mirror-y"),
        (ids::CROSSHAIR_MAKER_UNDO, "crosshair-maker-undo"),
        (ids::CROSSHAIR_MAKER_REDO, "crosshair-maker-redo"),
        (ids::CROSSHAIR_MAKER_CLEAR, "crosshair-maker-clear"),
        (ids::CROSSHAIR_MAKER_NEW, "crosshair-maker-new"),
        (ids::CROSSHAIR_MAKER_SIZE, "crosshair-maker-size"),
        (ids::CROSSHAIR_MAKER_COLOR, "crosshair-maker-color"),
        (
            ids::CROSSHAIR_MAKER_COLOR_HINT,
            "crosshair-maker-color-hint",
        ),
        (
            ids::CROSSHAIR_MAKER_PREVIEW_COLOR,
            "crosshair-maker-preview-color",
        ),
        (ids::CROSSHAIR_MAKER_PREVIEW, "crosshair-maker-preview"),
        (
            ids::CROSSHAIR_MAKER_BACKGROUND,
            "crosshair-maker-background",
        ),
        (ids::CROSSHAIR_MAKER_NAME, "crosshair-maker-name"),
        (
            ids::CROSSHAIR_MAKER_NAME_ERROR,
            "crosshair-maker-name-error",
        ),
        (ids::CROSSHAIR_MAKER_SAVE, "crosshair-maker-save"),
        (
            ids::CROSSHAIR_MAKER_SAVE_AS_NEW,
            "crosshair-maker-save-as-new",
        ),
        (ids::CROSSHAIRS_EDIT, "crosshairs-edit"),
        (ids::CROSSHAIRS_COPY_TO_MAKER, "crosshairs-copy-to-maker"),
        (ids::CROSSHAIRS_EXPORT_PNG, "crosshairs-export-png"),
        (ids::CROSSHAIR_MAKER_VIEW, "crosshair-maker-view"),
        (ids::CROSSHAIR_MAKER_ZOOM, "crosshair-maker-zoom"),
        (ids::CROSSHAIR_MAKER_ZOOM_IN, "crosshair-maker-zoom-in"),
        (ids::CROSSHAIR_MAKER_ZOOM_OUT, "crosshair-maker-zoom-out"),
        (ids::CROSSHAIR_MAKER_OPACITY, "crosshair-maker-opacity"),
        (
            ids::CROSSHAIR_MAKER_OPACITY_LABEL,
            "crosshair-maker-opacity-label",
        ),
        (ids::CROSSHAIR_MAKER_STARTERS, "crosshair-maker-starters"),
        (
            ids::CROSSHAIR_MAKER_IMPORT_PNG,
            "crosshair-maker-import-png",
        ),
        (
            ids::CROSSHAIR_MAKER_STARTER_DIALOG,
            "crosshair-maker-starter-dialog",
        ),
        (
            ids::CROSSHAIR_MAKER_STARTER_PREVIEW,
            "crosshair-maker-starter-preview",
        ),
        (
            ids::CROSSHAIR_MAKER_STARTER_CANCEL,
            "crosshair-maker-starter-cancel",
        ),
        (
            ids::CROSSHAIR_MAKER_STARTER_USE,
            "crosshair-maker-starter-use",
        ),
    ];
    for (id, want) in table {
        assert_eq!(id, want);
    }
    assert_eq!(
        name(ids::crosshair_maker_swatch(2)),
        "crosshair-maker-swatch:2"
    );
    assert_eq!(
        name(ids::crosshair_maker_divider(1)),
        "crosshair-maker-divider:1"
    );
    assert_eq!(
        name(ids::crosshair_maker_starter("cross-gap")),
        "crosshair-maker-starter:cross-gap"
    );
    assert_eq!(
        name(ids::crosshair_maker_starter_slider("gap")),
        "crosshair-maker-starter-slider:gap"
    );
    assert_eq!(
        name(ids::crosshair_maker_starter_value("gap")),
        "crosshair-maker-starter-value:gap"
    );
}

#[test]
fn weapon_icon_ids_are_keyed_by_script() {
    assert_eq!(
        name(ids::weapon_icon("tf_weapon_bat")),
        "weapon-icon:tf_weapon_bat"
    );
    assert_eq!(
        name(ids::weapon_slot_icon("tf_weapon_bat")),
        "weapon-slot-icon:tf_weapon_bat"
    );
}

#[test]
fn hud_maker_ids_are_kebab_case_and_keyed_by_part() {
    use tf2tools_core::PartKey;
    assert_eq!(ids::HUD_MAKER_NO_GAME, "hud-maker-no-game");
    assert_eq!(
        ids::HUD_MAKER_PANEL_DOCK_RIGHT,
        "hud-maker-panel-dock-right"
    );
    assert_eq!(ids::HUD_MAKER_PANEL_COLLAPSE, "hud-maker-panel-collapse");
    assert_eq!(ids::HUD_MAKER_PANEL_FLOAT, "hud-maker-panel-float");
    assert_eq!(ids::HUD_MAKER_PARTS_PANEL, "hud-maker-parts-panel");
    assert_eq!(ids::HUD_MAKER_HOVER, "hud-maker-hover");
    assert_eq!(ids::HUD_MAKER_HOVER_NAME, "hud-maker-hover-name");
    assert_eq!(ids::HUD_MAKER_TOOLBAR, "hud-maker-toolbar");
    assert_eq!(
        ids::HUD_MAKER_TOOLBAR_COLLAPSE,
        "hud-maker-toolbar-collapse"
    );
    assert_eq!(ids::HUD_MAKER_TOOLBAR_EXPAND, "hud-maker-toolbar-expand");
    assert_eq!(
        ids::HUD_MAKER_PARTS_PANEL_TITLE,
        "hud-maker-parts-panel-title"
    );
    assert_eq!(
        ids::HUD_MAKER_PARTS_PANEL_DOCK_LEFT,
        "hud-maker-parts-panel-dock-left"
    );
    assert_eq!(
        ids::HUD_MAKER_PARTS_PANEL_FLOAT,
        "hud-maker-parts-panel-float"
    );
    assert_eq!(
        ids::HUD_MAKER_PARTS_PANEL_DOCK_RIGHT,
        "hud-maker-parts-panel-dock-right"
    );
    assert_eq!(
        ids::HUD_MAKER_PARTS_PANEL_COLLAPSE,
        "hud-maker-parts-panel-collapse"
    );
    assert_eq!(name(ids::nav(Page::HudMaker)), "nav-hud-maker");
    let key = PartKey::new("HudPlayerStatus/Label#2");
    assert_eq!(
        name(ids::hud_maker_part(&key)),
        "hud-maker-part:HudPlayerStatus/Label#2"
    );
    assert_eq!(
        name(ids::hud_maker_row(&key)),
        "hud-maker-row:HudPlayerStatus/Label#2"
    );
    assert_eq!(ids::HUD_MAKER_SCREEN_MENU, "hud-maker-screen-menu");
    assert_eq!(
        name(ids::hud_maker_screen(tf2tools_core::Screen::new(
            1920, 1080
        ))),
        "hud-maker-screen:1920x1080"
    );
    assert_eq!(name(ids::hud_maker_reset("xpos")), "hud-maker-reset:xpos");
    assert_eq!(
        name(ids::hud_maker_colour_name("TanLight")),
        "hud-maker-colour-name:TanLight"
    );
    assert_eq!(
        name(ids::hud_maker_handle(
            tf2tools_app::pages::hud_maker::editor::Handle::TopRight
        )),
        "hud-maker-handle:top-right"
    );
    assert_eq!(ids::HUD_MAKER_NAME, "hud-maker-name");
    assert_eq!(ids::HUD_MAKER_ROW_TIP, "hud-maker-row-tip");
    assert_eq!(ids::HUD_MAKER_SAVE, "hud-maker-save");
    assert_eq!(ids::HUD_MAKER_INSTALL, "hud-maker-install");
    assert_eq!(ids::HUD_MAKER_SIZE_HIDDEN, "hud-maker-size-hidden");
    assert_eq!(ids::HUD_MAKER_SCENES, "hud-maker-scenes");
    assert_eq!(ids::HUD_MAKER_SCENE_TABS, "hud-maker-scene-tabs");
    assert_eq!(ids::HUD_MAKER_SCENE_NOTE, "hud-maker-scene-note");
    assert_eq!(
        name(ids::hud_maker_scene(tf2tools_core::Scene::Scoreboard)),
        "hud-maker-scene:scoreboard"
    );
    assert_eq!(
        name(ids::hud_maker_scene(tf2tools_core::Scene::Hud)),
        "hud-maker-scene:hud"
    );
    let list = tf2tools_core::PartKey::new("scores/BluePlayerList");
    assert_eq!(
        name(ids::hud_maker_list_row(&list, 2)),
        "hud-maker-list-row:scores/BluePlayerList/2"
    );
    assert_eq!(
        name(ids::hud_maker_list_ping(&list, 2)),
        "hud-maker-list-ping:scores/BluePlayerList/2"
    );
    assert_eq!(
        name(ids::hud_maker_list_header(&list)),
        "hud-maker-list-header:scores/BluePlayerList"
    );
    assert_eq!(
        name(ids::hud_maker_number("linegap")),
        "hud-maker-number:linegap"
    );
    assert_eq!(name(ids::hud_edit("my-hud-1a2b")), "hud-edit:my-hud-1a2b");
}

#[test]
fn situation_switch_ids_name_each_switch() {
    use tf2tools_core::SituationSwitch::*;
    let slugs = [
        (Meter, "meter"),
        (Aiming, "aiming"),
        (Effect, "effect"),
        (Powerup, "powerup"),
        (Typing, "typing"),
        (Portrait, "portrait"),
        (JustHit, "just-hit"),
        (Koth, "koth"),
    ];
    for (switch, slug) in slugs {
        assert_eq!(
            name(ids::hud_maker_situation_switch(switch)),
            format!("hud-maker-situation-switch:{slug}")
        );
    }
    // The minimal HUD is a model flag, not a situation switch, but sits among them.
    assert_eq!(
        ids::HUD_MAKER_MINMODE,
        "hud-maker-situation-switch:minimal-hud"
    );
    assert_eq!(ids::HUD_MAKER_MINMODE_CHIP, "hud-maker-minmode-chip");
}

#[test]
fn profile_editor_ids_are_stable() {
    for (el, want) in [
        (ids::profile_slot("hud"), "profile-slot:hud"),
        (
            ids::profile_slot_clear("crosshairs"),
            "profile-slot-clear:crosshairs",
        ),
        (
            ids::profile_slot_play("hitsound"),
            "profile-slot-play:hitsound",
        ),
        (
            ids::profile_slot_missing("killsound"),
            "profile-slot-missing:killsound",
        ),
        (ids::profile_picker("hud"), "profile-picker:hud"),
        (
            ids::profile_picker_search("hud"),
            "profile-picker-search:hud",
        ),
        (
            ids::profile_picker_empty("hitsound"),
            "profile-picker-empty:hitsound",
        ),
        (ids::profile_picker_store("hud"), "profile-picker-store:hud"),
        (ids::profile_pick_hud("rayshud"), "profile-pick-hud:rayshud"),
        (
            ids::profile_pick_sound("killsound", "bell"),
            "profile-pick-sound:killsound:bell",
        ),
        (ids::profile_pick_play("bell"), "profile-pick-play:bell"),
        (
            ids::weapon_clear("tf_weapon_bat"),
            "weapon-clear:tf_weapon_bat",
        ),
    ] {
        assert_eq!(name(el), want);
    }
    assert_eq!(
        [
            ids::PROFILES_NEW,
            ids::PROFILES_NEW_MENU,
            ids::PROFILES_NEW_EMPTY,
            ids::PROFILES_EMPTY_NEW,
            ids::PROFILES_EMPTY_START_EMPTY,
            ids::PROFILE_EDITOR,
            ids::PROFILE_EDITOR_BACK,
            ids::PROFILE_EDITOR_NAME,
            ids::PROFILE_EDITOR_NAME_TAKEN,
            ids::PROFILE_EDITOR_ACTIVATE,
            ids::PROFILE_EDITOR_ACTIVE_BADGE,
            ids::PROFILE_EDITOR_DELETE,
            ids::PROFILE_EDITOR_NOT_APPLIED,
            ids::PROFILE_PICKER_GRID,
            ids::PROFILE_PICKER_SOUNDS,
            ids::PROFILE_PICKER_SOUNDS_SCROLLBAR,
        ],
        [
            "profiles-new",
            "profiles-new-menu",
            "profiles-new-empty",
            "profiles-empty-new",
            "profiles-empty-start-empty",
            "profile-editor",
            "profile-editor-back",
            "profile-editor-name",
            "profile-editor-name-taken",
            "profile-editor-activate",
            "profile-editor-active-badge",
            "profile-editor-delete",
            "profile-editor-not-applied",
            "profile-picker-grid",
            "profile-picker-sounds",
            "profile-picker-sounds-scrollbar",
        ]
    );
}
