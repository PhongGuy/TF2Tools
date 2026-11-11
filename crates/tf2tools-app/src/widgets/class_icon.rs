//! A TF2 class's leaderboard icon, with the class's short name standing in when the bundled
//! PNG is missing.

use std::path::Path;

use gpui_kit::{
    AnyElement, IntoElement as _, ObjectFit, ParentElement as _, Pixels, Styled as _,
    StyledImage as _, div, img,
};
use tf2tools_core::TfClass;

/// The short name shown when the icon is missing.
pub fn class_short_name(class: TfClass) -> &'static str {
    // The players' own short names for the two long ones, as the loadout board's narrow headers.
    match class {
        TfClass::Demoman => "Demo",
        TfClass::Engineer => "Engie",
        TfClass::Scout
        | TfClass::Soldier
        | TfClass::Pyro
        | TfClass::Heavy
        | TfClass::Medic
        | TfClass::Sniper
        | TfClass::Spy
        | _ => class.name(),
    }
}

/// The icon at `size`, or `class_short_name` as small text when `path` is None.
pub fn class_icon(class: TfClass, path: Option<&Path>, size: Pixels) -> AnyElement {
    match path {
        Some(path) => img(path.to_path_buf())
            .size(size)
            .flex_none()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        None => div()
            .flex_none()
            .text_xs()
            .child(class_short_name(class))
            .into_any_element(),
    }
}
