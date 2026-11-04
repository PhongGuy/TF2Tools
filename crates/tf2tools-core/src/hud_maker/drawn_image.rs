//! Which texture an image part draws for the local player's team.

use super::model::{HudModel, Part};
use super::sample::Team;

// Image panels whose texture the game's code paints rather than a file key.
const CODE_IMAGES: [(&str, &str); 1] = [("PlayerStatusHealthImage", "../hud/health_color")];
// The freeze cam's team panels, RED then BLU, which the game sets from the death event.
const FREEZE_PANELS: [&str; 2] = ["../hud/color_panel_red", "../hud/color_panel_blu"];

/// The pictures the game's code paints into `part` whatever its keys say: both team panels
/// for `FreezePanelBG`, else a known part's picture when it has no `image` line.
pub(crate) fn code_pictures(part: &Part) -> Option<&'static [&'static str]> {
    if part.name().eq_ignore_ascii_case("FreezePanelBG") {
        return Some(&FREEZE_PANELS);
    }
    if part.value("image").is_some() {
        return None;
    }
    CODE_IMAGES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(part.name()))
        .map(|(_, image)| std::slice::from_ref(image))
}

/// The texture `part` draws when the local player is on `team`: the killer's team panel for
/// the freeze cam's `FreezePanelBG`, which the game sets from the death event whatever its
/// file says (the killer is on the other team); else a `CTFImagePanel`'s non-blank
/// `teambg_2` (RED) or `teambg_3` (BLU), else its `image` (blank included), else the texture
/// the game's code paints into a known part; `None` when it draws no texture.
pub fn drawn_image(part: &Part, team: Team) -> Option<&str> {
    if part.name().eq_ignore_ascii_case("FreezePanelBG") {
        return Some(match team {
            Team::Red => FREEZE_PANELS[1],
            Team::Blue => FREEZE_PANELS[0],
        });
    }
    let team_coloured = part
        .class()
        .is_some_and(|class| class.eq_ignore_ascii_case("CTFImagePanel"));
    let team_key = match team {
        Team::Red => "teambg_2",
        Team::Blue => "teambg_3",
    };
    let team_bg = team_coloured
        .then(|| part.value(team_key))
        .flatten()
        .map(|line| line.text())
        .filter(|text| !text.trim().is_empty());
    team_bg
        .or_else(|| part.value("image").map(|line| line.text()))
        .or_else(|| code_pictures(part).and_then(|names| names.first().copied()))
}

/// The picture `part` draws in `model`'s scene when the local player is on `team`: the
/// scene sample's code-set picture; else an unselected class button's `SubImage` draws its
/// button's non-blank `image_default`; else [`drawn_image`]. Outside a scene whose sample
/// names pictures it is exactly [`drawn_image`].
pub fn scene_image<'a>(model: &'a HudModel, part: &'a Part, team: Team) -> Option<&'a str> {
    let sample = model.scene_rule().sample();
    let parent = part
        .parent()
        .and_then(|key| model.part(key))
        .map(|p| p.name());
    if let Some(image) = sample.code_image(part.name(), parent) {
        return Some(image);
    }
    if part.name().eq_ignore_ascii_case("SubImage")
        && parent.is_some_and(|p| sample.is_class_button(p))
    {
        let default = (part.parent().and_then(|key| model.part(key)))
            .and_then(|button| button.value("image_default"))
            .map(|line| line.text())
            .filter(|text| !text.trim().is_empty());
        if default.is_some() {
            return default;
        }
    }
    drawn_image(part, team)
}
