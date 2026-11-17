//! The Addons page's pure rules: v5's card actions, their TF2 gating and the count line.

use gpui_kit::SharedString;

use crate::i18n::tr;

/// One button in an addon card's footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddonAction {
    Install,
    Uninstall,
    Update,
    Delete,
}

/// v5's card footer for an addon, in button order.
pub fn actions(installed: bool, update: bool) -> &'static [AddonAction] {
    use AddonAction::*;
    // An installed addon with an update drops Delete: v5 shows only Update and Uninstall.
    match (installed, update) {
        (false, false) => &[Install, Delete],
        (true, false) => &[Uninstall, Delete],
        (false, true) => &[Update, Install, Delete],
        (true, true) => &[Update, Uninstall],
    }
}

/// Whether `action` on an addon (installed or not) needs a TF2 install.
pub fn needs_tf2(action: AddonAction, installed: bool) -> bool {
    // Update on an installed addon reinstalls into tf/custom; otherwise it only downloads.
    match action {
        AddonAction::Install | AddonAction::Uninstall => true,
        AddonAction::Update => installed,
        AddonAction::Delete => false,
    }
}

/// v5's count line: "No addons in library" or "{library} in library, {installed} installed".
pub fn count_line(library: usize, installed: usize) -> String {
    if library == 0 {
        tr!("addons.count.none").to_string()
    } else {
        tr!(
            "addons.count.some",
            library = library,
            installed = installed
        )
        .to_string()
    }
}

/// The toast line when Null-Canceling Movement is turned off.
pub fn movement_off() -> SharedString {
    tr!("addons.movement.normal")
}

/// The toast line when Null-Canceling Movement is turned on: the keys now on it (in W, A, S,
/// D order), the directions with no key ("forward", "left", "back", "right"), and whether TF2
/// must restart first.
pub fn movement_on_text(keys: &[String], missing: &[&str], running: bool) -> String {
    if keys.is_empty() {
        return tr!("addons.movement.none").to_string();
    }
    let names: Vec<String> = keys.iter().map(|k| key_label(k)).collect();
    let mut sentences = vec![tr!("addons.movement.keys", keys = names.join(", ")).to_string()];
    if !missing.is_empty() {
        let words: Vec<String> = missing.iter().map(|id| direction(id).to_string()).collect();
        let directions = words.join(tr!("addons.movement.or").as_ref());
        sentences.push(tr!("addons.movement.unbound", directions = directions).to_string());
    }
    if running {
        sentences.push(tr!("addons.movement.restart").to_string());
    }
    sentences.join(" ")
}

// A direction id as the sentence names it; an unknown id passes through as it is.
fn direction(id: &str) -> SharedString {
    match id {
        "forward" => tr!("addons.movement.direction.forward"),
        "left" => tr!("addons.movement.direction.left"),
        "back" => tr!("addons.movement.direction.back"),
        "right" => tr!("addons.movement.direction.right"),
        other => SharedString::from(other.to_owned()),
    }
}

// A key as a player reads it: letters in capitals, arrow keys spelled out.
fn key_label(key: &str) -> String {
    match key.to_ascii_lowercase().strip_suffix("arrow") {
        Some("up") => tr!("addons.movement.arrow.up").to_string(),
        Some("down") => tr!("addons.movement.arrow.down").to_string(),
        Some("left") => tr!("addons.movement.arrow.left").to_string(),
        Some("right") => tr!("addons.movement.arrow.right").to_string(),
        _ => key.to_ascii_uppercase(),
    }
}
