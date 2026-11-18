//! The names of the game's classes, weapon slots, sound kinds and add-on sections, and the HUD
//! Maker's labels and enum names, in the current language.

use gpui_kit::SharedString;
use tf2tools_core::{
    AddonSection, BoardMode, MeterKind, Scene, SceneRule, SoundKind, StartingPoint, TfClass,
    WeaponSlot,
};

use super::tr;

/// The class's full name, such as "Heavy".
pub fn class_name(class: TfClass) -> SharedString {
    // A class core adds later keeps its English name until the catalogue names it.
    match class {
        TfClass::Scout => tr!("common.class.scout"),
        TfClass::Soldier => tr!("common.class.soldier"),
        TfClass::Pyro => tr!("common.class.pyro"),
        TfClass::Demoman => tr!("common.class.demoman"),
        TfClass::Heavy => tr!("common.class.heavy"),
        TfClass::Engineer => tr!("common.class.engineer"),
        TfClass::Medic => tr!("common.class.medic"),
        TfClass::Sniper => tr!("common.class.sniper"),
        TfClass::Spy => tr!("common.class.spy"),
        _ => class.name().into(),
    }
}

/// The class's name for a header or tab about as wide as the English: English keeps the full
/// name, and a language whose full name is longer shortens it.
pub fn class_compact_name(class: TfClass) -> SharedString {
    match class {
        TfClass::Scout => tr!("common.class_compact.scout"),
        TfClass::Soldier => tr!("common.class_compact.soldier"),
        TfClass::Pyro => tr!("common.class_compact.pyro"),
        TfClass::Demoman => tr!("common.class_compact.demoman"),
        TfClass::Heavy => tr!("common.class_compact.heavy"),
        TfClass::Engineer => tr!("common.class_compact.engineer"),
        TfClass::Medic => tr!("common.class_compact.medic"),
        TfClass::Sniper => tr!("common.class_compact.sniper"),
        TfClass::Spy => tr!("common.class_compact.spy"),
        _ => class.name().into(),
    }
}

/// The weapon slot's name, such as "Melee".
pub fn slot_name(slot: WeaponSlot) -> SharedString {
    match slot {
        WeaponSlot::Primary => tr!("common.slot.primary"),
        WeaponSlot::Secondary => tr!("common.slot.secondary"),
        WeaponSlot::Melee => tr!("common.slot.melee"),
        _ => slot.name().into(),
    }
}

/// The weapon slot's name for a narrow header; only the longer Russian names shorten.
pub fn slot_short_name(slot: WeaponSlot) -> SharedString {
    match slot {
        WeaponSlot::Secondary => tr!("common.slot_short.secondary"),
        WeaponSlot::Melee => tr!("common.slot_short.melee"),
        WeaponSlot::Primary | _ => slot_name(slot),
    }
}

/// The sound kind's name, such as "Killsound".
pub fn sound_kind_name(kind: SoundKind) -> SharedString {
    match kind {
        SoundKind::Hitsound => tr!("common.sound_kind.hitsound"),
        SoundKind::Killsound => tr!("common.sound_kind.killsound"),
        _ => kind.name().into(),
    }
}

/// The add-on section's name, such as "Skins"; a section core adds later keeps core's title.
pub fn addon_section_name(section: AddonSection) -> SharedString {
    match section.key() {
        "skins" => tr!("addons.section.skins"),
        "effects" => tr!("addons.section.effects"),
        "guis" => tr!("addons.section.guis"),
        "textures" => tr!("addons.section.textures"),
        "gamefiles" => tr!("addons.section.gamefiles"),
        "maps" => tr!("addons.section.maps"),
        _ => section.title().into(),
    }
}

/// The catalogue key for a label from core's HUD Maker tables: its English as a lowercase slug
/// under `hud_maker.label`, or `None` for a glyph with fewer than two ASCII letters.
pub fn maker_label_key(english: &str) -> Option<String> {
    if english.bytes().filter(u8::is_ascii_alphabetic).count() < 2 {
        return None;
    }
    let mut slug = String::with_capacity(english.len());
    for c in english.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_end_matches('_');
    // Built with format!, so the key scans never read a literal under the label prefix.
    Some(format!("hud_maker.label.{slug}"))
}

/// A label from core's HUD Maker tables in the current language; a label the catalogue lacks,
/// and a glyph, shows as given.
pub fn maker_label(english: &str) -> SharedString {
    let found = maker_label_key(english)
        .and_then(|key| crate::_rust_i18n_try_translate(super::current(), &key));
    match found {
        Some(text) => text.into_owned().into(),
        None => english.to_owned().into(),
    }
}

/// The HUD Maker scene's name, such as "Scoreboard".
pub fn scene_name(scene: Scene) -> SharedString {
    match scene {
        Scene::Hud => tr!("hud_maker.scene.hud"),
        Scene::Scoreboard => tr!("hud_maker.scene.scoreboard"),
        Scene::MainMenu => tr!("hud_maker.scene.main_menu"),
        Scene::FreezeCam => tr!("hud_maker.scene.freeze_cam"),
        Scene::WinPanel => tr!("hud_maker.scene.win_panel"),
        Scene::ClassSelect => tr!("hud_maker.scene.class_select"),
        Scene::Spectator => tr!("hud_maker.scene.spectator"),
        _ => SceneRule::get(scene).label().into(),
    }
}

