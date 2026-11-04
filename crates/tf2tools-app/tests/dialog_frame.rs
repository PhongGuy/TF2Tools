use gpui_kit::px;
use tf2tools_app::widgets::dialog_frame::{
    BOX_CHROME, DETAIL_WIDTH, DIALOG_PADDING, DIALOG_WIDTH, centred_top,
};

#[test]
fn a_box_that_fits_is_centred() {
    assert_eq!(centred_top(px(900.), px(300.), px(24.)), px(300.));
}

#[test]
fn a_box_taller_than_the_view_keeps_the_edge_margin() {
    assert_eq!(centred_top(px(400.), px(500.), px(24.)), px(24.));
}

#[test]
fn a_box_that_nearly_fills_the_view_keeps_the_margin() {
    assert_eq!(centred_top(px(900.), px(880.), px(24.)), px(24.));
}

#[test]
fn widths_are_v5_s_lg_and_2xl() {
    assert_eq!(DIALOG_WIDTH, px(512.));
    assert_eq!(DETAIL_WIDTH, px(672.));
    assert_eq!(BOX_CHROME, px(50.));
}

#[test]
fn the_padding_is_v5_s_p_6() {
    assert_eq!(DIALOG_PADDING, px(24.));
}
