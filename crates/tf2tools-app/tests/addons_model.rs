use tf2tools_app::pages::addons::model::{AddonAction, actions, count_line, needs_tf2};

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
