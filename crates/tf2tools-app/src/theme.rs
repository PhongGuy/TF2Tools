//! Installs the shipped "TF2Tools Dark" theme, v5's shadcn zinc dark, into gpui-kit's
//! global `Theme`. v5 is dark only, so there is no light theme or mode switch.

use std::borrow::Cow;

use gpui_kit::component::{Colorize as _, Theme, ThemeMode, ThemeRegistry};
use gpui_kit::{Anchor, App, Hsla};

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
