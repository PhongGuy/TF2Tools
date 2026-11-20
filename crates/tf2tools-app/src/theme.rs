//! Installs the shipped "TF2Tools Dark" theme, v5's shadcn zinc dark, into gpui-kit's
//! global `Theme`. v5 is dark only, so there is no light theme or mode switch.

use std::borrow::Cow;

use gpui_kit::component::{ActiveTheme as _, Colorize as _, Theme, ThemeMode, ThemeRegistry};
use gpui_kit::{Anchor, App, Font, FontFallbacks, Hsla, SharedString, Styled, font};

/// The shipped theme set, holding the one dark theme.
pub const THEME_JSON: &str = include_str!("../assets/theme/tf2tools.json");
/// The name of the shipped dark theme.
pub const THEME_NAME: &str = "TF2Tools Dark";
/// The bundled static Inter Regular, Medium, SemiBold and Bold (OFL). The system UI font is
/// variable, and the text system draws only its default instance, so weights never show.
pub const FONTS: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Bold.ttf"),
];

/// CJK families tried after the UI font, Simplified Chinese faces first.
pub const UI_FONT_FALLBACKS: [&str; 6] = [
    "Microsoft YaHei UI",
    "PingFang SC",
    "Noto Sans CJK SC",
    "Source Han Sans SC",
    "Source Han Sans CN",
    "Noto Sans SC",
];

/// `family` at normal weight and style with `UI_FONT_FALLBACKS` as its fallbacks.
pub fn ui_font(family: impl Into<SharedString>) -> Font {
    // Inter has no Han glyphs; naming the Chinese faces keeps the platform from picking a
    // Japanese face for shared ideographs.
    Font {
        fallbacks: Some(FontFallbacks::from_fonts(
            UI_FONT_FALLBACKS.map(String::from).to_vec(),
        )),
        ..font(family)
    }
}

/// `el` with the theme's UI font and its fallbacks, inherited by every text below it.
pub fn with_ui_font<E: Styled>(el: E, cx: &App) -> E {
    el.font(ui_font(cx.theme().font_family.clone()))
}

/// Registers the bundled fonts and the theme, makes it the kit's dark theme and switches
/// to dark mode, with toasts at the bottom right as in v5. Safe to call twice. Fonts or a
/// theme that fail to load leave the system fonts or the kit default and log to stderr.
pub fn install(cx: &mut App) {
    // Fonts go first so the theme below already resolves "Inter" on the first frame.
    let fonts = FONTS.iter().map(|f| Cow::Borrowed(*f)).collect();
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("tf2tools: bundled fonts failed to register ({err}); using system fonts");
    }
    if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(THEME_JSON) {
        eprintln!("tf2tools: the bundled theme failed to load: {err}");
        return;
    }
    if let Some(dark) = ThemeRegistry::global(cx).themes().get(THEME_NAME).cloned() {
        Theme::update(cx, |t| t.dark_theme = dark);
    }
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |t| {
        t.notification.placement = Anchor::BottomRight;
        // The kit clamps `list.active.background` to 20% alpha on load, which hides the fill.
        let opaque = t
            .dark_theme
            .colors
            .list_active
            .as_deref()
            .and_then(|hex| Hsla::parse_hex(hex).ok());
        if let Some(fill) = opaque {
            t.list_active = fill;
            t.tokens.list_active = fill.into();
        }
    });
}

/// A hovered card's fill. A hover background replaces the card's own instead of layering on
/// it, so v5's translucent accent/50 would land on the page background and match the card;
/// the opaque accent keeps v5's step above the resting card.
pub fn card_hover(theme: &Theme) -> Hsla {
    theme.accent
}

/// A team's solid fill under white text, as logs.tf heads a log with its score banner: its
/// muted teal-blue and brick-red, the blue a shade darker so white text clears 4.5:1.
pub fn team_fill(team: tf2tools_core::Team) -> Hsla {
    match team {
        tf2tools_core::Team::Red => gpui_kit::rgb(0xa75d50).into(),
        tf2tools_core::Team::Blue | _ => gpui_kit::rgb(0x567b88).into(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AppContext as _, Context, Font, FontStyle, FontWeight, IntoElement, ParentElement as _,
        Render, Styled as _, TestAppContext, Window, WindowOptions, canvas, div,
    };

    use super::{UI_FONT_FALLBACKS, ui_font, with_ui_font};

    #[test]
    fn the_fallbacks_try_simplified_chinese_faces_first() {
        assert_eq!(
            UI_FONT_FALLBACKS,
            [
                "Microsoft YaHei UI",
                "PingFang SC",
                "Noto Sans CJK SC",
                "Source Han Sans SC",
                "Source Han Sans CN",
                "Noto Sans SC",
            ]
        );
    }

    #[test]
    fn the_ui_font_carries_the_fallbacks() {
        let font = ui_font("Inter");
        assert_eq!(font.family.as_ref(), "Inter");
        assert_eq!(font.weight, FontWeight::NORMAL);
        assert_eq!(font.style, FontStyle::Normal);
        let listed = font.fallbacks.map(|f| f.fallback_list().to_vec());
        assert_eq!(listed, Some(UI_FONT_FALLBACKS.map(String::from).to_vec()));
    }

    // Records the text style a bold child of `with_ui_font` lays out with.
    struct Probe(Rc<RefCell<Option<Font>>>);

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let seen = Rc::clone(&self.0);
            let probe = canvas(
                move |_, window, _| *seen.borrow_mut() = Some(window.text_style().font()),
                |_, _, _, _| {},
            )
            .size_full();
            with_ui_font(div().size_full(), cx)
                .child(div().size_full().font_weight(FontWeight::BOLD).child(probe))
        }
    }

    #[gpui_kit::test]
    fn the_fallbacks_reach_text_under_the_root(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let seen = Rc::new(RefCell::new(None));
        let shared = Rc::clone(&seen);
        let win = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                    cx.new(|_| Probe(shared))
                })
            })
            .unwrap()
            .0;
        cx.run_until_parked();
        cx.update_window(win, |_, w, cx| w.render_frame(cx))
            .unwrap();
        let font = seen.borrow().clone().expect("the probe laid out");
        assert_eq!(font.family.as_ref(), "Inter");
        assert_eq!(font.weight, FontWeight::BOLD, "a child's weight still wins");
        assert_eq!(font.fallbacks, ui_font("Inter").fallbacks);
    }

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    // Every UI font the app builds goes through `ui_font`, and the root and the app's tooltip
    // set it, so no text path the app owns drops the fallbacks.
    #[test]
    fn every_app_font_carries_the_fallbacks() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        let bare: Vec<String> = files
            .iter()
            .filter(|p| **p != src.join("theme.rs"))
            .flat_map(|p| {
                let text = std::fs::read_to_string(p).unwrap();
                text.lines()
                    .enumerate()
                    .filter(|(_, line)| line.contains("..font("))
                    .map(|(n, _)| format!("{}:{}", p.display(), n + 1))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            bare.is_empty(),
            "build UI fonts with theme::ui_font: {bare:?}"
        );
        for (file, text) in [
            ("shell/root.rs", include_str!("shell/root.rs")),
            ("widgets/tip.rs", include_str!("widgets/tip.rs")),
        ] {
            assert!(text.contains("with_ui_font("), "{file} sets the UI font");
        }
    }
}
