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
