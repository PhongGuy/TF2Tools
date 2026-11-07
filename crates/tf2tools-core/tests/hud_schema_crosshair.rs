//! A HUD's own on-screen crosshair from its option schema: the symbol pick, and a size whose
//! text names another option (`{$outline ? ON : OFF}`), over a small hexhud copy.

use tf2tools_core::{
    Change, HudFiles, HudOptions, HudSchema, HudSchemas, OptionChange, OptionKind, OptionValue,
    Platform,
};

const FILE: &str = "customizations/crosshairs_and_hitmarker.res";
const STYLE: &str = "e:hex_val_xhair_style";
const SIZE: &str = "e:hex_val_xhair_size";
const OUTLINE: &str = "e:hex_val_xhair_outline";

const RES: &str = "\"Resource/HudLayout.res\"\n{\n\t\"CustomCrosshair\"\n\t{\n\t\t\"visible\" \"0\"\n\t\t\"labelText\" \"A\"\n\t\t\"font\" \"Size:18 | Outline:OFF\"\n\t}\n\t\"CustomCrosshairBlurShadow\"\n\t{\n\t\t\"visible\" \"0\"\n\t\t\"labelText\" \"A\"\n\t\t\"font\" \"Size:18 | Blur:ON\"\n\t}\n}\n";

fn hexhud() -> (HudFiles, HudOptions) {
    let layer = [(FILE.to_owned(), RES.as_bytes().to_vec())].into();
    let files = HudFiles::from_memory(vec![layer]).with_install_folder("hexhud");
    let schema: HudSchema = HudSchemas::bundled().for_name("hexhud").unwrap().clone();
    let options = HudOptions::read(&files, Some(&schema), Platform::Linux);
    (files, options)
}

fn choose(id: &str, value: OptionValue) -> Change {
    Change::option(1, OptionChange::new(OptionChange::SCHEMA, id, value))
}

fn text(options: &HudOptions, files: &HudFiles, changes: &[Change]) -> String {
    let switched = options.switch(files, changes).unwrap();
    String::from_utf8(switched.files().read(FILE).unwrap().unwrap().into_bytes()).unwrap()
}

#[test]
fn the_crosshair_symbol_is_a_pick_starting_at_the_copys_symbol() {
    let (_, options) = hexhud();
    let style = options.specs().iter().find(|s| s.id() == STYLE).unwrap();
    let OptionKind::Choice(choices) = style.kind() else {
        panic!("the symbol is a pick");
    };
    assert!(choices.len() > 60);
    assert!(choices.iter().any(|c| c.key() == "<" && c.label() == "<"));
    assert_eq!(
        options.value(STYLE, &[]),
        Some(OptionValue::Pick("A".into()))
    );
}

#[test]
fn picking_a_symbol_writes_it_to_every_crosshair_label_and_removal_puts_it_back() {
    let (files, options) = hexhud();
    let got = text(
        &options,
        &files,
        &[choose(STYLE, OptionValue::Pick("<".into()))],
    );
    assert_eq!(got.matches("\"labelText\" \"<\"").count(), 2, "{got}");
    assert_eq!(text(&options, &files, &[]), RES);
}

#[test]
fn a_size_names_the_outline_switch_as_it_stands() {
    let (files, options) = hexhud();
    assert!(options.specs().iter().any(|s| s.id() == OUTLINE));
    let size = choose(SIZE, OptionValue::Int(24));
    let got = text(&options, &files, std::slice::from_ref(&size));
    assert!(got.contains("\"font\" \"Size:24 | Outline:OFF\""), "{got}");
    assert!(got.contains("\"font\" \"Size:24 | Blur:ON\""), "{got}");
    let outline = choose(OUTLINE, OptionValue::Bool(true));
    let got = text(&options, &files, &[outline, size]);
    assert!(got.contains("\"font\" \"Size:24 | Outline:ON\""), "{got}");
}

#[test]
fn turning_the_outline_on_alone_rewrites_the_size_at_its_start() {
    let (files, options) = hexhud();
    let got = text(
        &options,
        &files,
        &[choose(OUTLINE, OptionValue::Bool(true))],
    );
    assert!(got.contains("\"font\" \"Size:18 | Outline:ON\""), "{got}");
    // Turning it back off is no stored choice at all, so the file is as shipped.
    let next = options
        .set(OUTLINE, OptionValue::Bool(false), &[], 1)
        .unwrap();
    assert_eq!(text(&options, &files, &next), RES);
}
