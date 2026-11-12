//! The "Add to profile" menu library cards share. It reads the profiles each time it opens, so
//! creates and deletes since the last open show, and a pick calls back with the profile's id.

use std::rc::Rc;

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{App, Context, ElementId, Window, div};
use tf2tools_core::{Profile, ProfileError, ProfileId};

use crate::ids;
use crate::services::Services;
use crate::widgets::labelled;

/// The success toast's title for `item` put in `profile`, naming its slot `role` when the item
/// type has several ("hitsound") and the item it replaced, if any.
pub fn added_title(
    item: &str,
    profile: &str,
    role: Option<&str>,
    replaced: Option<&str>,
) -> String {
    let role = role.map(|r| format!(" as its {r}")).unwrap_or_default();
    let replaced = replaced
        .map(|r| format!(" (replaced {r})"))
        .unwrap_or_default();
    format!("Added {item} to {profile}{role}{replaced}")
}

/// Called with the picked profile's id.
pub type OnPick = Rc<dyn Fn(ProfileId, &mut Window, &mut App)>;

/// The player's profiles as saved right now.
pub fn read_profiles(cx: &App) -> Result<Vec<Profile>, ProfileError> {
    cx.global::<Services>().library.profiles()
}

/// One run of profiles in the menu: an optional heading over them, each item's id, and what a
/// pick does.
pub struct Section {
    pub heading: Option<&'static str>,
    pub item_id: fn(&ProfileId) -> ElementId,
    pub on_pick: OnPick,
}

/// Adds every section to `menu`, each listing all `profiles`. No profiles, or a failed read,
/// gives a single disabled line in place of all the sections.
fn profile_items(
    menu: PopupMenu,
    profiles: &Result<Vec<Profile>, ProfileError>,
    sections: &[Section],
) -> PopupMenu {
    let note = |id: &'static str, text: &'static str| {
        PopupMenuItem::element(move |_, _| labelled(div(), id, text)).disabled(true)
    };
    let profiles = match profiles {
        Ok(profiles) if profiles.is_empty() => {
            return menu.item(note(ids::PROFILE_MENU_EMPTY, "No profiles yet"));
        }
        Ok(profiles) => profiles,
        Err(e) => {
            tracing::warn!(error = %e, "could not read profiles for the menu");
            return menu.item(note(
                ids::PROFILE_MENU_UNREADABLE,
                "Couldn't read your profiles",
            ));
        }
    };
    sections.iter().fold(menu, |menu, section| {
        let menu = match section.heading {
            Some(heading) => menu.label(heading),
            None => menu,
        };
        profiles.iter().fold(menu, |menu, profile| {
            let (id, name) = (profile.id().clone(), profile.name().to_owned());
            let element_id = (section.item_id)(&id);
            let on_pick = Rc::clone(&section.on_pick);
            menu.item(
                PopupMenuItem::element(move |_, _| {
                    labelled(div(), element_id.clone(), name.clone())
                })
                .on_click(move |_, window, cx| on_pick(id.clone(), window, cx)),
            )
        })
    })
}

/// A menu builder for `dropdown_menu` that reads the profiles once each time the menu opens.
pub fn profile_menu(
    sections: Vec<Section>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, _, cx| profile_items(menu, &read_profiles(cx), &sections)
}
