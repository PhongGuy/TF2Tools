//! The built-in HUD crosshair: the HUD's own on-screen crosshair label, offered only where
//! hudlayout.res has a CustomCrosshair element.

use crate::common;

use common::hud_stock::{hudlayout_with_killfeed, stock_files_with};
use tf2tools_core::{
    Change, HudModel, OptionChange, OptionKind, OptionValue, Platform, Rgba, Screen,
};

const ON: &str = "hud_crosshair";
const SYMBOL: &str = "hud_crosshair_symbol";
const SIZE: &str = "hud_crosshair_size";
const COLOUR: &str = "hud_crosshair_colour";

const SCHEME: &str = r#""Scheme"
{
  Colors { Crosshair "255 255 255 255" }
  Fonts
  {
    Crosshairs { "1" { name "TF2 Crosshairs" tall 24 antialias 0 } }
    Shared { "1" { name "TF2 Crosshairs" tall 24 } }
  }
}
"#;
const CROSSHAIR: &str = "  CustomCrosshair { ControlName CExLabel fieldName CustomCrosshair visible 0 enabled 0 zpos 2 xpos c-50 ypos c-50 wide 100 tall 100 font Crosshairs labelText \"+\" textAlignment center fgcolor Crosshair }\n";

fn model_with(element: &str) -> HudModel {
    let layout = hudlayout_with_killfeed(element);
    let files = stock_files_with(&[
        ("resource/clientscheme.res", Some(SCHEME)),
        ("scripts/hudlayout.res", Some(layout.as_str())),
    ]);
    HudModel::load(&files, Platform::Linux).unwrap()
}

fn model() -> HudModel {
    model_with(CROSSHAIR)
}

fn option(id: &str, value: OptionValue) -> Change {
    Change::option(1, OptionChange::new(OptionChange::BUILTIN, id, value))
}

fn key(m: &HudModel, k: &str) -> Option<String> {
    let doc = m.document("scripts/hudlayout.res")?;
    let lines = doc.find(&["Resource/HudLayout.res", "CustomCrosshair", k]);
    lines.last()?.text().map(str::to_owned)
}

#[test]
fn a_hud_without_a_custom_crosshair_is_not_offered_one() {
    let m = model_with("");
    assert!(m.options().iter().all(|o| !o.id().starts_with(ON)));
    assert!(m.set_option(ON, OptionValue::Bool(true), &[], 1).is_err());
}

#[test]
fn a_hud_with_a_custom_crosshair_offers_the_four_controls_at_its_own_values() {
    let m = model();
    let ids: Vec<&str> = (m.options().iter())
        .filter(|o| o.group() == "HUD crosshair")
        .map(|o| o.id())
        .collect();
    assert_eq!(ids, [ON, SYMBOL, SIZE, COLOUR]);
    let spec = |id: &str| *m.options().iter().find(|o| o.id() == id).unwrap();
    assert!(matches!(spec(ON).kind(), OptionKind::Toggle));
    let OptionKind::Choice(symbols) = spec(SYMBOL).kind() else {
        panic!("the symbol is a pick");
    };
    assert!(symbols.iter().any(|c| c.key() == "<"));
    assert!((m.options().iter()).all(|o| !o.id().starts_with(ON) || o.drawn()));
    assert_eq!(m.option_value(ON, &[]), Some(OptionValue::Bool(false)));
    assert_eq!(
        m.option_value(SYMBOL, &[]),
        Some(OptionValue::Pick("+".into()))
    );
    assert_eq!(m.option_value(SIZE, &[]), Some(OptionValue::Int(24)));
    assert_eq!(
        m.option_value(COLOUR, &[]),
        Some(OptionValue::Rgba([255, 255, 255, 255]))
    );
}

#[test]
fn turning_it_on_shows_the_label_and_turning_it_off_restores_the_file() {
    let m = model();
    let on = m.set_option(ON, OptionValue::Bool(true), &[], 1).unwrap();
    let e = m.with_changes(&on);
    assert_eq!(key(e.model(), "visible").as_deref(), Some("1"));
    assert_eq!(key(e.model(), "enabled").as_deref(), Some("1"));
    let off = m.set_option(ON, OptionValue::Bool(false), &on, 2).unwrap();
    assert!(off.is_empty(), "off is no stored choice: {off:?}");
    let pristine = m.document("scripts/hudlayout.res").unwrap().as_bytes();
    let back = m.with_changes(&off);
    assert_eq!(
        back.model()
            .document("scripts/hudlayout.res")
            .unwrap()
            .as_bytes(),
        pristine
    );
}

