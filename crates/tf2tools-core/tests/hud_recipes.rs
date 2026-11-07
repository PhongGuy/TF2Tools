//! Recipe files: reading untrusted input, writing, exporting and applying.

use crate::common;

use tf2tools_core::{
    AssetFile, Change, HudAssets, OptionChange, OptionValue, Recipe, RecipeError, SetKey,
};

const HEALTH_FILE: &str = "resource/ui/hudplayerhealth.res";
const HV_BLOCK: [&str; 2] = ["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"];

fn set(group: u32, key: &str, value: &str) -> Change {
    Change::set(
        group,
        SetKey::new(HEALTH_FILE, &HV_BLOCK, key, None, None, value),
    )
}

fn option(group: u32, id: &str, value: OptionValue) -> Change {
    Change::option(group, OptionChange::new(OptionChange::BUILTIN, id, value))
}

fn recipe_text(changes: &str, assets: &str) -> String {
    format!(
        r#"{{ "version": 1, "kind": "recipe", "name": "Test", "made_for": null,
            "changes": [ {changes} ], "assets": {{ {assets} }} }}"#
    )
}

const GOOD_SET: &str = r#"{ "group": 3, "set": { "file": "resource/ui/hudplayerhealth.res",
    "block": ["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
    "key": "fgcolor", "cond": null, "old": null, "new": "1 2 3 255" } }"#;

#[test]
fn a_recipe_reads_its_changes_and_assets() {
    let text = recipe_text(
        &format!(
            r#"{GOOD_SET}, {{ "group": 4, "asset": {{ "path": "resource/fonts/tf2tools/a.ttf",
                "source": ".tf2tools/fonts/a.ttf" }} }}"#
        ),
        r#"".tf2tools/fonts/a.ttf": "AAEC""#,
    );
    let (recipe, skipped) = Recipe::parse(text.as_bytes()).unwrap();
    assert_eq!(
        (recipe.name(), recipe.made_for(), skipped),
        ("Test", None, 0)
    );
    assert_eq!(recipe.changes().len(), 2);
    assert_eq!(recipe.changes()[0], set(3, "fgcolor", "1 2 3 255"));
    assert_eq!(
        recipe.asset(".tf2tools/fonts/a.ttf"),
        Some([0u8, 1, 2].as_slice())
    );
    assert!(!recipe.is_preset());
}

#[test]
fn a_written_recipe_reads_back_the_same() {
    let text = recipe_text(
        &format!(
            r#"{GOOD_SET}, {{ "group": 5, "option": {{ "source": "builtin", "id": "chat_top",
                "value": {{ "bool": true }} }} }}, {{ "group": 4, "asset": {{
                "path": "resource/fonts/tf2tools/a.ttf", "source": ".tf2tools/fonts/a.ttf" }} }}"#
        ),
        r#"".tf2tools/fonts/a.ttf": "AAEC""#,
    );
    let (recipe, _) = Recipe::parse(text.as_bytes()).unwrap();
    let (back, skipped) = Recipe::parse(&recipe.to_json()).unwrap();
    assert_eq!((back, skipped), (recipe, 0));
}

#[test]
fn changes_that_do_not_fit_are_skipped_and_counted() {
    let changes = format!(
        r#"{GOOD_SET},
        {{ "group": 1, "x-not-a-kind": {{}} }},
        {{ "group": 1, "set": {{}} }},
        {{ "group": 1, "set": {{ "file": "cfg/autoexec.res", "block": ["a"], "key": "k", "new": "v" }} }},
        {{ "group": 1, "asset": {{ "path": "resource/fonts/tf2tools/b.ttf", "source": ".tf2tools/fonts/b.ttf" }} }},
        "not an object""#
    );
    let (recipe, skipped) = Recipe::parse(recipe_text(&changes, "").as_bytes()).unwrap();
    assert_eq!((recipe.changes().len(), skipped), (1, 5));
}

#[test]
fn unsafe_or_unreadable_assets_are_dropped() {
    let changes = r#"{ "group": 1, "asset": { "path": "resource/fonts/tf2tools/a.ttf", "source": ".tf2tools/fonts/a.ttf" } }"#;
    for assets in [
        r#"".tf2tools/fonts/a.ttf": "not base64!""#,
        r#"".tf2tools/fonts/a.ttf": 7"#,
        r#"".tf2tools/../../evil.ttf": "AAEC""#,
        r#"".tf2tools/images/a.ttf": "AAEC""#,
    ] {
        let (recipe, skipped) = Recipe::parse(recipe_text(changes, assets).as_bytes()).unwrap();
        assert_eq!((recipe.changes().len(), skipped), (0, 1), "{assets}");
    }
}