/// The Scoreboard's match mode name, such as "Competitive".
pub fn board_mode_name(mode: BoardMode) -> SharedString {
    match mode {
        BoardMode::Casual => tr!("hud_maker.board_mode.casual"),
        BoardMode::Competitive => tr!("hud_maker.board_mode.competitive"),
        BoardMode::Mvm => tr!("hud_maker.board_mode.mvm"),
        _ => mode.label().into(),
    }
}

/// The weapons that show an item meter, as players name them.
pub fn meter_name(kind: MeterKind) -> SharedString {
    match kind {
        MeterKind::Plain => tr!("hud_maker.meter.plain"),
        MeterKind::Drink => tr!("hud_maker.meter.drink"),
        MeterKind::SodaPopper => tr!("hud_maker.meter.soda_popper"),
        MeterKind::Cleaver => tr!("hud_maker.meter.cleaver"),
        MeterKind::Minigun => tr!("hud_maker.meter.minigun"),
        MeterKind::Heads => tr!("hud_maker.meter.heads"),
        MeterKind::Focus => tr!("hud_maker.meter.focus"),
        MeterKind::Sword => tr!("hud_maker.meter.sword"),
        MeterKind::CowMangler => tr!("hud_maker.meter.cow_mangler"),
        MeterKind::Bison => tr!("hud_maker.meter.bison"),
        MeterKind::Spycicle => tr!("hud_maker.meter.spycicle"),
        MeterKind::Sapper => tr!("hud_maker.meter.sapper"),
        MeterKind::Revolver => tr!("hud_maker.meter.revolver"),
        MeterKind::Revenge => tr!("hud_maker.meter.revenge"),
        MeterKind::Pomson => tr!("hud_maker.meter.pomson"),
        MeterKind::Flamethrower => tr!("hud_maker.meter.flamethrower"),
        MeterKind::RocketPack => tr!("hud_maker.meter.rocket_pack"),
        MeterKind::Organs => tr!("hud_maker.meter.organs"),
        MeterKind::Killstreak => tr!("hud_maker.meter.killstreak"),
        MeterKind::Kart => tr!("hud_maker.meter.kart"),
        MeterKind::Canteen => tr!("hud_maker.meter.canteen"),
        _ => kind.label().into(),
    }
}

/// The starting point's name in the picker, such as "Big numbers".
pub fn starting_point_name(point: StartingPoint) -> SharedString {
    match point {
        StartingPoint::CompetitiveMinimal => tr!("hud_maker.start.competitive_minimal.name"),
        StartingPoint::BigNumbers => tr!("hud_maker.start.big_numbers.name"),
        StartingPoint::StreamerClean => tr!("hud_maker.start.streamer_clean.name"),
        StartingPoint::CentreHud => tr!("hud_maker.start.centre_hud.name"),
        _ => point.label().into(),
    }
}

