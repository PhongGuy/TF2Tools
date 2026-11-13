//! The Addons page's pure rules: v5's card actions, their TF2 gating and the count line.

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
        "No addons in library".to_owned()
    } else {
        format!("{library} in library, {installed} installed")
    }
}

/// The toast line when Null-Canceling Movement is turned off.
pub const MOVEMENT_OFF: &str = "Your movement keys are back to normal.";

/// The toast line when Null-Canceling Movement is turned on: the keys now on it (in W, A, S,
/// D order), the directions with no key, and whether TF2 must restart first.
pub fn movement_on_text(keys: &[String], missing: &[&str], running: bool) -> String {
    let mut text = if keys.is_empty() {
        "No movement keys were found in your TF2 settings, so it has nothing to use yet.".to_owned()
    } else {
        let names: Vec<String> = keys.iter().map(|k| key_label(k)).collect();
        let mut text = format!("Your {} keys now use it.", names.join(", "));
        if !missing.is_empty() {
            let directions = missing.join(" or ");
            text.push_str(&format!(
                " No key moves you {directions}, so set one in TF2."
            ));
        }
        text
    };
    if running && !keys.is_empty() {
        text.push_str(" It starts next time TF2 opens.");
    }
    text
}

// A key as a player reads it: letters in capitals, arrow keys spelled out.
fn key_label(key: &str) -> String {
    let lower = key.to_ascii_lowercase();
    match lower.strip_suffix("arrow") {
        Some(way @ ("up" | "down" | "left" | "right")) => {
            let mut chars = way.chars();
            let first = chars.next().map(|c| c.to_ascii_uppercase());
            format!("{}{} arrow", first.unwrap_or_default(), chars.as_str())
        }
        _ => key.to_ascii_uppercase(),
    }
}
