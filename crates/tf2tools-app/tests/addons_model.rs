use tf2tools_app::pages::addons::model::{
    AddonAction, MOVEMENT_OFF, actions, count_line, movement_on_text, needs_tf2,
};

#[test]
fn the_count_line_reads_like_v5() {
    assert_eq!(count_line(0, 0), "No addons in library");
    assert_eq!(count_line(3, 3), "3 in library, 3 installed");
    assert_eq!(count_line(5, 2), "5 in library, 2 installed");
}

#[test]
fn each_card_state_has_v5_s_actions() {
    use AddonAction::*;
    assert_eq!(actions(false, false), [Install, Delete]);
    assert_eq!(actions(true, false), [Uninstall, Delete]);
    assert_eq!(actions(false, true), [Update, Install, Delete]);
    assert_eq!(actions(true, true), [Update, Uninstall]);
}

#[test]
fn install_uninstall_and_a_reinstalling_update_need_tf2() {
    use AddonAction::*;
    assert!(needs_tf2(Install, false));
    assert!(needs_tf2(Uninstall, true));
    assert!(needs_tf2(Update, true));
    assert!(!needs_tf2(Update, false));
    assert!(!needs_tf2(Delete, true));
    assert!(!needs_tf2(Delete, false));
}

fn keys(names: &[&str]) -> Vec<String> {
    names.iter().map(|k| (*k).to_owned()).collect()
}

#[test]
fn turning_null_canceling_movement_on_names_the_keys() {
    let text = movement_on_text(&keys(&["w", "a", "s", "d"]), &[], false);
    assert_eq!(text, "Your W, A, S, D keys now use it.");
    let text = movement_on_text(&keys(&["w", "UPARROW", "a"]), &[], false);
    assert_eq!(text, "Your W, Up arrow, A keys now use it.");
}

#[test]
fn a_direction_without_a_key_is_named() {
    let text = movement_on_text(&keys(&["w", "s"]), &["left", "right"], false);
    assert_eq!(
        text,
        "Your W, S keys now use it. No key moves you left or right, so set one in TF2."
    );
    let text = movement_on_text(&keys(&["w", "s", "d"]), &["left"], false);
    assert!(
        text.ends_with("No key moves you left, so set one in TF2."),
        "{text}"
    );
}

#[test]
fn no_movement_keys_at_all_says_so() {
    let text = movement_on_text(&[], &["forward", "back", "left", "right"], false);
    assert_eq!(
        text,
        "No movement keys were found in your TF2 settings, so it has nothing to use yet."
    );
}

#[test]
fn with_tf2_running_it_starts_next_time() {
    let text = movement_on_text(&keys(&["w", "a", "s", "d"]), &[], true);
    assert_eq!(
        text,
        "Your W, A, S, D keys now use it. It starts next time TF2 opens."
    );
}

#[test]
fn turning_it_off_reads_plainly() {
    assert_eq!(MOVEMENT_OFF, "Your movement keys are back to normal.");
}