/// The starting point's one-sentence description in the picker.
pub fn starting_point_about(point: StartingPoint) -> SharedString {
    match point {
        StartingPoint::CompetitiveMinimal => tr!("hud_maker.start.competitive_minimal.about"),
        StartingPoint::BigNumbers => tr!("hud_maker.start.big_numbers.about"),
        StartingPoint::StreamerClean => tr!("hud_maker.start.streamer_clean.about"),
        StartingPoint::CentreHud => tr!("hud_maker.start.centre_hud.about"),
        _ => point.about().into(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tf2tools_core::{ClassSchema, OptionKind, OptionSpec, PanelRule, SampleSet, SceneRule};

    use super::{maker_label, maker_label_key};

    // One catalogue as flat key -> value, read without switching the locale.
    fn messages(code: &str) -> BTreeMap<String, String> {
        crate::_rust_i18n_backend()
            .messages_for_locale(code)
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    // `slug` under the label prefix, built so the key scans read no literal key here.
    fn key(slug: &str) -> Option<String> {
        Some(format!("{}.{slug}", ["hud_maker", "label"].join(".")))
    }

    // Every English label core's HUD Maker tables show, with the accessor it came from. Each
    // table task adds its walk here.
    fn core_labels() -> Vec<(String, &'static str)> {
        let mut out = Vec::new();
        let rules = PanelRule::all()
            .iter()
            .chain(SceneRule::every().iter().flat_map(|r| r.rules()));
        for rule in rules {
            out.push((rule.label().to_owned(), "PanelRule::label"));
            out.push((rule.shows_when().to_owned(), "PanelRule::shows_when"));
        }
        for scene in SceneRule::every() {
            for toggled in scene.code_toggled() {
                out.push((toggled.shows_when().to_owned(), "CodeToggled::shows_when"));
            }
        }
        for schema in ClassSchema::all() {
            for spec in schema.properties() {
                out.push((spec.label().to_owned(), "PropertySpec::label"));
                for &(choice, _) in spec.choices() {
                    out.push((choice.to_owned(), "PropertySpec::choices"));
                }
            }
        }
        for spec in OptionSpec::built_ins() {
            out.push((spec.label().to_owned(), "OptionSpec::label"));
            out.push((spec.group().to_owned(), "OptionSpec::group"));
            out.extend(
                spec.tooltip()
                    .map(|t| (t.to_owned(), "OptionSpec::tooltip")),
            );
            if let OptionKind::Choice(choices) = spec.kind() {
                out.extend(
                    choices
                        .iter()
                        .map(|c| (c.label().to_owned(), "Choice::label")),
                );
            }
        }
        for sample in SampleSet::ALL {
            for part in sample.game_filled_parts() {
                out.extend(
                    sample
                        .game_filled(part)
                        .map(|n| (n.to_owned(), "SampleSet::game_filled")),
                );
            }
        }
        out
    }

    // What `labels` lack: a key missing from en.yml or worded differently, a key missing from
    // ru.yml, and two different English labels sharing one key.
    fn label_gaps(labels: &[(String, &'static str)]) -> Vec<String> {
        let (en, ru) = (messages("en"), messages("ru"));
        let mut owner: BTreeMap<String, &str> = BTreeMap::new();
        let mut gaps = Vec::new();
        for (english, from) in labels {
            let Some(key) = maker_label_key(english) else {
                continue;
            };
            match owner.get(&key) {
                Some(other) if *other != english.as_str() => {
                    gaps.push(format!("{key}: {other:?} and {english:?} share a key"));
                }
                _ => {
                    owner.insert(key.clone(), english.as_str());
                }
            }
            if en.get(&key) != Some(english) {
                gaps.push(format!(
                    "{from}: {key} = {english:?} is missing from en.yml"
                ));
            }
            if !ru.contains_key(&key) {
                gaps.push(format!("{from}: {key} is missing from ru.yml"));
            }
        }
        gaps
    }

    #[test]
    fn a_label_key_is_its_english_slug_and_a_glyph_has_none() {
        assert_eq!(maker_label_key("Hide"), key("hide"));
        assert_eq!(
            maker_label_key("Background colour"),
            key("background_colour")
        );
        assert_eq!(maker_label_key("Collector's"), key("collector_s"));
        assert_eq!(
            maker_label_key("Bonk!, Crit-a-Cola and Mad Milk"),
            key("bonk_crit_a_cola_and_mad_milk")
        );
        assert_eq!(
            maker_label_key("  Shows in a timed round. "),
            key("shows_in_a_timed_round")
        );
        for glyph in ["+", "A", "a", "x2", "", "#"] {
            assert_eq!(maker_label_key(glyph), None, "{glyph:?}");
        }
    }

    #[test]
    fn a_label_without_a_key_shows_its_english_and_a_glyph_shows_as_it_is() {
        assert_eq!(
            maker_label("Not a real label").to_string(),
            "Not a real label"
        );
        assert_eq!(maker_label("+").to_string(), "+");
    }

    #[test]
    fn the_gap_check_names_a_missing_label_and_a_shared_key() {
        let labels = [
            ("Not a real label".to_owned(), "test"),
            ("Not a real-label".to_owned(), "test"),
        ];
        let gaps = label_gaps(&labels);
        let missing = key("not_a_real_label").unwrap();
        assert!(
            gaps.iter()
                .any(|g| g.contains(&missing) && g.contains("en.yml")),
            "{gaps:?}"
        );
        assert!(gaps.iter().any(|g| g.contains("share a key")), "{gaps:?}");
    }

    // The enum names' English must stay core's English byte for byte; the `_` arms never fire in
    // English because the catalogue answers, so this calls each name function under `en`.
    #[test]
    fn the_enum_names_read_cores_english_in_english() {
        use tf2tools_core::{BoardMode, MeterKind, Scene, SceneRule, StartingPoint};

        use super::{
            board_mode_name, meter_name, scene_name, starting_point_about, starting_point_name,
        };

        let _lock = crate::i18n::test_lock();
        crate::i18n::apply("en");
        for scene in Scene::ALL {
            assert_eq!(
                scene_name(scene).as_ref(),
                SceneRule::get(scene).label(),
                "{scene:?}"
            );
        }
        for mode in BoardMode::ALL {
            assert_eq!(board_mode_name(mode).as_ref(), mode.label(), "{mode:?}");
        }
        for kind in MeterKind::ALL {
            assert_eq!(meter_name(kind).as_ref(), kind.label(), "{kind:?}");
        }
        for point in StartingPoint::ALL {
            assert_eq!(
                starting_point_name(point).as_ref(),
                point.label(),
                "{point:?}"
            );
            assert_eq!(
                starting_point_about(point).as_ref(),
                point.about(),
                "{point:?}"
            );
        }
    }

    #[test]
    fn every_core_label_has_its_key_in_both_catalogues() {
        let gaps = label_gaps(&core_labels());
        assert!(gaps.is_empty(), "{gaps:#?}");
    }
}