#[test]
fn fonts_no_change_uses_are_not_kept() {
    // b.ttf travels with the file but no kept change names it; a.ttf's change is skipped.
    let changes = r#"{ "group": 1, "asset": { "path": "resource/fonts/tf2tools/a.ttf", "source": ".tf2tools/fonts/../a.ttf" } }"#;
    let assets = r#"".tf2tools/fonts/a.ttf": "AAEC", ".tf2tools/fonts/b.ttf": "AAEC""#;
    let (recipe, skipped) = Recipe::parse(recipe_text(changes, assets).as_bytes()).unwrap();
    assert_eq!(skipped, 1);
    assert_eq!(recipe.asset(".tf2tools/fonts/a.ttf"), None);
    assert_eq!(recipe.asset(".tf2tools/fonts/b.ttf"), None);
}

#[test]
fn a_font_of_exactly_16_mib_is_kept_and_one_byte_more_is_dropped() {
    use base64::Engine;
    let changes = r#"{ "group": 1, "asset": { "path": "resource/fonts/tf2tools/a.ttf", "source": ".tf2tools/fonts/a.ttf" } }"#;
    // Both sizes encode to the same length, so the early length check must let both through.
    for (len, kept) in [(16 << 20, 1), ((16 << 20) + 1, 0), ((16 << 20) + 3, 0)] {
        let data = base64::engine::general_purpose::STANDARD.encode(vec![0u8; len]);
        let assets = format!(r#"".tf2tools/fonts/a.ttf": "{data}""#);
        let (recipe, _) = Recipe::parse(recipe_text(changes, &assets).as_bytes()).unwrap();
        assert_eq!(recipe.changes().len(), kept, "{len} bytes");
    }
}

#[test]
fn hostile_recipe_files_are_refused_not_panicked() {
    let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
    let huge = format!("{}{}", recipe_text("", ""), " ".repeat(Recipe::MAX_BYTES));
    let damaged = [
        "{".to_owned(),
        "[]".to_owned(),
        deep,
        huge,
        recipe_text("", "").replace("\"recipe\"", "\"preset\""),
        recipe_text("", "").replace("\"Test\"", "12"),
        r#"{ "version": 1, "kind": "recipe", "name": "x", "changes": {} }"#.to_owned(),
        r#"{ "version": 1, "kind": "recipe", "name": "x", "changes": [], "assets": [] }"#
            .to_owned(),
        r#"{ "kind": "recipe", "name": "x", "changes": [] }"#.to_owned(),
        r#"{ "version": 1e99, "kind": "recipe", "name": "x", "changes": [] }"#.to_owned(),
    ];
    for text in &damaged {
        assert!(
            matches!(
                Recipe::parse(text.as_bytes()),
                Err(RecipeError::Damaged { .. })
            ),
            "{}",
            &text[..text.len().min(80)]
        );
    }
    let newer = recipe_text("", "").replace("\"version\": 1", "\"version\": 2");
    assert!(matches!(
        Recipe::parse(newer.as_bytes()),
        Err(RecipeError::Newer)
    ));
}

#[test]
fn a_recipe_keeps_at_most_ten_thousand_changes() {
    let one = r#"{ "group": 1, "option": { "source": "builtin", "id": "chat_top", "value": { "bool": true } } }"#;
    let many = vec![one; 10_001].join(",");
    let (recipe, skipped) = Recipe::parse(recipe_text(&many, "").as_bytes()).unwrap();
    assert_eq!((recipe.changes().len(), skipped), (10_000, 1));
}

#[test]
fn names_are_cleaned() {
    let named = |name: &str| {
        let text = recipe_text("", "").replace("\"Test\"", &serde_json::to_string(name).unwrap());
        Recipe::parse(text.as_bytes()).unwrap().0.name().to_owned()
    };
    assert_eq!(named("  Big\u{7}numbers\n "), "Bignumbers");
    assert_eq!(named(&"x".repeat(200)).chars().count(), 80);
    assert_eq!(named(" \t "), "Recipe");
    // Bidi controls and zero-width marks could make a shared name display as something else.
    assert_eq!(
        named("\u{202E}ab\u{202A}c\u{2066}d\u{2069}\u{200B}e\u{200F}\u{FEFF}f"),
        "abcdef"
    );
    assert_eq!(named("\u{200B}\u{2067}"), "Recipe");
}

#[test]
fn made_for_is_kept_when_well_formed() {
    let text = recipe_text("", "").replace(
        "\"made_for\": null",
        r#""made_for": { "repo": "a/b", "hash": "abc" }"#,
    );
    let (recipe, _) = Recipe::parse(text.as_bytes()).unwrap();
    assert_eq!(recipe.made_for(), Some(("a/b", "abc")));
    let odd = recipe_text("", "").replace("\"made_for\": null", r#""made_for": 3"#);
    assert_eq!(Recipe::parse(odd.as_bytes()).unwrap().0.made_for(), None);
}

#[test]
fn a_preset_holds_only_option_changes() {
    let text = recipe_text(
        r#"{ "group": 5, "option": { "source": "builtin", "id": "chat_top", "value": { "bool": true } } }"#,
        "",
    );
    let (recipe, _) = Recipe::parse(text.as_bytes()).unwrap();
    assert!(recipe.is_preset());
    assert_eq!(
        recipe.changes(),
        [option(5, "chat_top", OptionValue::Bool(true))]
    );
}

#[test]
fn a_recipe_carries_the_bytes_of_its_fonts() {
    let asset = Change::asset(
        2,
        AssetFile::new("resource/fonts/tf2tools/a.ttf", ".tf2tools/fonts/a.ttf"),
    );
    let lost = Change::asset(
        3,
        AssetFile::new("resource/fonts/tf2tools/b.ttf", ".tf2tools/fonts/b.ttf"),
    );
    let mut assets = HudAssets::new();
    assets.insert(".tf2tools/fonts/a.ttf", vec![9, 8, 7]);
    let changes = vec![set(1, "fgcolor", "1 2 3 255"), asset.clone(), lost];
    let recipe = Recipe::new("Mine", &changes, &assets);
    assert_eq!(recipe.changes(), [changes[0].clone(), asset]);
    let (back, skipped) = Recipe::parse(&recipe.to_json()).unwrap();
    assert_eq!(skipped, 0);
    assert_eq!(
        back.asset(".tf2tools/fonts/a.ttf"),
        Some([9u8, 8, 7].as_slice())
    );
    let image = Change::asset(
        4,
        AssetFile::new("materials/x.vtf", ".tf2tools/images/x.vtf"),
    );
    let mut with_image = assets.clone();
    with_image.insert(".tf2tools/images/x.vtf", vec![1]);
    assert!(
        Recipe::new("Img", &[image], &with_image)
            .changes()
            .is_empty()
    );
}

#[test]
fn a_preset_keeps_only_the_options() {
    let changes = vec![
        set(1, "fgcolor", "1 2 3 255"),
        option(2, "killfeed_rows", OptionValue::Int(6)),
    ];
    let preset = Recipe::preset("Rows", &changes);
    assert_eq!(preset.changes(), [changes[1].clone()]);
    assert!(preset.is_preset());
    assert!(!Recipe::preset("Empty", &changes[..1]).is_preset());
}

use common::hud_stock::stock_files;
use tf2tools_core::{HudModel, PartKey, Platform};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn stock_model() -> HudModel {
    HudModel::load(&stock_files(), Platform::Linux).unwrap()
}

fn recipe_of(changes: &[Change]) -> Recipe {
    Recipe::new("R", changes, &HudAssets::new())
}

#[test]
fn a_recipe_lands_as_one_new_group() {
    let m = stock_model();
    let before = vec![set(0, "labelText", "HP")];
    let recipe = recipe_of(&[set(7, "fgcolor", "1 2 3 255"), set(8, "xpos", "80")]);
    let applied = m.apply_recipe(&recipe, &before, 1);
    assert_eq!(applied.skipped(), 0);
    assert_eq!(
        applied.changes(),
        [
            before[0].clone(),
            set(1, "fgcolor", "1 2 3 255"),
            set(1, "xpos", "80")
        ]
    );
    let edited = m.with_changes(applied.changes());
    let part = edited.model().part(&PartKey::new(HV)).unwrap();
    assert_eq!(part.value("fgcolor").map(|l| l.text()), Some("1 2 3 255"));
}

#[test]
fn unresolved_recipe_changes_are_skipped_and_left_out() {
    let files = common::hud_stock::stock_files_with(&[("resource/ui/basechat.res", None)]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    let missing = Change::set(
        4,
        SetKey::new(
            HEALTH_FILE,
            &["Resource/UI/HudPlayerHealth.res", "NoSuchPanel"],
            "xpos",
            None,
            None,
            "1",
        ),
    );
    let recipe = recipe_of(&[
        set(1, "fgcolor", "1 2 3 255"),
        missing,
        option(2, "chat_top", OptionValue::Bool(true)),
    ]);
    // Without basechat.res the option cannot apply either.
    let applied = m.apply_recipe(&recipe, &[], 0);
    assert_eq!(applied.skipped(), 2);
    assert_eq!(applied.changes(), [set(0, "fgcolor", "1 2 3 255")]);
    assert_eq!(m.with_changes(applied.changes()).dropped(), 0);
}

#[test]
fn a_recipe_option_replaces_the_earlier_choice() {
    let m = stock_model();
    let before = vec![option(0, "item_normal", OptionValue::Rgba([1, 1, 1, 255]))];
    let recipe = recipe_of(&[option(9, "item_normal", OptionValue::Rgba([2, 2, 2, 255]))]);
    let applied = m.apply_recipe(&recipe, &before, 1);
    assert_eq!(
        applied.changes(),
        [option(1, "item_normal", OptionValue::Rgba([2, 2, 2, 255]))]
    );
}

#[test]
fn a_skipped_recipe_option_keeps_the_players_choice() {
    let layout = common::hud_stock::with_blocks(
        common::hud_stock::HUDLAYOUT,
        "  HudDeathNotice { xpos r640 ypos 18 wide 628 tall 468 MaxDeathNotices 4 }\n",
    );
    let files =
        common::hud_stock::stock_files_with(&[("scripts/hudlayout.res", Some(layout.as_str()))]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    let before = vec![option(0, "killfeed_rows", OptionValue::Int(6))];
    let recipe = recipe_of(&[option(9, "killfeed_rows", OptionValue::Int(25))]);
    let applied = m.apply_recipe(&recipe, &before, 1);
    assert_eq!(
        (applied.changes(), applied.skipped()),
        (before.as_slice(), 1)
    );
}

#[test]
fn an_empty_recipe_changes_nothing() {
    let m = stock_model();
    let before = vec![set(0, "labelText", "HP")];
    let applied = m.apply_recipe(&recipe_of(&[]), &before, 1);
    assert_eq!(
        (applied.changes(), applied.skipped()),
        (before.as_slice(), 0)
    );
}

use common::hud_stock::{HEALTH, stock_files_with};
use tf2tools_core::FontEdit;

const FONTS: &str = r#""Scheme" { "Colors" { "TanDark" "117 107 94 255" }
  "Fonts" { "HudFontMedium" { "1" { "name" "TF2 Build" "tall" "16" } } } }"#;

fn font_model() -> HudModel {
    let health = HEALTH.replace("font HudClassHealth", "font HudFontMedium");
    let ammo = r#""Resource/UI/HudAmmoWeapons.res" { AmmoInClip { ControlName CExLabel xpos 10 ypos 5 wide 55 tall 40 font HudFontMedium labelText %Ammo% } }"#;
    let files = stock_files_with(&[
        ("resource/clientscheme.res", Some(FONTS)),
        ("resource/ui/hudplayerhealth.res", Some(health.as_str())),
        ("resource/ui/hudammoweapons.res", Some(ammo)),
    ]);
    HudModel::load(&files, Platform::Linux).unwrap()
}

fn ttf(family: &str) -> Vec<u8> {
    common::sfnt::sfnt_bytes(&[(1, family), (4, &format!("{family} Regular"))], false)
}

/// A recipe that adds a font file and gives the health value a bigger clone of its font.
fn font_recipe(m: &HudModel) -> Recipe {
    let (file, mut changes) = m.add_font_file("cool.ttf", &ttf("Cool Sans"), 0).unwrap();
    let edited = m.with_changes(&changes).into_model();
    changes.extend(
        edited
            .edit_font(&PartKey::new(HV), "font", FontEdit::Size(24), 1080, 1)
            .unwrap(),
    );
    let mut assets = HudAssets::new();
    assets.insert(file.source(), ttf("Cool Sans"));
    Recipe::new("Fonts", &changes, &assets)
}

#[test]
fn a_font_recipe_adds_its_file() {
    let m = font_model();
    let recipe = font_recipe(&m);
    let applied = m.apply_recipe(&recipe, &[], 0);
    assert_eq!(applied.skipped(), 0);
    assert_eq!(applied.added().len(), 1);
    assert_eq!(applied.added()[0].1, ttf("Cool Sans"));
    let edited = m.with_changes(applied.changes());
    assert_eq!(edited.dropped(), 0);
    assert_eq!(edited.model().scheme().font_files().len(), 1);
}

#[test]
fn applying_a_font_recipe_twice_keeps_both_fonts_apart() {
    let m = font_model();
    let recipe = font_recipe(&m);
    let once = m.apply_recipe(&recipe, &[], 0);
    let twice = m.apply_recipe(&recipe, once.changes(), 1);
    assert_eq!(twice.skipped(), 0);
    let (a, b) = (&once.added()[0].0, &twice.added()[0].0);
    assert_ne!(a, b, "the second file gets its own name");
    let edited = m.with_changes(twice.changes());
    assert_eq!(edited.dropped(), 0);
    let model = edited.model();
    assert_eq!(model.scheme().font_files().len(), 2);
    let font = model
        .part(&PartKey::new(HV))
        .unwrap()
        .value("font")
        .unwrap()
        .text()
        .to_owned();
    assert_eq!(font, "TF2Tools_2");
    assert!(model.scheme().font("TF2Tools_1", 1080).is_some());
    assert!(model.scheme().font("TF2Tools_2", 1080).is_some());
}

#[test]
fn a_two_clone_recipe_onto_a_hud_with_a_clone_keeps_three_apart() {
    let m = font_model();
    let (hv, ammo) = (PartKey::new(HV), PartKey::new("HudWeaponAmmo/AmmoInClip"));
    // The HUD being edited already has TF2Tools_1.
    let base = m
        .edit_font(&hv, "font", FontEdit::Size(20), 1080, 0)
        .unwrap();
    // The recipe, made on a fresh HUD, holds TF2Tools_1 (health) and TF2Tools_2 (ammo).
    let mut recipe_changes = m
        .edit_font(&hv, "font", FontEdit::Size(24), 1080, 1)
        .unwrap();
    let step = m.with_changes(&recipe_changes).into_model();
    recipe_changes.extend(
        step.edit_font(&ammo, "font", FontEdit::Size(30), 1080, 2)
            .unwrap(),
    );
    let recipe = Recipe::new("Two", &recipe_changes, &HudAssets::new());
    let applied = m.apply_recipe(&recipe, &base, 3);
    assert_eq!(applied.skipped(), 0);
    let edited = m.with_changes(applied.changes());
    assert_eq!(edited.dropped(), 0);
    let scheme = edited.model().scheme();
    for n in 1..=3 {
        assert!(
            scheme.font(&format!("TF2Tools_{n}"), 1080).is_some(),
            "TF2Tools_{n}"
        );
    }
    let font = |key: &PartKey| {
        edited
            .model()
            .part(key)
            .unwrap()
            .value("font")
            .unwrap()
            .text()
            .to_owned()
    };
    // The recipe's _1 collides with the HUD's and becomes _3; its _2 is free and stays.
    assert_eq!(
        (font(&hv), font(&ammo)),
        ("TF2Tools_3".to_owned(), "TF2Tools_2".to_owned())
    );
}

#[test]
fn at_most_32_fonts_are_added_per_recipe() {
    let m = font_model();
    let (file, changes) = m.add_font_file("cool.ttf", &ttf("Cool Sans"), 0).unwrap();
    let mut assets = HudAssets::new();
    assets.insert(file.source(), ttf("Cool Sans"));
    let asset = changes[0].clone();
    let many: Vec<Change> = (0..33).map(|g| asset.in_group(g)).collect();
    let recipe = Recipe::new("Many", &many, &assets);
    let applied = m.apply_recipe(&recipe, &[], 0);
    assert_eq!(applied.added().len(), 32);
    assert_eq!(applied.skipped(), 1);
}

#[test]
fn a_font_that_is_not_a_font_is_skipped() {
    let m = font_model();
    let recipe = font_recipe(&m);
    let mut json: serde_json::Value = serde_json::from_slice(&recipe.to_json()).unwrap();
    let assets = json["assets"].as_object_mut().unwrap();
    for value in assets.values_mut() {
        *value = serde_json::Value::String("AAEC".into());
    }
    let (bad, _) = Recipe::parse(&serde_json::to_vec(&json).unwrap()).unwrap();
    let applied = m.apply_recipe(&bad, &[], 0);
    assert_eq!(applied.skipped(), 1);
    assert!(applied.added().is_empty());
}

#[test]
fn a_recipe_missing_its_font_file_still_applies_cleanly() {
    // Exported without the font's bytes, with the family set in a later action: the recipe
    // drops the font's CustomFontFiles line but keeps the clone using the family.
    let m = font_model();
    let (_, mut changes) = m.add_font_file("cool.ttf", &ttf("Cool Sans"), 0).unwrap();
    let edited = m.with_changes(&changes).into_model();
    changes.extend(
        edited
            .edit_font(
                &PartKey::new(HV),
                "font",
                FontEdit::Family("Cool Sans".into()),
                1080,
                1,
            )
            .unwrap(),
    );
    let recipe = Recipe::new("No file", &changes, &HudAssets::new());
    let (recipe, parse_skipped) = Recipe::parse(&recipe.to_json()).unwrap();
    assert_eq!(parse_skipped, 0);
    let applied = m.apply_recipe(&recipe, &[], 0);
    assert!(applied.added().is_empty());
    let edited = m.with_changes(applied.changes());
    assert_eq!(edited.dropped(), 0);
    let model = edited.model();
    assert!(
        model.scheme().font_files().is_empty(),
        "no slot points at a file that is not there"
    );
    // The clone still lands; the game falls back from the missing family by itself.
    let font = model
        .part(&PartKey::new(HV))
        .unwrap()
        .value("font")
        .unwrap();
    assert_eq!(font.text(), "TF2Tools_1");
    assert!(model.scheme().font("TF2Tools_1", 1080).is_some());
}

// Font attempts are bounded, and clone renames reach every place a name sits.
use tf2tools_core::{AddBlock, BlockLine};

/// An `Asset` change for a kept font file named `name`.
fn font_asset(group: u32, name: &str) -> Change {
    Change::asset(
        group,
        AssetFile::new(
            &format!("resource/fonts/tf2tools/{name}"),
            &format!(".tf2tools/fonts/{name}"),
        ),
    )
}

#[test]
fn refused_fonts_count_toward_the_cap() {
    let m = font_model();
    let mut assets = HudAssets::new();
    assets.insert(".tf2tools/fonts/junk.ttf", b"not a font".to_vec());
    assets.insert(".tf2tools/fonts/cool.ttf", ttf("Cool Sans"));
    let mut changes: Vec<Change> = (0..40).map(|g| font_asset(g, "junk.ttf")).collect();
    changes.push(font_asset(40, "cool.ttf"));
    let applied = m.apply_recipe(&Recipe::new("Junk", &changes, &assets), &[], 0);
    assert!(
        applied.added().is_empty(),
        "the good font comes past the cap"
    );
    assert_eq!(applied.skipped(), 41);
}

#[test]
fn ten_thousand_garbage_fonts_are_all_skipped() {
    let m = font_model();
    let mut assets = HudAssets::new();
    assets.insert(".tf2tools/fonts/junk.ttf", b"not a font".to_vec());
    let changes: Vec<Change> = (0..10_000).map(|g| font_asset(g, "junk.ttf")).collect();
    let applied = m.apply_recipe(&Recipe::new("Junk", &changes, &assets), &[], 0);
    assert!(applied.added().is_empty());
    assert_eq!(applied.skipped(), 10_000);
    assert!(applied.changes().is_empty());
}

#[test]
fn a_protected_family_recipe_font_is_skipped() {
    let m = font_model();
    let (file, changes) = m.add_font_file("cool.ttf", &ttf("Cool Sans"), 0).unwrap();
    let mut assets = HudAssets::new();
    assets.insert(file.source(), ttf("Marlett"));
    let applied = m.apply_recipe(&Recipe::new("Sys", &changes, &assets), &[], 0);
    assert!(applied.added().is_empty());
    assert_eq!(applied.skipped(), 1);
    let edited = m.with_changes(applied.changes());
    assert!(edited.model().scheme().font_files().is_empty());
}

#[test]
fn a_font_recipe_undoes_in_one_step() {
    let m = font_model();
    let before = vec![set(0, "labelText", "HP")];
    let applied = m.apply_recipe(&font_recipe(&m), &before, 5);
    assert_eq!(applied.added().len(), 1);
    let (recipe, rest): (Vec<Change>, Vec<Change>) = applied
        .changes()
        .iter()
        .cloned()
        .partition(|c| c.group() == 5);
    assert!(!recipe.is_empty());
    assert_eq!(
        rest, before,
        "dropping the recipe's group gives the list back"
    );
    let undone = m.with_changes(&rest);
    assert!(undone.model().scheme().font_files().is_empty());
}

/// A recipe defining `TF2Tools_1` for the health value, applied onto a HUD that has its own.
fn colliding_clone(m: &HudModel) -> (Vec<Change>, Vec<Change>) {
    let hv = PartKey::new(HV);
    let base = m
        .edit_font(&hv, "font", FontEdit::Size(20), 1080, 0)
        .unwrap();
    let recipe = m
        .edit_font(&hv, "font", FontEdit::Size(24), 1080, 1)
        .unwrap();
    (base, recipe)
}

#[test]
fn a_renamed_clone_is_renamed_inside_added_blocks_and_keys() {
    let m = font_model();
    let (base, mut changes) = colliding_clone(&m);
    let root = "Resource/UI/HudPlayerHealth.res";
    changes.push(Change::add_block(
        2,
        AddBlock::new(
            "resource/ui/hudplayerhealth.res",
            &[root],
            "MyLabel",
            None,
            vec![
                BlockLine::new("ControlName", "CExLabel", None),
                BlockLine::new("font", "TF2Tools_1", None),
            ],
        ),
    ));
    let label = [root, "PlayerStatusHealthValue"];
    changes.push(Change::set(
        3,
        SetKey::new(
            "resource/ui/hudplayerhealth.res",
            &label,
            "TF2Tools_1",
            None,
            None,
            "1",
        ),
    ));
    let applied = m.apply_recipe(&Recipe::new("Lines", &changes, &HudAssets::new()), &base, 4);
    assert_eq!(applied.skipped(), 0);
    let edited = m.with_changes(applied.changes());
    let doc = edited
        .model()
        .document("resource/ui/hudplayerhealth.res")
        .unwrap();
    let font = doc.find(&[root, "MyLabel", "font"]);
    assert_eq!(font.first().and_then(|e| e.text()), Some("TF2Tools_2"));
    assert!(
        !doc.find(&[root, "PlayerStatusHealthValue", "TF2Tools_2"])
            .is_empty()
    );
    assert!(
        doc.find(&[root, "PlayerStatusHealthValue", "TF2Tools_1"])
            .is_empty()
    );
}

#[test]
fn clone_numbers_in_block_paths_are_not_reused() {
    let m = font_model();
    let (base, mut changes) = colliding_clone(&m);
    changes.push(Change::set(
        2,
        SetKey::new(
            "resource/clientscheme.res",
            &["Scheme", "Fonts", "TF2Tools_5", "1"],
            "tall",
            None,
            None,
            "9",
        ),
    ));
    let applied = m.apply_recipe(&Recipe::new("Path", &changes, &HudAssets::new()), &base, 3);
    let edited = m.with_changes(applied.changes());
    let font = edited
        .model()
        .part(&PartKey::new(HV))
        .unwrap()
        .value("font")
        .unwrap()
        .text()
        .to_owned();
    assert_eq!(font, "TF2Tools_6");
}

// Closing fixes: an exported recipe holds only what reads back and names no font it left out.
use tf2tools_core::RemoveKey;

#[test]
fn an_exported_recipe_keeps_only_changes_on_safe_paths() {
    let block = ["Scheme", "Colors"];
    let bad = [
        Change::set(1, SetKey::new("../evil.res", &block, "k", None, None, "v")),
        Change::remove(
            1,
            RemoveKey::new("cfg/autoexec.res", &block, "k", None, None),
        ),
        Change::add_block(1, AddBlock::new("/abs.res", &block, "B", None, Vec::new())),
        Change::asset(
            1,
            AssetFile::new("cfg/evil.ttf", ".tf2tools/fonts/evil.ttf"),
        ),
    ];
    let mut assets = HudAssets::new();
    assets.insert(".tf2tools/fonts/evil.ttf", vec![1, 2, 3]);
    let mut changes = vec![set(1, "fgcolor", "1 2 3 255")];
    changes.extend(bad);
    let recipe = Recipe::new("Paths", &changes, &assets);
    assert_eq!(recipe.changes(), [changes[0].clone()]);
    assert_eq!(recipe.asset(".tf2tools/fonts/evil.ttf"), None);
    assert_eq!(recipe.fonts_left_out(), 1);
    let (back, skipped) = Recipe::parse(&recipe.to_json()).unwrap();
    assert_eq!((back.to_json(), skipped), (recipe.to_json(), 0));
}

#[test]
fn an_exported_recipe_counts_the_fonts_it_left_out() {
    let mut assets = HudAssets::new();
    assets.insert(".tf2tools/fonts/a.ttf", vec![1]);
    assets.insert(".tf2tools/images/c.ttf", vec![1]);
    let changes = [
        font_asset(1, "a.ttf"),
        font_asset(2, "missing.ttf"),
        Change::asset(
            3,
            AssetFile::new("resource/fonts/tf2tools/c.ttf", ".tf2tools/images/c.ttf"),
        ),
        set(4, "fgcolor", "1 2 3 255"),
    ];
    let recipe = Recipe::new("Count", &changes, &assets);
    assert_eq!(recipe.fonts_left_out(), 2);
    let (back, _) = Recipe::parse(&recipe.to_json()).unwrap();
    assert_eq!(
        back.fonts_left_out(),
        0,
        "a read recipe counts in parse's skipped"
    );
    assert_eq!(
        Recipe::new("None", &changes[..1], &assets).fonts_left_out(),
        0
    );
}

/// `base`, then adding cool.ttf and setting the health value's family to it in `group`, as the
/// app's add-font action does.
fn with_font_added(m: &HudModel, base: Vec<Change>, group: u32) -> Vec<Change> {
    let mut changes = base;
    let model = m.with_changes(&changes).into_model();
    let (_, added) = model
        .add_font_file("cool.ttf", &ttf("Cool Sans"), group)
        .unwrap();
    changes.extend(added);
    let model = m.with_changes(&changes).into_model();
    let family = FontEdit::Family("Cool Sans".into());
    changes.extend(
        model
            .edit_font(&PartKey::new(HV), "font", family, 1080, group)
            .unwrap(),
    );
    changes
}

#[test]
fn an_exported_recipe_drops_the_edits_of_a_font_it_left_out() {
    let m = font_model();
    let hv = PartKey::new(HV);
    // The family edit clones the font, and a later size edit changes that clone.
    let mut changes = with_font_added(&m, Vec::new(), 1);
    let model = m.with_changes(&changes).into_model();
    changes.extend(
        model
            .edit_font(&hv, "font", FontEdit::Size(30), 1080, 2)
            .unwrap(),
    );
    // A slot line naming the lost file in another action goes too.
    let slots = ["Scheme", "CustomFontFiles"];
    let path = "resource/fonts/tf2tools/cool.ttf";
    let slot = SetKey::new("resource/clientscheme.res", &slots, "9", None, None, path);
    changes.push(Change::set(4, slot));
    let other = set(3, "labelText", "HP");
    changes.push(other.clone());
    let recipe = Recipe::new("Lost", &changes, &HudAssets::new());
    assert_eq!(recipe.fonts_left_out(), 1);
    assert_eq!(recipe.changes(), [other]);

    // A clone made earlier stays; only the edit giving it the lost family goes.
    let base = m
        .edit_font(&hv, "font", FontEdit::Size(20), 1080, 0)
        .unwrap();
    let changes = with_font_added(&m, base.clone(), 1);
    let recipe = Recipe::new("Lost", &changes, &HudAssets::new());
    assert_eq!(recipe.changes(), base);
}