#[test]
fn symbol_size_and_colour_write_the_label_its_own_font_and_colour() {
    let m = model();
    let changes = [
        option(ON, OptionValue::Bool(true)),
        option(SYMBOL, OptionValue::Pick("<".into())),
        option(SIZE, OptionValue::Int(40)),
        option(COLOUR, OptionValue::Rgba([255, 0, 0, 200])),
    ];
    let e = m.with_changes(&changes);
    assert_eq!((e.applied().len(), e.dropped()), (4, 0));
    let shown = e.model();
    assert_eq!(key(shown, "labelText").as_deref(), Some("<"));
    assert_eq!(key(shown, "fgcolor").as_deref(), Some("255 0 0 200"));
    // The size goes into a font of the crosshair's own, so other parts sharing its font keep
    // theirs.
    let font = key(shown, "font").unwrap();
    assert_ne!(font, "Crosshairs");
    let variant = shown.scheme().font(&font, 1080).unwrap();
    assert_eq!((variant.family(), variant.tall()), ("TF2 Crosshairs", 40));
    assert_eq!(shown.scheme().font("Crosshairs", 1080).unwrap().tall(), 24);
    for (id, want) in [
        (SYMBOL, OptionValue::Pick("<".into())),
        (SIZE, OptionValue::Int(40)),
        (COLOUR, OptionValue::Rgba([255, 0, 0, 200])),
    ] {
        assert_eq!(m.option_value(id, &changes), Some(want), "{id}");
    }
    assert_eq!(
        shown.scheme().named_colour("Crosshair"),
        Some(Rgba::new(255, 255, 255, 255))
    );
}

#[test]
fn resetting_the_group_removes_every_crosshair_choice() {
    let m = model();
    let changes = [
        option(ON, OptionValue::Bool(true)),
        option(SIZE, OptionValue::Int(40)),
    ];
    let reset = m.reset_option_group("HUD crosshair", &changes);
    assert!(reset.is_empty());
    let back = m.with_changes(&reset);
    assert_eq!(key(back.model(), "font").as_deref(), Some("Crosshairs"));
    assert_eq!(key(back.model(), "visible").as_deref(), Some("0"));
}

#[test]
fn the_preview_reads_the_crosshair_label_as_the_game_merges_it() {
    let m = model();
    let screen = Screen::new(1920, 1080);
    let off = m.hud_crosshair(screen).unwrap();
    assert!(!off.shown());
    let changes = [
        option(ON, OptionValue::Bool(true)),
        option(SYMBOL, OptionValue::Pick("<".into())),
        option(COLOUR, OptionValue::Rgba([255, 0, 0, 200])),
    ];
    let e = m.with_changes(&changes);
    let on = e.model().hud_crosshair(screen).unwrap();
    assert!(on.shown());
    assert_eq!(on.symbol(), "<");
    assert_eq!(on.font(), "Crosshairs");
    assert_eq!(on.colour(), Rgba::new(255, 0, 0, 200));
    // c-50 with a 100-unit box: 225 px square, centred on the 1920x1080 screen.
    assert_eq!(
        (on.x(), on.y(), on.wide(), on.tall()),
        (960 - 112, 540 - 112, 225, 225)
    );
    assert!(model_with("").hud_crosshair(screen).is_none());
}

#[test]
fn a_crosshair_turned_on_by_a_base_file_shows() {
    let layout = hudlayout_with_killfeed(&CROSSHAIR.replace("visible 0 ", ""));
    let layout = format!("#base \"crosshair_on.res\"\n{layout}");
    let files = stock_files_with(&[
        ("resource/clientscheme.res", Some(SCHEME)),
        ("scripts/hudlayout.res", Some(layout.as_str())),
        (
            "scripts/crosshair_on.res",
            Some("\"x\" { CustomCrosshair { visible 1 labelText \"o\" } }"),
        ),
    ]);
    let m = HudModel::load(&files, Platform::Linux).unwrap();
    let crosshair = m.hud_crosshair(Screen::new(1920, 1080)).unwrap();
    // The base turns it on, and the file's own symbol wins over the base's.
    assert!(crosshair.shown());
    assert_eq!(crosshair.symbol(), "+");
}
